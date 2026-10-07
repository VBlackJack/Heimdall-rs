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

//! One Heimdall per configuration folder, as the C# `SingleInstanceGuard`: two instances
//! would both read the profile file at start and both write it back, the second write
//! dropping what the first recorded.
//!
//! The folder is owned through a lock on a file in it, held while the process runs: the
//! system releases it when the process ends, a crash included, so no stale lock is ever
//! left behind. A later launch finding the folder owned asks the owner to come forward
//! through a request file beside the lock, which the owner looks for and removes, as the
//! C# signals its activation event.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

/// Name of the file locked by the instance owning the folder; never removed, as removing
/// it would let two processes lock two different files of one name.
pub const LOCK_FILE_NAME: &str = "instance.lock";

/// Name of the file a later launch leaves for the owning instance to come forward.
pub const ACTIVATION_FILE_NAME: &str = "instance.activate";

/// Environment variable that turns the guard off when set to `0`, as the C#
/// `HEIMDALL_SINGLE_INSTANCE`; any other value leaves it on, so a typo cannot remove it.
pub const DISABLE_VARIABLE: &str = "HEIMDALL_SINGLE_INSTANCE";

/// The value of [`DISABLE_VARIABLE`] that turns the guard off.
const DISABLED_VALUE: &str = "0";

/// What [`acquire`] found: three outcomes, as the C# `SingleInstanceOutcome`, since "another
/// instance owns it" and "it could not be told" call for opposite actions.
#[derive(Debug)]
pub enum Ownership {
    /// This process owns the folder while the guard is kept.
    Owner(InstanceGuard),
    /// Another live instance owns it: this one must not touch its files.
    AlreadyRunning,
    /// Whether another instance owns it could not be told, the folder being unwritable or
    /// its file system not locking: the application starts unguarded, a refusal to start
    /// being a worse failure than the race guarded against.
    Unavailable(io::Error),
}

/// The lock on a configuration folder: kept for the life of the process, released when it
/// is dropped or the process ends.
#[derive(Debug)]
pub struct InstanceGuard {
    /// The locked file, held open: the lock goes with it.
    _file: File,
    /// The folder owned.
    dir: PathBuf,
}

impl InstanceGuard {
    /// The folder owned.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// Whether `value`, the value of [`DISABLE_VARIABLE`], turns the guard off: only an explicit
/// `0` does.
#[must_use]
pub fn disabled_by(value: Option<&str>) -> bool {
    value == Some(DISABLED_VALUE)
}

/// Whether the environment turns the guard off, as [`disabled_by`] reads it.
#[must_use]
pub fn disabled_by_environment() -> bool {
    disabled_by(std::env::var(DISABLE_VARIABLE).ok().as_deref())
}

/// Takes the configuration folder `dir` for this process, creating it if needed, or tells
/// that another instance has it. A request to come forward left by an earlier launch is
/// cleared once owned: no instance was there to answer it.
#[must_use]
pub fn acquire(dir: &Path) -> Ownership {
    let file = match fs::create_dir_all(dir).and_then(|()| {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join(LOCK_FILE_NAME))
    }) {
        Ok(file) => file,
        Err(error) => return Ownership::Unavailable(error),
    };
    match file.try_lock() {
        Ok(()) => {
            // Left by a launch that found an owner since gone: nobody is to come forward.
            let _ = take_activation_request(dir);
            Ownership::Owner(InstanceGuard {
                _file: file,
                dir: dir.to_owned(),
            })
        }
        Err(TryLockError::WouldBlock) => Ownership::AlreadyRunning,
        Err(TryLockError::Error(error)) => Ownership::Unavailable(error),
    }
}

/// Asks the instance owning `dir` to come forward, as the C# sets its activation event.
///
/// # Errors
///
/// When the request file cannot be written.
pub fn request_activation(dir: &Path) -> io::Result<()> {
    File::create(dir.join(ACTIVATION_FILE_NAME)).map(drop)
}

/// Whether a later launch asked the instance owning `dir` to come forward since last
/// looked; the request is taken, so it is answered once.
#[must_use]
pub fn take_activation_request(dir: &Path) -> bool {
    fs::remove_file(dir.join(ACTIVATION_FILE_NAME)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{
        ACTIVATION_FILE_NAME, Ownership, acquire, disabled_by, request_activation,
        take_activation_request,
    };

    #[test]
    fn a_second_lock_on_the_same_folder_is_refused() {
        let dir = tempfile::tempdir().expect("dir");
        let first = acquire(dir.path());
        assert!(matches!(first, Ownership::Owner(_)), "{first:?}");
        let second = acquire(dir.path());
        assert!(matches!(second, Ownership::AlreadyRunning), "{second:?}");
    }

    #[test]
    fn two_folders_are_owned_apart() {
        let one = tempfile::tempdir().expect("dir");
        let other = tempfile::tempdir().expect("dir");
        let first = acquire(one.path());
        let second = acquire(other.path());
        assert!(matches!(first, Ownership::Owner(_)), "{first:?}");
        assert!(matches!(second, Ownership::Owner(_)), "{second:?}");
    }

    #[test]
    fn a_dropped_guard_frees_the_folder() {
        let dir = tempfile::tempdir().expect("dir");
        let first = acquire(dir.path());
        assert!(matches!(first, Ownership::Owner(_)), "{first:?}");
        drop(first);
        let again = acquire(dir.path());
        assert!(matches!(again, Ownership::Owner(_)), "{again:?}");
    }

    #[test]
    fn a_folder_that_cannot_be_made_lets_the_application_start() {
        let dir = tempfile::tempdir().expect("dir");
        // A folder under a file: no system can create it, whoever runs the test.
        let file = dir.path().join("plain");
        std::fs::write(&file, "x").expect("written");
        let found = acquire(&file.join("config"));
        assert!(matches!(found, Ownership::Unavailable(_)), "{found:?}");
    }

    #[test]
    fn the_guard_creates_the_folder_it_owns() {
        let dir = tempfile::tempdir().expect("dir");
        let config = dir.path().join("first").join("start");
        let found = acquire(&config);
        let Ownership::Owner(guard) = found else {
            panic!("{found:?}");
        };
        assert_eq!(guard.dir(), config);
    }

    #[test]
    fn a_request_to_come_forward_is_answered_once() {
        let dir = tempfile::tempdir().expect("dir");
        let owner = acquire(dir.path());
        assert!(matches!(owner, Ownership::Owner(_)), "{owner:?}");
        assert!(!take_activation_request(dir.path()));
        assert!(matches!(acquire(dir.path()), Ownership::AlreadyRunning));
        request_activation(dir.path()).expect("asked");
        assert!(take_activation_request(dir.path()));
        assert!(!take_activation_request(dir.path()));
    }

    #[test]
    fn a_request_left_with_no_owner_is_cleared_when_owned() {
        let dir = tempfile::tempdir().expect("dir");
        request_activation(dir.path()).expect("asked");
        let owner = acquire(dir.path());
        assert!(matches!(owner, Ownership::Owner(_)), "{owner:?}");
        assert!(!dir.path().join(ACTIVATION_FILE_NAME).exists());
    }

    #[test]
    fn only_an_explicit_zero_turns_the_guard_off() {
        assert!(disabled_by(Some("0")));
        assert!(!disabled_by(None));
        assert!(!disabled_by(Some("1")));
        assert!(!disabled_by(Some("off")));
        assert!(!disabled_by(Some("")));
    }
}
