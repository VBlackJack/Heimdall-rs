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

//! The one runner of the system programs the tools start: `tracert`, `traceroute`, `arp`,
//! `ping` and `nslookup` read line by line as they print ([`run_lines`]), Windows
//! `PowerShell`, `netsh`, `ip`, `ss` and `nmcli` read whole once they end ([`run`]), and
//! one error type, [`ProgramError`], for every way they fail. With them, the programs the
//! user lists as external tools, started and left to run ([`launch_detached`]).
//!
//! Every program is named by its full path (the system folder on Windows, fixed folders
//! elsewhere, never the `PATH`), runs without a window and nothing on its input, its
//! messages untranslated on Linux, within a time limit past which it is killed. Its output
//! is decoded as UTF-8 when it is, else in the code page a console program writes in. A
//! program that fails is said to have failed, with its own words: never an empty list.

use std::io::{BufRead as _, BufReader, Read as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

use super::service_status::wait_within;

/// How often a running program, or a waiting probe, looks at its stop and its deadline.
pub const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// The most lines read from a program run by [`run_lines`]: what a traceroute or an ARP
/// table prints is far below, and a program that never ends its output is cut there.
pub const MAX_OUTPUT_LINES: usize = 10_000;

/// The most of a program's output [`run`] reads, each stream: a table of every connection
/// of a busy server is a few megabytes of JSON at most. Past it the pipe is closed, the
/// program ends or runs out of time.
pub const MAX_OUTPUT_BYTES: u64 = 32 * 1024 * 1024;

/// Where a Unix system program is looked for, in order, as the files crate's privileged
/// commands do: never the `PATH` whoever starts Heimdall sets.
const UNIX_SYSTEM_FOLDERS: [&str; 4] = ["/usr/sbin", "/usr/bin", "/sbin", "/bin"];

/// The process gets no console window, as the C# `CreateNoWindow`.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A console program gets a console of its own, as Windows' shell gives one to a program it
/// starts: never the one Heimdall may have been started from.
#[cfg(windows)]
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// Windows `PowerShell` inside the system folder, as the C# `SystemExecutablePath`.
#[cfg(windows)]
const POWERSHELL_PROGRAM: &str = super::service_status::POWERSHELL_PROGRAM;

/// The switches every `PowerShell` here starts with.
#[cfg(windows)]
const POWERSHELL_SWITCHES: [&str; 3] = ["-NoLogo", "-NoProfile", "-NonInteractive"];

/// Switch of a script given as text. Not `-EncodedCommand`: with it, `PowerShell` writes
/// its progress and errors on the error output as CLIXML, unreadable as a failure's words.
/// The scripts here hold no double quote, so the text passes as one argument.
#[cfg(windows)]
const COMMAND_SWITCH: &str = "-Command";

/// The variable `PowerShell` loads its modules from: an inherited one could name `PowerShell`
/// 7's modules, or modules planted by whoever started Heimdall, as the services list.
#[cfg(windows)]
const MODULE_PATH_VARIABLE: &str = "PSModulePath";

/// What every script starts with: its output in UTF-8, so names keep their accents; no
/// progress records; an error ends it with its message alone on the error output and a
/// failure exit code.
pub const POWERSHELL_PRELUDE: &str = "$ErrorActionPreference = 'Stop'; \
     $ProgressPreference = 'SilentlyContinue'; \
     [Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
     trap { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }; ";

/// The UTF-8 byte order mark, which a program may write first.
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// The variable naming the language of a Linux program's messages: set to `C`, so that what
/// is read is never translated.
#[cfg(target_os = "linux")]
const LOCALE_VARIABLE: &str = "LC_ALL";

/// The untranslated locale.
#[cfg(target_os = "linux")]
const UNTRANSLATED_LOCALE: &str = "C";

/// Why a system program gave nothing to read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProgramError {
    /// This system has no way to read it: not Windows nor Linux, or the program that reads
    /// it is not installed where it is looked for.
    #[error("not available on this system")]
    NotAvailable,
    /// The program could not be started: the system's message.
    #[error("the command could not be started: {0}")]
    NotStarted(String),
    /// The program did not end within its time limit, and was killed.
    #[error("the command did not end in time")]
    TimedOut,
    /// The program ran and failed: its exit code, and what it said.
    #[error("the command failed")]
    Failed {
        /// Exit code, when there is one.
        code: Option<i32>,
        /// What the program said about it, on one line; empty when it said nothing.
        detail: String,
    },
    /// The program's output is not what was asked for: why.
    #[error("the command's output could not be read: {0}")]
    Unreadable(String),
}

