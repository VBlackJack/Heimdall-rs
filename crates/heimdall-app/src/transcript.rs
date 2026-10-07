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

//! A session's transcript, as the C# Heimdall's session log: its output as plain text in a
//! file of its own, named by protocol, host and start (UTC), between a header and a footer
//! the window words; continued in `.1.log`, `.2.log`... past a size cap. A file is never
//! written over, and a transcript dropped unfinished is finished then. On Unix only its owner may read it; on Windows it takes the rights of the
//! folder it is in.
//!
//! Transcripts older than the retention the settings give are removed at start, as the C#
//! `PruneExpiredTranscripts`: only the files this module writes, in the folder itself.

use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use heimdall_core::utc::UtcTime;
use heimdall_term::PlainText;

/// Largest a transcript file grows before it continues in the next, as the C# one.
pub const TRANSCRIPT_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Extension of a transcript file.
const EXTENSION: &str = "log";

/// Most names tried for a new transcript before giving up.
const MOST_NAMES: u32 = 10_000;

/// Line ending after the header and before the footer, the platform's.
const NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// The characters a file name cannot hold on Windows, left out on every platform.
const NOT_IN_NAMES: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// How every translation of a transcript's header opens, as the C# `HeaderMarker`: a file
/// that does not open with it was not written here, whatever its name.
pub const HEADER_MARKER: &str = "=====";

/// The byte order mark a file written by another tool may open with, before its text.
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Seconds in a day of retention.
const SECONDS_PER_DAY: u64 = 86_400;

/// Digits of the date and of the time in a transcript's name, `20260927_210503`.
const DATE_DIGITS: usize = 8;
const TIME_DIGITS: usize = 6;

/// The session a transcript is of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptContext {
    /// Its protocol, as the C# names it: SSH, TELNET, LOCAL.
    pub protocol: String,
    /// The host it reaches, or where it runs.
    pub host: String,
    /// Its title.
    pub title: String,
    /// When it started.
    pub started: SystemTime,
}

/// A transcript's first and last lines, worded by the window in its language.
#[derive(Clone)]
pub struct TranscriptLines {
    /// The header of the transcript of a session.
    pub header: Arc<dyn Fn(&TranscriptContext) -> String + Send + Sync>,
    /// The footer, from when it ended and how long it lasted.
    pub footer: Arc<dyn Fn(SystemTime, Duration) -> String + Send + Sync>,
}

impl fmt::Debug for TranscriptLines {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TranscriptLines")
    }
}

/// A transcript being written.
#[derive(Debug)]
pub struct Transcript {
    /// The first file, without its extension: continuations are named after it.
    stem: PathBuf,
    /// The first file.
    first: PathBuf,
    file: File,
    /// Bytes in the file being written.
    written: u64,
    /// Continuations opened.
    part: u32,
    text: PlainText,
    started: SystemTime,
    max_bytes: u64,
    lines: Option<TranscriptLines>,
    /// Its footer is written: nothing more goes in.
    finished: bool,
}

impl Transcript {
    /// Starts the transcript of `context` in `dir`, created if need be, with its header
    /// when `lines` word one; files grow to `max_bytes` before continuing.
    ///
    /// # Errors
    ///
    /// Returns the error of the folder or the file that could not be made.
    pub fn start(
        dir: &Path,
        context: &TranscriptContext,
        lines: Option<&TranscriptLines>,
        max_bytes: u64,
    ) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let name = file_name(context);
        let (first, file) = create_unique(dir, &name)?;
        let mut transcript = Self {
            stem: first.with_extension(""),
            first,
            file,
            written: 0,
            part: 0,
            text: PlainText::new(),
            started: context.started,
            max_bytes,
            lines: lines.cloned(),
            finished: false,
        };
        if let Some(lines) = lines {
            transcript.put(&format!("{}{NEWLINE}", (lines.header)(context)))?;
        }
        Ok(transcript)
    }

    /// The first file written.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.first
    }

    /// Adds the text of `bytes`, a chunk of the session's output.
    ///
    /// # Errors
    ///
    /// Returns the error of the file that could not be written.
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        let text = self.text.feed(bytes);
        self.put(&text)
    }

    /// Ends the transcript at `now`: what the output left unfinished, then the footer when
    /// its lines word one. Once only: a second call writes nothing.
    ///
    /// # Errors
    ///
    /// Returns the error of the file that could not be written.
    pub fn finish(&mut self, now: SystemTime) -> io::Result<()> {
        if std::mem::replace(&mut self.finished, true) {
            return Ok(());
        }
        let rest = self.text.finish();
        self.put(&rest)?;
        if let Some(lines) = self.lines.clone() {
            let lasted = now.duration_since(self.started).unwrap_or_default();
            let footer = format!("{NEWLINE}{}{NEWLINE}", (lines.footer)(now, lasted));
            self.put(&footer)?;
        }
        self.file.flush()
    }

    /// Writes `text`, continuing in a new file first when it would pass the cap; a chunk
    /// larger than the cap fills a file of its own.
    fn put(&mut self, text: &str) -> io::Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let size = u64::try_from(text.len()).unwrap_or(u64::MAX);
        if self.written > 0 && self.written.saturating_add(size) > self.max_bytes {
            self.part += 1;
            let path = PathBuf::from(format!("{}.{}.{EXTENSION}", self.stem.display(), self.part));
            self.file = open(&path, false)?;
            self.written = self.file.metadata()?.len();
        }
        self.file.write_all(text.as_bytes())?;
        self.written += size;
        Ok(())
    }
}

impl Drop for Transcript {
    /// A transcript dropped unfinished, its tab closed or the application leaving, is
    /// finished now; a failure then has no one left to be told.
    fn drop(&mut self) {
        let _ = self.finish(SystemTime::now());
    }
}

