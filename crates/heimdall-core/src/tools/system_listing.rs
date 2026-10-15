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

//! How the OPENPORTS, NETIF, ROUTES and WIFI tools read the system: what they run, how
//! long it may take, how its output is decoded and how a failure is told.
//!
//! The C# reads these tables through Win32 and .NET calls (`GetExtendedTcpTable`,
//! `NetworkInterface.GetAllNetworkInterfaces`); without FFI the same facts come from a
//! structured source instead: Windows `PowerShell` writing JSON, `ip -j`, `ss`, `nmcli -t`.
//! Never the text a console program words in the user's language, except `netsh`, which
//! has nothing structured for the visible Wi-Fi networks.
//!
//! Every program is named by its full path (the system folder on Windows, fixed folders
//! elsewhere, never the `PATH`), runs without a window and nothing on its input, within a
//! time limit past which it is killed. A program that fails is said to have failed, with
//! its own words: never an empty list.

use std::io::Read as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Deserializer};
use thiserror::Error;

use super::default_creds::escape_csv;
use super::service_status::wait_within;

/// How long a listing may take, as the C# services list's `CancelAfter(30s)`: Windows
/// `PowerShell` takes a few seconds to start on a busy machine.
pub const LIST_TIME_LIMIT: Duration = Duration::from_secs(30);

/// How long the Wi-Fi scan may take, as the C# `WifiScanService.ProcessTimeoutMs`.
pub const WIFI_TIME_LIMIT: Duration = Duration::from_secs(10);

/// How often a running program is looked at.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// The most of a program's output read, each stream: a table of every connection of a busy
/// server is a few megabytes of JSON at most. Past it the pipe is closed, the program ends
/// or runs out of time.
pub const MAX_OUTPUT_BYTES: u64 = 32 * 1024 * 1024;

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

/// Where `ip` is looked for: by its full path, never on the `PATH`.
#[cfg(target_os = "linux")]
pub const IP_CANDIDATES: [&str; 4] = ["/usr/sbin/ip", "/sbin/ip", "/usr/bin/ip", "/bin/ip"];

/// The UTF-8 byte order mark, which a program may write first.
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// The variable naming the language of a Linux program's messages: set to `C`, so that what
/// is read is never translated.
#[cfg(target_os = "linux")]
const LOCALE_VARIABLE: &str = "LC_ALL";

/// The untranslated locale.
#[cfg(target_os = "linux")]
const UNTRANSLATED_LOCALE: &str = "C";

/// Why a table could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ListingError {
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

/// What a program that ended in time said.
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
    pub fn failure(&self) -> ListingError {
        let mut detail = one_line(&decode_text(&self.stderr));
        if detail.is_empty() {
            detail = one_line(&decode_text(&self.stdout));
        }
        ListingError::Failed {
            code: self.code,
            detail,
        }
    }
}

/// Runs `command` without a window, nothing on its input, its outputs read as they are
/// written, within `limit`: past it, the command is killed.
///
/// # Errors
///
/// The command could not be started, or did not end in time.
pub fn run(mut command: Command, limit: Duration) -> Result<Finished, ListingError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The process gets no console window, as the C# `CreateNoWindow`.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| ListingError::NotStarted(error.to_string()))?;
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
        Ok(None) => Err(ListingError::TimedOut),
        Err(error) => Err(ListingError::NotStarted(error.to_string())),
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

/// Runs `script` in Windows `PowerShell` by its full path in the system folder, started
/// there, the inherited module path dropped, after [`POWERSHELL_PRELUDE`]; its standard
/// output, which the script writes in UTF-8.
///
/// # Errors
///
/// Windows does not say where its system folder is, or the script could not be started,
/// did not end in time, or failed.
#[cfg(windows)]
pub fn run_powershell(script: &str, limit: Duration) -> Result<String, ListingError> {
    let folder = crate::paths::system_dir().ok_or(ListingError::NotAvailable)?;
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

/// Runs the Linux program `program` with `arguments`, its messages untranslated; its
/// standard output.
///
/// # Errors
///
/// The program could not be started, did not end in time, or failed.
#[cfg(target_os = "linux")]
pub fn run_linux(
    program: &std::path::Path,
    arguments: &[&str],
    limit: Duration,
) -> Result<String, ListingError> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .env(LOCALE_VARIABLE, UNTRANSLATED_LOCALE)
        .current_dir("/");
    let finished = run(command, limit)?;
    if !finished.success {
        return Err(finished.failure());
    }
    Ok(decode_text(&finished.stdout))
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

/// The text of a program's output: UTF-8 when it is (its byte order mark left out), else
/// the system's OEM code page, the one a Windows console program writes in when its output
/// is a pipe (cp850 on a French Windows, cp437 on an American one), decoded by Windows
/// itself. Elsewhere, bytes that are not UTF-8 are read as U+FFFD.
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
    String::from_utf8_lossy(bytes).into_owned()
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

/// `json` read as `T`.
///
/// # Errors
///
/// [`ListingError::Unreadable`], with why.
pub fn parse_json<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, ListingError> {
    serde_json::from_str(json.trim()).map_err(|error| ListingError::Unreadable(error.to_string()))
}

/// A list as `ConvertTo-Json` writes one: an array, a lone object when the list has one
/// element, nothing (`null`) when it is empty.
#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    Many(Vec<T>),
    One(T),
}

