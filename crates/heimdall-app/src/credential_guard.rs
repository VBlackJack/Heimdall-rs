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

//! Whether Credential Guard runs on this computer, as the C# `CredentialGuardService`:
//! the `SecurityServicesRunning` property of the Device Guard WMI class
//! `root\Microsoft\Windows\DeviceGuard:Win32_DeviceGuard` holds `1` when it does.
//!
//! The C# reads WMI in its own process. This crate cannot, so Windows `PowerShell` reads it:
//! `powershell.exe` from the system folder Windows names (never from the environment), its
//! arguments each its own with no shell reading them, no profile loaded, no window, nothing
//! to read on its input, its errors dropped, its output read on a thread of its own and the
//! whole check bounded by [`CHECK_TIME_LIMIT`], past which the process is killed. The script
//! uses only what the constrained language mode allows. Only the digits and the commas of
//! the output are read, as the C# reads the property's integers.
//!
//! Only a definitive answer (running or not) is kept, for as long as the application runs,
//! as the C# keeps it; an answer that could not be had is asked again the next time. One
//! check runs at a time: whoever needs the answer meanwhile waits for that one.
//!
//! Two limits, accepted: where `AppLocker` or Windows Defender Application Control keep
//! `powershell.exe` from starting, the answer cannot be had and an embedded RDP session is
//! refused while the setting is on, the reason shown in Settings; and a `PowerShell` module
//! of the same name placed by the same user in a folder of `PSModulePath` could answer in
//! place of the real one. That user can turn the setting off as well, so this is no
//! boundary between privileges: the check guards against a computer without Credential
//! Guard, not against its own user.

use std::fmt;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// How long a check may take: well above the second and a half `PowerShell` takes to start
/// and answer on a loaded computer, and below what a user would take for a connection stuck.
/// Past it, `powershell.exe` is killed and the answer cannot be had.
pub const CHECK_TIME_LIMIT: Duration = Duration::from_secs(15);

/// Windows `PowerShell`, under the system folder.
pub const POWERSHELL_PROGRAM: &str = r"WindowsPowerShell\v1.0\powershell.exe";

/// `PowerShell`'s switches: no banner, no profile, nothing asked.
pub const POWERSHELL_SWITCHES: [&str; 3] = ["-NoLogo", "-NoProfile", "-NonInteractive"];

/// The switch the script follows.
pub const POWERSHELL_COMMAND_SWITCH: &str = "-Command";

/// The script: the services running, as their numbers joined by commas, or the exit code
/// [`NO_INSTANCE_EXIT_CODE`] when the class has no instance. `-join` rather than a .NET
/// method, which the constrained language mode refuses.
pub const DETECTION_SCRIPT: &str = "$ErrorActionPreference='Stop'; \
     $ProgressPreference='SilentlyContinue'; \
     $d = Get-CimInstance -Namespace 'root\\Microsoft\\Windows\\DeviceGuard' \
     -ClassName Win32_DeviceGuard; \
     if ($null -eq $d) { exit 3 }; \
     @($d.SecurityServicesRunning) -join ','";

/// The exit code of the script when `Win32_DeviceGuard` has no instance.
pub const NO_INSTANCE_EXIT_CODE: i32 = 3;

/// The number `SecurityServicesRunning` holds for Credential Guard.
const CREDENTIAL_GUARD_SERVICE: u32 = 1;

/// What separates the numbers of the output.
const SERVICE_SEPARATOR: char = ',';

/// The name of the thread a check's output is read on.
#[cfg(windows)]
const READER_THREAD: &str = "credential-guard";

/// Whether Credential Guard runs, as the C# `CredentialGuardState` says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// It runs.
    Active,
    /// It does not run.
    Inactive,
    /// The answer could not be had, and why.
    Indeterminate(Failure),
}

impl Status {
    /// Whether it is an answer, kept for as long as the application runs.
    #[must_use]
    pub fn is_definitive(&self) -> bool {
        !matches!(self, Self::Indeterminate(_))
    }

    /// Whether it lets an embedded RDP session open while the setting asks for it.
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// Why the answer could not be had, as the C# `FailureReason` says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// This is not Windows.
    NotWindows,
    /// Windows did not say where its system folder is.
    NoSystemFolder,
    /// `powershell.exe` did not start, and why: `AppLocker` or Windows Defender Application
    /// Control refusing it says so here.
    NotStarted(String),
    /// No answer within [`CHECK_TIME_LIMIT`].
    TimedOut,
    /// `PowerShell` ended with this exit code, `None` when it has none.
    Exited(Option<i32>),
    /// `Win32_DeviceGuard` has no instance.
    NoInstance,
    /// The output could not be read, and why.
    Unread(String),
    /// `SecurityServicesRunning` holds nothing.
    NoValue,
    /// `SecurityServicesRunning` holds something that is not a service number.
    InvalidValue,
}