/// How a program run by [`run_lines`] ended, when it did not fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    /// It ended by itself, its output read whole.
    Exited,
    /// The stop was asked: it was ended.
    Stopped,
}

/// What a program run by [`run`] that ended in time said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    /// It exited with success.
    pub success: bool,
    /// Its exit code, when it has one.
    pub code: Option<i32>,
    /// Its standard output, at most [`MAX_OUTPUT_BYTES`].
    pub stdout: Vec<u8>,
    /// Its standard error, at most [`MAX_OUTPUT_BYTES`].
    pub stderr: Vec<u8>,
}

impl Finished {
    /// The failure this run is: its exit code, and its error output's words, its standard
    /// output's when the error output is empty (`netsh` writes its errors there).
    #[must_use]
    pub fn failure(&self) -> ProgramError {
        let mut detail = one_line(&decode_text(&self.stderr));
        if detail.is_empty() {
            detail = one_line(&decode_text(&self.stdout));
        }
        ProgramError::Failed {
            code: self.code,
            detail,
        }
    }
}

/// A system program by its whole path: `windows_name` in Windows' system folder, as
/// Windows says where it is; elsewhere `unix_name` in the first of the system folders
/// holding it. `None` when there is none.
#[must_use]
pub fn system_tool(windows_name: &str, unix_name: &str) -> Option<PathBuf> {
    if cfg!(windows) {
        crate::paths::system_program(windows_name).filter(|program| program.is_file())
    } else {
        UNIX_SYSTEM_FOLDERS
            .iter()
            .map(|folder| Path::new(folder).join(unix_name))
            .find(|program| program.is_file())
    }
}

/// The first of `candidates` that is a file: a program found by its full path, never on
/// the `PATH`.
#[must_use]
pub fn find_program(candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_absolute() && path.is_file())
}

/// Starts `command` as every program here starts: no window, nothing on its input, its
/// standard output piped, its error output as `stderr` says. On Windows it starts in the
/// system folder unless told otherwise, as the C# `SystemExecutablePath` sets it; on Linux
/// its messages are untranslated.
fn start(mut command: Command, stderr: Stdio) -> Result<Child, ProgramError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(stderr);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(CREATE_NO_WINDOW);
        if command.get_current_dir().is_none()
            && let Some(folder) = crate::paths::system_dir()
        {
            command.current_dir(folder);
        }
    }
    #[cfg(target_os = "linux")]
    command.env(LOCALE_VARIABLE, UNTRANSLATED_LOCALE);
    command
        .spawn()
        .map_err(|error| ProgramError::NotStarted(error.to_string()))
}

/// How a program started by [`launch_detached`] shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    /// As it shows itself: a window program its windows, a console program a console of its
    /// own on Windows, as the C# `UseShellExecute` starts it.
    Own,
    /// A console program without its console window, as the C# `RunHidden`. A window
    /// program still shows its windows: Windows lets a program choose.
    Hidden,
}

/// Starts `command`, a program the user chose, and leaves it running: nothing on its
/// standard input, output and error but what it inherits, so that a console program writes
/// in its own console; no time limit; a thread of its own reaps it when it ends, which the
/// log says. Its process identifier.
///
/// # Errors
///
/// [`ProgramError::NotStarted`], with the system's words.
pub fn launch_detached(mut command: Command, window: Window) -> Result<u32, ProgramError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(match window {
            Window::Own => CREATE_NEW_CONSOLE,
            Window::Hidden => CREATE_NO_WINDOW,
        });
    }
    #[cfg(not(windows))]
    let _ = window;
    let mut child = command
        .spawn()
        .map_err(|error| ProgramError::NotStarted(error.to_string()))?;
    let id = child.id();
    thread::spawn(move || match child.wait() {
        Ok(status) => log::info!("process {id} ended: {status}"),
        Err(error) => log::warn!("process {id} could not be waited for: {error}"),
    });
    Ok(id)
}

