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

//! System services enumeration and lifecycle dashboard, as the C# `ServiceStatusService`.
//!
//! Lists the operating system's services with their status and start type, and starts, stops
//! or restarts one. On Windows an action runs in Windows `PowerShell` named by its full path
//! in the system folder (never found on the `PATH`), as administrator through the elevation
//! prompt, which is the confirmation; the service name is checked against the list just read,
//! written as a single-quoted literal and handed over base64-encoded, as the C#
//! `BuildServiceActionScript` and `CreateElevatedServiceActionStartInfo`. Every command runs
//! within a time limit, past which it is killed.

use std::io;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use data_encoding::BASE64;
use thiserror::Error;

/// How long the service list may take, as the C# view model's `CancelAfter(30s)`.
pub const LIST_TIME_LIMIT: Duration = Duration::from_secs(30);

/// How long an action may take: the elevation prompt waits for the user, and a service may
/// take a while to start or stop.
pub const ACTION_TIME_LIMIT: Duration = Duration::from_secs(120);

/// How often a running command is looked at.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Windows `PowerShell` inside the system folder, as the C# `SystemExecutablePath`.
pub const POWERSHELL_PROGRAM: &str = r"WindowsPowerShell\v1.0\powershell.exe";

/// The switches every `PowerShell` here starts with.
const POWERSHELL_SWITCHES: [&str; 3] = ["-NoLogo", "-NoProfile", "-NonInteractive"];

/// Switch of a script given as text.
#[cfg(windows)]
const COMMAND_SWITCH: &str = "-Command";

/// Switch of a script given base64-encoded, UTF-16LE, as the C#.
const ENCODED_COMMAND_SWITCH: &str = "-EncodedCommand";

/// The variable `PowerShell` loads its modules from: an inherited one could name `PowerShell`
/// 7's modules, or modules planted by whoever started Heimdall. Windows `PowerShell` builds its
/// own when it is not there, as the credential guard check does.
#[cfg(windows)]
const MODULE_PATH_VARIABLE: &str = "PSModulePath";

/// Script that lists the services as CSV, which the parser expects, as the C#
/// `ServiceListScript`; its output in UTF-8, so display names keep their accents.
#[cfg(windows)]
const SERVICE_LIST_SCRIPT: &str = "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
     Get-Service | Select-Object Name,DisplayName,Status,StartType | ConvertTo-Csv -NoTypeInformation";

/// Exit code of the elevating script when Windows started nothing: the prompt declined, or
/// the start refused.
pub const NOT_ELEVATED_EXIT_CODE: i32 = 3;

/// Longest service name Windows accepts, in characters.
const MAX_SERVICE_NAME_LENGTH: usize = 256;

/// Characters refused in a service name: Windows refuses the slashes; the quotes (ASCII and
/// typographic) and the backtick could end or escape a `PowerShell` literal, and the wildcard
/// characters would make `-Name` match other services.
const FORBIDDEN_NAME_CHARACTERS: [char; 15] = [
    '/', '\\', '\'', '"', '`', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}', '\u{201C}',
    '\u{201D}', '\u{201E}', '*', '?', '[',
];

/// Closing bracket of a wildcard set, refused with the opening one.
const WILDCARD_SET_END: char = ']';

/// Suffix of a systemd service unit.
const UNIT_SUFFIX: &str = ".service";

/// Characters of a systemd unit name besides letters and digits, as `systemd.unit(5)` lists
/// them.
const UNIT_NAME_PUNCTUATION: [char; 5] = [':', '-', '_', '.', '@'];

/// Where `systemctl` is looked for: by its full path, never on the `PATH`.
#[cfg(not(windows))]
const SYSTEMCTL_CANDIDATES: [&str; 2] = ["/usr/bin/systemctl", "/bin/systemctl"];

/// The units' status said as on Windows.
const RUNNING: &str = "Running";
const STOPPED: &str = "Stopped";

