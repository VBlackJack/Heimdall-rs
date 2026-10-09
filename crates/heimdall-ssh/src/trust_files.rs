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

//! How the files of trusted host keys are written: `known_hosts` and its pins, one writer
//! at a time, and a file rewritten whole only by a file beside it moved over it.
//!
//! The C# `HostKeyStore` serialises its writes behind one lock and replaces its file
//! through a temporary one (`KnownHostsExporter.WriteAtomic` for the OpenSSH file). Here
//! a connection records a pinned key from its own task while the window learns, forgets or
//! imports keys: without one lock, a key appended between another writer's read and its
//! rewrite would be lost, and two rewrites of the pins would keep only one of them. One
//! Heimdall-rs runs per configuration folder (`heimdall_core::instance`), so a lock of the
//! process is the lock of the files.

use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

/// Held by whoever reads then writes `known_hosts` or its pins.
static TRUST_FILES: Mutex<()> = Mutex::new(());

/// The right to write the trust files, until dropped. Taken for one read and its write,
/// never across an `await` or a question to the user.
pub(crate) struct TrustLock {
    _held: MutexGuard<'static, ()>,
}

/// Takes the lock of the trust files, waiting for the writer holding it.
///
/// A writer that panicked left the files whole, each rewrite being atomic: the lock is
/// taken all the same.
pub(crate) fn lock() -> TrustLock {
    TrustLock {
        _held: TRUST_FILES.lock().unwrap_or_else(PoisonError::into_inner),
    }
}

/// Name a file beside its target takes, before it is moved over it, when the target has
/// none of its own.
const FALLBACK_NAME: &str = "known_hosts";

/// Replaces `path` with `text` whole: written beside it first, flushed to disk, given its
/// permissions, then moved over it. A link is followed to the file it names. A read-only
/// file is left as it is. On any failure the file is left as it was and the one beside it
/// removed.
///
/// # Errors
///
/// The folder cannot be created, the file is read-only, or the system refused a step.
pub(crate) fn replace(path: &Path, text: &str) -> io::Result<()> {
    replace_with(path, text, File::write_all)
}

/// [`replace`], the text written by `write`: a failing writer stands for a full disk.
fn replace_with(
    path: &Path,
    text: &str,
    write: impl FnOnce(&mut File, &[u8]) -> io::Result<()>,
) -> io::Result<()> {
    let path = &followed(path);
    let existing = match fs::metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if existing
        .as_ref()
        .is_some_and(|metadata| metadata.permissions().readonly())
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let beside = beside(dir, path);
    let written = File::create_new(&beside)
        .and_then(|mut file| {
            write(&mut file, text.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| match &existing {
            Some(metadata) => fs::set_permissions(&beside, metadata.permissions()),
            None => Ok(()),
        })
        .and_then(|()| fs::rename(&beside, path));
    if let Err(error) = written {
        let _ = fs::remove_file(&beside);
        return Err(error);
    }
    Ok(())
}

/// `path`, or the file it names when it is a link.
pub(crate) fn followed(path: &Path) -> PathBuf {
    let link = fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink());
    if link {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
    } else {
        path.to_owned()
    }
}

/// A name for the file written beside `path` before it replaces it, never used before.
fn beside(dir: &Path, path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map_or_else(|| FALLBACK_NAME.into(), |name| name.to_string_lossy());
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    dir.join(format!(".{name}.{}.{stamp}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a full disk does: part of the text written, then an error.
    fn half_then_full(file: &mut File, bytes: &[u8]) -> io::Result<()> {
        file.write_all(&bytes[..bytes.len() / 2])?;
        Err(io::Error::new(io::ErrorKind::StorageFull, "disk full"))
    }

    #[test]
    fn a_rewrite_that_fails_halfway_leaves_the_old_file_whole_and_nothing_beside_it() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("known_hosts");
        fs::write(&path, "web.lab ssh-ed25519 AAAA\n").expect("seeded");
        let failed = replace_with(&path, "db.lab ssh-ed25519 BBBB\n", half_then_full);
        assert_eq!(
            failed.expect_err("refused").kind(),
            io::ErrorKind::StorageFull
        );
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "web.lab ssh-ed25519 AAAA\n",
            "the old file whole"
        );
        let names: Vec<_> = fs::read_dir(dir.path())
            .expect("listed")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(names, ["known_hosts"], "the file beside it removed");

        replace(&path, "db.lab ssh-ed25519 BBBB\n").expect("replaced");
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "db.lab ssh-ed25519 BBBB\n"
        );
    }

    #[test]
    fn a_missing_file_and_its_folder_are_created() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("config").join("known_hosts.pins");
        replace(&path, "web.lab SHA256:x\n").expect("written");
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "web.lab SHA256:x\n"
        );
    }

    #[test]
    fn the_lock_is_taken_again_after_a_writer_panicked_holding_it() {
        let panicked = std::thread::spawn(|| {
            let _held = lock();
            panic!("a writer failed");
        })
        .join();
        assert!(panicked.is_err());
        drop(lock());
    }
}
