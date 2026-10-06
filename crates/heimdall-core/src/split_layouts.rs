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

//! The splits made between saved profiles, as the C# `SplitLayoutMemory` keeps them: for
//! each pair, how it was placed, the share its first side took and when, the most recent
//! first, at most [`MAX_ENTRIES`]. A new split of a pair starts at the share it was last
//! given, mirrored when the pair comes the other way round. One small file beside the
//! profiles; losing it loses no session.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::profile::ProfileId;
use crate::store::{StoreError, write_atomic};

/// Name of the file, beside the profiles.
pub const SPLIT_LAYOUTS_FILE_NAME: &str = "split-layouts.toml";

/// Version of the file this build writes and reads, as the C# `CurrentSchemaVersion`.
const SPLIT_LAYOUTS_VERSION: u32 = 1;

/// Most pairs kept, as the C# `MaxEntries`: a new one beyond drops the oldest.
pub const MAX_ENTRIES: usize = 50;

/// Smallest share a split's first side takes, as the C# `SplitRatio` clamp.
pub const MIN_RATIO: f32 = 0.1;

/// Largest share a split's first side takes.
pub const MAX_RATIO: f32 = 0.9;

/// The whole of a split, which the shares of its two sides make up.
const WHOLE: f32 = 1.0;

/// How the two sides of a split were placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    /// Side by side: the C# `Vertical`.
    SideBySide,
    /// One above the other: the C# `Horizontal`.
    Stacked,
}

/// A split remembered, as the C# `SplitLayoutEntry`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitLayoutEntry {
    /// The profile of the first side, left or above.
    pub first: ProfileId,
    /// The profile of the second side, right or below.
    pub second: ProfileId,
    /// How the two were placed.
    pub orientation: Orientation,
    /// The share the first side took.
    pub ratio: f32,
    /// When it was last made or resized, in seconds since the Unix epoch.
    pub last_used: u64,
}

impl SplitLayoutEntry {
    /// Whether it is the split of `a` and `b`, in either order.
    fn pairs(&self, a: &ProfileId, b: &ProfileId) -> bool {
        (self.first == *a && self.second == *b) || (self.first == *b && self.second == *a)
    }

    /// The other side of a split `id` is one side of; `None` when it is neither.
    fn partner_of(&self, id: &ProfileId) -> Option<&ProfileId> {
        if self.first == *id {
            Some(&self.second)
        } else if self.second == *id {
            Some(&self.first)
        } else {
            None
        }
    }
}

/// On-disk shape of the file.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SplitLayoutsFile {
    #[serde(default)]
    version: u32,
    #[serde(default, rename = "entry")]
    entries: Vec<SplitLayoutEntry>,
}

/// The splits remembered, read once and written on each one recorded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SplitLayouts {
    /// The file they are kept in; `None` keeps them for the run only.
    path: Option<PathBuf>,
    /// The most recent first.
    entries: Vec<SplitLayoutEntry>,
}