impl fmt::Display for Failure {
    /// In English, as the log says it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotWindows => f.write_str("not Windows"),
            Self::NoSystemFolder => f.write_str(heimdall_core::paths::SYSTEM_FOLDER_UNKNOWN),
            Self::NotStarted(detail) => write!(f, "PowerShell did not start: {detail}"),
            Self::TimedOut => write!(f, "no answer within {} seconds", CHECK_TIME_LIMIT.as_secs()),
            Self::Exited(Some(code)) => write!(f, "PowerShell ended with code {code}"),
            Self::Exited(None) => f.write_str("PowerShell ended without an exit code"),
            Self::NoInstance => f.write_str("Win32_DeviceGuard returned no instance"),
            Self::Unread(detail) => write!(f, "the output was not read: {detail}"),
            Self::NoValue => f.write_str("SecurityServicesRunning returned no value"),
            Self::InvalidValue => f.write_str("SecurityServicesRunning contained an invalid value"),
        }
    }
}

/// What `output`, the script's, says, as the C# `MapSecurityServicesRunning` reads the
/// property: nothing is [`Failure::NoValue`]; a `1` met before anything else is
/// [`Status::Active`]; a value that is not a number met before it is
/// [`Failure::InvalidValue`]; numbers without a `1` are [`Status::Inactive`]. Only ASCII
/// digits make a number; the line break after the output is not part of it.
#[must_use]
pub fn parse(output: &str) -> Status {
    let output = output.trim_end_matches(['\r', '\n']);
    if output.is_empty() {
        return Status::Indeterminate(Failure::NoValue);
    }
    for value in output.split(SERVICE_SEPARATOR) {
        let Some(service) = service_number(value) else {
            return Status::Indeterminate(Failure::InvalidValue);
        };
        if service == CREDENTIAL_GUARD_SERVICE {
            return Status::Active;
        }
    }
    Status::Inactive
}

/// `value` as a service number: ASCII digits only, within `u32`, as the C# converts it.
fn service_number(value: &str) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

/// The command asking `PowerShell` in `system_dir`, each argument its own, no shell reading
/// them.
#[must_use]
pub fn detection_command(system_dir: &std::path::Path) -> std::process::Command {
    let mut command = std::process::Command::new(system_dir.join(POWERSHELL_PROGRAM));
    command
        .args(POWERSHELL_SWITCHES)
        .arg(POWERSHELL_COMMAND_SWITCH)
        .arg(DETECTION_SCRIPT);
    command
}

/// What finds whether Credential Guard runs: `PowerShell`, or another in tests. It waits on
/// the system: it is called off the UI thread and outside any async task.
pub trait Probe: Send + Sync {
    /// Whether Credential Guard runs.
    fn detect(&self) -> Status;
}

/// Credential Guard as Windows `PowerShell` reads it from WMI.
#[derive(Debug, Clone, Copy, Default)]
pub struct PowerShell;

impl Probe for PowerShell {
    fn detect(&self) -> Status {
        detect_with_powershell()
    }
}

/// Elsewhere there is no Credential Guard to read.
#[cfg(not(windows))]
fn detect_with_powershell() -> Status {
    Status::Indeterminate(Failure::NotWindows)
}

