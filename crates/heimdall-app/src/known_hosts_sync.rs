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

//! The import of the user's OpenSSH `known_hosts` at startup, as the C#
//! `KnownHostsStartupSync`: when chosen, once, in the background, its keys trusted as
//! [`known_hosts_import::sync`] decides, and what it did said in the log only.
//!
//! The file is read as the C# `KnownHostsImporter.ImportFile` reads it: a missing file is
//! nothing to import, a larger one than [`MAX_FILE_BYTES`] is refused whole, and its byte
//! order mark is read as .NET's `StreamReader` reads it. Only a file is read: a folder, a
//! device or a pipe in its place is said and left.

use std::fs::{self, File};
use std::io::{self, Read as _};
use std::path::Path;

use heimdall_ssh::KnownHosts;
use heimdall_ssh::KnownHostsError;
use heimdall_ssh::known_hosts_import::{
    self, HostKeyDiagnostic, HostKeyNote, HostKeysSynced, MAX_FILE_BYTES, Malformed,
};

use crate::text_codec;

/// Why the file to import was not read.
#[derive(Debug)]
pub enum SourceProblem {
    /// Something else than a file has its name.
    NotAFile,
    /// Larger than [`MAX_FILE_BYTES`].
    TooLarge,
    /// The system refused to read it.
    Unreadable(io::Error),
    /// Its byte order mark names an encoding its bytes do not follow.
    Undecodable,
}

/// The text of the `known_hosts` at `path`: `None` when there is none, as the C# finds
/// nothing to import.
///
/// # Errors
///
/// [`SourceProblem`] when it is no file, too large, or cannot be read.
pub fn read_source(path: &Path) -> Result<Option<String>, SourceProblem> {
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => return Err(SourceProblem::NotAFile),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(SourceProblem::Unreadable(error)),
    }
    let file = File::open(path).map_err(SourceProblem::Unreadable)?;
    // What was opened, not only what the name pointed at a moment before.
    if !file
        .metadata()
        .map_err(SourceProblem::Unreadable)?
        .is_file()
    {
        return Err(SourceProblem::NotAFile);
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(SourceProblem::Unreadable)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
        return Err(SourceProblem::TooLarge);
    }
    text_codec::decode_as_read_all_text(&bytes)
        .map(Some)
        .map_err(|_| SourceProblem::Undecodable)
}

/// Imports the keys of the `known_hosts` at `source` into `store`, as the C# does at
/// startup, and says in the log what it did, in the C# words: each line left out, each
/// conflict with both fingerprints, then a summary. A file that cannot be read imports
/// nothing.
///
/// # Errors
///
/// [`KnownHostsError`] when `store` or its pins cannot be read, or a key recorded: said in
/// the log as well.
pub fn run(source: &Path, store: &KnownHosts) -> Result<HostKeysSynced, KnownHostsError> {
    let shown = source.display();
    let text = match read_source(source) {
        Ok(Some(text)) => text,
        Ok(None) => String::new(),
        Err(problem) => {
            match problem {
                SourceProblem::NotAFile => {
                    log::warn!("known_hosts import skipped: '{shown}' is not a file");
                }
                SourceProblem::TooLarge => log::warn!(
                    "known_hosts import refused: file '{shown}' exceeds {MAX_FILE_BYTES} bytes."
                ),
                SourceProblem::Unreadable(error) => {
                    log::warn!("known_hosts import skipped: I/O error reading '{shown}': {error}");
                }
                SourceProblem::Undecodable => {
                    log::warn!("known_hosts import skipped: decoding error in '{shown}'");
                }
            }
            String::new()
        }
    };
    let parsed = known_hosts_import::parse(&text);
    for diagnostic in &parsed.diagnostics {
        log_diagnostic(diagnostic);
    }
    match known_hosts_import::sync(&parsed.candidates, store) {
        Ok(done) => {
            for conflict in &done.conflicts {
                if conflict.within_file {
                    log::warn!(
                        "known_hosts import conflict for {}:{}: the file gives another key of the same algorithm: other={} imported={}",
                        conflict.host,
                        conflict.port,
                        conflict.existing,
                        conflict.imported
                    );
                } else {
                    log::warn!(
                        "known_hosts import conflict for {}:{}: existing={} imported={}",
                        conflict.host,
                        conflict.port,
                        conflict.existing,
                        conflict.imported
                    );
                }
            }
            log::info!(
                "known_hosts startup sync completed: imported={}, matched={}, conflicts={}",
                done.imported,
                done.matched,
                done.conflicts.len()
            );
            Ok(done)
        }
        Err(error) => {
            log::warn!("known_hosts startup sync failed: {error}");
            Err(error)
        }
    }
}

