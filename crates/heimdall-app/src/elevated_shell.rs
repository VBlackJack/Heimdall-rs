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

//! A local shell run as administrator, as the C# "External window" elevation: Windows starts
//! it in a window of its own, through its elevation prompt, never in a tab. The tab only says
//! where it runs: Windows hands back no process to follow.
//!
//! Nothing runs unelevated in its place: a prompt declined, or a start refused, is said, and
//! that is all.

use std::io;
use std::path::{Path, PathBuf};

use heimdall_term::local::{self, program};

use crate::local_driver::LocalShell;

/// Whether a shell can be run as administrator here: Windows only.
pub const SUPPORTED: bool = cfg!(windows);

/// The verb Windows starts a program as administrator with.
pub const RUNAS_VERB: &str = "runas";

/// `ERROR_CANCELLED`: the user declined the elevation prompt.
pub const ERROR_CANCELLED: i32 = 1223;

/// What Windows is asked to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElevatedLaunch {
    /// The program, a full path.
    pub program: PathBuf,
    /// The verb it is started with: [`RUNAS_VERB`].
    pub verb: &'static str,
    /// Its arguments, as one Windows argument string.
    pub parameters: String,
    /// The folder it starts in.
    pub working_directory: PathBuf,
    /// Where it starts when that folder is not there any more: the home folder, as the C#.
    pub fallback_directory: PathBuf,
}

/// How a start went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElevatedOutcome {
    /// Started in a window of its own.
    Started,
    /// The user declined the elevation prompt.
    Cancelled,
    /// Windows could not start it: the system's message.
    Failed(String),
    /// Not on Windows: there is no elevation prompt to go through.
    Unsupported,
}

/// What Windows is asked to start for `shell`: its program found as a local shell's is, a
/// full path, never in the current folder; its arguments as one line, as the C# hands them
/// over; the folder it names, else `home`.
///
/// # Errors
///
/// The program is refused or found nowhere, or an argument or the folder holds a NUL.
pub fn launch_request(shell: &LocalShell, home: &Path) -> io::Result<ElevatedLaunch> {
    program::check_arguments(&shell.arguments, shell.working_directory.as_deref())?;
    let program = local::program_path(shell.program.as_deref())?;
    if !program.is_absolute() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    Ok(ElevatedLaunch {
        program,
        verb: RUNAS_VERB,
        parameters: local::windows_arguments(&shell.arguments),
        working_directory: shell
            .working_directory
            .clone()
            .unwrap_or_else(|| home.to_owned()),
        fallback_directory: home.to_owned(),
    })
}

/// Starts `request` in a window of its own, through the elevation prompt. It waits while the
/// prompt is shown: call it off the UI thread.
#[must_use]
pub fn launch(request: &ElevatedLaunch) -> ElevatedOutcome {
    #[cfg(windows)]
    return launch_with(request, windows::shell_execute);
    #[cfg(not(windows))]
    return {
        log::warn!(
            "elevated shell {} not started: Windows only",
            request.program.display()
        );
        ElevatedOutcome::Unsupported
    };
}

