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

//! Which file a local shell runs, decided here rather than by the operating system's search.
//!
//! On Windows the command line reaches `CreateProcessW` with no application name, and that
//! search begins in the directory of the running executable and in the current directory: a
//! bare `powershell` would prefer any `powershell.exe` left next to Heimdall. On Unix a program
//! name with a slash is found from the folder the shell starts in. So a name is looked up in
//! the absolute entries of `PATH` only, a relative path is refused, and the program handed on
//! is always a full path.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

/// Windows `PowerShell` under the system root: the shell opened when none is named, as the C#
/// Heimdall opens it.
#[cfg(windows)]
const WINDOWS_POWERSHELL: &str = r"System32\WindowsPowerShell\v1.0\powershell.exe";

/// The shell run on Unix when `$SHELL` names none.
#[cfg(unix)]
pub const FALLBACK_SHELL: &str = "/bin/sh";

/// Extension `CreateProcessW` adds to a program name that has none.
#[cfg(windows)]
const DEFAULT_EXTENSION: &str = "exe";

/// Why a program cannot be run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProgramError {
    /// No name, or one with spaces around it.
    #[error("the program name is empty or has spaces around it")]
    Blank,
    /// A quote or a NUL, which would end or reshape the command line.
    #[error("the program name holds a quote or a NUL character")]
    ForbiddenCharacter,
    /// A path relative to a folder that depends on how Heimdall was started.
    #[error("the program is a relative path; give a full path or a bare name")]
    RelativePath,
    /// The name is in no absolute folder of `PATH`.
    #[error("{0} is in no folder of PATH")]
    NotFound(String),
    /// A NUL in an argument or in the folder, which would cut the command line short.
    #[error("an argument or the folder holds a NUL character")]
    NulInArgument,
}

impl From<ProgramError> for io::Error {
    fn from(error: ProgramError) -> Self {
        let kind = match error {
            ProgramError::NotFound(_) => io::ErrorKind::NotFound,
            _ => io::ErrorKind::InvalidInput,
        };
        Self::new(kind, error)
    }
}

/// The full path of `program`, looked up in the absolute entries of `path_var` when it is a
/// bare name. `is_file` says whether a candidate exists.
///
/// # Errors
///
/// [`ProgramError`] when the name is blank, holds a quote or a NUL, is a relative path, or is
/// found in no folder.
pub fn resolve(
    program: &str,
    path_var: Option<&OsStr>,
    is_file: impl Fn(&Path) -> bool,
) -> Result<PathBuf, ProgramError> {
    if program.is_empty() || program.trim() != program {
        return Err(ProgramError::Blank);
    }
    if program.contains(['"', '\0']) {
        return Err(ProgramError::ForbiddenCharacter);
    }
    let path = Path::new(program);
    if path.is_absolute() {
        return Ok(path.to_owned());
    }
    if path.components().count() != 1 || program.contains(std::path::is_separator) {
        return Err(ProgramError::RelativePath);
    }
    let folders = path_var.map(std::env::split_paths).into_iter().flatten();
    for folder in folders.filter(|folder| folder.is_absolute()) {
        for candidate in candidates(&folder, program) {
            if is_file(&candidate) {
                return Ok(candidate);
            }
        }
    }
    Err(ProgramError::NotFound(program.to_owned()))
}

/// Files a bare name may be in `folder`: the name itself, and on Windows the name with `.exe`
/// when it has no extension, as `CreateProcessW` tries it.
fn candidates(folder: &Path, name: &str) -> Vec<PathBuf> {
    let exact = folder.join(name);
    #[cfg(windows)]
    if Path::new(name).extension().is_none() {
        return vec![exact.clone(), exact.with_extension(DEFAULT_EXTENSION)];
    }
    vec![exact]
}

/// Windows `PowerShell` under `system_root`.
#[cfg(windows)]
#[must_use]
pub fn default_shell(system_root: &Path) -> PathBuf {
    system_root.join(WINDOWS_POWERSHELL)
}

