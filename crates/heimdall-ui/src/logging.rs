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
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(test)]
use log::Level;
use log::{LevelFilter, Log, Metadata, Record};

/// Name of the log file.
const LOG_FILE_NAME: &str = "heimdall.log";

/// Name the previous log file is kept under.
const PREVIOUS_LOG_FILE_NAME: &str = "heimdall.log.1";

/// Bytes in a megabyte.
const BYTES_PER_MEGABYTE: u64 = 1024 * 1024;

/// Size in megabytes past which the log file is set aside, as the C# `FileLogger` caps a
/// file at 10 MB.
const MAX_LOG_MEGABYTES: u64 = 10;

/// Size past which the log file is set aside, at start and while it is written: an entry
/// that would pass it starts the next file, so no entry is split.
const MAX_LOG_BYTES: u64 = MAX_LOG_MEGABYTES * BYTES_PER_MEGABYTE;

/// Days the set-aside log is kept after it was last written, as the C# `LogRetentionDays`.
const LOG_RETENTION_DAYS: u64 = 30;

/// Seconds in a day.
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

/// Environment variable choosing the level of every crate: `error` to `trace`.
const LEVEL_VARIABLE: &str = "HEIMDALL_LOG";

/// Environment variable that turns on the debug lines of the full-screen diagnostics, and
/// nothing else: no other target, in this workspace or outside it, is made more verbose.
const FULLSCREEN_TRACE_VARIABLE: &str = "HEIMDALL_LOG_FULLSCREEN";

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

/// The open log file and how much it holds.
struct LogState {
    /// `None` when a file set aside could not be replaced: lines are dropped, as when the log
    /// could not be opened at all.
    file: Option<File>,
    written: u64,
    dir: PathBuf,
}

impl LogState {
    /// Adds `bytes` as one entry. An entry that would take the file past `cap` goes to a
    /// fresh file instead (the full one is set aside), unless the file is empty: one
    /// oversized entry is never refused, as the C# logger never refuses a batch.
    fn append(&mut self, bytes: &[u8], cap: u64) {
        let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if self.written > 0 && self.written.saturating_add(length) > cap {
            self.roll_over();
        }
        if let Some(file) = self.file.as_mut()
            && file.write_all(bytes).is_ok()
        {
            self.written = self.written.saturating_add(length);
        }
    }

    /// Sets the full file aside and continues in a new one.
    fn roll_over(&mut self) {
        // The handle goes first: Windows does not rename a file another handle may lock.
        self.file = None;
        rotate_log(&self.dir);
        self.file = open_file(&self.dir.join(LOG_FILE_NAME))
            .ok()
            .map(|(file, _)| file);
        self.written = self
            .file
            .as_ref()
            .and_then(|file| file.metadata().ok())
            .map_or(0, |meta| meta.len());
    }
}

struct FileLogger {
    state: Mutex<LogState>,
    own: LevelFilter,
    others: LevelFilter,
    /// The full-screen diagnostics are written at debug.
    fullscreen_trace: bool,
}

/// Crates whose most verbose levels write server data as is, capped whatever
/// `HEIMDALL_LOG` asks: suppaftp logs every listing line it parses at `trace`, which holds
/// the server's file names.
const CAPPED_TARGETS: [(&str, LevelFilter); 1] = [("suppaftp", LevelFilter::Debug)];