/// Ends `child` and reaps it. Already ended, the kill fails: the wait still reaps it.
fn end(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Runs `command` (see [`start`]), its outputs read as they are written, within `limit`:
/// past it, the command is killed.
///
/// # Errors
///
/// [`ProgramError::NotStarted`] or [`ProgramError::TimedOut`].
pub fn run(command: Command, limit: Duration) -> Result<Finished, ProgramError> {
    let mut child = start(command, Stdio::piped())?;
    let stdout = reader(child.stdout.take());
    let stderr = reader(child.stderr.take());
    match wait_within(&mut child, limit, POLL_INTERVAL) {
        Ok(Some(status)) => Ok(Finished {
            success: status.success(),
            code: status.code(),
            stdout: stdout.join().unwrap_or_default(),
            stderr: stderr.join().unwrap_or_default(),
        }),
        // The readers end with the outputs, once whatever still holds them is gone: they
        // are not waited for.
        Ok(None) => Err(ProgramError::TimedOut),
        Err(error) => Err(ProgramError::NotStarted(error.to_string())),
    }
}

/// A thread reading at most [`MAX_OUTPUT_BYTES`] of `output`, so that a full pipe never
/// holds the program.
fn reader<R: std::io::Read + Send + 'static>(output: Option<R>) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(output) = output {
            let _ = output.take(MAX_OUTPUT_BYTES).read_to_end(&mut bytes);
        }
        bytes
    })
}

