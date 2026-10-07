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

//! The two shared session logs of the C# Heimdall, beside the transcripts in their folder:
//! `session-events.log`, a remote desktop's connect and disconnect (`SessionEventLog`), and
//! `session-operations.log`, each change a Files tab makes on its server
//! (`SessionOperationLog`, fed by `LoggingRemoteBrowser`).
//!
//! Both are NDJSON as the C# writes them: one JSON object a line, its property names
//! English and in the C# order, its time ISO 8601 UTC to the 100 nanoseconds, an optional
//! property left out rather than null, no byte order mark, a line feed after each. Past
//! [`SESSION_LOG_MAX_BYTES`] a log continues in `.1.log`, `.2.log`... and a later run goes
//! on in the last one. Neither is pruned: the transcripts' retention leaves them, as the C#
//! says it does.
//!
//! What goes in is said by whoever records it, never what a file holds or a secret: a host
//! without any `user@` before it, a path, a size, an outcome. Every character that could
//! break a line, or show as something else, is written as a JSON `\u` escape.
//!
//! The lines are written by a thread of their own, buffered, so a caller on the window's
//! thread only hands a line over.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use heimdall_core::utc::UtcTime;
use heimdall_files::{Refusal, RemotePath, RemoteSession};
use tokio_stream::{Stream, StreamExt as _};
use tokio_util::sync::CancellationToken;

use crate::files::{
    CopySource, FileOperation, FilesError, TransferEvent, TransferRequest, TransferState,
};

/// Name of the log of remote desktops' connects and disconnects, as the C# names it.
pub const SESSION_EVENTS_FILE: &str = "session-events.log";

/// Name of the log of the Files tabs' changes on their servers, as the C# names it.
pub const SESSION_OPERATIONS_FILE: &str = "session-operations.log";

/// Largest a log file grows before it continues in the next, as the C# caps both.
pub const SESSION_LOG_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Extension of a log file and of its continuations.
const EXTENSION: &str = "log";

/// What ends each line, whatever the platform, as NDJSON wants it.
const LINE_END: &str = "\n";

/// Name of the thread writing the logs.
const WRITER_THREAD: &str = "session-logs";

/// Nanoseconds in one tick of the C# round-trip time format.
const NANOS_PER_TICK: u32 = 100;

/// Owner read and write only, for the logs on Unix.
#[cfg(unix)]
const PRIVATE_FILE_MODE: u32 = 0o600;

/// A session's connect or disconnect, as the C# `SessionEventKind` names it in the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEventKind {
    /// The session reached its connected state.
    Connected,
    /// The session ended.
    Disconnected,
}

impl SessionEventKind {
    /// Its word in the log.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Connected => "Connected",
            Self::Disconnected => "Disconnected",
        }
    }
}

/// What ended a session, as the C# `endTrigger` says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndTrigger {
    /// The server, or the connection, ended it.
    Remote,
    /// The user disconnected it.
    User,
    /// Its tab, or the application, closed.
    Teardown,
}

impl EndTrigger {
    /// Its word in the log.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Remote => "remote",
            Self::User => "user",
            Self::Teardown => "teardown",
        }
    }
}

/// One line of the events log: a remote desktop connected or disconnected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEvent {
    /// Its protocol, as the C# writes it: `RDP`, `VNC`.
    pub protocol: &'static str,
    /// Connected or disconnected.
    pub kind: SessionEventKind,
    /// The server; any `user@` before it is left out when written.
    pub host: String,
    /// The session's title.
    pub title: Option<String>,
    /// Why an RDP session ended, as the C# key names it: `RDP_ADMIN_DISCONNECT`.
    pub reason: Option<String>,
    /// How long it was connected; on a disconnect only.
    pub duration: Option<Duration>,
    /// What ended it.
    pub end_trigger: Option<EndTrigger>,
}

