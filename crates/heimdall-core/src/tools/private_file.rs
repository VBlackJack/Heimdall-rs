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
//! writes one (`SecureFileWriter.cs:40-63`): a new file, nobody else able to open it, its
//! access restricted to its owner before a byte of the key is written in it.
//!
//! The key goes first into a new file of a random name in the same folder, which then takes
//! the target's place in one rename: a key already there survives any failure on the way.
//!
//! On Unix that file is made with mode 0600, never readable by another account. On Windows
//! it is made with no sharing at all, as the C#'s `FileShare.None`: while its handle is open
//! nobody else can open, rename or delete it. With the handle still open its access list is
//! replaced by one granting the current user, the Administrators and SYSTEM alone (`icacls`,
//! which changes an access list through a handle the sharing does not refuse), and only then
//! is the key written: Win32-OpenSSH refuses a key other accounts can read.
//!
//! A log file a session writes, its transcript or a session log, is made the same way as
//! the C# `RestrictedLogFile.Create` makes one (`RestrictedLogFile.cs:37-67`): new and empty,
//! restricted while nobody else can open it, and only then opened to add to it. A transcript
//! can hold a password a server echoed, and its folder can be one the user chose, whose
//! access list is wider than the user's.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::folder_acl;

/// Owner read and write only.
#[cfg(unix)]
const PRIVATE_MODE: u32 = 0o600;

/// No sharing: nobody else may open the file while it is being written, as `FileShare.None`.
#[cfg(windows)]
const SHARE_NONE: u32 = 0;

/// What starts and ends the name of the file written before it takes the target's place.
const TEMPORARY_PREFIX: &str = ".";
const TEMPORARY_SUFFIX: &str = ".tmp";

/// Random bytes in that name.
const TEMPORARY_RANDOM_BYTES: usize = 8;

/// Tries at a free name, should one drawn already exist.
const TEMPORARY_TRIES: usize = 8;

/// Writes `bytes` at `path`, readable by its owner alone, replacing what was there in one
/// rename.
///
/// # Errors
///
/// The error of the creation, the restriction, the write or the rename: on any of them the
/// file written is removed, and what was at `path` is left as it was.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_private_with(path, bytes, restrict_file)
}

/// [`write_private`] with `restrict` restricting the access of the file it is given, for a
/// test that makes the restriction fail.
fn write_private_with(
    path: &Path,
    bytes: &[u8],
    restrict: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let target = absolute(path);
    let (temporary, mut file) = create_temporary(&target)?;
    let written = restrict(&temporary)
        .and_then(|()| file.write_all(bytes))
        .and_then(|()| file.sync_all());
    // Closed before the rename: a handle without delete sharing refuses it.
    drop(file);
    let renamed = written.and_then(|()| fs::rename(&temporary, &target));
    if renamed.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    renamed
}

/// Opens the new log file `path` to add to it, refused if one is there, readable by its
/// owner alone before a byte is written in it.
///
/// # Errors
///
/// The error of the creation or the restriction, a file there already included: when the
/// restriction fails the file made is removed, so that the next try does not take it for
/// one restricted, as the C# `RestrictedLogFile.Create`.
pub fn create_private_log(path: &Path) -> io::Result<File> {
    open_private_log_with(path, true, restrict_file)
}

/// Opens the log file `path` to add to it, made if need be, readable by its owner alone
/// before a byte is written in it. One already there, from an earlier run, is restricted
/// again before anything is added: it may have been made before its folder or this was.
///
/// # Errors
///
/// The error of the creation, the restriction or the opening: nothing is to be written in
/// a file that could not be restricted.
pub fn open_private_log(path: &Path) -> io::Result<File> {
    open_private_log_with(path, false, restrict_file)
}

/// The file `file` restricted to its owner on Windows, as a key is.
fn restrict_file(file: &Path) -> io::Result<()> {
    folder_acl::restrict_file(file).map_err(io::Error::other)
}

/// [`create_private_log`] when `new`, else [`open_private_log`], with `restrict` restricting
/// the access of the file it is given, for a test that makes the restriction fail.
fn open_private_log_with(
    path: &Path,
    new: bool,
    restrict: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<File> {
    let path = absolute(path);
    match create_private(&path) {
        Ok(made) => {
            // Restricted while the handle, sharing nothing, keeps everyone else out, and
            // the file is still empty; closed after, as that handle refuses a second one.
            let restricted = restrict(&path);
            drop(made);
            if let Err(error) = restricted {
                let _ = fs::remove_file(&path);
                return Err(error);
            }
        }
        Err(error) if !new && error.kind() == io::ErrorKind::AlreadyExists => restrict(&path)?,
        Err(error) => return Err(error),
    }
    // Never made here: should the file have gone meanwhile, nothing is written at all.
    OpenOptions::new().append(true).open(&path)
}

/// `path` made from the current folder when it is relative: `icacls` is only ever given an
/// absolute path.
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}

