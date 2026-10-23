/*
 * Copyright 2026 Julien Bombled
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! What is kept of the use of the command library: the favorites and the history of the
//! commands sent, in one file beside the user's commands.
//!
//! The C# keeps them in the TwinShell database (`UserFavorite.cs`, `CommandHistory.cs`).
//! Its history rows hold the command and the values as typed, sealed by the vault
//! (`CommandLibraryViewModel.History.cs:123-146`). Nothing here is sealed, so nothing typed
//! is kept: a row holds the command as its pattern, each value still a `{name}`, and the
//! names of the values, never what was put in them. A replay therefore asks the values
//! again, and the command that reaches a terminal is made from what is typed then.
//! The C# keeps 90 days (`AppConstants.CommandHistoryRetentionDays`) and shows the latest 50
//! (`HistoryPageSize`); both bounds apply to what is stored here.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::private_file::write_private;

/// The file of favorites and history, beside the user's commands.
pub const ACTIVITY_FILE: &str = "command-library-state.json";

/// The most rows of history kept, as the C# `HistoryPageSize`.
pub const MAX_HISTORY_ENTRIES: usize = 50;

/// The days a row is kept, as the C# `CommandHistoryRetentionDays`.
pub const HISTORY_RETENTION_DAYS: i64 = 90;

/// The most favorites kept: the library is far smaller, a file past this is not ours.
pub const MAX_FAVORITES: usize = 2000;

/// The largest file read.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// The seconds of a day.
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

/// The version of the file written.
const FORMAT_VERSION: u32 = 1;

/// The kept name of a command of the user, after this prefix its identity.
pub const USER_KEY_PREFIX: &str = "user:";

/// The kept name of a command of the catalog, after this prefix its identity in the catalogue.
pub const CATALOG_KEY_PREFIX: &str = "catalog:";

/// One command sent, as the history keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// The command it came from: `catalog:<identity>` or `user:<identity>`.
    pub command: String,
    /// Its title when it was sent.
    pub title: String,
    /// The command as its pattern, each value a `{name}`: what was typed is never kept.
    pub pattern: String,
    /// The names of the values it asked for.
    pub parameters: Vec<String>,
    /// The name of the terminal tab that received it.
    pub target: String,
    /// When, in seconds since 1970.
    pub at: i64,
}

/// Why the file could not be used.
#[derive(Debug)]
pub enum ActivityError {
    /// It could not be read.
    Io(io::Error),
    /// It is too large or is not ours.
    Format,
}

/// The favorites and the history.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CommandActivity {
    version: u32,
    favorites: Vec<String>,
    history: Vec<HistoryEntry>,
}

/// The kept name of a command of the catalog.
#[must_use]
pub fn catalog_key(id: &str) -> String {
    format!("{CATALOG_KEY_PREFIX}{id}")
}

/// The kept name of a command of the user.
#[must_use]
pub fn user_key(id: &str) -> String {
    format!("{USER_KEY_PREFIX}{id}")
}

impl CommandActivity {
    /// The favorites, in the order they were starred.
    #[must_use]
    pub fn favorites(&self) -> &[String] {
        &self.favorites
    }

    /// Whether the command named `key` is a favorite.
    #[must_use]
    pub fn is_favorite(&self, key: &str) -> bool {
        self.favorites.iter().any(|each| each == key)
    }

    /// Stars the command named `key`, or unstars it, as the C# `ToggleFavoriteAsync`; whether
    /// it is a favorite now. A file already full of favorites takes no more.
    pub fn toggle_favorite(&mut self, key: &str) -> bool {
        if let Some(index) = self.favorites.iter().position(|each| each == key) {
            self.favorites.remove(index);
            return false;
        }
        if self.favorites.len() >= MAX_FAVORITES {
            return false;
        }
        self.favorites.push(key.to_owned());
        true
    }

    /// Renames the commands of the catalog `renames` name, as `(old, new)` pairs of the
    /// names after [`CATALOG_KEY_PREFIX`], in the favorites and in the history: the command
    /// library moved from ten commands written in code to the shipped catalogue, and what the
    /// user starred or sent stays theirs. A favorite that is already there under the new
    /// name is kept once. Whether anything changed.
    pub fn rename_catalog_keys(&mut self, renames: &[(&str, &str)]) -> bool {
        let renamed = |key: &str| -> Option<String> {
            let name = key.strip_prefix(CATALOG_KEY_PREFIX)?;
            renames
                .iter()
                .find(|(old, _)| *old == name)
                .map(|(_, new)| catalog_key(new))
        };
        let mut changed = false;
        let mut favorites: Vec<String> = Vec::with_capacity(self.favorites.len());
        for favorite in &self.favorites {
            let kept = renamed(favorite).map_or_else(
                || favorite.clone(),
                |new| {
                    changed = true;
                    new
                },
            );
            if favorites.contains(&kept) {
                changed = true;
            } else {
                favorites.push(kept);
            }
        }
        self.favorites = favorites;
        for entry in &mut self.history {
            if let Some(new) = renamed(&entry.command) {
                entry.command = new;
                changed = true;
            }
        }
        changed
    }

    /// The history, the latest first.
    #[must_use]
    pub fn history(&self) -> &[HistoryEntry] {
        &self.history
    }

    /// Adds `entry` as the latest, then drops what is older than the retention and what is
    /// past the cap.
    pub fn record(&mut self, entry: HistoryEntry) {
        let now = entry.at;
        self.history.insert(0, entry);
        self.prune(now);
    }

    /// Drops the rows older than [`HISTORY_RETENTION_DAYS`] at `now` and those past
    /// [`MAX_HISTORY_ENTRIES`].
    pub fn prune(&mut self, now: i64) {
        let oldest = now.saturating_sub(HISTORY_RETENTION_DAYS.saturating_mul(SECONDS_PER_DAY));
        self.history.retain(|entry| entry.at >= oldest);
        self.history.truncate(MAX_HISTORY_ENTRIES);
    }

    /// Empties the history; the favorites stay.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// The state kept at `path`; an empty one when there is no file.
    ///
    /// # Errors
    ///
    /// [`ActivityError`] when the file cannot be read or is not ours: the caller must then
    /// not save over it.
    pub fn load(path: &Path) -> Result<Self, ActivityError> {
        let length = match fs::metadata(path) {
            Ok(meta) => meta.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(ActivityError::Io(error)),
        };
        if length > MAX_FILE_BYTES {
            return Err(ActivityError::Format);
        }
        let text = fs::read_to_string(path).map_err(ActivityError::Io)?;
        let mut activity: Self = serde_json::from_str(&text).map_err(|_| ActivityError::Format)?;
        if activity.version > FORMAT_VERSION || activity.favorites.len() > MAX_FAVORITES {
            return Err(ActivityError::Format);
        }
        activity.history.truncate(MAX_HISTORY_ENTRIES);
        Ok(activity)
    }

    /// Keeps the state at `path`, readable by its owner alone.
    ///
    /// # Errors
    ///
    /// The error of writing the file; what was there is left as it was.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        let mut kept = self.clone();
        kept.version = FORMAT_VERSION;
        let text = serde_json::to_string_pretty(&kept).map_err(io::Error::other)?;
        write_private(path, text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(at: i64) -> HistoryEntry {
        HistoryEntry {
            command: catalog_key("mtr-probe"),
            title: "Network path quality".to_owned(),
            pattern: "mtr {host}".to_owned(),
            parameters: vec!["host".to_owned()],
            target: "web-1".to_owned(),
            at,
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("heimdall-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn a_favorite_is_toggled_on_then_off() {
        let mut activity = CommandActivity::default();
        assert!(activity.toggle_favorite("user:1"));
        assert!(activity.is_favorite("user:1"));
        assert!(!activity.toggle_favorite("user:1"));
        assert!(!activity.is_favorite("user:1"));
    }

    #[test]
    fn the_history_keeps_the_latest_first_and_the_cap() {
        let mut activity = CommandActivity::default();
        for at in 0..i64::try_from(MAX_HISTORY_ENTRIES + 5).unwrap_or(0) {
            activity.record(entry(1_000 + at));
        }
        assert_eq!(activity.history().len(), MAX_HISTORY_ENTRIES);
        assert!(activity.history()[0].at > activity.history()[1].at);
    }

    #[test]
    fn a_row_past_the_retention_is_dropped() {
        let mut activity = CommandActivity::default();
        let now = 10 * HISTORY_RETENTION_DAYS * SECONDS_PER_DAY;
        activity.record(entry(now - SECONDS_PER_DAY));
        activity.record(entry(now));
        assert_eq!(activity.history().len(), 2);
        // The day the older row turns past the retention, it goes and the other stays.
        activity.prune(now + HISTORY_RETENTION_DAYS * SECONDS_PER_DAY);
        assert_eq!(activity.history().len(), 1);
        assert_eq!(activity.history()[0].at, now);
    }

    #[test]
    fn the_catalog_names_are_renamed_in_the_favorites_and_the_history() {
        let mut activity = CommandActivity::default();
        activity.toggle_favorite(&catalog_key("mtr-probe"));
        activity.toggle_favorite("user:1");
        activity.toggle_favorite(&catalog_key("network-trace-route"));
        activity.toggle_favorite(&catalog_key("linux-kill-port"));
        activity.record(entry(5));
        let renames = [("mtr-probe", "network-trace-route")];
        assert!(activity.rename_catalog_keys(&renames));
        assert_eq!(
            activity.favorites(),
            [
                catalog_key("network-trace-route").as_str(),
                "user:1",
                catalog_key("linux-kill-port").as_str()
            ]
        );
        assert_eq!(
            activity.history()[0].command,
            catalog_key("network-trace-route")
        );
        // Done once: nothing more to rename, a user's command of the same name untouched.
        assert!(!activity.rename_catalog_keys(&renames));
        activity.toggle_favorite("user:mtr-probe");
        assert!(!activity.rename_catalog_keys(&renames));
        assert!(activity.is_favorite("user:mtr-probe"));
    }

    #[test]
    fn every_legacy_snippet_a_file_names_leads_to_a_command_of_the_catalogue() {
        use crate::tools::command_library::{LEGACY_SNIPPET_IDS, shipped};
        let mut activity = CommandActivity::default();
        for (old, _) in LEGACY_SNIPPET_IDS {
            activity.toggle_favorite(&catalog_key(old));
        }
        assert!(activity.rename_catalog_keys(&LEGACY_SNIPPET_IDS));
        for favorite in activity.favorites() {
            let id = favorite.strip_prefix(CATALOG_KEY_PREFIX).expect("catalog");
            assert!(shipped().get(id).is_some(), "{id}");
        }
    }

    #[test]
    fn clearing_the_history_keeps_the_favorites() {
        let mut activity = CommandActivity::default();
        activity.toggle_favorite("user:1");
        activity.record(entry(5));
        activity.clear_history();
        assert!(activity.history().is_empty());
        assert!(activity.is_favorite("user:1"));
    }

    #[test]
    fn a_row_keeps_the_pattern_and_survives_a_save() {
        let mut activity = CommandActivity::default();
        activity.record(entry(5));
        let folder = scratch("activity");
        let path = folder.join(ACTIVITY_FILE);
        activity.save(&path).expect("saved");
        let text = fs::read_to_string(&path).expect("read");
        assert!(text.contains("mtr {host}"));
        let back = CommandActivity::load(&path).expect("loaded");
        assert_eq!(back.history(), activity.history());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_missing_file_is_empty_and_a_foreign_one_is_refused() {
        let folder = scratch("activity-bad");
        fs::create_dir_all(&folder).expect("folder");
        let path = folder.join(ACTIVITY_FILE);
        assert_eq!(
            CommandActivity::load(&path).expect("empty"),
            CommandActivity::default()
        );
        fs::write(&path, "not json").expect("write");
        assert!(matches!(
            CommandActivity::load(&path),
            Err(ActivityError::Format)
        ));
        let _ = fs::remove_dir_all(&folder);
    }
}