/// `PROTOCOL_host_20260927_210503.log`, what a file name cannot hold made `_`.
fn file_name(context: &TranscriptContext) -> String {
    format!(
        "{}_{}_{}",
        context.protocol,
        context.host,
        UtcTime::of(context.started).compact()
    )
    .chars()
    .map(|c| {
        if c.is_control() || NOT_IN_NAMES.contains(&c) {
            '_'
        } else {
            c
        }
    })
    .collect()
}

/// A new file `name.log` in `dir`, or `name_1.log`, `name_2.log`... when taken.
fn create_unique(dir: &Path, name: &str) -> io::Result<(PathBuf, File)> {
    for tried in 0..MOST_NAMES {
        let path = if tried == 0 {
            dir.join(format!("{name}.{EXTENSION}"))
        } else {
            dir.join(format!("{name}_{tried}.{EXTENSION}"))
        };
        match open(&path, true) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::from(io::ErrorKind::AlreadyExists))
}

/// Opens `path` to add to it, made if need be; `new` refuses one already there. On Unix
/// only its owner may read it.
fn open(path: &Path, new: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.append(true);
    if new {
        options.create_new(true);
    } else {
        options.create(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Removes from `folder` the transcripts whose newest file was last written more than
/// `retention_days` days before `now`, as the C# `PruneExpiredTranscripts`; 0 keeps every
/// one. Only the files named as this module names them, the first opening with
/// [`HEADER_MARKER`], are touched, and only in `folder` itself: no folder inside it is
/// entered and no link followed. A transcript and its continuations go together, once the
/// newest of them has expired. Returns how many files were removed, and logs it; a file that
/// cannot be removed stays for the next start.
pub fn prune_expired(folder: &Path, retention_days: u32, now: SystemTime) -> usize {
    if retention_days == 0 {
        return 0;
    }
    let kept = Duration::from_secs(u64::from(retention_days) * SECONDS_PER_DAY);
    let Some(cutoff) = now.checked_sub(kept) else {
        return 0;
    };
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return 0,
        Err(error) => {
            log::warn!(
                "transcript retention could not list {}: {error}",
                folder.display()
            );
            return 0;
        }
    };
    let mut transcripts: HashMap<String, Vec<FoundFile>> = HashMap::new();
    for entry in entries.flatten() {
        // The entry itself, a link never followed: a link or a folder is left alone.
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let name = entry.file_name();
        let Some((stem, first)) = name.to_str().and_then(transcript_stem) else {
            continue;
        };
        transcripts
            .entry(stem.to_owned())
            .or_default()
            .push(FoundFile {
                path: entry.path(),
                first,
                modified: entry.metadata().and_then(|found| found.modified()).ok(),
            });
    }
    let mut removed = 0;
    for files in transcripts.into_values() {
        let expired = files
            .iter()
            .all(|file| file.modified.is_some_and(|modified| modified < cutoff));
        let ours = files
            .iter()
            .any(|file| file.first && opens_with_header(&file.path));
        if !expired || !ours {
            continue;
        }
        for file in files {
            match fs::remove_file(&file.path) {
                Ok(()) => removed += 1,
                Err(error) => log::warn!(
                    "transcript retention could not remove {}: {error}",
                    file.path.display()
                ),
            }
        }
    }
    if removed > 0 {
        log::info!(
            "transcript retention removed {removed} file(s) older than {retention_days} day(s)"
        );
    }
    removed
}

/// A file of a transcript, found in its folder.
struct FoundFile {
    path: PathBuf,
    /// The transcript's first file, which holds its header, not a continuation.
    first: bool,
    /// When it was last written, when the system says.
    modified: Option<SystemTime>,
}

/// The transcript file `name` belongs to, named without its continuation's number and its
/// extension, and whether it is the first file: `name` as [`file_name`], [`create_unique`] and
/// a continuation make it, `..._20260927_210503[_N][.N].log`, as the C# `TranscriptFileName`
/// reads it. `None` for any other name.
fn transcript_stem(name: &str) -> Option<(&str, bool)> {
    let (base, extension) = name.rsplit_once('.')?;
    if !extension.eq_ignore_ascii_case(EXTENSION) {
        return None;
    }
    let (stem, first) = match base.rsplit_once('.') {
        Some((stem, part)) if is_number(part) => (stem, false),
        _ => (base, true),
    };
    let parts: Vec<&str> = stem.rsplitn(4, '_').collect();
    let stamped = |date: &str, time: &str| {
        date.len() == DATE_DIGITS && is_number(date) && time.len() == TIME_DIGITS && is_number(time)
    };
    // Something before the date, then the date and the time, then a name's number if taken.
    let named = match parts.as_slice() {
        [time, date, _, ..] if stamped(date, time) => true,
        [taken, time, date, _] => is_number(taken) && stamped(date, time),
        _ => false,
    };
    named.then_some((stem, first))
}

/// Whether `text` is digits only, one at least.
fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// Whether the file at `path` opens with a transcript's header, after a byte order mark if
/// it has one; a file that cannot be read is not claimed.
fn opens_with_header(path: &Path) -> bool {
    use std::io::Read as _;

    let wanted = UTF8_BOM.len() + HEADER_MARKER.len();
    let mut start = Vec::with_capacity(wanted);
    let Ok(file) = File::open(path) else {
        return false;
    };
    if file
        .take(u64::try_from(wanted).unwrap_or(u64::MAX))
        .read_to_end(&mut start)
        .is_err()
    {
        return false;
    }
    start
        .strip_prefix(UTF8_BOM)
        .unwrap_or(&start)
        .starts_with(HEADER_MARKER.as_bytes())
}