/// A single system service entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServiceEntry {
    /// Internal system service name (e.g. "wuauserv", "sshd.service").
    pub name: String,
    /// User-friendly display title.
    pub display_name: String,
    /// Current execution state ("Running", "Stopped").
    pub status: String,
    /// Startup trigger mode (e.g. "Automatic", "Manual", "Disabled"; "enabled", "static").
    pub start_type: String,
}

impl ServiceEntry {
    /// Formats the service entry as a CSV line.
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        format!(
            "\"{}\",\"{}\",\"{}\",\"{}\"",
            self.name, self.display_name, self.status, self.start_type
        )
    }

    /// Checks if the service is currently in a running state.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.status.eq_ignore_ascii_case(RUNNING)
    }
}

/// Service administrative action verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    /// Start the stopped service.
    Start,
    /// Stop the running service.
    Stop,
    /// Restart the service.
    Restart,
}

impl ServiceAction {
    /// The `PowerShell` cmdlet of the action, as the C#.
    #[must_use]
    pub fn cmdlet(self) -> &'static str {
        match self {
            Self::Start => "Start-Service",
            Self::Stop => "Stop-Service",
            Self::Restart => "Restart-Service",
        }
    }

    /// The `systemctl` verb of the action.
    #[must_use]
    pub fn systemctl_verb(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

/// Why the services could not be listed, or an action not run.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ServiceError {
    /// The name holds a character that cannot be handed over safely.
    #[error("the service name is not valid")]
    InvalidName,
    /// The name is not in the list just read.
    #[error("the service is not in the list just read")]
    UnknownName,
    /// Windows did not say where its system folder is.
    #[error("Windows PowerShell could not be located")]
    NoPowerShell,
    /// `systemctl` is in none of the folders it is looked for in.
    #[error("systemctl could not be located")]
    NoSystemctl,
    /// The command could not be started: the system's message.
    #[error("the command could not be started: {0}")]
    NotStarted(String),
    /// The command did not end within its time limit, and was killed.
    #[error("the command did not end in time")]
    TimedOut,
    /// Windows started nothing as administrator: the prompt was declined or the start refused.
    #[error("the action was not started as administrator")]
    NotElevated,
    /// The action ran and failed: its exit code, and what it said when it could be read.
    #[error("the action failed")]
    ActionFailed {
        /// Exit code, when there is one.
        code: Option<i32>,
        /// What the command wrote on its error output, when it could be read.
        detail: String,
    },
    /// The list command failed: its exit code, and what it said.
    #[error("the services could not be listed")]
    ListFailed {
        /// Exit code, when there is one.
        code: Option<i32>,
        /// What the command wrote on its error output.
        detail: String,
    },
}

/// Parser for PowerShell `ConvertTo-Csv` output.
pub struct PowershellServiceCsvParser;

impl PowershellServiceCsvParser {
    /// Parses CSV text output from PowerShell into a list of [`ServiceEntry`].
    #[must_use]
    pub fn parse(csv: &str) -> Vec<ServiceEntry> {
        let mut entries = Vec::new();

        for raw_line in csv.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            let fields = Self::parse_csv_line(line);
            if fields.len() < 4 {
                continue;
            }

            // Header line check
            if fields[0].eq_ignore_ascii_case("Name")
                && fields[1].eq_ignore_ascii_case("DisplayName")
            {
                continue;
            }

            entries.push(ServiceEntry {
                name: fields[0].clone(),
                display_name: fields[1].clone(),
                status: fields[2].clone(),
                start_type: fields[3].clone(),
            });
        }

        entries
    }

    fn parse_csv_line(line: &str) -> Vec<String> {
        let mut fields = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;

        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '"' {
                if in_quotes && i + 1 < chars.len() && chars[i + 1] == '"' {
                    current.push('"');
                    i += 1;
                } else {
                    in_quotes = !in_quotes;
                }
            } else if c == ',' && !in_quotes {
                fields.push(current.trim().to_owned());
                current.clear();
            } else {
                current.push(c);
            }
            i += 1;
        }

        fields.push(current.trim().to_owned());
        fields
    }
}

