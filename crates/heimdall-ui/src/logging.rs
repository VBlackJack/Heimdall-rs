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

//! A log file and crash reports, with no dependency beyond the `log` facade.
//!
//! A release build has no console on Windows, so the log file and the crash report are
//! the only traces a user can send. What reaches them is what the code formats: messages,
//! answers, typed input and clipboard text have redacted `Debug` forms, and nothing logs
//! terminal output. Levels above `info` for other crates (russh logs packets at `trace`)
//! are opt-in through `HEIMDALL_LOG`.

use std::backtrace::Backtrace;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::panic::{self, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use log::Level;
use log::{LevelFilter, Log, Metadata, Record};

/// Name of the log file.
const LOG_FILE_NAME: &str = "heimdall.log";

/// Name the previous log file is kept under.
const PREVIOUS_LOG_FILE_NAME: &str = "heimdall.log.1";

/// Size past which the log file is set aside at start.
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// Environment variable choosing the level of every crate: `error` to `trace`.
const LEVEL_VARIABLE: &str = "HEIMDALL_LOG";

/// Prefix of the crates of this workspace.
const OWN_TARGET_PREFIX: &str = "heimdall";

/// Owner read and write only, for the log and crash files on Unix.
#[cfg(unix)]
const PRIVATE_FILE_MODE: u32 = 0o600;

/// Longest panic message kept in a crash report, in characters.
const MAX_PANIC_MESSAGE_CHARS: usize = 1024;

/// Whether the log is written, as the settings' diagnostics log switch says: on until they
/// are read. Crash reports are written whatever it says, as they are the trace of a crash.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// Writes the log, or stops writing it, from now on.
pub fn set_enabled(on: bool) {
    let was = ENABLED.swap(on, Ordering::Relaxed);
    if was && !on {
        log::logger().flush();
    }
}

struct FileLogger {
    file: Mutex<File>,
    own: LevelFilter,
    others: LevelFilter,
}

/// The most verbose level written for `target`.
fn limit_for(own: LevelFilter, others: LevelFilter, target: &str) -> LevelFilter {
    if target.starts_with(OWN_TARGET_PREFIX) {
        own
    } else {
        others
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= limit_for(self.own, self.others, metadata.target())
    }

    fn log(&self, record: &Record<'_>) {
        if !ENABLED.load(Ordering::Relaxed) || !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} {}: {}\n",
            timestamp(),
            record.level(),
            record.target(),
            record.args()
        );
        if let Ok(mut file) = self.file.lock() {
            // A log that cannot be written must not take the application down.
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

/// Seconds and milliseconds since the Unix epoch, UTC.
fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}", now.as_secs(), now.subsec_millis())
}

/// Levels for the workspace's crates and for the others, from `HEIMDALL_LOG`.
fn levels(requested: Option<&str>) -> (LevelFilter, LevelFilter) {
    match requested.and_then(|value| value.trim().parse::<LevelFilter>().ok()) {
        Some(level) => (level, level),
        None => (LevelFilter::Info, LevelFilter::Warn),
    }
}

/// Options creating a file only its owner can read: host names, user names and paths
/// are in the log.
fn private_file() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(PRIVATE_FILE_MODE);
    }
    options
}

fn open_log(dir: &Path) -> std::io::Result<File> {
    fs::create_dir_all(dir)?;
    let path = dir.join(LOG_FILE_NAME);
    if fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_LOG_BYTES) {
        // Another instance may hold the file open (Windows): keep appending rather than
        // run without a log.
        let _ = fs::rename(&path, dir.join(PREVIOUS_LOG_FILE_NAME));
    }
    private_file().append(true).open(path)
}

