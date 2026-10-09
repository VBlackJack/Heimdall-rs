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

//! A file holding a private key, written as the C# `SecureFileWriter.WriteAndProtect`
//! writes one (`SecureFileWriter.cs:40-63`): any file already there removed, a new one
//! made, its access restricted to its owner before a byte of the key is written in it.
//!
//! On Unix the file is made with mode 0600, never readable by another account. On Windows
//! it is made empty, its access list replaced by one granting the current user, the
//! Administrators and SYSTEM alone (`icacls`, as the folders are restricted), and only then
//! written: Win32-OpenSSH refuses a key other accounts can read.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::Path;

use crate::folder_acl;

/// Owner read and write only.
#[cfg(unix)]
const PRIVATE_MODE: u32 = 0o600;

/// Writes `bytes` at `path`, readable by its owner alone, replacing what was there.
///
/// # Errors
///
/// The error of the removal, the creation, the restriction or the write: on any of them
/// nothing of `bytes` is left readable by others, the file being removed again when its
/// access could not be restricted.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    // A file already there keeps its own access list: it is removed, and the new one made
    // with `create_new`, so a file put there in between is refused rather than written.
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut file = create_private(path)?;
    if let Err(error) = folder_acl::restrict_file(&absolute(path)) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(io::Error::other(error));
    }
    file.write_all(bytes)?;
    file.sync_all()
}

/// `path` made from the current folder when it is relative: `icacls` is only ever given an
/// absolute path.
fn absolute(path: &Path) -> std::path::PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}

/// A new, empty file at `path`, made with mode 0600 on Unix.
fn create_private(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(PRIVATE_MODE);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_private_file_replaces_what_was_there() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("id_rsa");
        fs::write(&path, "stale").expect("written");
        write_private(
            &path,
            b"-----BEGIN PRIVATE KEY-----\nBBBB\n-----END PRIVATE KEY-----\n",
        )
        .expect("written");
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "-----BEGIN PRIVATE KEY-----\nBBBB\n-----END PRIVATE KEY-----\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_private_file_is_readable_by_its_owner_alone() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("id_ed25519");
        write_private(&path, b"key").expect("written");
        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