/// The service units `systemctl list-units --plain --no-legend` lists, with their start type
/// from `systemctl list-unit-files --no-legend`: `UNIT LOAD ACTIVE SUB DESCRIPTION` per unit,
/// a status bullet in front taken away when an older `systemctl` still writes one, so that a
/// failed unit is listed too. A template's instance takes the template's start type.
#[must_use]
pub fn parse_systemctl_units(units: &str, unit_files: &str) -> Vec<ServiceEntry> {
    let start_types: std::collections::HashMap<&str, &str> = unit_files
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            Some((words.next()?, words.next()?))
        })
        .collect();
    let start_type = |unit: &str| {
        start_types
            .get(unit)
            .or_else(|| {
                let (prefix, _) = unit.split_once('@')?;
                start_types.get(format!("{prefix}@{UNIT_SUFFIX}").as_str())
            })
            .map_or_else(String::new, |&state| state.to_owned())
    };
    units
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace().peekable();
            // A bullet ("●", "×", "*") is one character that is no letter or digit.
            if words.peek().is_some_and(|first| {
                first.chars().count() == 1 && !first.chars().all(char::is_alphanumeric)
            }) {
                words.next();
            }
            let unit = words.next().filter(|unit| unit.ends_with(UNIT_SUFFIX))?;
            let _load = words.next()?;
            let active = words.next()?;
            let _sub = words.next()?;
            let description = words.collect::<Vec<_>>().join(" ");
            Some(ServiceEntry {
                name: unit.to_owned(),
                display_name: if description.is_empty() {
                    unit.to_owned()
                } else {
                    description
                },
                status: if active == "active" { RUNNING } else { STOPPED }.to_owned(),
                start_type: start_type(unit),
            })
        })
        .collect()
}

/// Whether `name` can be handed to Windows `PowerShell` as a service name: not empty, not
/// longer than Windows allows, without surrounding spaces, control characters, slashes,
/// quotes, backtick or wildcard characters.
#[must_use]
pub fn is_valid_windows_service_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_SERVICE_NAME_LENGTH
        && name.trim() == name
        && !name.chars().any(|c| {
            c.is_control() || FORBIDDEN_NAME_CHARACTERS.contains(&c) || c == WILDCARD_SET_END
        })
}

/// Whether `name` is a systemd service unit's name: letters, digits and `:-_.@`, ending in
/// `.service`, never starting with a dash that `systemctl` would read as a switch.
#[must_use]
pub fn is_valid_unit_name(name: &str) -> bool {
    name.len() > UNIT_SUFFIX.len()
        && name.chars().count() <= MAX_SERVICE_NAME_LENGTH
        && name.ends_with(UNIT_SUFFIX)
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || UNIT_NAME_PUNCTUATION.contains(&c))
}

/// `name` checked before an action: valid for this system, and one of `known`, the services
/// just listed.
///
/// # Errors
///
/// The name holds a character that cannot be handed over safely, or is not in `known`.
pub fn checked_service_name<'a>(
    name: &str,
    known: &'a [ServiceEntry],
) -> Result<&'a str, ServiceError> {
    let valid = if cfg!(windows) {
        is_valid_windows_service_name(name)
    } else {
        is_valid_unit_name(name)
    };
    if !valid {
        return Err(ServiceError::InvalidName);
    }
    known
        .iter()
        .find(|service| service.name == name)
        .map(|service| service.name.as_str())
        .ok_or(ServiceError::UnknownName)
}

/// `text` as a complete `PowerShell` single-quoted literal, each quote doubled, as the C#
/// `PowerShellSingleQuotedString.Quote`.
#[must_use]
pub fn powershell_single_quoted(text: &str) -> String {
    format!("'{}'", crate::winrm::quoted_content(text))
}

/// `script` as `-EncodedCommand` takes it: its UTF-16LE bytes in base64, as the C#
/// `Convert.ToBase64String(Encoding.Unicode.GetBytes(script))`.
#[must_use]
pub fn encoded_command(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    BASE64.encode(&bytes)
}

/// The script the elevated `PowerShell` runs: the cmdlet of `action` on the service `name`,
/// written as a literal, an error stopping it with a failure exit code.
#[must_use]
pub fn service_action_script(action: ServiceAction, name: &str) -> String {
    format!(
        "$ErrorActionPreference = 'Stop'; {} -Name {}",
        action.cmdlet(),
        powershell_single_quoted(name)
    )
}