/// `PowerShell` started from the system folder Windows names, within [`CHECK_TIME_LIMIT`]:
/// past it, the process is killed.
#[cfg(windows)]
fn detect_with_powershell() -> Status {
    use std::io::Read as _;
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;

    let Some(folder) = heimdall_core::paths::system_dir() else {
        return Status::Indeterminate(Failure::NoSystemFolder);
    };
    let mut child = match detection_command(&folder)
        .creation_flags(crate::citrix::CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return Status::Indeterminate(Failure::NotStarted(error.to_string())),
    };
    // Read as it is written, so a full pipe never holds it: the reading ends with it.
    let Some(mut stdout) = child.stdout.take() else {
        child_gone(&mut child);
        return Status::Indeterminate(Failure::Unread("no output".to_owned()));
    };
    let reader = std::thread::Builder::new()
        .name(READER_THREAD.to_owned())
        .spawn(move || {
            let mut output = Vec::new();
            stdout.read_to_end(&mut output).map(|_| output)
        });
    let reader = match reader {
        Ok(reader) => reader,
        Err(error) => {
            child_gone(&mut child);
            return Status::Indeterminate(Failure::Unread(error.to_string()));
        }
    };
    let status = crate::citrix_session::wait_within(&mut child, CHECK_TIME_LIMIT);
    // Its state unread, it may still run: ended, so that its output ends too.
    if status.is_err() {
        child_gone(&mut child);
    }
    let output = reader.join();
    match status {
        Err(error) => Status::Indeterminate(Failure::Unread(error.to_string())),
        Ok(None) => Status::Indeterminate(Failure::TimedOut),
        Ok(Some(status)) if status.code() == Some(NO_INSTANCE_EXIT_CODE) => {
            Status::Indeterminate(Failure::NoInstance)
        }
        Ok(Some(status)) if !status.success() => {
            Status::Indeterminate(Failure::Exited(status.code()))
        }
        Ok(Some(_)) => match output {
            Ok(Ok(output)) => parse(&String::from_utf8_lossy(&output)),
            Ok(Err(error)) => Status::Indeterminate(Failure::Unread(error.to_string())),
            Err(_) => Status::Indeterminate(Failure::Unread("the reader stopped".to_owned())),
        },
    }
}

/// Ends a check given up before it was waited for.
#[cfg(windows)]
fn child_gone(child: &mut std::process::Child) {
    use crate::citrix_session::Waitable as _;
    child.kill_and_wait();
}

/// The detection with its answer kept, as the C# service: a definitive answer for as long
/// as the application runs, one check at a time.
pub struct Detector {
    /// What finds the answer.
    probe: Arc<dyn Probe>,
    /// The definitive answer, once had: read without waiting.
    definitive: OnceLock<Status>,
    /// Held while a check runs, so that a second one waits for the first one's answer.
    checking: tokio::sync::Mutex<()>,
}

impl fmt::Debug for Detector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Detector")
            .field("definitive", &self.definitive.get())
            .finish_non_exhaustive()
    }
}

impl Default for Detector {
    fn default() -> Self {
        Self::with_probe(Arc::new(PowerShell))
    }
}

impl Detector {
    /// The detection through `probe`, nothing known yet.
    #[must_use]
    pub fn with_probe(probe: Arc<dyn Probe>) -> Self {
        Self {
            probe,
            definitive: OnceLock::new(),
            checking: tokio::sync::Mutex::new(()),
        }
    }

    /// The definitive answer, when one was had; never waits.
    #[must_use]
    pub fn cached(&self) -> Option<Status> {
        self.definitive.get().cloned()
    }

    /// Keeps `status` when it is definitive and none is kept yet.
    pub fn record(&self, status: &Status) {
        if status.is_definitive() {
            // Kept already: the first answer stays, as the C# keeps it.
            let _ = self.definitive.set(status.clone());
        }
    }