impl SessionEvent {
    /// Its line, at `at`, without the line end, as the C# `SessionEventLog` writes it.
    #[must_use]
    pub fn line(&self, at: SystemTime) -> String {
        let mut line = JsonLine::new();
        line.text("ts", &round_trip_utc(at));
        line.text("protocol", self.protocol);
        line.text("event", self.kind.token());
        line.text("host", without_user(&self.host));
        if let Some(title) = &self.title {
            line.text("title", title);
        }
        if let Some(reason) = &self.reason {
            line.text("reason", reason);
        }
        if let Some(duration) = self.duration {
            line.number("durationMs", millis(duration));
        }
        if let Some(trigger) = self.end_trigger {
            line.text("endTrigger", trigger.token());
        }
        line.end()
    }
}

/// A change made on a server from a Files tab, as the C# `SessionOperationKind` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    /// A file or folder sent to the server.
    Upload,
    /// A file or folder brought from the server.
    Download,
    /// An entry deleted.
    Delete,
    /// An entry renamed or moved.
    Rename,
    /// A folder created.
    Mkdir,
    /// An entry copied on the server itself.
    Copy,
    /// Permission bits changed.
    Chmod,
}

impl OperationKind {
    /// Its word in the log.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Upload => "upload",
            Self::Download => "download",
            Self::Delete => "delete",
            Self::Rename => "rename",
            Self::Mkdir => "mkdir",
            Self::Copy => "copy",
            Self::Chmod => "chmod",
        }
    }
}

/// The kind of failure of an operation, as the C# `OperationErrorClassifier` sorts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Refused for want of permission, on the server or on this computer.
    Permission,
    /// This computer's file system failed.
    Io,
    /// Anything else.
    Other,
}

impl ErrorCategory {
    /// The category of `error`.
    #[must_use]
    pub fn of(error: &FilesError) -> Self {
        match error {
            FilesError::Server {
                refusal: Refusal::PermissionDenied,
                ..
            }
            | FilesError::SudoPasswordRejected => Self::Permission,
            FilesError::Local { .. } => Self::Io,
            _ => Self::Other,
        }
    }

    /// Its word in the log.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Permission => "permission",
            Self::Io => "io",
            Self::Other => "other",
        }
    }
}

/// How an operation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationOutcome {
    /// Done.
    Success,
    /// Failed, of this kind.
    Error(ErrorCategory),
    /// Stopped by the user.
    Cancelled,
}

impl OperationOutcome {
    /// The outcome of `result`.
    #[must_use]
    pub fn of<T>(result: &Result<T, FilesError>) -> Self {
        match result {
            Ok(_) => Self::Success,
            Err(error) => Self::Error(ErrorCategory::of(error)),
        }
    }

    /// Its word in the log.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Error(_) => "error",
            Self::Cancelled => "cancelled",
        }
    }
}

/// One line of the operations log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationRecord {
    /// What was done.
    pub kind: OperationKind,
    /// The server's path it was done to; a rename's or a copy's source.
    pub remote_path: String,
    /// A rename's or a copy's destination.
    pub remote_path_to: Option<String>,
    /// A transfer's file on this computer.
    pub local_path: Option<String>,
    /// The bytes a transfer moved; on success only.
    pub bytes: Option<u64>,
    /// How long it took.
    pub duration: Duration,
    /// How it ended.
    pub outcome: OperationOutcome,
    /// It ran as root, through sudo.
    pub privileged: bool,
}

impl OperationRecord {
    /// Its line for `protocol` on `host`, at `at`, without the line end, as the C#
    /// `SessionOperationLog` writes it.
    #[must_use]
    pub fn line(&self, protocol: &str, host: &str, at: SystemTime) -> String {
        let mut line = JsonLine::new();
        line.text("ts", &round_trip_utc(at));
        line.text("protocol", protocol);
        line.text("op", self.kind.token());
        line.text("host", without_user(host));
        line.text("remotePath", &self.remote_path);
        if let Some(to) = &self.remote_path_to {
            line.text("remotePathTo", to);
        }
        if let Some(local) = &self.local_path {
            line.text("localPath", local);
        }
        if let Some(bytes) = self.bytes {
            line.number("bytes", bytes);
        }
        line.number("durationMs", millis(self.duration));
        line.text("result", self.outcome.token());
        if let OperationOutcome::Error(category) = self.outcome {
            line.text("errorCategory", category.token());
        }
        if self.privileged {
            line.raw("privileged", "true");
        }
        line.end()
    }
}