/// The script that starts `powershell` as administrator through the elevation prompt with the
/// encoded `action_script`, in `system_folder`, waits for it and ends with its exit code; with
/// [`NOT_ELEVATED_EXIT_CODE`] when Windows started nothing. Only cmdlets, so that it runs in
/// the constrained language mode too.
#[must_use]
pub fn elevation_script(powershell: &Path, system_folder: &Path, action_script: &str) -> String {
    let arguments = format!(
        "{} {ENCODED_COMMAND_SWITCH} {}",
        POWERSHELL_SWITCHES.join(" "),
        encoded_command(action_script)
    );
    format!(
        "$ErrorActionPreference = 'Stop'; \
         try {{ $p = Start-Process -FilePath {} -ArgumentList {} -WorkingDirectory {} \
         -Verb RunAs -WindowStyle Hidden -Wait -PassThru }} \
         catch {{ exit {NOT_ELEVATED_EXIT_CODE} }}; \
         if ($null -eq $p) {{ exit {NOT_ELEVATED_EXIT_CODE} }}; exit $p.ExitCode",
        powershell_single_quoted(&powershell.to_string_lossy()),
        powershell_single_quoted(&arguments),
        powershell_single_quoted(&system_folder.to_string_lossy()),
    )
}

/// Something that runs and can be waited for and killed: a child process, or a test's own.
pub trait Waitable {
    /// How it ended.
    type Status;
    /// How it ended, if it did, without waiting.
    ///
    /// # Errors
    ///
    /// Its state cannot be read.
    fn try_wait(&mut self) -> io::Result<Option<Self::Status>>;
    /// Ends it, and waits for it to be gone.
    fn kill_and_reap(&mut self);
}

impl Waitable for Child {
    type Status = ExitStatus;

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        Child::try_wait(self)
    }

    fn kill_and_reap(&mut self) {
        // Already ended, the kill fails: the wait still reaps it.
        let _ = self.kill();
        let _ = self.wait();
    }
}

/// Waits for `child` to end within `limit`, looking every `poll`: `None` once the limit is
/// past, the child then killed. A state that cannot be read kills it too.
///
/// # Errors
///
/// The child's state cannot be read.
pub fn wait_within<W: Waitable>(
    child: &mut W,
    limit: Duration,
    poll: Duration,
) -> io::Result<Option<W::Status>> {
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) if Instant::now() >= deadline => {
                child.kill_and_reap();
                return Ok(None);
            }
            Ok(None) => thread::sleep(poll),
            Err(error) => {
                child.kill_and_reap();
                return Err(error);
            }
        }
    }
}

/// What a command that ended in time said.
struct Finished {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Runs `command` without a window, nothing on its input, its outputs read as they are
/// written, within `limit`: past it, the command is killed.
fn run_bounded(mut command: Command, limit: Duration) -> Result<Finished, ServiceError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| ServiceError::NotStarted(error.to_string()))?;
    let stdout = reader(child.stdout.take());
    let stderr = reader(child.stderr.take());
    match wait_within(&mut child, limit, POLL_INTERVAL) {
        Ok(Some(status)) => Ok(Finished {
            status,
            stdout: stdout.join().unwrap_or_default(),
            stderr: stderr.join().unwrap_or_default(),
        }),
        // The readers end with the outputs, once whatever still holds them is gone: they are
        // not waited for.
        Ok(None) => Err(ServiceError::TimedOut),
        Err(error) => Err(ServiceError::NotStarted(error.to_string())),
    }
}

/// A thread reading `output` to its end, so that a full pipe never holds the command.
fn reader<R: io::Read + Send + 'static>(output: Option<R>) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut output) = output {
            let _ = output.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// Windows `PowerShell` by its full path in the system folder, started there, the inherited
