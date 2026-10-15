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

//! What the network tools of the third batch share: a host checked before it reaches a
//! probe or a command line, as the C# `InputValidator.Validate(host, "Address")`; a system
//! program run by its whole path, with a deadline and a stop; probes run a bounded number
//! at a time, as the C# `SemaphoreSlim` around its probes; and a stop that holds while a
//! probe waits.

use std::io::{BufRead as _, BufReader};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

/// The longest DNS name, in characters (RFC 1035 section 3.1, without the root's dot).
pub const MAX_HOST_NAME_LEN: usize = 253;

/// How often a running program or a waiting probe looks at its stop and its deadline.
pub const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// The most lines read from a program: what a traceroute or an ARP table prints is far
/// below, and a program that never ends its output is cut there.
pub const MAX_OUTPUT_LINES: usize = 10_000;

/// Where a Unix system program is looked for, in order, as the files crate's privileged
/// commands do: never the `PATH` whoever starts Heimdall sets.
const UNIX_SYSTEM_FOLDERS: [&str; 4] = ["/usr/sbin", "/usr/bin", "/sbin", "/bin"];

/// Character sequences never valid inside a DNS name, as the C# `InvalidDnsSequences`.
const INVALID_DNS_SEQUENCES: [&str; 3] = ["..", ".-", "-."];

/// Whether `host` is an IP address or a DNS name, as the C# `"Address"` pattern accepts:
/// letters, digits, hyphens and dots, starting with a letter or a digit and ending with
/// neither a dot nor a hyphen. A host starting with `-` is refused, so it is never read as
/// an option by the program it is given to.
#[must_use]
pub fn is_valid_address(host: &str) -> bool {
    if host.parse::<IpAddr>().is_ok() {
        return true;
    }
    let Some(first) = host.chars().next() else {
        return false;
    };
    host.len() <= MAX_HOST_NAME_LEN
        && first.is_ascii_alphanumeric()
        && !host.ends_with(['.', '-'])
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        && !INVALID_DNS_SEQUENCES
            .iter()
            .any(|sequence| host.contains(sequence))
}

/// A system program by its whole path: `windows_name` in Windows' system folder, as
/// Windows says where it is; elsewhere `unix_name` in the first of the system folders
/// holding it. `None` when there is none.
#[must_use]
pub fn system_tool(windows_name: &str, unix_name: &str) -> Option<PathBuf> {
    if cfg!(windows) {
        crate::paths::system_program(windows_name)
    } else {
        UNIX_SYSTEM_FOLDERS
            .iter()
            .map(|folder| Path::new(folder).join(unix_name))
            .find(|program| program.is_file())
    }
}

/// How a program run by [`run_lines`] ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    /// It ended by itself, its output read whole.
    Exited,
    /// The stop was asked: it was ended.
    Stopped,
    /// Its deadline passed: it was ended.
    TimedOut,
    /// It could not be started.
    NotStarted,
}