/// The writer of both logs, shared by the application and the Files work it hands out.
/// The first line starts the thread that writes them; the last handle dropped waits for it
/// to write what it was given.
#[derive(Debug, Clone, Default)]
pub struct SessionLogs {
    writer: Arc<Writer>,
}

impl SessionLogs {
    /// Logs that continue in a new file past `max_bytes`.
    #[must_use]
    pub fn with_max_bytes(max_bytes: u64) -> Self {
        Self {
            writer: Arc::new(Writer {
                max_bytes,
                channel: Mutex::default(),
            }),
        }
    }

    /// Adds `event` to the events log of `folder`.
    pub fn event(&self, folder: &Path, event: &SessionEvent) {
        self.writer.send(Command::Append {
            file: folder.join(SESSION_EVENTS_FILE),
            line: event.line(SystemTime::now()),
        });
    }

    /// Adds `record`, of `protocol` on `host`, to the operations log of `folder`.
    pub fn operation(&self, folder: &Path, protocol: &str, host: &str, record: &OperationRecord) {
        self.writer.send(Command::Append {
            file: folder.join(SESSION_OPERATIONS_FILE),
            line: record.line(protocol, host, SystemTime::now()),
        });
    }

    /// Waits until every line handed over so far is written to its file.
    pub fn sync(&self) {
        let (done, wait) = mpsc::channel();
        // No thread yet: no line was handed over.
        let asked = self.writer.channel.lock().is_ok_and(|channel| {
            channel
                .as_ref()
                .is_some_and(|(sender, _)| sender.send(Command::Sync(done)).is_ok())
        });
        if asked {
            let _ = wait.recv();
        }
    }
}

/// The thread writing the logs and the way lines reach it.
#[derive(Debug)]
struct Writer {
    max_bytes: u64,
    channel: Mutex<Option<(Sender<Command>, JoinHandle<()>)>>,
}

impl Default for Writer {
    fn default() -> Self {
        Self {
            max_bytes: SESSION_LOG_MAX_BYTES,
            channel: Mutex::default(),
        }
    }
}

impl Writer {
    /// Hands `command` to the thread, started if need be.
    fn send(&self, command: Command) {
        let Ok(mut channel) = self.channel.lock() else {
            return;
        };
        if channel.is_none() {
            let (sender, receiver) = mpsc::channel();
            let max_bytes = self.max_bytes;
            match std::thread::Builder::new()
                .name(WRITER_THREAD.to_owned())
                .spawn(move || write_lines(&receiver, max_bytes))
            {
                Ok(thread) => *channel = Some((sender, thread)),
                Err(error) => {
                    log::warn!("the session logs cannot be written: {error}");
                    return;
                }
            }
        }
        if let Some((sender, _)) = channel.as_ref() {
            // The thread ends only once every sender is gone: it is there.
            let _ = sender.send(command);
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let taken = self.channel.get_mut().ok().and_then(Option::take);
        if let Some((sender, thread)) = taken {
            // Without a sender the thread writes what it has, then ends.
            drop(sender);
            let _ = thread.join();
        }
    }
}

/// What the writing thread is asked.
#[derive(Debug)]
enum Command {
    /// Add `line` to the log whose first file is `file`.
    Append { file: PathBuf, line: String },
    /// Say when everything before is written.
    Sync(Sender<()>),
}

/// The writing thread: each batch of lines handed over is written, then flushed.
fn write_lines(receiver: &Receiver<Command>, max_bytes: u64) {
    let mut logs: HashMap<PathBuf, LogFile> = HashMap::new();
    while let Ok(first) = receiver.recv() {
        let mut waiting = Vec::new();
        for command in std::iter::once(first).chain(receiver.try_iter()) {
            match command {
                Command::Append { file, line } => logs
                    .entry(file.clone())
                    .or_insert_with(|| LogFile::new(file, max_bytes))
                    .append(&line),
                Command::Sync(done) => waiting.push(done),
            }
        }
        for log in logs.values_mut() {
            log.flush();
        }
        for done in waiting {
            let _ = done.send(());
        }
    }
    for log in logs.values_mut() {
        log.flush();
    }
}

/// A log being written: its first file, and the continuation lines go to now.
#[derive(Debug)]
struct LogFile {
    first: PathBuf,
    max_bytes: u64,
    /// Number of the continuation written, 0 for the first file; `None` until found.
    part: Option<u32>,
    /// Bytes in the file written.
    written: u64,
    file: Option<BufWriter<File>>,
    /// A failure was logged: the next ones are not.
    failed: bool,
}

impl LogFile {
    fn new(first: PathBuf, max_bytes: u64) -> Self {
        Self {
            first,
            max_bytes,
            part: None,
            written: 0,
            file: None,
            failed: false,
        }
    }