/// module path dropped, as the C# `CreateServiceListStartInfo`.
#[cfg(windows)]
fn powershell_command(system_folder: &Path) -> Command {
    let mut command = Command::new(system_folder.join(POWERSHELL_PROGRAM));
    command
        .args(POWERSHELL_SWITCHES)
        .current_dir(system_folder)
        .env_remove(MODULE_PATH_VARIABLE);
    command
}

/// `systemctl` by its full path, never found on the `PATH`.
#[cfg(not(windows))]
fn systemctl() -> Result<std::path::PathBuf, ServiceError> {
    SYSTEMCTL_CANDIDATES
        .iter()
        .map(std::path::PathBuf::from)
        .find(|path| path.is_file())
        .ok_or(ServiceError::NoSystemctl)
}

/// Lists the system's services.
///
/// # Errors
///
/// The command could not be found or started, did not end in time, or failed.
pub fn run_services_command() -> Result<Vec<ServiceEntry>, ServiceError> {
    #[cfg(windows)]
    {
        let folder = crate::paths::system_dir().ok_or(ServiceError::NoPowerShell)?;
        let mut command = powershell_command(&folder);
        command.arg(COMMAND_SWITCH).arg(SERVICE_LIST_SCRIPT);
        let finished = run_bounded(command, LIST_TIME_LIMIT)?;
        if !finished.status.success() {
            return Err(ServiceError::ListFailed {
                code: finished.status.code(),
                detail: String::from_utf8_lossy(&finished.stderr).trim().to_owned(),
            });
        }
        Ok(PowershellServiceCsvParser::parse(&String::from_utf8_lossy(
            &finished.stdout,
        )))
    }

    #[cfg(not(windows))]
    {
        let program = systemctl()?;
        let listed = |arguments: &[&str]| -> Result<String, ServiceError> {
            let mut command = Command::new(&program);
            command.args(arguments);
            let finished = run_bounded(command, LIST_TIME_LIMIT)?;
            if !finished.status.success() {
                return Err(ServiceError::ListFailed {
                    code: finished.status.code(),
                    detail: String::from_utf8_lossy(&finished.stderr).trim().to_owned(),
                });
            }
            Ok(String::from_utf8_lossy(&finished.stdout).into_owned())
        };
        let units = listed(&[
            "list-units",
            "--type=service",
            "--all",
            "--no-pager",
            "--no-legend",
            "--plain",
        ])?;
        let unit_files = listed(&[
            "list-unit-files",
            "--type=service",
            "--no-pager",
            "--no-legend",
        ])?;
        Ok(parse_systemctl_units(&units, &unit_files))
    }
}

/// Runs `action` on the service `name`, which must be one of `known`, the services just
/// listed: on Windows as administrator through the elevation prompt, elsewhere through
/// `systemctl`, which asks for its own authorization.
///
/// # Errors
///
/// The name is refused, the command could not be found or started, was not elevated, did not
/// end in time, or failed.
pub fn run_service_action(
    action: ServiceAction,
    name: &str,
    known: &[ServiceEntry],
) -> Result<(), ServiceError> {
    let name = checked_service_name(name, known)?;

    #[cfg(windows)]
    {
        let folder = crate::paths::system_dir().ok_or(ServiceError::NoPowerShell)?;
        let script = elevation_script(
            &folder.join(POWERSHELL_PROGRAM),
            &folder,
            &service_action_script(action, name),
        );
        let mut command = powershell_command(&folder);
        command
            .arg(ENCODED_COMMAND_SWITCH)
            .arg(encoded_command(&script));
        let finished = run_bounded(command, ACTION_TIME_LIMIT)?;
        match finished.status.code() {
            Some(0) => Ok(()),
            Some(NOT_ELEVATED_EXIT_CODE) => Err(ServiceError::NotElevated),
            code => Err(ServiceError::ActionFailed {
                code,
                detail: String::new(),
            }),
        }
    }

    #[cfg(not(windows))]
    {
        let mut command = Command::new(systemctl()?);
        command.arg(action.systemctl_verb()).arg("--").arg(name);
        let finished = run_bounded(command, ACTION_TIME_LIMIT)?;
        if finished.status.success() {
            Ok(())
        } else {
            Err(ServiceError::ActionFailed {
                code: finished.status.code(),
                detail: String::from_utf8_lossy(&finished.stderr).trim().to_owned(),
            })
        }
    }
}