    /// Whether Credential Guard runs: the answer kept, else a check on a blocking thread of
    /// the runtime, after the one running, if any, has answered. An answer that could not
    /// be had is said in the log, as the C# `LogCredentialGuardCheckFailed`.
    pub async fn status(&self) -> Status {
        if let Some(status) = self.cached() {
            return status;
        }
        let _checking = self.checking.lock().await;
        if let Some(status) = self.cached() {
            return status;
        }
        let probe = Arc::clone(&self.probe);
        let status = tokio::task::spawn_blocking(move || probe.detect())
            .await
            .unwrap_or_else(|error| Status::Indeterminate(Failure::Unread(error.to_string())));
        match &status {
            Status::Indeterminate(failure) => {
                log::warn!("Credential Guard check failed: {failure}");
            }
            definitive => {
                log::info!("Credential Guard: {definitive:?}");
                self.record(definitive);
            }
        }
        status
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn the_output_is_read_as_the_csharp_reads_the_property() {
        for (output, expected) in [
            ("1\r\n", Status::Active),
            ("1", Status::Active),
            ("2,1", Status::Active),
            ("1,2", Status::Active),
            ("2", Status::Inactive),
            ("0", Status::Inactive),
            ("2,3\n", Status::Inactive),
            ("", Status::Indeterminate(Failure::NoValue)),
            ("\r\n", Status::Indeterminate(Failure::NoValue)),
            ("x", Status::Indeterminate(Failure::InvalidValue)),
            ("2,x", Status::Indeterminate(Failure::InvalidValue)),
            ("2,,1", Status::Indeterminate(Failure::InvalidValue)),
            // A 1 met first answers before what follows is read, as the C# loop.
            ("1,x", Status::Active),
            (" 1", Status::Indeterminate(Failure::InvalidValue)),
            ("-1", Status::Indeterminate(Failure::InvalidValue)),
            ("+1", Status::Indeterminate(Failure::InvalidValue)),
            ("4294967296", Status::Indeterminate(Failure::InvalidValue)),
            ("\u{0661}", Status::Indeterminate(Failure::InvalidValue)),
            ("4294967295", Status::Inactive),
        ] {
            assert_eq!(parse(output), expected, "{output:?}");
        }
    }

    #[test]
    fn the_command_is_powershell_in_the_system_folder_with_its_arguments_apart() {
        let command = detection_command(std::path::Path::new("sys"));
        assert_eq!(
            std::path::Path::new(command.get_program()),
            std::path::Path::new("sys").join(POWERSHELL_PROGRAM)
        );
        let arguments: Vec<_> = command.get_args().collect();
        assert_eq!(
            arguments,
            [
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                DETECTION_SCRIPT
            ]
        );
        assert_eq!(
            DETECTION_SCRIPT,
            r"$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; $d = Get-CimInstance -Namespace 'root\Microsoft\Windows\DeviceGuard' -ClassName Win32_DeviceGuard; if ($null -eq $d) { exit 3 }; @($d.SecurityServicesRunning) -join ','"
        );
        assert!(
            DETECTION_SCRIPT.contains(&format!("exit {NO_INSTANCE_EXIT_CODE} ")),
            "the exit code read back"
        );
        assert!(!DETECTION_SCRIPT.contains('"'), "nothing to quote again");
    }

    /// A probe answering each of `answers` in turn, counting its calls.
    struct Answers {
        answers: Mutex<Vec<Status>>,
        calls: AtomicUsize,
    }

    impl Answers {
        fn new(answers: Vec<Status>) -> Arc<Self> {
            Arc::new(Self {
                answers: Mutex::new(answers),
                calls: AtomicUsize::new(0),
            })
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl Probe for Answers {
        fn detect(&self) -> Status {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.answers.lock().expect("answers").remove(0)
        }
    }

    #[tokio::test]
    async fn only_a_definitive_answer_is_kept() {
        let probe = Answers::new(vec![
            Status::Indeterminate(Failure::TimedOut),
            Status::Inactive,
            Status::Active,
        ]);
        let detector = Detector::with_probe(probe.clone());
        assert_eq!(detector.cached(), None, "nothing known before a check");
        assert_eq!(
            detector.status().await,
            Status::Indeterminate(Failure::TimedOut)
        );
        assert_eq!(detector.cached(), None, "asked again next time");
        assert_eq!(detector.status().await, Status::Inactive);
        assert_eq!(detector.cached(), Some(Status::Inactive));
        assert_eq!(detector.status().await, Status::Inactive, "kept");
        assert_eq!(probe.calls(), 2, "never asked once known");
        // An answer recorded later does not replace the first.
        detector.record(&Status::Active);
        assert_eq!(detector.cached(), Some(Status::Inactive));
    }

    #[test]
    fn an_answer_recorded_is_kept_only_when_definitive() {
        let detector = Detector::with_probe(Answers::new(Vec::new()));
        detector.record(&Status::Indeterminate(Failure::NoValue));
        assert_eq!(detector.cached(), None);
        detector.record(&Status::Active);
        assert_eq!(detector.cached(), Some(Status::Active));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn checks_asked_together_wait_for_one() {
        let probe = Answers::new(vec![Status::Active]);
        let detector = Arc::new(Detector::with_probe(probe.clone()));
        let asked: Vec<_> = (0..4)
            .map(|_| {
                let detector = Arc::clone(&detector);
                tokio::spawn(async move { detector.status().await })
            })
            .collect();
        for answer in asked {
            assert_eq!(answer.await.expect("answer"), Status::Active);
        }
        assert_eq!(probe.calls(), 1, "one check at a time, its answer shared");
    }

    #[test]
    fn a_failure_is_said_in_english_for_the_log() {
        assert_eq!(
            Failure::Exited(Some(1)).to_string(),
            "PowerShell ended with code 1"
        );
        assert_eq!(Failure::TimedOut.to_string(), "no answer within 15 seconds");
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_than_windows_the_answer_cannot_be_had() {
        assert_eq!(
            PowerShell.detect(),
            Status::Indeterminate(Failure::NotWindows)
        );
    }
}