    /// File `part` of the log: the first for 0, else `name.N.log`.
    fn part_path(&self, part: u32) -> PathBuf {
        if part == 0 {
            return self.first.clone();
        }
        let stem = self
            .first
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.first
            .with_file_name(format!("{stem}.{part}.{EXTENSION}"))
    }

    /// Adds `line` and a line end; a failure drops it, said once in the diagnostics log.
    fn append(&mut self, line: &str) {
        if let Err(error) = self.try_append(line) {
            // Opened again for the next line.
            self.file = None;
            if !self.failed {
                self.failed = true;
                log::warn!(
                    "a line of {} could not be written: {error}",
                    self.first.display()
                );
            }
        }
    }

    fn try_append(&mut self, line: &str) -> io::Result<()> {
        let bytes = u64::try_from(line.len() + LINE_END.len()).unwrap_or(u64::MAX);
        let part = match self.part {
            Some(part) => part,
            None => self.resume(),
        };
        if self.written > 0 && self.written.saturating_add(bytes) > self.max_bytes {
            self.flush();
            self.file = None;
            let next = part + 1;
            self.part = Some(next);
            self.written = fs::metadata(self.part_path(next)).map_or(0, |found| found.len());
            log::info!(
                "{} reached its size cap, continuing in {}",
                self.first.display(),
                self.part_path(next).display()
            );
        }
        if self.file.is_none() {
            let path = self.part_path(self.part.unwrap_or_default());
            if let Some(folder) = path.parent() {
                fs::create_dir_all(folder)?;
            }
            self.file = Some(BufWriter::new(open(&path)?));
        }
        if let Some(file) = self.file.as_mut() {
            file.write_all(line.as_bytes())?;
            file.write_all(LINE_END.as_bytes())?;
        }
        self.written = self.written.saturating_add(bytes);
        Ok(())
    }

    /// Goes on where an earlier run stopped: the last continuation there, with its size.
    fn resume(&mut self) -> u32 {
        let mut part = 0;
        while self.part_path(part + 1).exists() {
            part += 1;
        }
        self.part = Some(part);
        self.written = fs::metadata(self.part_path(part)).map_or(0, |found| found.len());
        part
    }

    fn flush(&mut self) {
        if let Some(file) = self.file.as_mut()
            && let Err(error) = file.flush()
        {
            log::warn!("{} could not be flushed: {error}", self.first.display());
        }
    }
}

/// Opens `path` to add to it, made if need be. On Unix only its owner may read it; on
/// Windows it takes the rights of the folder it is in, as a transcript does.
fn open(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(PRIVATE_FILE_MODE);
    }
    options.open(path)
}

/// A JSON object written a property at a time, in the order given.
struct JsonLine(String);

impl JsonLine {
    fn new() -> Self {
        Self(String::from("{"))
    }

    /// Adds `key` with `value`, already JSON.
    fn raw(&mut self, key: &str, value: &str) {
        if self.0.len() > 1 {
            self.0.push(',');
        }
        push_json_string(&mut self.0, key);
        self.0.push(':');
        self.0.push_str(value);
    }

    fn text(&mut self, key: &str, value: &str) {
        let mut quoted = String::with_capacity(value.len() + 2);
        push_json_string(&mut quoted, value);
        self.raw(key, &quoted);
    }

    fn number(&mut self, key: &str, value: u64) {
        self.raw(key, &value.to_string());
    }

    fn end(mut self) -> String {
        self.0.push('}');
        self.0
    }
}