/// Runs `program` with `args`, no window, no input and its errors discarded, handing each
/// line it prints to `on_line` as it comes; ends it when `stop` is set or `deadline` passes,
/// as the C# kills `arp` after its 5 seconds. The working folder is the system folder on
/// Windows, as the C# `SystemExecutablePath` sets it.
pub fn run_lines(
    program: &Path,
    args: &[&str],
    deadline: Duration,
    stop: &AtomicBool,
    mut on_line: impl FnMut(&str),
) -> RunEnd {
    let started = Instant::now();
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
        if let Some(folder) = crate::paths::system_dir() {
            command.current_dir(folder);
        }
    }
    let Ok(mut child) = command.spawn() else {
        return RunEnd::NotStarted;
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return RunEnd::NotStarted;
    };
    let (lines, read) = mpsc::channel::<Vec<u8>>();
    // Its own thread, as a read blocks: the loop below keeps watching the stop and the
    // deadline. It ends with the program's output, ended or killed.
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.split(b'\n').take(MAX_OUTPUT_LINES) {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    let end = loop {
        if stop.load(Ordering::Relaxed) {
            break RunEnd::Stopped;
        }
        if started.elapsed() >= deadline {
            break RunEnd::TimedOut;
        }
        match read.recv_timeout(POLL_INTERVAL) {
            Ok(line) => {
                let text = String::from_utf8_lossy(&line);
                on_line(text.trim_end_matches('\r'));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break RunEnd::Exited,
        }
    };
    if end == RunEnd::Exited {
        // The output is over: the program is given what is left of its deadline to exit.
        while started.elapsed() < deadline && !stop.load(Ordering::Relaxed) {
            if !matches!(child.try_wait(), Ok(None)) {
                return RunEnd::Exited;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    if end == RunEnd::Exited {
        if stop.load(Ordering::Relaxed) {
            RunEnd::Stopped
        } else {
            RunEnd::TimedOut
        }
    } else {
        end
    }
}

/// Runs `work` on every one of `jobs`, `at_once` at most at the same time, handing each
/// result to `on_result` on the calling thread as it comes, as the C# runs its probes
/// behind a `SemaphoreSlim`. Once `stop` is set no job starts; those running finish.
pub fn for_each_bounded<J, R>(
    jobs: Vec<J>,
    at_once: usize,
    stop: &AtomicBool,
    work: impl Fn(J) -> R + Sync,
    mut on_result: impl FnMut(R),
) where
    J: Send,
    R: Send,
{
    let workers = at_once.clamp(1, jobs.len().max(1));
    let queue = Mutex::new(jobs.into_iter());
    let (results, received) = mpsc::channel();
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let results = results.clone();
            let queue = &queue;
            let work = &work;
            scope.spawn(move || {
                loop {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let next = queue.lock().unwrap_or_else(PoisonError::into_inner).next();
                    let Some(job) = next else { break };
                    if results.send(work(job)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(results);
        for result in received {
            on_result(result);
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use super::{RunEnd, for_each_bounded, is_valid_address, run_lines, system_tool};

    #[test]
    fn an_address_is_an_ip_or_a_dns_name_never_an_option() {
        for valid in [
            "10.0.0.1",
            "2001:db8::1",
            "::1",
            "files.lab",
            "srv-01.corp.example.com",
            "1e100.net",
            "localhost",
        ] {
            assert!(is_valid_address(valid), "{valid}");
        }
        for refused in [
            "",
            "-n",
            "--help",
            "host name",
            "a..b",
            "a.-b",
            "a-.b",
            "host.",
            "host-",
            ".host",
            "h\u{e9}te.lab",
            "host;rm",
            "host\u{0}",
        ] {
            assert!(!is_valid_address(refused), "{refused:?}");
        }
        assert!(!is_valid_address(&"a".repeat(super::MAX_HOST_NAME_LEN + 1)));
    }

    #[test]
    fn bounded_jobs_never_run_more_than_asked_and_all_results_come_back() {
        let running = AtomicUsize::new(0);
        let most = AtomicUsize::new(0);
        let stop = AtomicBool::new(false);
        let mut seen = Vec::new();
        for_each_bounded(
            (0..40).collect(),
            5,
            &stop,
            |job: u32| {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                most.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                running.fetch_sub(1, Ordering::SeqCst);
                job * 2
            },
            |result| seen.push(result),
        );
        seen.sort_unstable();
        assert_eq!(seen, (0..40).map(|job| job * 2).collect::<Vec<_>>());
        assert!(most.load(Ordering::SeqCst) <= 5, "five at once at most");
        assert!(most.load(Ordering::SeqCst) > 1, "more than one at once");
    }

    #[test]
    fn once_stopped_no_bounded_job_starts() {
        let stop = AtomicBool::new(false);
        let started = AtomicUsize::new(0);
        let mut results = 0;
        for_each_bounded(
            (0..1000).collect::<Vec<u32>>(),
            2,
            &stop,
            |_| {
                // The third job asks the stop: the other worker may be in its own job.
                if started.fetch_add(1, Ordering::SeqCst) + 1 == 3 {
                    stop.store(true, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(2));
            },
            |()| results += 1,
        );
        let started = started.load(Ordering::SeqCst);
        assert!(
            started <= 4,
            "the running ones finish, no more start: {started}"
        );
        assert_eq!(results, started, "every job started is handed back");
    }

    /// A shell, or Windows' command interpreter, by its whole path.
    fn shell() -> (std::path::PathBuf, &'static str) {
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
        assert_eq!(end, RunEnd::Exited);
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
        let end = run_lines(
            &program,
            &[flag, waiting],
            Duration::from_millis(500),
            &stop,
            |line| lines.push(line.trim().to_owned()),
        );
        assert_eq!(end, RunEnd::TimedOut);
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
        assert_eq!(end, RunEnd::Stopped);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "not waited for"
        );
    }

    #[test]
    fn a_program_that_cannot_start_says_so() {
        let stop = AtomicBool::new(false);
        let end = run_lines(
            std::path::Path::new("/no/such/program"),
            &[],
            Duration::from_secs(1),
            &stop,
            |_| {},
        );
        assert_eq!(end, RunEnd::NotStarted);
    }
}