/// A line left out, said as the C# `LogDiagnostic` says it: a line it could not read as a
/// warning, one it read and chose not to import as information.
fn log_diagnostic(diagnostic: &HostKeyDiagnostic) {
    let (code, context) = match &diagnostic.note {
        HostKeyNote::HashedHost => ("HashedEntryNotSupported", None),
        HostKeyNote::CertAuthority => ("CertAuthorityNotSupported", None),
        HostKeyNote::Revoked => ("RevokedEntryNotSupported", None),
        HostKeyNote::HostPattern(pattern) => ("UnsupportedHostPattern", Some(pattern.clone())),
        HostKeyNote::UnsupportedKey(kind) => ("UnsupportedKeyType", Some(kind.clone())),
        HostKeyNote::Malformed(why) => (
            "MalformedLine",
            match why {
                Malformed::TooLong | Malformed::BadKey => None,
                Malformed::Fields(count) => Some(count.to_string()),
                Malformed::Marker(marker) => Some(marker.clone()),
            },
        ),
    };
    let line = diagnostic.line;
    let message = context.map_or_else(
        || format!("known_hosts line {line}: {code}"),
        |context| format!("known_hosts line {line}: {code} ({context})"),
    );
    if diagnostic.note.is_warning() {
        log::warn!("{message}");
    } else {
        log::info!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_nothing_and_a_folder_is_no_file() {
        let dir = tempfile::tempdir().expect("dir");
        assert!(matches!(
            read_source(&dir.path().join("known_hosts")),
            Ok(None)
        ));
        assert!(matches!(
            read_source(dir.path()),
            Err(SourceProblem::NotAFile)
        ));
    }

    #[test]
    fn a_byte_order_mark_is_read_and_left_out() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("known_hosts");
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "web.lab ssh-ed25519 AAAA\n".encode_utf16() {
            bytes.extend(unit.to_le_bytes());
        }
        fs::write(&path, bytes).expect("written");
        assert_eq!(
            read_source(&path).expect("read").as_deref(),
            Some("web.lab ssh-ed25519 AAAA\n")
        );
        fs::write(&path, b"\xEF\xBB\xBFdb.lab ssh-ed25519 BBBB\n").expect("written");
        assert_eq!(
            read_source(&path).expect("read").as_deref(),
            Some("db.lab ssh-ed25519 BBBB\n")
        );
        // A UTF-16 mark with half a character after it.
        fs::write(&path, [0xFF, 0xFE, 0x41]).expect("written");
        assert!(matches!(
            read_source(&path),
            Err(SourceProblem::Undecodable)
        ));
    }

    #[test]
    fn a_file_larger_than_the_limit_is_refused_whole() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("known_hosts");
        let file = File::create(&path).expect("created");
        file.set_len(MAX_FILE_BYTES + 1).expect("sized");
        drop(file);
        assert!(matches!(read_source(&path), Err(SourceProblem::TooLarge)));
        let store = KnownHosts::new(dir.path().join("store").join("known_hosts"));
        let done = run(&path, &store).expect("ran");
        assert_eq!(done, HostKeysSynced::default());
        assert!(!store.path().exists(), "nothing written");
    }
}