/// Builds a CSV string from a slice of [`ServiceEntry`].
#[must_use]
pub fn build_services_csv(entries: &[ServiceEntry]) -> String {
    let mut csv = String::from("Name,DisplayName,Status,StartType\n");
    for s in entries {
        csv.push_str(&s.to_csv_line());
        csv.push('\n');
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(name: &str) -> ServiceEntry {
        ServiceEntry {
            name: name.to_owned(),
            display_name: name.to_owned(),
            status: STOPPED.to_owned(),
            start_type: String::new(),
        }
    }

    #[test]
    fn test_powershell_service_csv_parser() {
        let sample = "\
\"Name\",\"DisplayName\",\"Status\",\"StartType\"
\"Appinfo\",\"Application Information\",\"Running\",\"Manual\"
\"AudioSrv\",\"Windows Audio\",\"Running\",\"Automatic\"
\"wuauserv\",\"Windows Update\",\"Stopped\",\"Manual\"
";
        let services = PowershellServiceCsvParser::parse(sample);
        assert_eq!(services.len(), 3);

        assert_eq!(services[0].name, "Appinfo");
        assert_eq!(services[0].display_name, "Application Information");
        assert_eq!(services[0].status, "Running");
        assert_eq!(services[0].start_type, "Manual");
        assert!(services[0].is_running());

        assert_eq!(services[1].name, "AudioSrv");
        assert!(services[1].is_running());

        assert_eq!(services[2].name, "wuauserv");
        assert_eq!(services[2].status, "Stopped");
        assert!(!services[2].is_running());
    }

    #[test]
    fn systemd_units_are_read_with_their_status_and_start_type_failed_ones_included() {
        let units = "\
cron.service loaded active running Regular background program processing daemon
\u{25CF} broken.service loaded failed failed Broken thing
\u{00D7} other.service not-found inactive dead other.service
getty@tty1.service loaded active running Getty on tty1
";
        let files = "\
cron.service enabled enabled
broken.service disabled enabled
getty@.service enabled enabled
";
        let services = parse_systemctl_units(units, files);
        let seen: Vec<(&str, &str, &str, &str)> = services
            .iter()
            .map(|s| {
                (
                    s.name.as_str(),
                    s.display_name.as_str(),
                    s.status.as_str(),
                    s.start_type.as_str(),
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                (
                    "cron.service",
                    "Regular background program processing daemon",
                    "Running",
                    "enabled"
                ),
                ("broken.service", "Broken thing", "Stopped", "disabled"),
                ("other.service", "other.service", "Stopped", ""),
                ("getty@tty1.service", "Getty on tty1", "Running", "enabled"),
            ]
        );
    }

    #[test]
    fn a_windows_service_name_with_quotes_control_characters_or_wildcards_is_refused() {
        assert!(is_valid_windows_service_name("wuauserv"));
        assert!(is_valid_windows_service_name("Some Service_1.2-x"));
        for bad in [
            "",
            " wuauserv",
            "a'b",
            "a\"b",
            "a\u{2019}b",
            "a\u{201C}b",
            "a`b",
            "a\nb",
            "a\u{0}b",
            "a/b",
            "a\\b",
            "*",
            "wu?userv",
            "a[1]",
            "x'; Remove-Item C:\\ -Recurse; '",
        ] {
            assert!(!is_valid_windows_service_name(bad), "{bad:?}");
        }
        assert!(!is_valid_windows_service_name(
            &"a".repeat(MAX_SERVICE_NAME_LENGTH + 1)
        ));
    }

    #[test]
    fn a_unit_name_is_a_service_unit_without_a_leading_dash() {
        assert!(is_valid_unit_name("sshd.service"));
        assert!(is_valid_unit_name("getty@tty1.service"));
        for bad in [
            ".service",
            "sshd",
            "-H.service",
            "a b.service",
            "a;b.service",
            "a\n.service",
        ] {
            assert!(!is_valid_unit_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_name_not_in_the_list_just_read_is_refused() {
        let name = if cfg!(windows) {
            "wuauserv"
        } else {
            "sshd.service"
        };
        let known = [service(name)];
        assert_eq!(checked_service_name(name, &known), Ok(name));
        let other = if cfg!(windows) {
            "Spooler"
        } else {
            "cups.service"
        };
        assert_eq!(
            checked_service_name(other, &known),
            Err(ServiceError::UnknownName)
        );
        assert_eq!(
            checked_service_name("a'b", &[service("a'b")]),
            Err(ServiceError::InvalidName),
            "a refused name stays refused even when listed"
        );
        assert_eq!(
            run_service_action(ServiceAction::Start, other, &known),
            Err(ServiceError::UnknownName),
            "nothing runs for a name not listed"
        );
    }

    #[test]
    fn single_quoted_literals_double_every_kind_of_single_quote() {
        assert_eq!(powershell_single_quoted("wuauserv"), "'wuauserv'");
        assert_eq!(powershell_single_quoted("a'b"), "'a''b'");
        assert_eq!(
            powershell_single_quoted("a\u{2019}b"),
            "'a\u{2019}\u{2019}b'"
        );
    }

    #[test]
    fn the_encoded_command_is_utf16le_in_base64() {
        // As `[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('Get-Date'))`.
        assert_eq!(encoded_command("Get-Date"), "RwBlAHQALQBEAGEAdABlAA==");
        let script = service_action_script(ServiceAction::Restart, "wuauserv");
        assert_eq!(
            script,
            "$ErrorActionPreference = 'Stop'; Restart-Service -Name 'wuauserv'"
        );
        let decoded = BASE64
            .decode(encoded_command(&script).as_bytes())
            .expect("base64");
        let units: Vec<u16> = decoded
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(String::from_utf16(&units).expect("utf16"), script);
    }

    #[test]
    fn the_elevation_script_runs_the_encoded_action_as_administrator_by_full_path() {
        let powershell = Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        let folder = Path::new(r"C:\Windows\System32");
        let action = service_action_script(ServiceAction::Stop, "wuauserv");
        let script = elevation_script(powershell, folder, &action);
        assert!(script.contains(
            r"Start-Process -FilePath 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'"
        ));
        assert!(script.contains(&format!(
            "-ArgumentList '-NoLogo -NoProfile -NonInteractive -EncodedCommand {}'",
            encoded_command(&action)
        )));
        assert!(script.contains(r"-WorkingDirectory 'C:\Windows\System32'"));
        assert!(script.contains("-Verb RunAs"));
        assert!(script.contains("-Wait -PassThru"));
        assert!(script.contains(&format!("exit {NOT_ELEVATED_EXIT_CODE}")));
        // The name is nowhere in clear: only inside the encoded action.
        assert!(!script.contains("wuauserv"));
    }

    /// A process of the test's own: ends after `ends_after` looks, or never.
    struct Fake {
        ends_after: Option<u32>,
        looks: u32,
        killed: bool,
    }

    impl Waitable for Fake {
        type Status = i32;

        fn try_wait(&mut self) -> io::Result<Option<i32>> {
            self.looks += 1;
            Ok(self
                .ends_after
                .filter(|&after| self.looks > after)
                .map(|_| 0))
        }

        fn kill_and_reap(&mut self) {
            self.killed = true;
        }
    }

    #[test]
    fn a_command_past_its_time_limit_is_killed() {
        let mut never = Fake {
            ends_after: None,
            looks: 0,
            killed: false,
        };
        let waited = wait_within(
            &mut never,
            Duration::from_millis(30),
            Duration::from_millis(5),
        )
        .expect("waited");
        assert_eq!(waited, None);
        assert!(never.killed);

        let mut quick = Fake {
            ends_after: Some(2),
            looks: 0,
            killed: false,
        };
        let waited = wait_within(&mut quick, Duration::from_secs(5), Duration::from_millis(1))
            .expect("waited");
        assert_eq!(waited, Some(0));
        assert!(!quick.killed);
    }
}