/// Refuses a NUL in any argument or in the folder: Windows ends the command line at the
/// first one, dropping whatever follows.
///
/// # Errors
///
/// [`ProgramError::NulInArgument`].
pub fn check_arguments(
    arguments: &super::LocalArguments,
    working_directory: Option<&Path>,
) -> Result<(), ProgramError> {
    let folder_has_nul =
        working_directory.is_some_and(|folder| folder.as_os_str().to_string_lossy().contains('\0'));
    let argument_has_nul = match arguments {
        super::LocalArguments::List(args) => args.iter().any(|arg| arg.contains('\0')),
        super::LocalArguments::WindowsLine(line) => line.contains('\0'),
    };
    if folder_has_nul || argument_has_nul {
        return Err(ProgramError::NulInArgument);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;
    #[cfg(windows)]
    use std::path::PathBuf;

    use super::{ProgramError, check_arguments, resolve};
    use crate::local::LocalArguments;

    #[cfg(unix)]
    const BIN: &str = "/usr/bin";
    #[cfg(windows)]
    const BIN: &str = r"C:\Windows\System32";

    fn path_var(folders: &[&str]) -> OsString {
        std::env::join_paths(folders).expect("joined")
    }

    #[test]
    fn a_bare_name_is_found_in_path() {
        let path = path_var(&[BIN]);
        let found = resolve("tool", Some(&path), |candidate| {
            candidate.starts_with(BIN) && candidate.file_stem() == Some("tool".as_ref())
        })
        .expect("found");
        assert!(found.is_absolute(), "{found:?}");
        assert!(found.starts_with(BIN));
    }

    #[test]
    fn relative_folders_of_path_are_never_searched() {
        // `.` and `bin` would be the current folder: the one a planted file waits in.
        let path = path_var(&[".", "bin"]);
        assert_eq!(
            resolve("tool", Some(&path), |_| true),
            Err(ProgramError::NotFound("tool".to_owned()))
        );
    }

    #[test]
    fn a_name_found_nowhere_is_refused_rather_than_left_to_the_system() {
        let path = path_var(&[BIN]);
        assert_eq!(
            resolve("tool", Some(&path), |_| false),
            Err(ProgramError::NotFound("tool".to_owned()))
        );
        assert_eq!(
            resolve("tool", None, |_| true),
            Err(ProgramError::NotFound("tool".to_owned()))
        );
    }

    #[test]
    fn a_full_path_is_kept_as_given() {
        let full = Path::new(BIN).join("My Tools").join("x");
        let text = full.to_str().expect("utf-8");
        assert_eq!(resolve(text, None, |_| false), Ok(full.clone()));
    }

    #[test]
    fn a_relative_path_is_refused() {
        for relative in ["./tool", "bin/tool", "../tool"] {
            assert_eq!(
                resolve(relative, None, |_| true),
                Err(ProgramError::RelativePath),
                "{relative}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_relative_forms_are_refused_too() {
        // Relative to the current folder of a drive, or to the root of the current drive.
        for relative in [r"bin\tool", r"C:tool.exe", r"\tool.exe"] {
            assert_eq!(
                resolve(relative, None, |_| true),
                Err(ProgramError::RelativePath),
                "{relative}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_name_without_extension_is_found_as_an_exe() {
        let path = path_var(&[BIN]);
        let found = resolve("pwsh", Some(&path), |candidate| {
            candidate.extension() == Some("exe".as_ref())
        })
        .expect("found");
        assert_eq!(found, PathBuf::from(BIN).join("pwsh.exe"));
    }

    #[cfg(windows)]
    #[test]
    fn the_default_is_windows_powershell_by_its_full_path() {
        assert_eq!(
            super::default_shell(Path::new(r"C:\Windows")),
            PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
        );
    }

    #[test]
    fn blank_names_quotes_and_nul_are_refused() {
        assert_eq!(resolve("", None, |_| true), Err(ProgramError::Blank));
        assert_eq!(resolve(" sh", None, |_| true), Err(ProgramError::Blank));
        assert_eq!(resolve("sh ", None, |_| true), Err(ProgramError::Blank));
        assert_eq!(
            resolve("x\"y", None, |_| true),
            Err(ProgramError::ForbiddenCharacter)
        );
        assert_eq!(
            resolve("x\0y", None, |_| true),
            Err(ProgramError::ForbiddenCharacter)
        );
    }

    #[test]
    fn a_nul_in_an_argument_or_the_folder_is_refused() {
        let list = |args: &[&str]| {
            LocalArguments::List(args.iter().map(|arg| (*arg).to_owned()).collect())
        };
        assert_eq!(
            check_arguments(&list(&["/c", "x\0"]), None),
            Err(ProgramError::NulInArgument)
        );
        assert_eq!(
            check_arguments(&LocalArguments::WindowsLine("/c x\0 /p".to_owned()), None),
            Err(ProgramError::NulInArgument)
        );
        assert_eq!(
            check_arguments(&list(&[]), Some(Path::new("a\0b"))),
            Err(ProgramError::NulInArgument)
        );
        assert_eq!(
            check_arguments(&list(&["-l"]), Some(Path::new(BIN))),
            Ok(())
        );
    }
}