/// Adds `text` to `out` as a JSON string. A quote and a backslash are escaped, and so is
/// any character that could end the line or show as something it is not: controls, other
/// whitespace than the plain space, format characters, line and paragraph separators.
fn push_json_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control()
                || (c.is_whitespace() && c != ' ')
                || crate::text::is_invisible(c) =>
            {
                let mut units = [0_u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `time` as the C# round-trip format writes a UTC time: `2026-10-07T12:34:56.1234567Z`.
#[must_use]
pub fn round_trip_utc(time: SystemTime) -> String {
    let utc = UtcTime::of(time);
    let ticks = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos() / NANOS_PER_TICK);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{ticks:07}Z",
        utc.year, utc.month, utc.day, utc.hour, utc.minute, utc.second
    )
}

/// `host` without the `user@` it may start with, split at the last `@`: a host never holds
/// one, a user name may.
#[must_use]
pub fn without_user(host: &str) -> &str {
    host.rsplit_once('@').map_or(host, |(_, host)| host)
}

/// Whole milliseconds of `duration`.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Where a Files tab's changes on its server are recorded: its server's log, its protocol
/// and host. Handed to the work of the tab only while session logging is on for it.
#[derive(Debug, Clone)]
pub struct OperationJournal {
    logs: SessionLogs,
    folder: PathBuf,
    protocol: &'static str,
    host: String,
}

impl OperationJournal {
    /// Records the changes made over `client` on `host` in the operations log of `folder`.
    #[must_use]
    pub fn new(logs: SessionLogs, folder: PathBuf, client: &RemoteSession, host: &str) -> Self {
        let protocol = match client {
            RemoteSession::Sftp(_) => "SFTP",
            RemoteSession::Ftp(_) => "FTP",
        };
        Self {
            logs,
            folder,
            protocol,
            host: host.to_owned(),
        }
    }

    /// Adds `record` to the log.
    pub fn record(&self, record: &OperationRecord) {
        self.logs
            .operation(&self.folder, self.protocol, &self.host, record);
    }
}

/// The line a server change `operation` makes, its time and outcome still to come. `None`
/// for a change on this computer, which the C# does not log either.
fn pending_record(operation: &FileOperation) -> Option<OperationRecord> {
    let (kind, path, to, privileged) = match operation {
        FileOperation::RemoteMakeFolder { path, .. } => (OperationKind::Mkdir, path, None, false),
        FileOperation::RemoteRename { from, to, .. } => {
            (OperationKind::Rename, from, Some(to), false)
        }
        FileOperation::RemoteRemove { path, .. } => (OperationKind::Delete, path, None, false),
        FileOperation::RemoteSetPermissions { path, .. } => {
            (OperationKind::Chmod, path, None, false)
        }
        FileOperation::RemoteSudoRemove { path, .. } => (OperationKind::Delete, path, None, true),
        FileOperation::LocalMakeFolder { .. }
        | FileOperation::LocalRename { .. }
        | FileOperation::LocalRemove { .. }
        | FileOperation::LocalPaste { .. } => return None,
    };
    Some(OperationRecord {
        kind,
        remote_path: path.display(),
        remote_path_to: to.map(RemotePath::display),
        local_path: None,
        bytes: None,
        duration: Duration::ZERO,
        outcome: OperationOutcome::Success,
        privileged,
    })
}

/// [`crate::files::file_operation`], recorded in `journal` when given and the operation
/// changes the server.
///
/// # Errors
///
/// [`FilesError`] from the server or this computer.
pub async fn file_operation_recorded(
    operation: FileOperation,
    journal: Option<OperationJournal>,
) -> Result<(), FilesError> {
    let record = journal.and_then(|journal| Some((journal, pending_record(&operation)?)));
    let started = Instant::now();
    let result = crate::files::file_operation(operation).await;
    if let Some((journal, mut record)) = record {
        record.duration = started.elapsed();
        record.outcome = OperationOutcome::of(&result);
        journal.record(&record);
    }
    result
}

