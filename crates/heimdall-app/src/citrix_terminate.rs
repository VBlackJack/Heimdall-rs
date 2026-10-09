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

//! Terminating a Citrix tab's session, as the C# `EmbeddedCitrixView` "Terminate": asked
//! first, and only for a client this tab found itself, never the launcher.
//!
//! The C# kills the client it started, and asks a shared one to close this session's
//! window. Without window messages, the client is asked to close by Windows' own
//! `taskkill.exe`, from the system folder, without `/F`: it closes as from its own close
//! button. When it is still running after [`TERMINATE_GRACE`], the user may force it, with
//! `/F`. Both name the client's image as a filter, so `taskkill.exe` checks the process is
//! still the client: a process id reused by another program is never touched. Only the
//! exit code is read, the output being in the language of Windows.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::citrix_session::{CLIENT_IMAGE, LIST_TIME_LIMIT, Waitable, wait_within};

/// How long the client is given to close once asked, before forcing it is offered.
pub const TERMINATE_GRACE: Duration = Duration::from_secs(10);

/// How long `taskkill.exe` may take, as a listing of the client's processes: past it, it is
/// killed and the request given up.
pub const TERMINATE_TIME_LIMIT: Duration = LIST_TIME_LIMIT;

/// Windows' process killer, in the system folder.
const TASKKILL_PROGRAM: &str = "taskkill.exe";
/// `taskkill.exe`'s switch naming the process id.
const TASKKILL_PID: &str = "/PID";
/// `taskkill.exe`'s filter switch.
const TASKKILL_FILTER: &str = "/FI";
/// The filter's condition, before the client's image name.
const TASKKILL_IMAGE_FILTER: &str = "IMAGENAME eq ";
/// `taskkill.exe`'s switch ending the process at once, not asking it.
const TASKKILL_FORCE: &str = "/F";
/// `taskkill.exe`'s exit code when no such process runs.
const TASKKILL_NOT_FOUND: i32 = 128;

/// What a request to end the client came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminateResult {
    /// `taskkill.exe` accepted it: the client was asked to close, or ended when forced.
    Requested,
    /// No such client runs any more.
    Gone,
    /// `taskkill.exe` refused it, with this exit code.
    Refused(i32),
    /// `taskkill.exe` did not answer within [`TERMINATE_TIME_LIMIT`], and was killed.
    TimedOut,
    /// `taskkill.exe` could not run, and why, for the log.
    NotRun(String),
}

impl TerminateResult {
    /// What `taskkill.exe`'s exit code `code` says.
    #[must_use]
    pub fn of_exit(code: i32) -> Self {
        match code {
            0 => Self::Requested,
            TASKKILL_NOT_FOUND => Self::Gone,
            code => Self::Refused(code),
        }
    }

    /// Whether the request reached the client, or found it gone.
    #[must_use]
    pub fn accepted(&self) -> bool {
        matches!(self, Self::Requested | Self::Gone)
    }
}

/// The command ending client process `pid`: `taskkill.exe` in `system_dir`, each argument
/// its own, no shell reading them; the client's image as a filter, and `/F` when `force`.
#[must_use]
pub fn taskkill_command(system_dir: &Path, pid: u32, force: bool) -> Command {
    let mut command = Command::new(system_dir.join(TASKKILL_PROGRAM));
    command
        .arg(TASKKILL_PID)
        .arg(pid.to_string())
        .arg(TASKKILL_FILTER)
        .arg(format!("{TASKKILL_IMAGE_FILTER}{CLIENT_IMAGE}"));
    if force {
        command.arg(TASKKILL_FORCE);
    }
    command
}

/// What ends the client: `taskkill.exe`, or another in tests.
pub trait ClientTerminator: Send + Sync {
    /// Asks client process `pid` to close, or ends it when `force`.
    fn terminate(&self, pid: u32, force: bool) -> TerminateResult;
}

/// The client ended by Windows' `taskkill.exe`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Taskkill;

impl ClientTerminator for Taskkill {
    fn terminate(&self, pid: u32, force: bool) -> TerminateResult {
        run_taskkill(pid, force)
    }
}

/// What `process`, a `taskkill.exe` started, came to within `limit`: past it, killed.
pub fn await_taskkill(process: &mut impl Waitable, limit: Duration) -> TerminateResult {
    match wait_within(process, limit) {
        Ok(Some(status)) => status.code().map_or_else(
            || TerminateResult::NotRun("ended without an exit code".to_owned()),
            TerminateResult::of_exit,
        ),
        Ok(None) => TerminateResult::TimedOut,
        Err(error) => TerminateResult::NotRun(error.to_string()),
    }
}

/// Ends client process `pid` by `taskkill.exe` from the system folder Windows names, with
/// no window and nothing read, within [`TERMINATE_TIME_LIMIT`].
#[cfg(windows)]
fn run_taskkill(pid: u32, force: bool) -> TerminateResult {
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    let Some(folder) = heimdall_core::paths::system_dir() else {
        return TerminateResult::NotRun("the system folder is unknown".to_owned());
    };
    let spawned = taskkill_command(&folder, pid, force)
        .creation_flags(crate::citrix::CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match spawned {
        Ok(mut child) => await_taskkill(&mut child, TERMINATE_TIME_LIMIT),
        Err(error) => TerminateResult::NotRun(error.to_string()),
    }
}

/// Citrix Workspace's client is a Windows program.
#[cfg(not(windows))]
fn run_taskkill(_pid: u32, _force: bool) -> TerminateResult {
    TerminateResult::NotRun("not on Windows".to_owned())
}

/// A request to end the client, once asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Termination {
    /// The client process it was asked of.
    pid: u32,
    /// Whether it forced it.
    force: bool,
    /// When it was asked.
    at: Instant,
    /// What it came to, once `taskkill.exe` answered.
    result: Option<TerminateResult>,
}