/// Starts `request` with `start`, which asks Windows: its folder first checked, the home
/// folder taken when it is not there. The program is named in the log, never its arguments:
/// they may carry anything.
#[must_use]
pub fn launch_with(
    request: &ElevatedLaunch,
    start: impl FnOnce(&ElevatedLaunch) -> io::Result<()>,
) -> ElevatedOutcome {
    let mut started = request.clone();
    if !started.working_directory.is_dir() {
        started
            .working_directory
            .clone_from(&request.fallback_directory);
    }
    log::info!(
        "elevated shell {} starting in its own window, in {}",
        started.program.display(),
        started.working_directory.display()
    );
    match start(&started) {
        Ok(()) => ElevatedOutcome::Started,
        Err(error) if error.raw_os_error() == Some(ERROR_CANCELLED) => {
            log::info!(
                "elevated shell {}: the elevation prompt was declined",
                started.program.display()
            );
            ElevatedOutcome::Cancelled
        }
        Err(error) => {
            log::warn!(
                "elevated shell {} failed to start: {error}",
                started.program.display()
            );
            ElevatedOutcome::Failed(error.to_string())
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::io;
    use std::path::Path;

    use winsafe::{self as w, co};

    use super::ElevatedLaunch;

    /// Asks Windows to start `request` with its verb, on this thread, which waits while the
    /// prompt is shown. COM is set up first, as the shell may hand the verb to an extension.
    pub(super) fn shell_execute(request: &ElevatedLaunch) -> io::Result<()> {
        let _com = w::CoInitializeEx(co::COINIT::APARTMENTTHREADED | co::COINIT::DISABLE_OLE1DDE)
            .map_err(|error| io::Error::other(error.to_string()))?;
        let file = text(&request.program)?;
        let directory = text(&request.working_directory)?;
        w::ShellExecuteEx(&w::SHELLEXECUTEINFO {
            // Done once it returns; a failure is said in the tab, not in a box of Windows.
            mask: co::SEE_MASK::NOASYNC | co::SEE_MASK::FLAG_NO_UI,
            verb: Some(request.verb),
            file,
            parameters: Some(request.parameters.as_str()).filter(|line| !line.is_empty()),
            directory: Some(directory),
            show: co::SW::SHOWNORMAL,
            ..Default::default()
        })
        .map_err(|error| {
            i32::try_from(error.raw()).map_or_else(
                |_| io::Error::other(error.to_string()),
                io::Error::from_raw_os_error,
            )
        })
    }

    /// `path` as the text Windows is given; one that is not valid Unicode is refused rather
    /// than changed.
    fn text(path: &Path) -> io::Result<&str> {
        path.to_str()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidFilename))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io;
    use std::path::PathBuf;

    use heimdall_term::local::LocalArguments;

    use super::{
        ERROR_CANCELLED, ElevatedLaunch, ElevatedOutcome, RUNAS_VERB, launch_request, launch_with,
    };
    use crate::local_driver::LocalShell;

    fn shell(program: Option<String>, arguments: LocalArguments) -> LocalShell {
        LocalShell {
            name: "Admin".to_owned(),
            program,
            arguments,
            working_directory: None,
            environment: Vec::new(),
        }
    }

    /// A full path on this platform, never looked for.
    fn full(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn the_request_names_the_full_program_the_runas_verb_the_arguments_and_the_folder() {
        let program = full("pwsh.exe");
        let home = full("home");
        let mut asked = shell(
            Some(program.to_string_lossy().into_owned()),
            LocalArguments::WindowsLine("-NoExit -Command \"Get-Date\"".to_owned()),
        );
        asked.working_directory = Some(full("work"));
        let request = launch_request(&asked, &home).expect("built");
        assert_eq!(
            request,
            ElevatedLaunch {
                program,
                verb: RUNAS_VERB,
                parameters: "-NoExit -Command \"Get-Date\"".to_owned(),
                working_directory: full("work"),
                fallback_directory: home.clone(),
            }
        );
        assert_eq!(request.verb, "runas");

        // Listed arguments quoted as the C runtime reads them back; no folder is the home.
        let listed = shell(
            Some(full("tool.exe").to_string_lossy().into_owned()),
            LocalArguments::List(vec!["/k".to_owned(), "two words".to_owned()]),
        );
        let request = launch_request(&listed, &home).expect("built");
        assert_eq!(request.parameters, "/k \"two words\"");
        assert_eq!(request.working_directory, home);
    }

    #[test]
    fn a_program_that_is_not_a_full_path_or_found_is_never_asked_for() {
        let home = full("home");
        for program in [r"tools\x.exe", "./x", "no-such-shell-anywhere", "a\"b"] {
            let asked = shell(Some(program.to_owned()), LocalArguments::default());
            assert!(launch_request(&asked, &home).is_err(), "{program}");
        }
        let nul = shell(
            Some(full("tool.exe").to_string_lossy().into_owned()),
            LocalArguments::WindowsLine("/c x\0 y".to_owned()),
        );
        assert!(launch_request(&nul, &home).is_err(), "a NUL cuts the line");
    }

    fn request(folder: PathBuf, fallback: PathBuf) -> ElevatedLaunch {
        ElevatedLaunch {
            program: full("pwsh.exe"),
            verb: RUNAS_VERB,
            parameters: "-NoLogo".to_owned(),
            working_directory: folder,
            fallback_directory: fallback,
        }
    }

    #[test]
    fn a_start_asks_windows_once_with_the_request_and_says_it_started() {
        let folder = tempfile::tempdir().expect("folder");
        let asked = RefCell::new(Vec::new());
        let wanted = request(folder.path().to_owned(), full("home"));
        let outcome = launch_with(&wanted, |launch| {
            asked.borrow_mut().push(launch.clone());
            Ok(())
        });
        assert_eq!(outcome, ElevatedOutcome::Started);
        assert_eq!(asked.into_inner(), [wanted]);
    }

    #[test]
    fn a_folder_gone_starts_in_the_home_folder() {
        let home = tempfile::tempdir().expect("home");
        let gone = home.path().join("gone");
        let mut started = None;
        let outcome = launch_with(&request(gone, home.path().to_owned()), |launch| {
            started = Some(launch.working_directory.clone());
            Ok(())
        });
        assert_eq!(outcome, ElevatedOutcome::Started);
        assert_eq!(started.as_deref(), Some(home.path()));
    }

    #[test]
    fn the_prompt_declined_is_said_cancelled_and_any_other_refusal_failed() {
        let home = full("home");
        let cancelled = launch_with(&request(home.clone(), home.clone()), |_| {
            Err(io::Error::from_raw_os_error(ERROR_CANCELLED))
        });
        assert_eq!(cancelled, ElevatedOutcome::Cancelled);
        let failed = launch_with(&request(home.clone(), home), |_| {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        });
        let ElevatedOutcome::Failed(detail) = failed else {
            panic!("{failed:?}");
        };
        assert!(!detail.is_empty());
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_than_windows_nothing_is_started() {
        const { assert!(!super::SUPPORTED) };
        let home = full("home");
        assert_eq!(
            super::launch(&request(home.clone(), home)),
            ElevatedOutcome::Unsupported
        );
    }
}
