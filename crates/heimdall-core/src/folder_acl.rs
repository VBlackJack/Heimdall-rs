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

//! The folders Heimdall-rs writes in, restricted on Windows to the current user, the
//! Administrators and SYSTEM, as the C# `AclEnforcer`.
//!
//! A folder's access list is replaced by a protected one: what it inherited from its parent
//! is removed, and the three get full control, inherited by what the folder holds. Files
//! made in it afterwards take that list, so no file is written with the wider one.
//!
//! `icacls.exe` does the change, run from the system folder with its arguments one by one,
//! never through a shell. Its output is localised and never read: its exit code decides.
//! Elsewhere nothing is done, the files being made private by their own mode.

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::paths;

/// SID of the local Administrators group, `BUILTIN\Administrators`.
pub const ADMINISTRATORS_SID: &str = "S-1-5-32-544";

/// SID of the local system account, `NT AUTHORITY\SYSTEM`.
pub const SYSTEM_SID: &str = "S-1-5-18";

/// Removes the inherited entries and protects the list from its parent's.
const REMOVE_INHERITED: &str = "/inheritance:r";

/// Grants the entries that follow, replacing what each account had.
const GRANT_REPLACING: &str = "/grant:r";

/// Says nothing of what succeeded.
const QUIET: &str = "/q";

/// What marks an account given by its SID rather than its name.
const SID_MARK: &str = "*";

/// Full control, inherited by the files and the folders inside.
const FULL_CONTROL_INHERITED: &str = ":(OI)(CI)F";

/// What every SID string starts with: `S`, then revision 1.
const SID_START: &str = "S-1-";

/// Separator of a SID string's parts.
const SID_SEPARATOR: char = '-';

/// A SID string's parts at the least: `S`, the revision and the authority.
const SID_MIN_PARTS: usize = 3;

/// Why a folder was not restricted.
#[derive(Debug, thiserror::Error)]
pub enum RestrictError {
    /// The folder could not be made.
    #[error("the folder could not be made: {0}")]
    NotMade(#[source] io::Error),
    /// The path is not absolute: `icacls` could read it as one of its options.
    #[error("the path is not absolute")]
    NotAbsolute,
    /// The current user's SID could not be read.
    #[error("the current user could not be read: {0}")]
    NoUser(String),
    /// The system folder could not be read.
    #[error("the system folder could not be read: {0}")]
    NoSystemFolder(String),
    /// `icacls` did not start.
    #[error("icacls did not start: {0}")]
    NotStarted(#[source] io::Error),
    /// `icacls` ended with an error.
    #[error("icacls failed with exit code {}", exit_code(*.0))]
    Refused(Option<i32>),
}

/// An exit code to say, or `none`.
fn exit_code(code: Option<i32>) -> String {
    code.map_or_else(|| "none".to_owned(), |code| code.to_string())
}

/// A folder left with the access it had, and why.
#[derive(Debug)]
pub struct Unrestricted {
    /// The folder.
    pub folder: PathBuf,
    /// Why it was not restricted.
    pub error: RestrictError,
}

impl fmt::Display for Unrestricted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the folder {} is not restricted to the user, Administrators and SYSTEM: {}",
            self.folder.display(),
            self.error
        )
    }
}