/// [`crate::files::move_remote`], each rename recorded in `journal` when given.
pub async fn move_remote_recorded(
    client: RemoteSession,
    moves: Vec<(RemotePath, RemotePath)>,
    journal: Option<OperationJournal>,
) -> Vec<(RemotePath, Result<(), FilesError>)> {
    let mut results = Vec::with_capacity(moves.len());
    for (from, to) in moves {
        let started = Instant::now();
        let result = client
            .rename(&from, &to)
            .await
            .map_err(|e| FilesError::from(&e));
        if let Some(journal) = &journal {
            journal.record(&OperationRecord {
                kind: OperationKind::Rename,
                remote_path: from.display(),
                remote_path_to: Some(to.display()),
                local_path: None,
                bytes: None,
                duration: started.elapsed(),
                outcome: OperationOutcome::of(&result),
                privileged: false,
            });
        }
        results.push((from, result));
    }
    results
}

/// [`crate::files::copy_remote`], each copy recorded in `journal` when given, to the copy
/// made, or to the name it would have had.
pub async fn copy_remote_recorded(
    client: RemoteSession,
    shell: Option<heimdall_ssh::Connection>,
    sources: Vec<CopySource>,
    folder: RemotePath,
    cancel: CancellationToken,
    journal: Option<OperationJournal>,
) -> Vec<(RemotePath, Result<RemotePath, FilesError>)> {
    let mut results = Vec::with_capacity(sources.len());
    for source in sources {
        let started = Instant::now();
        let result =
            crate::files::copy_one(&client, shell.as_ref(), &source, &folder, &cancel).await;
        if let Some(journal) = &journal {
            let to = match &result {
                Ok(made) => made.clone(),
                Err(_) => folder.join(source.path.file_name().unwrap_or_default()),
            };
            journal.record(&OperationRecord {
                kind: OperationKind::Copy,
                remote_path: source.path.display(),
                remote_path_to: Some(to.display()),
                local_path: None,
                bytes: None,
                duration: started.elapsed(),
                outcome: OperationOutcome::of(&result),
                privileged: false,
            });
        }
        let failed = result.is_err();
        results.push((source.path, result));
        if failed {
            break;
        }
    }
    results
}

/// [`crate::files::transfer_events`], the transfer recorded in `journal` when given once it
/// ends: a file or a whole folder, one line.
pub fn transfer_events_recorded(
    request: TransferRequest,
    journal: Option<OperationJournal>,
) -> impl Stream<Item = TransferEvent> + Send + 'static {
    let mut recorder = journal.map(|journal| TransferRecorder::new(journal, &request));
    crate::files::transfer_events(request).map(move |event| {
        if let Some(recorder) = recorder.as_mut() {
            recorder.observe(&event);
        }
        event
    })
}

/// A transfer being followed, to record how it ended.
struct TransferRecorder {
    journal: OperationJournal,
    record: OperationRecord,
    started: Instant,
    /// Bytes done so far.
    done: u64,
}

impl TransferRecorder {
    fn new(journal: OperationJournal, request: &TransferRequest) -> Self {
        let kind = match request.direction {
            crate::files::Direction::Download => OperationKind::Download,
            crate::files::Direction::Upload => OperationKind::Upload,
        };
        Self {
            journal,
            record: OperationRecord {
                kind,
                remote_path: request.remote.display(),
                remote_path_to: None,
                local_path: Some(request.local.display().to_string()),
                bytes: None,
                duration: Duration::ZERO,
                outcome: OperationOutcome::Success,
                privileged: false,
            },
            started: Instant::now(),
            done: 0,
        }
    }

