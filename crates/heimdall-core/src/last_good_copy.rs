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

//! The last good copy kept beside the profiles and the settings, as the C# `ConfigManager`
//! keeps one beside `servers.json` and `settings.json` (`ConfigManager.cs:1371-1462`).
//!
//! Each save first copies the file it replaces to its backup, when that file is readable:
//! a damaged file never replaces the one good copy there is. A file left empty or zeroed by
//! a power cut is then read from its backup instead, and the application says so; the
//! damaged file itself is replaced by the next save. With no usable backup the file fails
//! as it always did. The vault keeps its own backup and is not concerned.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::store::{StoreError, write_atomic};

/// What is added to a file's name to name its last good copy, as the C# `BackupPathFor`.
const BACKUP_SUFFIX: &str = ".bak";

/// The last good copy kept beside `path`: its name with `.bak` added, as the C#
/// `BackupPathFor`.
#[must_use]
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(BACKUP_SUFFIX);
    PathBuf::from(name)
}

/// What reading a profiles or settings file found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Document {
    /// No file there.
    Missing,
    /// The file's text: readable, or unreadable with no usable copy, to fail as before.
    Read(String),
    /// The file was unreadable: the text of its last good copy.
    Recovered(String),
}

/// Whether `text` is readable TOML, as the C# `IsReadableJson`: neither blank nor broken.
fn readable(text: &str) -> bool {
    !text.trim().is_empty() && text.parse::<toml::Table>().is_ok()
}

/// The text of `path`'s last good copy, when there is one and it is readable.
fn last_good_text(path: &Path) -> Option<String> {
    fs::read_to_string(backup_path(path))
        .ok()
        .filter(|text| readable(text))
}

/// Reads the file at `path`, falling back to its last good copy when the file itself is not
/// readable, as the C# `ReadDocumentTextAsync`: the copy is only read here, the damaged file
/// being replaced by the next save.
///
/// # Errors
///
/// The error reading the file, other than its absence; a file that is not UTF-8 with no
/// usable copy is [`io::ErrorKind::InvalidData`], as reading it as text always said.
pub(crate) fn read_document(path: &Path) -> io::Result<Document> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Document::Missing),
        Err(error) => return Err(error),
    };
    match String::from_utf8(bytes) {
        Ok(text) if readable(&text) => Ok(Document::Read(text)),
        unreadable => match last_good_text(path) {
            Some(copy) => {
                log::error!(
                    "{} is not readable; loaded the last good copy from {}. The damaged file \
                     is replaced at the next save.",
                    path.display(),
                    backup_path(path).display()
                );
                Ok(Document::Recovered(copy))
            }
            None => unreadable
                .map(Document::Read)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        },
    }
}

/// Writes `text` to `path` as [`write_atomic`] does, the file it replaces kept first as its
/// last good copy when it is readable, as the C# `WriteTextAsync`. The copy is written the
/// same way, through a temporary file beside it. A copy that cannot be kept is logged and
/// the save goes ahead: a missing backup must not cost the user a save.
///
/// # Errors
///
/// [`StoreError`] when writing `path` fails.
pub(crate) fn write_keeping_copy(path: &Path, text: &str) -> Result<(), StoreError> {
    keep_copy(path);
    write_atomic(path, text)
}

/// Copies the file at `path` to its backup, when it is readable.
fn keep_copy(path: &Path) {
    let current = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Err(error) => {
            log::warn!("could not keep a backup of {}: {error}", path.display());
            return;
        }
    };
    let Ok(current) = String::from_utf8(current) else {
        return;
    };
    if !readable(&current) {
        return;
    }
    if let Err(error) = write_atomic(&backup_path(path), &current) {
        log::warn!("could not keep a backup of {}: {error}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backup_is_named_as_the_csharp_one() {
        assert_eq!(
            backup_path(Path::new("dir/settings.toml")),
            PathBuf::from("dir/settings.toml.bak")
        );
    }

    #[test]
    fn a_blank_zeroed_or_broken_file_is_not_readable() {
        assert!(readable("version = 1\n"));
        assert!(!readable(""));
        assert!(!readable("  \n"));
        assert!(!readable("\0\0\0\0"));
        assert!(!readable("version = "));
    }

    #[test]
    fn a_save_keeps_the_readable_file_it_replaces_and_never_a_damaged_one() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("settings.toml");
        write_keeping_copy(&path, "version = 1\n").expect("first save");
        assert!(!backup_path(&path).exists(), "nothing replaced yet");
        write_keeping_copy(&path, "version = 2\n").expect("second save");
        assert_eq!(
            fs::read_to_string(backup_path(&path)).expect("copy"),
            "version = 1\n"
        );
        fs::write(&path, "\0\0\0").expect("damaged");
        write_keeping_copy(&path, "version = 3\n").expect("third save");
        assert_eq!(
            fs::read_to_string(backup_path(&path)).expect("copy"),
            "version = 1\n",
            "the damaged file did not replace the good copy"
        );
    }

    #[test]
    fn an_unreadable_file_is_read_from_its_copy_and_fails_as_before_without_one() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("profiles.toml");
        assert_eq!(read_document(&path).expect("read"), Document::Missing);
        fs::write(&path, "").expect("emptied");
        assert_eq!(
            read_document(&path).expect("read"),
            Document::Read(String::new()),
            "no copy: the text as it is"
        );
        fs::write(backup_path(&path), "broken = ").expect("broken copy");
        assert_eq!(
            read_document(&path).expect("read"),
            Document::Read(String::new()),
            "an unreadable copy is no copy"
        );
        fs::write(backup_path(&path), "version = 1\n").expect("good copy");
        assert_eq!(
            read_document(&path).expect("read"),
            Document::Recovered("version = 1\n".to_owned())
        );
        fs::write(&path, [0xff, 0xfe, 0x00]).expect("not text");
        assert_eq!(
            read_document(&path).expect("read"),
            Document::Recovered("version = 1\n".to_owned())
        );
        fs::remove_file(backup_path(&path)).expect("copy gone");
        assert_eq!(
            read_document(&path).expect_err("not text").kind(),
            io::ErrorKind::InvalidData
        );
        fs::write(&path, "version = 2\n").expect("good file");
        assert_eq!(
            read_document(&path).expect("read"),
            Document::Read("version = 2\n".to_owned())
        );
    }
}