/// Whether `text` reads as a SID string: `S-1-`, then decimal parts separated by `-`.
///
/// What is put in `icacls`' grant is checked first: a colon or a bracket would change it.
#[must_use]
pub fn is_sid(text: &str) -> bool {
    let parts: Vec<&str> = text.split(SID_SEPARATOR).collect();
    text.starts_with(SID_START)
        && parts.len() >= SID_MIN_PARTS
        && parts[1..]
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// The arguments `icacls` is given to restrict `folder`, `user_sid` being the current
/// user's: inheritance removed, full control to the user, the Administrators and SYSTEM,
/// inherited inside, nothing said of success.
#[must_use]
pub fn icacls_arguments(folder: &Path, user_sid: &str) -> Vec<OsString> {
    let grant = |sid: &str| OsString::from(format!("{SID_MARK}{sid}{FULL_CONTROL_INHERITED}"));
    vec![
        folder.as_os_str().to_owned(),
        REMOVE_INHERITED.into(),
        GRANT_REPLACING.into(),
        grant(user_sid),
        grant(ADMINISTRATORS_SID),
        grant(SYSTEM_SID),
        QUIET.into(),
    ]
}

/// Restricts `folder` to the current user, the Administrators and SYSTEM, on Windows;
/// does nothing elsewhere. What is already in it takes the new list, unless its own was
/// protected.
///
/// Run again on a folder already restricted, it changes nothing: it is not skipped, as
/// the C# runs it at each start, and it is quick.
///
/// # Errors
///
/// [`RestrictError`] when the folder keeps the access it had.
pub fn restrict(folder: &Path) -> Result<(), RestrictError> {
    #[cfg(windows)]
    {
        windows::restrict(folder)
    }
    #[cfg(not(windows))]
    {
        let _ = folder;
        Ok(())
    }
}

/// Makes each of `folders` and restricts it, on Windows: the ones left with the access
/// they had, and why. One failing does not stop the others. Elsewhere nothing is made.
#[must_use]
pub fn restrict_all(folders: &[PathBuf]) -> Vec<Unrestricted> {
    if !cfg!(windows) {
        return Vec::new();
    }
    folders
        .iter()
        .filter_map(|folder| {
            std::fs::create_dir_all(folder)
                .map_err(RestrictError::NotMade)
                .and_then(|()| restrict(folder))
                .err()
                .map(|error| Unrestricted {
                    folder: folder.clone(),
                    error,
                })
        })
        .collect()
}

/// The folders of Heimdall-rs made and restricted, on Windows: the configuration folder
/// and the local data folder, with the logs and the files being edited inside it. Run at
/// start, before anything is written in them; the ones left as they were, and why.
#[must_use]
pub fn restrict_app_folders() -> Vec<Unrestricted> {
    let folders: Vec<PathBuf> = [paths::config_dir(), paths::data_dir()]
        .into_iter()
        .flatten()
        .collect();
    restrict_all(&folders)
}

#[cfg(windows)]
pub use windows::{current_user_sid, icacls_program};

#[cfg(windows)]
mod windows {
    use std::os::windows::process::CommandExt as _;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    use winsafe::{self as w, co};

    use super::{RestrictError, icacls_arguments, is_sid};

    /// Windows' program changing access lists, in the system folder.
    const ICACLS_PROGRAM: &str = "icacls.exe";

    /// `CREATE_NO_WINDOW`: no console flashes up while `icacls` runs.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// The current user's SID, as a string: the user of the process's token.
    ///
    /// # Errors
    ///
    /// [`RestrictError::NoUser`] when the token cannot be read, or holds no SID.
    pub fn current_user_sid() -> Result<String, RestrictError> {
        let token = w::HACCESSTOKEN::GetCurrentProcessToken();
        let info = token
            .GetTokenInformation(co::TOKEN_INFORMATION_CLASS::User)
            .map_err(|error| RestrictError::NoUser(error.to_string()))?;
        let w::TokenInfo::User(user) = info else {
            return Err(RestrictError::NoUser("not the token's user".to_owned()));
        };
        let sid = user
            .User
            .Sid()
            .ok_or_else(|| RestrictError::NoUser("no SID in the token".to_owned()))?;
        let text = w::ConvertSidToStringSid(sid)
            .map_err(|error| RestrictError::NoUser(error.to_string()))?;
        if is_sid(&text) {
            Ok(text)
        } else {
            Err(RestrictError::NoUser("not a SID".to_owned()))
        }
    }

    /// `icacls.exe` in the system folder, as Windows says where it is: never one of the
    /// same name found first somewhere else.
    ///
    /// # Errors
    ///
    /// [`RestrictError::NoSystemFolder`] when Windows does not say, or says a relative path.
    pub fn icacls_program() -> Result<PathBuf, RestrictError> {
        let folder = PathBuf::from(
            w::GetSystemDirectory()
                .map_err(|error| RestrictError::NoSystemFolder(error.to_string()))?,
        );
        if folder.is_absolute() {
            Ok(folder.join(ICACLS_PROGRAM))
        } else {
            Err(RestrictError::NoSystemFolder("not absolute".to_owned()))
        }
    }

    pub(super) fn restrict(folder: &Path) -> Result<(), RestrictError> {
        if !folder.is_absolute() {
            return Err(RestrictError::NotAbsolute);
        }
        let user = current_user_sid()?;
        let status = Command::new(icacls_program()?)
            .args(icacls_arguments(folder, &user))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(RestrictError::NotStarted)?;
        if status.success() {
            Ok(())
        } else {
            Err(RestrictError::Refused(status.code()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use super::{
        ADMINISTRATORS_SID, RestrictError, SYSTEM_SID, Unrestricted, icacls_arguments, is_sid,
    };

    const USER: &str = "S-1-5-21-1111111111-2222222222-3333333333-1001";

    #[test]
    fn icacls_is_given_each_argument_alone_the_three_sids_and_no_shell() {
        let folder = Path::new(r"C:\Users\Some One\AppData\Roaming\Heimdall-rs\config");
        let expected: Vec<OsString> = [
            r"C:\Users\Some One\AppData\Roaming\Heimdall-rs\config",
            "/inheritance:r",
            "/grant:r",
            "*S-1-5-21-1111111111-2222222222-3333333333-1001:(OI)(CI)F",
            "*S-1-5-32-544:(OI)(CI)F",
            "*S-1-5-18:(OI)(CI)F",
            "/q",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        assert_eq!(icacls_arguments(folder, USER), expected);
    }

    #[test]
    fn the_well_known_sids_are_the_administrators_and_system() {
        assert_eq!(ADMINISTRATORS_SID, "S-1-5-32-544");
        assert_eq!(SYSTEM_SID, "S-1-5-18");
        assert!(is_sid(ADMINISTRATORS_SID) && is_sid(SYSTEM_SID));
    }

    #[test]
    fn only_a_sid_string_reads_as_a_sid() {
        assert!(is_sid(USER));
        assert!(is_sid("S-1-5"));
        for text in [
            "",
            "S-1",
            "S-1-",
            "S-2-5-18",
            "s-1-5-18",
            "S-1-5-",
            "S-1-5--18",
            "S-1-5-18:(OI)",
            "S-1-5-18 ",
            "Administrators",
            "*S-1-5-18",
        ] {
            assert!(!is_sid(text), "{text:?}");
        }
    }

    #[test]
    fn a_folder_left_as_it_was_is_said_with_its_reason() {
        let said = Unrestricted {
            folder: PathBuf::from("config"),
            error: RestrictError::Refused(Some(2)),
        }
        .to_string();
        assert!(said.starts_with("the folder config is not restricted"));
        assert!(said.ends_with("icacls failed with exit code 2"), "{said}");
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_nothing_is_made_or_changed() {
        let dir = tempfile::tempdir().expect("dir");
        let folder = dir.path().join("config");
        assert!(super::restrict_all(std::slice::from_ref(&folder)).is_empty());
        assert!(!folder.exists());
        assert!(super::restrict(dir.path()).is_ok());
    }
}