impl SplitLayouts {
    /// The splits kept at `path`; none when there is no file.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file is there and cannot be read, does not parse, or
    /// was written by a newer version.
    pub fn load(path: &Path) -> Result<Self, StoreError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self::kept_at(path));
            }
            Err(source) => {
                return Err(StoreError::Io {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        let file: SplitLayoutsFile = toml::from_str(&text).map_err(|source| StoreError::Parse {
            path: path.to_owned(),
            source,
        })?;
        if file.version > SPLIT_LAYOUTS_VERSION {
            return Err(StoreError::UnsupportedVersion {
                path: path.to_owned(),
                found: file.version,
                expected: SPLIT_LAYOUTS_VERSION,
            });
        }
        let mut entries = file.entries;
        entries.truncate(MAX_ENTRIES);
        Ok(Self {
            path: Some(path.to_owned()),
            entries,
        })
    }

    /// The splits kept at `path`, and why none were read when one was there. A file that
    /// cannot be read starts empty and is replaced at the next split recorded, as the C#
    /// memory treats a failed load; one written by a newer version starts empty too, but
    /// is never written over: what is recorded then lasts as long as the run.
    #[must_use]
    pub fn open(path: &Path) -> (Self, Option<StoreError>) {
        match Self::load(path) {
            Ok(layouts) => (layouts, None),
            Err(error @ StoreError::UnsupportedVersion { .. }) => (Self::default(), Some(error)),
            Err(error) => (Self::kept_at(path), Some(error)),
        }
    }

    /// No split remembered yet, kept at `path`.
    fn kept_at(path: &Path) -> Self {
        Self {
            path: Some(path.to_owned()),
            entries: Vec::new(),
        }
    }

    /// The file they are kept in; `None` while they last as long as the run.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The splits remembered, the most recent first.
    #[must_use]
    pub fn entries(&self) -> &[SplitLayoutEntry] {
        &self.entries
    }

    /// Records the split of `first` and `second`, `first` left or above, in place of the one
    /// of the same pair in either order, first among them, the oldest dropped beyond
    /// [`MAX_ENTRIES`]; and writes the file. A share that is no number is not recorded.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file cannot be written; the split is remembered for
    /// the run all the same.
    pub fn record(
        &mut self,
        first: &ProfileId,
        second: &ProfileId,
        orientation: Orientation,
        ratio: f32,
    ) -> Result<(), StoreError> {
        if !ratio.is_finite() {
            return Ok(());
        }
        self.entries.retain(|entry| !entry.pairs(first, second));
        let last_used = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        self.entries.insert(
            0,
            SplitLayoutEntry {
                first: first.clone(),
                second: second.clone(),
                orientation,
                ratio,
                last_used,
            },
        );
        self.entries.truncate(MAX_ENTRIES);
        self.save()
    }

    /// The share `first` takes in the split of this exact pair last recorded, `first` left
    /// or above: mirrored when it was recorded the other way round, as the C# `FindRatio`,
    /// and held within [`MIN_RATIO`] and [`MAX_RATIO`]; `None` when the pair is unknown.
    #[must_use]
    pub fn ratio(&self, first: &ProfileId, second: &ProfileId) -> Option<f32> {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.pairs(first, second))?;
        let ratio = if entry.first == *first {
            entry.ratio
        } else {
            WHOLE - entry.ratio
        };
        ratio
            .is_finite()
            .then_some(ratio.clamp(MIN_RATIO, MAX_RATIO))
    }

    /// The profiles `id` was split with, the most recent first, as the C# `FindAllPartners`.
    #[must_use]
    pub fn partners(&self, id: &ProfileId) -> Vec<&ProfileId> {
        self.entries
            .iter()
            .filter_map(|entry| entry.partner_of(id))
            .collect()
    }

    fn save(&self) -> Result<(), StoreError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let text = toml::to_string_pretty(&SplitLayoutsFile {
            version: SPLIT_LAYOUTS_VERSION,
            entries: self.entries.clone(),
        })?;
        write_atomic(path, &text)
    }
}