/// Runs `program` with `args` (see [`start`]), its error output discarded, handing each
/// line it prints to `on_line` as it comes, decoded as [`decode_text`] does; ends it when
/// `stop` is set or `deadline` passes, as the C# kills `arp` after its 5 seconds. Whatever
/// its exit code, a program that ended by itself has [`RunEnd::Exited`]: what it printed
/// is what is read.
///
/// # Errors
///
/// [`ProgramError::NotStarted`], or [`ProgramError::TimedOut`] once it was ended past its
/// deadline.
pub fn run_lines(
    program: &Path,
    args: &[&str],
    deadline: Duration,
    stop: &AtomicBool,
    mut on_line: impl FnMut(&str),
) -> Result<RunEnd, ProgramError> {
    let started = Instant::now();
    let mut command = Command::new(program);
    command.args(args);
    let mut child = start(command, Stdio::null())?;
    let Some(stdout) = child.stdout.take() else {
        end(&mut child);
        return Err(ProgramError::NotStarted(String::new()));
    };
    let (lines, read) = mpsc::channel::<Vec<u8>>();
    // Its own thread, as a read blocks: the loop below keeps watching the stop and the
    // deadline. It ends with the program's output, ended or killed.
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.split(b'\n').take(MAX_OUTPUT_LINES) {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    loop {
        if stop.load(Ordering::Relaxed) || started.elapsed() >= deadline {
            end(&mut child);
            return stopped_or_timed_out(stop);
        }
        match read.recv_timeout(POLL_INTERVAL) {
            Ok(line) => on_line(decode_text(&line).trim_end_matches('\r')),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    // The output is over: the program is given what is left of its deadline to exit.
    while started.elapsed() < deadline && !stop.load(Ordering::Relaxed) {
        if !matches!(child.try_wait(), Ok(None)) {
            return Ok(RunEnd::Exited);
        }
        thread::sleep(POLL_INTERVAL);
    }
    end(&mut child);
    stopped_or_timed_out(stop)
}

/// How a program ended before its time ended: stopped when the stop was asked, the stop
/// winning over a deadline past at the same time; else past its deadline.
fn stopped_or_timed_out(stop: &AtomicBool) -> Result<RunEnd, ProgramError> {
    if stop.load(Ordering::Relaxed) {
        Ok(RunEnd::Stopped)
    } else {
        Err(ProgramError::TimedOut)
    }
}

/// Runs `script` in Windows `PowerShell` by its full path in the system folder, started
/// there, the inherited module path dropped, after [`POWERSHELL_PRELUDE`]; its standard
/// output, which the script writes in UTF-8.
///
/// # Errors
///
/// Windows does not say where its system folder is, or the script could not be started,
/// did not end in time, or failed.
#[cfg(windows)]
pub fn run_powershell(script: &str, limit: Duration) -> Result<String, ProgramError> {
    let folder = crate::paths::system_dir().ok_or(ProgramError::NotAvailable)?;
    let mut command = Command::new(folder.join(POWERSHELL_PROGRAM));
    command
        .args(POWERSHELL_SWITCHES)
        .arg(COMMAND_SWITCH)
        .arg(format!("{POWERSHELL_PRELUDE}{script}"))
        .current_dir(&folder)
        .env_remove(MODULE_PATH_VARIABLE);
    let finished = run(command, limit)?;
    if !finished.success {
        return Err(finished.failure());
    }
    Ok(decode_text(&finished.stdout))
}

/// Runs the Linux program `program` with `arguments`, started in the root folder, its
/// messages untranslated; its standard output.
///
/// # Errors
///
/// The program could not be started, did not end in time, or failed.
#[cfg(target_os = "linux")]
pub fn run_linux(
    program: &Path,
    arguments: &[&str],
    limit: Duration,
) -> Result<String, ProgramError> {
    let mut command = Command::new(program);
    command.args(arguments).current_dir("/");
    let finished = run(command, limit)?;
    if !finished.success {
        return Err(finished.failure());
    }
    Ok(decode_text(&finished.stdout))
}

/// The text of a program's output: UTF-8 when it is (its byte order mark left out), else
/// the system's OEM code page, the one a Windows console program writes in when its output
/// is a pipe (cp850 on a French Windows, cp437 on an American one), decoded by Windows
/// itself. Elsewhere, as code page 850.
#[must_use]
pub fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    #[cfg(windows)]
    {
        use winsafe::co;
        if let Ok(units) = winsafe::MultiByteToWideChar(co::CP::OEMCP, co::MBC::NoValue, bytes) {
            return String::from_utf16_lossy(&units);
        }
    }
    // Elsewhere, or when Windows cannot say: code page 850, the OEM code page of Western
    // European Windows, never a replacement character in place of a letter.
    crate::code_page::decode_cp850(bytes)
}

/// `text` on one line: its lines trimmed, the empty ones left out, the others joined by a
/// space.
#[must_use]
pub fn one_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    use super::*;

    /// A shell, or Windows' command interpreter, by its whole path.
    fn shell() -> (PathBuf, &'static str) {
        if cfg!(windows) {
            (system_tool("cmd.exe", "sh").expect("cmd"), "/c")
        } else {
            (system_tool("cmd.exe", "sh").expect("sh"), "-c")
        }
    }

    #[test]
    fn a_program_is_found_by_its_whole_path_and_its_lines_read_as_they_come() {
        let (program, flag) = shell();
        assert!(program.is_absolute());
        let stop = AtomicBool::new(false);
        let mut lines = Vec::new();
        let end = run_lines(
            &program,
            &[flag, "echo one&& echo two"],
            Duration::from_secs(30),
            &stop,
            |line| lines.push(line.trim().to_owned()),
        );
        assert_eq!(end, Ok(RunEnd::Exited));
        assert_eq!(lines, ["one", "two"]);
        assert!(system_tool("no-such-tool.exe", "no-such-tool").is_none());
    }

    #[test]
    fn a_program_past_its_deadline_or_stopped_is_ended() {
        let (program, flag) = shell();
        // A program printing a line and then waiting long.
        let waiting = if cfg!(windows) {
            "echo started&& ping -n 30 127.0.0.1 > NUL"
        } else {
            "echo started; sleep 30"
        };
        let stop = AtomicBool::new(false);
        let started = Instant::now();
        let mut lines = Vec::new();
        // Long enough for a shell to start on a busy machine, far below the wait.
        let end = run_lines(
            &program,
            &[flag, waiting],
            Duration::from_secs(5),
            &stop,
            |line| lines.push(line.trim().to_owned()),
        );
        assert_eq!(end, Err(ProgramError::TimedOut));
        assert_eq!(lines, ["started"]);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "not waited for"
        );

        let stop = AtomicBool::new(true);
        let started = Instant::now();
        let end = run_lines(
            &program,
            &[flag, waiting],
            Duration::from_secs(60),
            &stop,
            |_| {},
        );
        assert_eq!(end, Ok(RunEnd::Stopped));
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "not waited for"
        );
    }

    #[test]
    fn a_program_that_cannot_start_says_so_with_the_systems_words() {
        let stop = AtomicBool::new(false);
        let missing = std::env::temp_dir().join("heimdall-no-such-program");
        let end = run_lines(&missing, &[], Duration::from_secs(1), &stop, |_| {});
        assert!(
            matches!(&end, Err(ProgramError::NotStarted(detail)) if !detail.is_empty()),
            "{end:?}"
        );
        assert!(matches!(
            run(Command::new(missing), Duration::from_secs(30)),
            Err(ProgramError::NotStarted(_))
        ));
    }

    #[cfg(not(windows))]
    #[test]
    fn a_line_that_is_not_utf8_is_read_in_the_console_code_page() {
        let (program, flag) = shell();
        let stop = AtomicBool::new(false);
        let mut lines = Vec::new();
        // "réseau" as a French Windows console writes it: 0x82 is "é" in cp850.
        let end = run_lines(
            &program,
            &[flag, r"printf 'r\202seau\n'"],
            Duration::from_secs(30),
            &stop,
            |line| lines.push(line.to_owned()),
        );
        assert_eq!(end, Ok(RunEnd::Exited));
        assert_eq!(lines, ["r\u{e9}seau"]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn every_linux_program_runs_untranslated() {
        let (program, flag) = shell();
        let stop = AtomicBool::new(false);
        let mut lines = Vec::new();
        let end = run_lines(
            &program,
            &[flag, "echo $LC_ALL"],
            Duration::from_secs(30),
            &stop,
            |line| lines.push(line.to_owned()),
        );
        assert_eq!(end, Ok(RunEnd::Exited));
        assert_eq!(lines, ["C"]);
        let output = run_linux(&program, &[flag, "echo $LC_ALL"], Duration::from_secs(30));
        assert_eq!(output.as_deref().map(str::trim), Ok("C"));
    }

    #[test]
    fn utf8_output_is_read_as_is_its_byte_order_mark_left_out() {
        let name = "Nom de l\u{2019}interface\u{a0}: R\u{e9}seau";
        assert_eq!(decode_text(name.as_bytes()), name);
        assert_eq!(decode_text(b"\xEF\xBB\xBFok"), "ok");
    }

    #[test]
    fn output_that_is_not_utf8_never_fails_to_decode() {
        // "réseau" in cp850, as netsh writes it on a French console: 0x82 is "é".
        let text = decode_text(b"Type de r\x82seau : Infrastructure");
        // In the system's OEM code page on Windows (cp437 and cp850 both read 0x82 as
        // "é"), in cp850 elsewhere: never U+FFFD in place of a letter.
        assert_eq!(text, "Type de r\u{e9}seau : Infrastructure");
    }

    #[test]
    fn a_failure_is_said_with_the_programs_words_on_one_line() {
        let finished = Finished {
            success: false,
            code: Some(1),
            stdout: b"\r\nThe Wireless AutoConfig Service (wlansvc) is not running.\r\n\r\n"
                .to_vec(),
            stderr: Vec::new(),
        };
        assert_eq!(
            finished.failure(),
            ProgramError::Failed {
                code: Some(1),
                detail: "The Wireless AutoConfig Service (wlansvc) is not running.".to_owned()
            }
        );
        let finished = Finished {
            stderr: b"Access denied\r\nmore\r\n".to_vec(),
            ..finished
        };
        assert_eq!(
            finished.failure(),
            ProgramError::Failed {
                code: Some(1),
                detail: "Access denied more".to_owned()
            }
        );
    }

    #[test]
    fn a_program_launched_is_left_running_and_one_missing_says_why() {
        let (program, flag) = shell();
        let marker = tempfile::tempdir().expect("dir");
        let file = marker.path().join("ran");
        let mut command = Command::new(&program);
        command.arg(flag);
        // As `cmd.exe` reads its line: as written, not quoted for the C runtime.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            command.raw_arg(format!("echo ran> \"{}\"", file.display()));
        }
        #[cfg(not(windows))]
        command.arg(format!("echo ran > '{}'", file.display()));
        let id = launch_detached(command, Window::Hidden).expect("started");
        assert!(id > 0);
        let started = Instant::now();
        while !file.exists() && started.elapsed() < Duration::from_secs(20) {
            thread::sleep(POLL_INTERVAL);
        }
        assert!(file.exists(), "it ran");
        let missing = std::env::temp_dir().join("heimdall-no-such-program");
        assert!(matches!(
            launch_detached(Command::new(missing), Window::Own),
            Err(ProgramError::NotStarted(detail)) if !detail.is_empty()
        ));
    }

    #[test]
    fn a_program_is_only_found_by_its_full_path() {
        assert_eq!(find_program(&["netsh.exe", "ip"]), None);
        let missing = std::env::temp_dir().join("heimdall-no-such-program");
        assert_eq!(find_program(&[missing.to_str().expect("text")]), None);
    }

    #[test]
    fn a_program_past_its_time_is_killed_and_said_to_have_timed_out() {
        #[cfg(windows)]
        let command = {
            let mut command =
                Command::new(crate::paths::system_program("ping.exe").expect("the system folder"));
            command.args(["-n", "30", "127.0.0.1"]);
            command
        };
        #[cfg(not(windows))]
        let command = {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", "sleep 30"]);
            command
        };
        let started = Instant::now();
        assert_eq!(
            run(command, Duration::from_millis(300)),
            Err(ProgramError::TimedOut)
        );
        assert!(started.elapsed() < Duration::from_secs(20));
    }
}
