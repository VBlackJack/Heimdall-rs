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

//! The sessions open when the application last closed, as the C# session snapshot keeps
//! them: the saved profiles their tabs came from, in tab order, offered to reopen at the
//! next start. A tab of nothing saved, a session typed in Quick Connect, is not kept: there
//! is nothing to reopen it from.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::profile::ProfileId;
use crate::store::{StoreError, write_atomic};

/// Name of the snapshot's file, beside the profiles.
pub const SESSION_SNAPSHOT_FILE_NAME: &str = "session-snapshot.toml";

/// A session to reopen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotEntry {
    /// The saved profile its tab came from.
    pub profile: ProfileId,
    /// An SSH profile's files rather than its shell.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub files: bool,
}

/// The sessions open when the application closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    /// When it was taken, in seconds since the Unix epoch.
    pub saved_at: u64,
    /// The sessions, in tab order.
    #[serde(default)]
    pub sessions: Vec<SnapshotEntry>,
}

impl SessionSnapshot {
    /// The snapshot of `sessions`, taken now.
    #[must_use]
    pub fn now(sessions: Vec<SnapshotEntry>) -> Self {
        let saved_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        Self { saved_at, sessions }
    }

    /// When it was taken.
    #[must_use]
    pub fn saved_at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.saved_at)
    }
}

/// The snapshot's file beside `profiles_file`.
#[must_use]
pub fn snapshot_path(profiles_file: &Path) -> PathBuf {
    profiles_file.with_file_name(SESSION_SNAPSHOT_FILE_NAME)
}

/// The snapshot at `path`; `None` when there is none, or it holds no session, or it cannot
/// be read: nothing is offered then.
#[must_use]
pub fn load(path: &Path) -> Option<SessionSnapshot> {
    let text = fs::read_to_string(path).ok()?;
    let snapshot: SessionSnapshot = toml::from_str(&text).ok()?;
    (!snapshot.sessions.is_empty()).then_some(snapshot)
}

/// Keeps `snapshot` at `path`; one holding no session leaves no file.
///
/// # Errors
///
/// Returns [`StoreError`] when the file cannot be written or the old one removed.
pub fn save(path: &Path, snapshot: &SessionSnapshot) -> Result<(), StoreError> {
    if snapshot.sessions.is_empty() {
        return clear(path);
    }
    let text = toml::to_string_pretty(snapshot)?;
    write_atomic(path, &text)
}

/// Removes the snapshot at `path`, once answered; none there is fine.
///
/// # Errors
///
/// Returns [`StoreError`] when the file is there and cannot be removed.
pub fn clear(path: &Path) -> Result<(), StoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(StoreError::Io {
            path: path.to_owned(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snapshot_is_kept_read_back_and_cleared_and_an_empty_one_leaves_no_file() {
        let dir = tempfile::tempdir().expect("dir");
        let path = snapshot_path(&dir.path().join("profiles.toml"));
        assert_eq!(load(&path), None, "none yet");
        let snapshot = SessionSnapshot::now(vec![
            SnapshotEntry {
                profile: ProfileId::new("web"),
                files: false,
            },
            SnapshotEntry {
                profile: ProfileId::new("web"),
                files: true,
            },
        ]);
        save(&path, &snapshot).expect("save");
        assert_eq!(load(&path), Some(snapshot));
        clear(&path).expect("clear");
        assert!(!path.exists());
        clear(&path).expect("clearing none is fine");

        save(&path, &SessionSnapshot::now(Vec::new())).expect("save");
        assert!(!path.exists(), "nothing to reopen, no file");
        fs::write(&path, "not toml [").expect("write");
        assert_eq!(load(&path), None, "unreadable: nothing offered");
    }
}