/// Starts logging to `dir` and writes crash reports there. Without a directory, or when
/// the file cannot be opened, the application runs without a log.
pub fn init(dir: Option<PathBuf>) {
    let Some(dir) = dir else {
        return;
    };
    let (own, others) = levels(std::env::var(LEVEL_VARIABLE).ok().as_deref());
    if let Ok(file) = open_log(&dir) {
        let logger = FileLogger {
            file: Mutex::new(file),
            own,
            others,
        };
        if log::set_boxed_logger(Box::new(logger)).is_ok() {
            log::set_max_level(own.max(others));
        }
    }
    install_panic_hook(dir);
    log::info!(
        "Heimdall {} started on {} {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
}

fn install_panic_hook(dir: PathBuf) {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let report = crash_report(info);
        log::error!("panic: {}", first_line(&report));
        let path = dir.join(format!("crash-{}.txt", timestamp().replace('.', "-")));
        if let Ok(mut file) = private_file().write(true).truncate(true).open(path) {
            let _ = file.write_all(report.as_bytes());
        }
        previous(info);
    }));
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

/// The panic message, printable characters only and bounded.
fn panic_message(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    let raw = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or_default();
    printable(raw)
}

/// `raw` without control characters other than line feeds, bounded.
fn printable(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(MAX_PANIC_MESSAGE_CHARS)
        .collect()
}

fn crash_report(info: &PanicHookInfo<'_>) -> String {
    let mut report = String::new();
    let location = info
        .location()
        .map(|at| format!("{}:{}:{}", at.file(), at.line(), at.column()))
        .unwrap_or_default();
    let _ = writeln!(
        report,
        "Heimdall {} crashed at {location}",
        env!("CARGO_PKG_VERSION")
    );
    let _ = writeln!(
        report,
        "thread: {}",
        std::thread::current().name().unwrap_or_default()
    );
    let _ = writeln!(report, "time: {}", timestamp());
    let _ = writeln!(report, "message: {}", panic_message(info));
    let _ = writeln!(report, "\n{}", Backtrace::force_capture());
    report
}

/// Whether `level` is written for `target` under `HEIMDALL_LOG=requested`.
#[cfg(test)]
fn written(requested: Option<&str>, target: &str, level: Level) -> bool {
    let (own, others) = levels(requested);
    level <= limit_for(own, others, target)
}

#[cfg(test)]
mod tests {
    use log::Level;

    use super::{MAX_PANIC_MESSAGE_CHARS, first_line, printable, written};

    #[test]
    fn by_default_other_crates_log_only_warnings() {
        assert!(written(None, "heimdall_app::app", Level::Info));
        assert!(!written(None, "heimdall_app::app", Level::Debug));
        assert!(!written(None, "russh::session", Level::Info));
        assert!(written(None, "russh::session", Level::Warn));
    }

    #[test]
    fn the_variable_sets_every_crate_and_garbage_is_ignored() {
        assert!(written(Some("trace"), "russh::session", Level::Trace));
        assert!(!written(Some("error"), "heimdall_app", Level::Warn));
        assert!(!written(Some("loud"), "russh", Level::Info));
    }

    #[test]
    fn the_log_line_of_a_crash_is_its_first_line() {
        assert_eq!(first_line("a\nb"), "a");
        assert_eq!(first_line(""), "");
    }

    #[test]
    fn a_panic_message_is_printable_and_bounded() {
        assert_eq!(printable("bad\x1b[2Jline\r\nnext"), "bad[2Jline\nnext");
        let long = "x".repeat(MAX_PANIC_MESSAGE_CHARS + 10);
        assert_eq!(printable(&long).chars().count(), MAX_PANIC_MESSAGE_CHARS);
    }

    #[cfg(unix)]
    #[test]
    fn only_the_owner_reads_the_log() {
        use std::os::unix::fs::PermissionsExt as _;

        use super::open_log;
        let dir = tempfile::tempdir().expect("dir");
        drop(open_log(dir.path()).expect("opened"));
        let mode = std::fs::metadata(dir.path().join(super::LOG_FILE_NAME))
            .expect("exists")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