/// The most verbose level written for `target`.
fn limit_for(
    own: LevelFilter,
    others: LevelFilter,
    fullscreen_trace: bool,
    target: &str,
) -> LevelFilter {
    if fullscreen_trace && target == crate::fullscreen_chrome::LOG_TARGET {
        return own.max(LevelFilter::Debug);
    }
    if target.starts_with(OWN_TARGET_PREFIX) {
        return own;
    }
    CAPPED_TARGETS
        .iter()
        .find(|(prefix, _)| target.starts_with(prefix))
        .map_or(others, |(_, cap)| others.min(*cap))
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level()
            <= limit_for(
                self.own,
                self.others,
                self.fullscreen_trace,
                metadata.target(),
            )
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
        if let Ok(mut state) = self.state.lock() {
            // A log that cannot be written must not take the application down.
            state.append(line.as_bytes(), MAX_LOG_BYTES);
        }
    }

    fn flush(&self) {
        if let Ok(mut state) = self.state.lock()
            && let Some(file) = state.file.as_mut()
        {
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

/// The log file of `dir`, opened to add to it, and why it is not restricted to the user
/// when it is not.
///
/// It is restricted on Windows to the user, the Administrators and SYSTEM before a line is
/// written in it, as the C# `FileLogger` restricts its file, should its folder not have been.
/// Restriction failing, it is written all the same with its folder's access, as the C# goes
/// on: what the code logs is redacted, and a log is the one trace a user can send.
fn open_log(dir: &Path) -> std::io::Result<(File, Option<std::io::Error>)> {
    open_log_capped(dir, MAX_LOG_BYTES)
}

/// `open_log`, setting the file aside first when it holds more than `cap` bytes.
fn open_log_capped(dir: &Path, cap: u64) -> std::io::Result<(File, Option<std::io::Error>)> {
    fs::create_dir_all(dir)?;
    let path = dir.join(LOG_FILE_NAME);
    if fs::metadata(&path).is_ok_and(|meta| meta.len() > cap) {
        rotate_log(dir);
    }
    open_file(&path)
}

/// Keeps the log file of `dir` as the previous log, replacing the one kept before.
fn rotate_log(dir: &Path) {
    // Another instance may hold the file open (Windows): keep appending rather than run
    // without a log.
    let _ = fs::rename(dir.join(LOG_FILE_NAME), dir.join(PREVIOUS_LOG_FILE_NAME));
}

/// Deletes the previous log of `dir` when it was last written `retention` or more before
/// `now`, and returns whether it did.
///
/// Only that one name is a candidate, by exact name: the log in use, the crash reports and
/// anything else a user keeps in the folder are never touched, as the C# purge decides by the
/// exact shape of a name. Best effort: a file that is held stays for the next start.
fn purge_expired(dir: &Path, retention: Duration, now: SystemTime) -> bool {
    let path = dir.join(PREVIOUS_LOG_FILE_NAME);
    let expired = fs::metadata(&path)
        .ok()
        .filter(std::fs::Metadata::is_file)
        .and_then(|meta| meta.modified().ok())
        .and_then(|written| now.duration_since(written).ok())
        .is_some_and(|age| age >= retention);
    expired && fs::remove_file(path).is_ok()
}

/// The log file at `path`, opened to add to it, and why it is not restricted when it is not.
fn open_file(path: &Path) -> std::io::Result<(File, Option<std::io::Error>)> {
    match heimdall_core::tools::private_file::open_private_log(path) {
        Ok(file) => Ok((file, None)),
        Err(unrestricted) => Ok((private_file().append(true).open(path)?, Some(unrestricted))),
    }
}

/// Starts logging to `dir` and writes crash reports there. Without a directory, or when
/// the file cannot be opened, the application runs without a log.
pub fn init(dir: Option<PathBuf>) {
    let Some(dir) = dir else {
        return;
    };
    let (own, others) = levels(std::env::var(LEVEL_VARIABLE).ok().as_deref());
    let fullscreen_trace =
        std::env::var_os(FULLSCREEN_TRACE_VARIABLE).is_some_and(|v| !v.is_empty());
    let mut unrestricted = None;
    purge_expired(
        &dir,
        Duration::from_secs(LOG_RETENTION_DAYS * SECONDS_PER_DAY),
        SystemTime::now(),
    );
    if let Ok((file, error)) = open_log(&dir) {
        unrestricted = error;
        let written = file.metadata().map_or(0, |meta| meta.len());
        let logger = FileLogger {
            state: Mutex::new(LogState {
                file: Some(file),
                written,
                dir: dir.clone(),
            }),
            own,
            others,
            fullscreen_trace,
        };
        if log::set_boxed_logger(Box::new(logger)).is_ok() {
            let trace = if fullscreen_trace {
                LevelFilter::Debug
            } else {
                LevelFilter::Off
            };
            log::set_max_level(own.max(others).max(trace));
        }
    }
    install_panic_hook(dir);
    if let Some(error) = unrestricted {
        log::warn!("the diagnostics log is not restricted to the user: {error}");
    }
    log::info!(
        "Heimdall {} started on {} {}",
        heimdall_app::update_check::display_version(),
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
        heimdall_app::update_check::display_version()
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
    level <= limit_for(own, others, false, target)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

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
    fn the_listing_lines_suppaftp_traces_are_never_written() {
        assert!(!written(Some("trace"), "suppaftp::list", Level::Trace));
        assert!(written(Some("trace"), "suppaftp::list", Level::Debug));
        assert!(!written(None, "suppaftp::list", Level::Info));
        assert!(written(None, "suppaftp", Level::Warn));
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

    #[cfg(windows)]
    #[test]
    fn on_windows_the_log_is_restricted_before_a_line_is_written() {
        let dir = tempfile::tempdir().expect("dir");
        let (file, unrestricted) = super::open_log(dir.path()).expect("opened");
        drop(file);
        assert!(unrestricted.is_none(), "{unrestricted:?}");
        // Opened again at the next start: restricted again, still written.
        let (file, unrestricted) = super::open_log(dir.path()).expect("opened again");
        drop(file);
        assert!(unrestricted.is_none(), "{unrestricted:?}");
    }

    #[cfg(unix)]
    #[test]
    fn only_the_owner_reads_the_log() {
        use std::os::unix::fs::PermissionsExt as _;

        use super::open_log;
        let dir = tempfile::tempdir().expect("dir");
        let (file, unrestricted) = open_log(dir.path()).expect("opened");
        assert!(unrestricted.is_none(), "{unrestricted:?}");
        drop(file);
        let mode = std::fs::metadata(dir.path().join(super::LOG_FILE_NAME))
            .expect("exists")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    /// Opens the log of `dir` as `init` does.
    fn state_in(dir: &std::path::Path, cap: u64) -> super::LogState {
        let (file, _) = super::open_log_capped(dir, cap).expect("opened");
        let written = file.metadata().map_or(0, |meta| meta.len());
        super::LogState {
            file: Some(file),
            written,
            dir: dir.to_path_buf(),
        }
    }

    fn seed(path: &std::path::Path, age: Duration) {
        std::fs::write(path, "content").expect("seeded");
        let file = std::fs::File::options()
            .write(true)
            .open(path)
            .expect("opened");
        file.set_modified(SystemTime::now() - age).expect("aged");
    }

    #[test]
    fn a_log_at_its_cap_continues_in_a_new_file_with_no_entry_split() {
        const TINY_CAP: u64 = 400;
        let dir = tempfile::tempdir().expect("dir");
        let mut state = state_in(dir.path(), TINY_CAP);
        let entries: Vec<String> = (0..30)
            .map(|i| {
                format!(
                    "entry-{i:03}-{}
",
                    "x".repeat(40)
                )
            })
            .collect();
        for entry in &entries {
            state.append(entry.as_bytes(), TINY_CAP);
        }
        drop(state);

        let current = std::fs::read_to_string(dir.path().join(super::LOG_FILE_NAME)).expect("log");
        let previous =
            std::fs::read_to_string(dir.path().join(super::PREVIOUS_LOG_FILE_NAME)).expect("old");
        assert!(current.len() as u64 <= TINY_CAP, "{}", current.len());
        assert!(previous.len() as u64 <= TINY_CAP, "{}", previous.len());
        // What is kept is the end of the log, whole and in order.
        let kept = format!("{previous}{current}");
        assert!(!kept.is_empty());
        assert!(entries.concat().ends_with(&kept), "{kept}");
        assert!(kept.lines().all(|line| line.starts_with("entry-")));
    }

    #[test]
    fn an_oversized_entry_in_an_empty_file_is_written_whole() {
        let dir = tempfile::tempdir().expect("dir");
        let mut state = state_in(dir.path(), 8);
        state.append(
            b"longer than the cap
",
            8,
        );
        drop(state);
        let current = std::fs::read_to_string(dir.path().join(super::LOG_FILE_NAME)).expect("log");
        assert_eq!(
            current,
            "longer than the cap
"
        );
        assert!(!dir.path().join(super::PREVIOUS_LOG_FILE_NAME).exists());
    }

    #[test]
    fn a_restart_does_not_append_to_a_file_past_its_cap() {
        const TINY_CAP: u64 = 16;
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(
            dir.path().join(super::LOG_FILE_NAME),
            "a line longer than the cap
",
        )
        .expect("seeded");
        let state = state_in(dir.path(), TINY_CAP);
        assert_eq!(state.written, 0);
        drop(state);
        assert!(dir.path().join(super::PREVIOUS_LOG_FILE_NAME).exists());
    }

    #[test]
    fn only_a_previous_log_past_the_retention_is_purged() {
        let retention = Duration::from_secs(super::LOG_RETENTION_DAYS * super::SECONDS_PER_DAY);
        let old = retention + Duration::from_secs(super::SECONDS_PER_DAY);
        let dir = tempfile::tempdir().expect("dir");
        let previous = dir.path().join(super::PREVIOUS_LOG_FILE_NAME);
        let active = dir.path().join(super::LOG_FILE_NAME);
        let crash = dir.path().join("crash-1-000.txt");
        let notes = dir.path().join("heimdall.log.notes");
        for path in [&active, &crash, &notes] {
            seed(path, old);
        }
        seed(&previous, Duration::from_secs(super::SECONDS_PER_DAY));

        assert!(!super::purge_expired(
            dir.path(),
            retention,
            SystemTime::now()
        ));
        assert!(previous.exists(), "a recent previous log stays");

        seed(&previous, old);
        assert!(super::purge_expired(
            dir.path(),
            retention,
            SystemTime::now()
        ));
        assert!(!previous.exists());
        assert!(active.exists(), "the log in use is never a candidate");
        assert!(crash.exists(), "a crash report is not a diagnostic log");
        assert!(notes.exists());
        // Nothing to purge, nothing to fail on.
        assert!(!super::purge_expired(
            dir.path(),
            retention,
            SystemTime::now()
        ));
    }

    #[test]
    fn the_full_screen_trace_switch_opens_its_own_target_and_no_other() {
        use log::LevelFilter;

        use super::limit_for;

        let (own, others) = (LevelFilter::Info, LevelFilter::Warn);
        let target = crate::fullscreen_chrome::LOG_TARGET;
        assert_eq!(limit_for(own, others, false, target), LevelFilter::Info);
        assert_eq!(limit_for(own, others, true, target), LevelFilter::Debug);
        for other in [
            "heimdall_app::app",
            "ironrdp_connector::connection",
            "russh::session",
        ] {
            assert_eq!(
                limit_for(own, others, true, other),
                limit_for(own, others, false, other),
                "{other} is no more verbose with the switch on"
            );
        }
    }
}