/// The file beside `profiles_file`.
#[must_use]
pub fn split_layouts_path(profiles_file: &Path) -> PathBuf {
    profiles_file.with_file_name(SPLIT_LAYOUTS_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> ProfileId {
        ProfileId::new(name)
    }

    fn path(dir: &tempfile::TempDir) -> PathBuf {
        split_layouts_path(&dir.path().join("profiles.toml"))
    }

    #[test]
    fn the_most_recent_comes_first_and_the_oldest_goes_beyond_the_most_kept() {
        let mut layouts = SplitLayouts::default();
        for n in 0..=MAX_ENTRIES {
            layouts
                .record(&id("a"), &id(&n.to_string()), Orientation::SideBySide, 0.5)
                .expect("kept nowhere");
        }
        assert_eq!(layouts.entries().len(), MAX_ENTRIES);
        assert_eq!(layouts.entries()[0].second, id(&MAX_ENTRIES.to_string()));
        assert_eq!(
            layouts.ratio(&id("a"), &id("0")),
            None,
            "the oldest dropped"
        );
        assert_eq!(layouts.ratio(&id("a"), &id("1")), Some(0.5));
        let partners = layouts.partners(&id("a"));
        assert_eq!(partners.len(), MAX_ENTRIES);
        assert_eq!(
            *partners[0],
            id(&MAX_ENTRIES.to_string()),
            "most recent first"
        );
        assert!(layouts.partners(&id("z")).is_empty());
    }

    #[test]
    fn the_same_pair_in_either_order_is_recorded_once_and_moves_first() {
        let mut layouts = SplitLayouts::default();
        layouts
            .record(&id("a"), &id("b"), Orientation::SideBySide, 0.25)
            .expect("kept");
        layouts
            .record(&id("c"), &id("d"), Orientation::Stacked, 0.5)
            .expect("kept");
        layouts
            .record(&id("b"), &id("a"), Orientation::Stacked, 0.6)
            .expect("kept");
        assert_eq!(layouts.entries().len(), 2);
        let entry = &layouts.entries()[0];
        assert_eq!((&entry.first, &entry.second), (&id("b"), &id("a")));
        assert_eq!(entry.orientation, Orientation::Stacked);
        assert_eq!(layouts.ratio(&id("b"), &id("a")), Some(0.6));
        layouts
            .record(&id("a"), &id("b"), Orientation::Stacked, f32::NAN)
            .expect("ignored");
        assert_eq!(
            layouts.ratio(&id("b"), &id("a")),
            Some(0.6),
            "no share is not one"
        );
    }

    #[test]
    fn a_pair_the_other_way_round_gives_the_mirrored_share_within_the_clamp() {
        let mut layouts = SplitLayouts::default();
        layouts
            .record(&id("a"), &id("b"), Orientation::SideBySide, 0.25)
            .expect("kept");
        assert_eq!(layouts.ratio(&id("a"), &id("b")), Some(0.25));
        assert_eq!(layouts.ratio(&id("b"), &id("a")), Some(0.75), "1 - ratio");
        assert_eq!(layouts.ratio(&id("a"), &id("c")), None, "another pair");
        layouts
            .record(&id("a"), &id("c"), Orientation::SideBySide, 0.02)
            .expect("kept");
        assert_eq!(layouts.ratio(&id("a"), &id("c")), Some(MIN_RATIO));
        assert_eq!(layouts.ratio(&id("c"), &id("a")), Some(MAX_RATIO));
    }

    #[test]
    fn what_is_recorded_is_written_and_read_back() {
        let dir = tempfile::tempdir().expect("dir");
        let path = path(&dir);
        let (mut layouts, error) = SplitLayouts::open(&path);
        assert!(error.is_none(), "no file: none remembered, nothing wrong");
        assert!(layouts.entries().is_empty());
        layouts
            .record(&id("a"), &id("b"), Orientation::Stacked, 0.25)
            .expect("written");
        layouts
            .record(&id("c"), &id("a"), Orientation::SideBySide, 0.4)
            .expect("written");
        let read = SplitLayouts::load(&path).expect("read");
        assert_eq!(read, layouts);
        assert_eq!(read.ratio(&id("b"), &id("a")), Some(0.75));
        let text = fs::read_to_string(&path).expect("text");
        assert!(text.contains("version = 1"), "{text}");
    }

    #[test]
    fn a_file_that_does_not_read_starts_empty_and_is_replaced() {
        let dir = tempfile::tempdir().expect("dir");
        let path = path(&dir);
        fs::write(&path, "not = [toml").expect("written");
        let (mut layouts, error) = SplitLayouts::open(&path);
        assert!(matches!(error, Some(StoreError::Parse { .. })), "{error:?}");
        assert!(layouts.entries().is_empty());
        assert_eq!(layouts.path(), Some(path.as_path()));
        layouts
            .record(&id("a"), &id("b"), Orientation::SideBySide, 0.25)
            .expect("written");
        assert_eq!(SplitLayouts::load(&path).expect("read"), layouts);
    }

    #[test]
    fn a_newer_file_starts_empty_and_is_never_written_over() {
        let dir = tempfile::tempdir().expect("dir");
        let path = path(&dir);
        let newer = "version = 2\n\n[[entry]]\nfirst = \"a\"\nsecond = \"b\"\n\
                     orientation = \"stacked\"\nratio = 0.25\nlast_used = 1\n";
        fs::write(&path, newer).expect("written");
        let (mut layouts, error) = SplitLayouts::open(&path);
        assert!(
            matches!(
                error,
                Some(StoreError::UnsupportedVersion {
                    found: 2,
                    expected: SPLIT_LAYOUTS_VERSION,
                    ..
                })
            ),
            "{error:?}"
        );
        assert!(layouts.entries().is_empty());
        assert_eq!(layouts.path(), None, "kept for the run only");
        layouts
            .record(&id("a"), &id("c"), Orientation::SideBySide, 0.4)
            .expect("kept nowhere");
        assert_eq!(layouts.ratio(&id("a"), &id("c")), Some(0.4));
        assert_eq!(fs::read_to_string(&path).expect("text"), newer, "untouched");
    }
}