impl Termination {
    /// Client process `pid` asked to close, or forced when `force`, at `at`.
    #[must_use]
    pub fn started(pid: u32, force: bool, at: Instant) -> Self {
        Self {
            pid,
            force,
            at,
            result: None,
        }
    }

    /// The client process it was asked of.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Whether it forced it.
    #[must_use]
    pub fn force(&self) -> bool {
        self.force
    }

    /// `taskkill.exe` answered `result`, at `at`: the grace period counts from the answer.
    pub fn answered(&mut self, result: TerminateResult, at: Instant) {
        self.result = Some(result);
        self.at = at;
    }

    /// What is offered for it at `now`, its client still running.
    #[must_use]
    pub fn offer(&self, now: Instant) -> TerminateOffer {
        match &self.result {
            None => TerminateOffer::Pending { force: self.force },
            Some(result) if result.accepted() && now < self.at + TERMINATE_GRACE => {
                TerminateOffer::Asked { force: self.force }
            }
            Some(result) => TerminateOffer::Force(result.clone()),
        }
    }
}

/// What a Citrix tab offers to end its session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminateOffer {
    /// Nothing: no client this tab found itself runs.
    Nothing,
    /// Terminate, asked first.
    Terminate,
    /// A request is under way, forced or not.
    Pending {
        /// Whether it forces the client.
        force: bool,
    },
    /// The client was asked and is given time to close.
    Asked {
        /// Whether it was forced.
        force: bool,
    },
    /// Force terminate, asked first: the last request, which came to this, did not end the
    /// client.
    Force(TerminateResult),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_is_taskkill_from_the_system_folder_filtered_on_the_client() {
        let folder = Path::new("/windows/system32");
        let arguments = |force| -> Vec<String> {
            taskkill_command(folder, 4242, force)
                .get_args()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(
            Path::new(taskkill_command(folder, 4242, false).get_program()),
            folder.join("taskkill.exe").as_path()
        );
        assert_eq!(
            arguments(false),
            ["/PID", "4242", "/FI", "IMAGENAME eq wfica32.exe"]
        );
        assert_eq!(
            arguments(true),
            ["/PID", "4242", "/FI", "IMAGENAME eq wfica32.exe", "/F"]
        );
    }

    #[test]
    fn only_the_exit_code_is_read() {
        assert_eq!(TerminateResult::of_exit(0), TerminateResult::Requested);
        assert_eq!(TerminateResult::of_exit(128), TerminateResult::Gone);
        assert_eq!(TerminateResult::of_exit(1), TerminateResult::Refused(1));
        assert!(TerminateResult::Gone.accepted());
        assert!(!TerminateResult::Refused(1).accepted());
        assert!(!TerminateResult::TimedOut.accepted());
    }

    /// A `taskkill.exe` exiting with success once looked at `runs` times, or never.
    struct Process {
        runs: Option<u32>,
        looked: u32,
        killed: u32,
    }

    impl Waitable for Process {
        fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
            self.looked += 1;
            Ok(self
                .runs
                .filter(|runs| self.looked > *runs)
                .map(|_| std::process::ExitStatus::default()))
        }

        fn kill_and_wait(&mut self) {
            self.killed += 1;
        }
    }

    #[test]
    fn a_taskkill_past_its_limit_is_killed() {
        let mut stuck = Process {
            runs: None,
            looked: 0,
            killed: 0,
        };
        // No time at all: looked at once, then killed, without waiting.
        assert_eq!(
            await_taskkill(&mut stuck, Duration::ZERO),
            TerminateResult::TimedOut
        );
        assert_eq!(stuck.killed, 1);

        let mut quick = Process {
            runs: Some(0),
            looked: 0,
            killed: 0,
        };
        assert_eq!(
            await_taskkill(&mut quick, TERMINATE_TIME_LIMIT),
            TerminateResult::Requested
        );
        assert_eq!(quick.killed, 0);
    }

    #[test]
    fn forcing_is_offered_after_the_grace_period_or_at_once_when_refused() {
        let start = Instant::now();
        let mut asked = Termination::started(7, false, start);
        assert_eq!(asked.offer(start), TerminateOffer::Pending { force: false });
        let answered = start + Duration::from_secs(1);
        asked.answered(TerminateResult::Requested, answered);
        assert_eq!(
            asked.offer(answered),
            TerminateOffer::Asked { force: false }
        );
        let almost = answered + TERMINATE_GRACE.saturating_sub(Duration::from_millis(1));
        assert_eq!(asked.offer(almost), TerminateOffer::Asked { force: false });
        assert_eq!(
            asked.offer(answered + TERMINATE_GRACE),
            TerminateOffer::Force(TerminateResult::Requested)
        );

        let mut refused = Termination::started(7, false, start);
        refused.answered(TerminateResult::Refused(1), start);
        assert_eq!(
            refused.offer(start),
            TerminateOffer::Force(TerminateResult::Refused(1))
        );

        let mut timed_out = Termination::started(7, true, start);
        assert_eq!(
            timed_out.offer(start),
            TerminateOffer::Pending { force: true }
        );
        timed_out.answered(TerminateResult::TimedOut, start);
        assert_eq!(
            timed_out.offer(start),
            TerminateOffer::Force(TerminateResult::TimedOut)
        );
    }
}
