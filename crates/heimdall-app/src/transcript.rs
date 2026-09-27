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
