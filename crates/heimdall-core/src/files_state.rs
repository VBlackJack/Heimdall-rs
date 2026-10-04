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

//! What the Files tabs keep between runs, as the C# `SftpBrowserStateStore`: the folders
//! bookmarked on each server, and the folder the last download went to. One small file
//! beside the profiles; losing it loses no session.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::{StoreError, write_atomic};

/// Name of the file, beside the profile file.
pub const FILES_STATE_FILE_NAME: &str = "files-state.toml";

/// Format version written into the file.
const FILES_STATE_VERSION: u32 = 1;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FilesStateFile {
    #[serde(default)]
    version: u32,
    /// The folders bookmarked, by server.
    #[serde(default)]
    bookmarks: BTreeMap<String, Vec<String>>,
    /// The folder of this computer the last download went to.
    #[serde(default)]
    last_download_folder: Option<PathBuf>,
}

/// The Files tabs' state, read once and written on each change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesState {
    path: PathBuf,
    file: FilesStateFile,
}

impl FilesState {
    /// The state kept at `path`: empty when there is none yet, or when it cannot be read,
    /// as the C# store treats a failed read.
    #[must_use]
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let file = fs::read_to_string(&path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default();
        Self { path, file }
    }

    /// The file it is kept in.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The folders bookmarked on `server`, in the order they were.
    #[must_use]
    pub fn bookmarks(&self, server: &str) -> &[String] {
        self.file.bookmarks.get(server).map_or(&[], Vec::as_slice)
    }

    /// Replaces the folders bookmarked on `server`, and writes the file.
    ///
    /// # Errors
    ///
    /// The file could not be written.
    pub fn set_bookmarks(
        &mut self,
        server: &str,
        bookmarks: Vec<String>,
    ) -> Result<(), StoreError> {
        if bookmarks.is_empty() {
            self.file.bookmarks.remove(server);
        } else {
            self.file.bookmarks.insert(server.to_owned(), bookmarks);
        }
        self.save()
    }

    /// The folder the last download went to, while it still is one.
    #[must_use]
    pub fn last_download_folder(&self) -> Option<&Path> {
        self.file
            .last_download_folder
            .as_deref()
            .filter(|folder| folder.is_dir())
    }

    /// Remembers the folder a download went to, and writes the file when it changed.
    ///
    /// # Errors
    ///
    /// The file could not be written.
    pub fn set_last_download_folder(&mut self, folder: &Path) -> Result<(), StoreError> {
        if self.file.last_download_folder.as_deref() == Some(folder) {
            return Ok(());
        }
        self.file.last_download_folder = Some(folder.to_owned());
        self.save()
    }

    fn save(&mut self) -> Result<(), StoreError> {
        self.file.version = FILES_STATE_VERSION;
        let text = toml::to_string_pretty(&self.file)?;
        write_atomic(&self.path, &text)
    }
}

impl Default for FilesState {
    /// A state kept nowhere: what it remembers lasts as long as the run.
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            file: FilesStateFile::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FILES_STATE_FILE_NAME, FilesState};

    #[test]
    fn bookmarks_are_kept_by_server_and_the_last_download_folder_while_it_is_one() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(FILES_STATE_FILE_NAME);
        let mut state = FilesState::open(&path);
        assert!(state.bookmarks("sftp://a").is_empty());
        state
            .set_bookmarks("sftp://a", vec!["/srv".to_owned(), "/var/log".to_owned()])
            .expect("saved");
        state.set_last_download_folder(dir.path()).expect("saved");
        let reread = FilesState::open(&path);
        assert_eq!(reread.bookmarks("sftp://a"), ["/srv", "/var/log"]);
        assert!(reread.bookmarks("sftp://b").is_empty(), "another server");
        assert_eq!(reread.last_download_folder(), Some(dir.path()));

        let mut state = reread;
        state.set_bookmarks("sftp://a", Vec::new()).expect("saved");
        assert!(FilesState::open(&path).bookmarks("sftp://a").is_empty());
        state
            .set_last_download_folder(&dir.path().join("gone"))
            .expect("saved");
        assert_eq!(
            FilesState::open(&path).last_download_folder(),
            None,
            "a folder no longer there"
        );
    }

    #[test]
    fn a_file_that_does_not_read_is_an_empty_state() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(FILES_STATE_FILE_NAME);
        std::fs::write(&path, "not = [toml").expect("written");
        assert!(FilesState::open(&path).bookmarks("sftp://a").is_empty());
    }
}