/// A new file of a random name beside `target`, open for writing, made private.
fn create_temporary(target: &Path) -> io::Result<(PathBuf, File)> {
    let folder = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target.file_name().unwrap_or_default();
    let mut last = io::Error::from(io::ErrorKind::AlreadyExists);
    for _ in 0..TEMPORARY_TRIES {
        let mut random = [0_u8; TEMPORARY_RANDOM_BYTES];
        sealvault::random::fill(&mut random).map_err(io::Error::other)?;
        let mut file_name = OsString::from(TEMPORARY_PREFIX);
        file_name.push(name);
        file_name.push(".");
        for byte in random {
            file_name.push(format!("{byte:02x}"));
        }
        file_name.push(TEMPORARY_SUFFIX);
        let temporary = folder.join(file_name);
        match create_private(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => last = error,
            Err(error) => return Err(error),
        }
    }
    Err(last)
}

/// A new, empty file at `path`, refused if one is there: mode 0600 on Unix, no sharing on
/// Windows.
fn create_private(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(PRIVATE_MODE);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.share_mode(SHARE_NONE);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The files of `dir`, by name.
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("listed")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

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
        assert_eq!(names(dir.path()), ["id_rsa"], "no file left beside it");
    }

    #[test]
    fn a_failed_restriction_writes_nothing_and_keeps_the_old_key() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("id_rsa");
        fs::write(&path, "old key").expect("written");
        let failed = write_private_with(&path, b"new key", |temporary| {
            // The key is written only after the restriction: the file is still empty here.
            assert_eq!(fs::metadata(temporary).expect("made").len(), 0);
            Err(io::Error::other("refused"))
        });
        assert!(failed.is_err());
        assert_eq!(fs::read_to_string(&path).expect("read"), "old key");
        assert_eq!(names(dir.path()), ["id_rsa"], "the file written removed");
    }

    #[test]
    fn the_file_is_written_beside_its_target_and_takes_its_place() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("id_ed25519");
        write_private_with(&path, b"key", |temporary| {
            assert_eq!(temporary.parent(), Some(dir.path()));
            assert!(!path.exists(), "the target appears only at the rename");
            Ok(())
        })
        .expect("written");
        assert_eq!(fs::read(&path).expect("read"), b"key");
    }

    #[test]
    fn a_log_is_restricted_while_still_empty_then_opened_to_add_to_it() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("session.log");
        let mut file = open_private_log_with(&path, true, |made| {
            assert_eq!(
                fs::metadata(made).expect("made").len(),
                0,
                "nothing in it yet"
            );
            #[cfg(windows)]
            assert!(
                OpenOptions::new().read(true).open(made).is_err(),
                "nobody else opens it before it is restricted"
            );
            Ok(())
        })
        .expect("opened");
        file.write_all(b"one\n").expect("written");
        drop(file);
        let mut again = open_private_log(&path).expect("opened again");
        again.write_all(b"two\n").expect("written");
        drop(again);
        assert_eq!(fs::read_to_string(&path).expect("read"), "one\ntwo\n");
    }

    #[test]
    fn a_new_log_refuses_a_file_already_there() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("session.log");
        fs::write(&path, "someone else's").expect("written");
        let refused = create_private_log(&path).expect_err("refused");
        assert_eq!(refused.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&path).expect("read"), "someone else's");
    }

    #[test]
    fn a_log_already_there_is_restricted_again_before_more_is_added() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("session.log");
        fs::write(&path, "earlier\n").expect("written");
        let restricted = std::cell::Cell::new(false);
        let file = open_private_log_with(&path, false, |found| {
            assert_eq!(found, path);
            restricted.set(true);
            Ok(())
        })
        .expect("opened");
        drop(file);
        assert!(restricted.get());
        let refused = open_private_log_with(&path, false, |_| Err(io::Error::other("refused")));
        assert!(
            refused.is_err(),
            "nothing added to a file left unrestricted"
        );
        assert_eq!(fs::read_to_string(&path).expect("read"), "earlier\n");
    }

    #[test]
    fn a_log_that_cannot_be_restricted_is_removed_and_nothing_is_written() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("session.log");
        for new in [true, false] {
            let failed = open_private_log_with(&path, new, |_| Err(io::Error::other("refused")));
            assert_eq!(failed.expect_err("refused").to_string(), "refused");
            assert!(names(dir.path()).is_empty(), "the file made is removed");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_log_is_readable_by_its_owner_alone() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("session.log");
        drop(create_private_log(&path).expect("opened"));
        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(windows)]
    #[test]
    fn nobody_else_opens_the_file_while_it_is_written() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("id_rsa");
        write_private_with(&path, b"key", |temporary| {
            let other = OpenOptions::new().read(true).open(temporary);
            assert!(other.is_err(), "no sharing while the handle is open");
            let renamed = fs::rename(temporary, temporary.with_extension("moved"));
            assert!(renamed.is_err(), "nor renamed away");
            Ok(())
        })
        .expect("written");
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