/// Reads a list written by `ConvertTo-Json`, which writes a list of one as its element and
/// an empty one as `null`, into a `Vec`.
///
/// # Errors
///
/// The value is neither a list, nor an element, nor `null`.
pub fn one_or_many<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(match Option::<OneOrMany<T>>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(OneOrMany::Many(items)) => items,
        Some(OneOrMany::One(item)) => vec![item],
    })
}

/// Reads a text `ConvertTo-Json` may write as `null` into a `String`, empty for `null`.
///
/// # Errors
///
/// The value is neither a text nor `null`.
pub fn text_or_null<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

/// A CSV document: `headers`, then each of `rows`, every cell quoted and made unable to
/// start a spreadsheet formula, as the C# `InputValidator.SanitizeCsvCell`: an SSID, a
/// process or an interface name is chosen by someone else.
#[must_use]
pub fn csv_document<I>(headers: &[String], rows: I) -> String
where
    I: IntoIterator<Item = Vec<String>>,
{
    let line = |cells: &[String]| {
        cells
            .iter()
            .map(|cell| escape_csv(cell))
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut csv = line(headers);
    csv.push('\n');
    for row in rows {
        csv.push_str(&line(&row));
        csv.push('\n');
    }
    csv
}

/// The IPv4 mask of a prefix of `length` bits, dotted (`24` is `255.255.255.0`); `None`
/// past 32.
#[must_use]
pub fn ipv4_mask(length: u8) -> Option<std::net::Ipv4Addr> {
    const IPV4_BITS: u8 = 32;
    if length > IPV4_BITS {
        return None;
    }
    let bits = u32::MAX
        .checked_shl(u32::from(IPV4_BITS - length))
        .unwrap_or(0);
    Some(std::net::Ipv4Addr::from(bits))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Listed {
        #[serde(default, deserialize_with = "one_or_many")]
        rows: Vec<u32>,
    }

    #[test]
    fn a_list_reads_whether_powershell_wrote_it_whole_alone_or_null() {
        let rows = |json: &str| parse_json::<Listed>(json).map(|listed| listed.rows);
        assert_eq!(rows(r#"{"rows":[1,2]}"#), Ok(vec![1, 2]));
        assert_eq!(rows(r#"{"rows":7}"#), Ok(vec![7]));
        assert_eq!(rows(r#"{"rows":null}"#), Ok(vec![]));
        assert_eq!(rows("{}"), Ok(vec![]));
        assert!(matches!(rows("not json"), Err(ListingError::Unreadable(_))));
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
        assert!(text.starts_with("Type de r"));
        assert!(text.ends_with("seau : Infrastructure"));
        #[cfg(windows)]
        {
            // Decoded by Windows in the system's OEM code page: never U+FFFD in place of
            // a letter (cp437 and cp850 both read 0x82 as "é").
            assert!(!text.contains('\u{FFFD}'), "{text}");
        }
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
            ListingError::Failed {
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
            ListingError::Failed {
                code: Some(1),
                detail: "Access denied more".to_owned()
            }
        );
    }

    #[test]
    fn every_csv_cell_is_quoted_and_a_formula_defused() {
        let csv = csv_document(
            &["SSID".to_owned(), "Signal".to_owned()],
            [vec!["=HYPERLINK(\"x\")".to_owned(), "80%".to_owned()]],
        );
        assert_eq!(
            csv,
            "\"SSID\",\"Signal\"\n\"'=HYPERLINK(\"\"x\"\")\",\"80%\"\n"
        );
    }

    #[test]
    fn a_prefix_length_gives_its_dotted_mask() {
        assert_eq!(ipv4_mask(0), Some(std::net::Ipv4Addr::new(0, 0, 0, 0)));
        assert_eq!(
            ipv4_mask(20),
            Some(std::net::Ipv4Addr::new(255, 255, 240, 0))
        );
        assert_eq!(
            ipv4_mask(32),
            Some(std::net::Ipv4Addr::new(255, 255, 255, 255))
        );
        assert_eq!(ipv4_mask(33), None);
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
        let started = std::time::Instant::now();
        assert_eq!(
            run(command, Duration::from_millis(300)),
            Err(ListingError::TimedOut)
        );
        assert!(started.elapsed() < Duration::from_secs(20));
    }

    #[test]
    fn a_program_that_cannot_start_is_said_so() {
        let missing = std::env::temp_dir().join("heimdall-no-such-program");
        assert!(matches!(
            run(Command::new(missing), LIST_TIME_LIMIT),
            Err(ListingError::NotStarted(_))
        ));
    }
}