    /// Follows `event`; the end of the transfer is recorded. One that left entries out is
    /// not a success: the log must not say the destination holds everything.
    fn observe(&mut self, event: &TransferEvent) {
        match event {
            TransferEvent::Progress(bytes) => self.done = *bytes,
            TransferEvent::Finished(state) => {
                let outcome = match state {
                    TransferState::Done => OperationOutcome::Success,
                    TransferState::Cancelled => OperationOutcome::Cancelled,
                    TransferState::Failed(error) => {
                        OperationOutcome::Error(ErrorCategory::of(error))
                    }
                    _ => OperationOutcome::Error(ErrorCategory::Other),
                };
                self.record.outcome = outcome;
                self.record.bytes = (outcome == OperationOutcome::Success).then_some(self.done);
                self.record.duration = self.started.elapsed();
                self.journal.record(&self.record);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{
        EndTrigger, ErrorCategory, OperationKind, OperationOutcome, OperationRecord,
        SESSION_EVENTS_FILE, SESSION_OPERATIONS_FILE, SessionEvent, SessionEventKind, SessionLogs,
        round_trip_utc, without_user,
    };

    fn event(kind: SessionEventKind) -> SessionEvent {
        SessionEvent {
            protocol: "RDP",
            kind,
            host: "admin@corp@srv.lab".to_owned(),
            title: Some("Build \"box\"".to_owned()),
            reason: None,
            duration: None,
            end_trigger: None,
        }
    }

    fn operation(path: &str) -> OperationRecord {
        OperationRecord {
            kind: OperationKind::Upload,
            remote_path: path.to_owned(),
            remote_path_to: None,
            local_path: Some("C:\\work\\a.txt".to_owned()),
            bytes: Some(42),
            duration: Duration::from_millis(1500),
            outcome: OperationOutcome::Success,
            privileged: false,
        }
    }

    #[test]
    fn a_time_is_written_as_the_csharp_round_trip_format() {
        let at = UNIX_EPOCH + Duration::new(951_782_400 + 3_723, 123_456_789);
        assert_eq!(round_trip_utc(at), "2000-02-29T01:02:03.1234567Z");
        assert_eq!(round_trip_utc(UNIX_EPOCH), "1970-01-01T00:00:00.0000000Z");
    }

    #[test]
    fn the_host_loses_any_user_before_it_split_at_the_last_at_sign() {
        assert_eq!(without_user("alice@corp.example@srv"), "srv");
        assert_eq!(without_user("srv.lab"), "srv.lab");
    }

    #[test]
    fn a_connect_line_has_the_csharp_properties_in_order_and_leaves_out_what_is_absent() {
        let line = event(SessionEventKind::Connected).line(UNIX_EPOCH);
        assert_eq!(
            line,
            r#"{"ts":"1970-01-01T00:00:00.0000000Z","protocol":"RDP","event":"Connected","host":"srv.lab","title":"Build \"box\""}"#
        );
    }

    #[test]
    fn a_disconnect_line_says_why_how_long_and_what_ended_it() {
        let mut ended = event(SessionEventKind::Disconnected);
        ended.title = None;
        ended.reason = Some("RDP_ADMIN_DISCONNECT".to_owned());
        ended.duration = Some(Duration::from_millis(61_250));
        ended.end_trigger = Some(EndTrigger::Teardown);
        let parsed: serde_json::Value =
            serde_json::from_str(&ended.line(UNIX_EPOCH)).expect("JSON");
        assert_eq!(parsed["event"], "Disconnected");
        assert_eq!(parsed["reason"], "RDP_ADMIN_DISCONNECT");
        assert_eq!(parsed["durationMs"], 61_250);
        assert_eq!(parsed["endTrigger"], "teardown");
        assert!(parsed.get("title").is_none());
    }

    #[test]
    fn an_operation_line_has_the_csharp_properties_and_its_outcome() {
        let line = operation("/srv/a.txt").line("SFTP", "root@srv", UNIX_EPOCH);
        assert_eq!(
            line,
            r#"{"ts":"1970-01-01T00:00:00.0000000Z","protocol":"SFTP","op":"upload","host":"srv","remotePath":"/srv/a.txt","localPath":"C:\\work\\a.txt","bytes":42,"durationMs":1500,"result":"success"}"#
        );
        let mut failed = operation("/srv/a.txt");
        failed.kind = OperationKind::Rename;
        failed.remote_path_to = Some("/srv/b.txt".to_owned());
        failed.local_path = None;
        failed.bytes = None;
        failed.outcome = OperationOutcome::Error(ErrorCategory::Permission);
        failed.privileged = true;
        let parsed: serde_json::Value =
            serde_json::from_str(&failed.line("FTP", "srv", UNIX_EPOCH)).expect("JSON");
        assert_eq!(parsed["op"], "rename");
        assert_eq!(parsed["remotePathTo"], "/srv/b.txt");
        assert_eq!(parsed["result"], "error");
        assert_eq!(parsed["errorCategory"], "permission");
        assert_eq!(parsed["privileged"], true);
        assert!(parsed.get("bytes").is_none() && parsed.get("localPath").is_none());
    }

    #[test]
    fn a_path_that_would_break_the_line_stays_on_it_and_reads_back_whole() {
        let path = "/srv/evil\nname\r\u{2028}\u{202E}\u{1b}[2J\u{E0041}.txt";
        let line = operation(path).line("SFTP", "srv", UNIX_EPOCH);
        assert!(!line.contains('\n') && !line.contains('\r'), "{line}");
        assert!(
            line.chars()
                .all(|c| !c.is_control() && !crate::text::is_invisible(c)),
            "{line}"
        );
        let parsed: serde_json::Value = serde_json::from_str(&line).expect("JSON");
        assert_eq!(parsed["remotePath"], path);
    }

    #[test]
    fn lines_go_to_their_logs_and_continue_in_a_numbered_file_past_the_cap() {
        const CAP: u64 = 400;
        let folder = tempfile::tempdir().expect("folder");
        let logs = SessionLogs::with_max_bytes(CAP);
        for index in 0..5 {
            logs.operation(
                folder.path(),
                "SFTP",
                "srv",
                &operation(&format!("/srv/{index}")),
            );
        }
        logs.event(folder.path(), &event(SessionEventKind::Connected));
        logs.sync();
        let read =
            |name: &str| std::fs::read_to_string(folder.path().join(name)).unwrap_or_default();
        let parts = [
            read(SESSION_OPERATIONS_FILE),
            read("session-operations.1.log"),
            read("session-operations.2.log"),
        ];
        for part in &parts {
            assert!(!part.is_empty(), "{parts:?}");
            assert!(u64::try_from(part.len()).expect("small") <= CAP, "{part}");
            assert!(
                !part.starts_with('\u{FEFF}') && part.ends_with('\n'),
                "{part}"
            );
        }
        let paths: Vec<String> = parts
            .iter()
            .flat_map(|part| part.lines())
            .map(|line| {
                let parsed: serde_json::Value = serde_json::from_str(line).expect("JSON");
                parsed["remotePath"].as_str().expect("path").to_owned()
            })
            .collect();
        assert_eq!(paths, ["/srv/0", "/srv/1", "/srv/2", "/srv/3", "/srv/4"]);
        assert_eq!(read(SESSION_EVENTS_FILE).lines().count(), 1);
    }

    #[test]
    fn a_later_run_goes_on_in_the_last_continuation() {
        let folder = tempfile::tempdir().expect("folder");
        std::fs::write(folder.path().join(SESSION_OPERATIONS_FILE), "{}\n").expect("first");
        std::fs::write(folder.path().join("session-operations.1.log"), "{}\n").expect("second");
        let logs = SessionLogs::default();
        logs.operation(folder.path(), "SFTP", "srv", &operation("/srv/a"));
        drop(logs);
        let second = std::fs::read_to_string(folder.path().join("session-operations.1.log"))
            .expect("second");
        assert_eq!(second.lines().count(), 2, "{second}");
    }

    #[test]
    fn the_transcripts_retention_leaves_the_session_logs_alone() {
        let folder = tempfile::tempdir().expect("folder");
        for name in [
            SESSION_EVENTS_FILE,
            SESSION_OPERATIONS_FILE,
            "session-operations.1.log",
        ] {
            let path = folder.path().join(name);
            std::fs::write(&path, "{}\n").expect("written");
            let old = std::fs::File::options()
                .write(true)
                .open(&path)
                .expect("open");
            old.set_modified(UNIX_EPOCH).expect("aged");
        }
        let removed = crate::transcript::prune_expired(
            folder.path(),
            1,
            UNIX_EPOCH + Duration::from_hours(400 * 24),
        );
        assert_eq!(removed, 0);
        assert!(folder.path().join(SESSION_EVENTS_FILE).exists());
        assert!(folder.path().join("session-operations.1.log").exists());
    }

    #[cfg(unix)]
    #[test]
    fn only_the_owner_reads_a_session_log() {
        use std::os::unix::fs::PermissionsExt as _;

        let folder = tempfile::tempdir().expect("folder");
        let logs = SessionLogs::default();
        logs.event(folder.path(), &event(SessionEventKind::Connected));
        logs.sync();
        let mode = std::fs::metadata(folder.path().join(SESSION_EVENTS_FILE))
            .expect("written")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
