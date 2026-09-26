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

//! The profile file of Heimdall-rs.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::profile::SshProfile;

/// Format version written into the profile file.
pub const PROFILE_FILE_VERSION: u32 = 1;

/// On-disk shape of the profile file.
#[derive(Debug, Serialize, Deserialize)]
struct ProfileFile {
    version: u32,
    #[serde(default)]
    ssh: Vec<SshProfile>,
}

/// Why the profile file could not be read or written.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The file or its directory could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// File or directory concerned.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// The file is not valid TOML for this format.
    #[error("{path}: {source}")]
    Parse {
        /// File concerned.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: toml::de::Error,
    },
    /// The profiles could not be serialised.
    #[error("profiles could not be serialised: {0}")]
    Serialize(#[from] toml::ser::Error),
    /// The file was written by a newer or unknown format.
    #[error("{path}: format version {found}, expected {expected}")]
    UnsupportedVersion {
        /// File concerned.
        path: PathBuf,
        /// Version found in the file.
        found: u32,
        /// Version this build reads.
        expected: u32,
    },
}

/// Outcome of [`ProfileStore::merge`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// Profiles whose identifier was not in the store.
    pub added: usize,
    /// Profiles whose identifier was in the store with different content.
    pub updated: usize,
    /// Profiles already in the store with the same content.
    pub unchanged: usize,
}

/// Profiles held in one file.
#[derive(Debug)]
pub struct ProfileStore {
    path: PathBuf,
    ssh: Vec<SshProfile>,
}

impl ProfileStore {
    /// An empty store that will save to `path`, without reading it. For when the existing
    /// file is unreadable: saving then writes to `path`, never over the unreadable file,
    /// provided the caller passes another path.
    #[must_use]
    pub fn empty(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            ssh: Vec::new(),
        }
    }

    /// Opens the store at `path`; a missing file is an empty store.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file exists and cannot be read, does not parse, or has
    /// another format version.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let path = path.into();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    path,
                    ssh: Vec::new(),
                });
            }
            Err(source) => return Err(StoreError::Io { path, source }),
        };
        let file: ProfileFile = match toml::from_str(&text) {
            Ok(file) => file,
            Err(source) => return Err(StoreError::Parse { path, source }),
        };
        if file.version != PROFILE_FILE_VERSION {
            return Err(StoreError::UnsupportedVersion {
                path,
                found: file.version,
                expected: PROFILE_FILE_VERSION,
            });
        }
        Ok(Self {
            path,
            ssh: file.ssh,
        })
    }

    /// File this store reads and writes.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// SSH profiles, in file order.
    #[must_use]
    pub fn ssh_profiles(&self) -> &[SshProfile] {
        &self.ssh
    }

    /// Adds or replaces profiles by identifier; the order of existing profiles is kept.
    pub fn merge(&mut self, incoming: impl IntoIterator<Item = SshProfile>) -> MergeReport {
        let mut report = MergeReport::default();
        for profile in incoming {
            match self
                .ssh
                .iter_mut()
                .find(|existing| existing.id == profile.id)
            {
                Some(existing) if *existing == profile => report.unchanged += 1,
                Some(existing) => {
                    *existing = profile;
                    report.updated += 1;
                }
                None => {
                    self.ssh.push(profile);
                    report.added += 1;
                }
            }
        }
        report
    }

    /// Writes the store to its file, creating the directory if needed.
    ///
    /// The content goes to a temporary file in the same directory, which then replaces the
    /// profile file, so an interruption never leaves a truncated file behind.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when serialisation or any file operation fails.
    pub fn save(&self) -> Result<(), StoreError> {
        let text = toml::to_string_pretty(&ProfileFile {
            version: PROFILE_FILE_VERSION,
            ssh: self.ssh.clone(),
        })?;
        let dir = self
            .path
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let io_error = |path: &Path| {
            let path = path.to_owned();
            move |source| StoreError::Io { path, source }
        };
        fs::create_dir_all(dir).map_err(io_error(dir))?;
        let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(io_error(dir))?;
        temp.write_all(text.as_bytes())
            .map_err(io_error(temp.path()))?;
        temp.as_file().sync_all().map_err(io_error(temp.path()))?;
        temp.persist(&self.path)
            .map_err(|error| io_error(&self.path)(error.error))?;
        Ok(())
    }
}
