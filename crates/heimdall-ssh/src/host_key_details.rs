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

//! What the C# Heimdall's `HostKeyEntry` says of a trusted key besides the key: where it
//! came from, when it was first trusted, when a server last presented it. Kept in a file of
//! its own beside `known_hosts`, which OpenSSH reads and whose format stays OpenSSH's.
//!
//! One line per key: `<pattern> SHA256:<fingerprint> source=<user|imported|unknown>
//! first=<unix seconds> last=<unix seconds>`, a date left out when it is not known. The
//! file decides nothing: `known_hosts` and its pins alone say which keys are trusted, and a
//! line whose key they no longer trust is dropped at the next write. A line read but not
//! understood, and an attribute not known, are kept as they are; a line longer than
//! [`MAX_LINE`] is dropped, and a file larger than [`MAX_FILE_BYTES`] is read as empty and
//! replaced at the next write. Written under the lock of the trust files and replaced
//! whole, as the pins are; a failed write is said in the log, and never fails a connection.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use russh::keys::PublicKey;

use crate::known_hosts::{KnownHosts, KnownHostsError, fingerprint, plain_host, validate_host};
use crate::pins::{is_fingerprint, pattern};
use crate::trust_files::{self, TrustLock};

/// Added to the `known_hosts` file name for the file of details.
const DETAILS_SUFFIX: &str = ".details";

/// How old the last seen time of a key must be before a check that went through writes it
/// again: a server reached over and over rewrites the file once a minute at most.
pub const LAST_SEEN_RESOLUTION: Duration = Duration::from_secs(60);

/// Largest file read: a larger one is read as empty, and replaced at the next write.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Longest line read: a longer one is dropped.
pub const MAX_LINE: usize = 4096;

/// The attribute of the source.
const SOURCE: &str = "source";
/// The attribute of the time the key was first trusted.
const FIRST: &str = "first";
/// The attribute of the time a server last presented the key.
const LAST: &str = "last";
/// The source of a key the user accepted.
const SOURCE_USER: &str = "user";
/// The source of a key imported from a `known_hosts` file.
const SOURCE_IMPORTED: &str = "imported";
/// The source of a key whose origin is not known.
const SOURCE_UNKNOWN: &str = "unknown";

/// Where a trusted key came from, as the C# `HostKeySource` says it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum HostKeySource {
    /// The user accepted it: the C# `UserConfirmed`.
    User,
    /// Imported from a `known_hosts` file: the C# `ImportedKnownHosts`.
    Imported,
    /// Not known: a key trusted before the details were kept, or one the C# shipped with.
    #[default]
    Unknown,
}

impl HostKeySource {
    /// As the file writes it.
    fn name(self) -> &'static str {
        match self {
            Self::User => SOURCE_USER,
            Self::Imported => SOURCE_IMPORTED,
            Self::Unknown => SOURCE_UNKNOWN,
        }
    }

    /// As the file writes it; a value not known reads as unknown.
    fn named(name: &str) -> Self {
        match name {
            SOURCE_USER => Self::User,
            SOURCE_IMPORTED => Self::Imported,
            _ => Self::Unknown,
        }
    }
}

/// What is known of a trusted key besides the key, as the C# `HostKeyEntry` keeps it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HostKeyDetails {
    /// Where it came from.
    pub source: HostKeySource,
    /// When it was first trusted; `None` when not known.
    pub first_seen: Option<SystemTime>,
    /// When a server last presented it; `None` when not known.
    pub last_seen: Option<SystemTime>,
}

/// A key of a server: its host in lower case, its port, its fingerprint.
pub(crate) type ServerKey = (String, u16, String);

/// The details of one key, as a line of the file holds them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    host: String,
    port: u16,
    fingerprint: String,
    source: HostKeySource,
    first: Option<u64>,
    last: Option<u64>,
    /// Attributes not known, kept as read.
    other: Vec<String>,
}

impl Row {
    fn new(host: &str, port: u16, fingerprint: &str, source: HostKeySource) -> Self {
        Self {
            host: host.to_owned(),
            port,
            fingerprint: fingerprint.to_owned(),
            source,
            first: None,
            last: None,
            other: Vec::new(),
        }
    }

    fn is(&self, host: &str, port: u16, fingerprint: &str) -> bool {
        self.host == host && self.port == port && self.fingerprint == fingerprint
    }

    fn key(&self) -> ServerKey {
        (self.host.clone(), self.port, self.fingerprint.clone())
    }

    fn details(&self) -> HostKeyDetails {
        HostKeyDetails {
            source: self.source,
            first_seen: self.first.map(time_of),
            last_seen: self.last.map(time_of),
        }
    }

    /// The line read, when it reads as one: a pattern naming one host plainly, a
    /// fingerprint, then attributes.
    fn parse(line: &str) -> Option<Self> {
        let mut fields = line.split_whitespace();
        let (host, port) = plain_host(fields.next()?)?;
        let host = validate_host(&host).ok()?;
        let fingerprint = fields.next().filter(|field| is_fingerprint(field))?;
        let mut row = Self::new(&host, port, fingerprint, HostKeySource::Unknown);
        let (mut source, mut first, mut last) = (None, None, None);
        for field in fields {
            match field.split_once('=') {
                Some((SOURCE, value)) => {
                    if source.is_none() {
                        source = Some(HostKeySource::named(value));
                    }
                }
                Some((FIRST, value)) => {
                    if first.is_none() {
                        first = value.parse().ok();
                    }
                }
                Some((LAST, value)) => {
                    if last.is_none() {
                        last = value.parse().ok();
                    }
                }
                _ => row.other.push(field.to_owned()),
            }
        }
        row.source = source.unwrap_or_default();
        row.first = first;
        row.last = last;
        Some(row)
    }

    /// The line written.
    fn line(&self) -> String {
        let mut line = format!(
            "{} {} {SOURCE}={}",
            pattern(&self.host, self.port),
            self.fingerprint,
            self.source.name()
        );
        // Writing into a `String` cannot fail.
        if let Some(first) = self.first {
            let _ = write!(line, " {FIRST}={first}");
        }
        if let Some(last) = self.last {
            let _ = write!(line, " {LAST}={last}");
        }
        for other in &self.other {
            line.push(' ');
            line.push_str(other);
        }
        line
    }
}

/// A line of the file: details read, or a line kept as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    Row(Row),
    Kept(String),
}

/// The file as read: its lines, its text, and whether it was too large to read.
#[derive(Debug, Default)]
struct Read {
    lines: Vec<Line>,
    text: String,
    oversized: bool,
}

/// What a write changes for a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Change<'a> {
    /// The key is trusted, from `source`, at `now`: first seen kept when known, last seen
    /// `now`.
    Trusted {
        host: &'a str,
        port: u16,
        fingerprint: &'a str,
        source: HostKeySource,
        now: u64,
    },
    /// The key carried over from the C# Heimdall, with what the C# knew of it.
    Carried {
        host: &'a str,
        port: u16,
        fingerprint: &'a str,
        source: HostKeySource,
        first: Option<u64>,
        last: Option<u64>,
    },
    /// A server presented the key at `now`, a trusted one: last seen `now`, unless what is
    /// recorded is less than `resolution` seconds older. Nothing for a key not trusted in
    /// `known_hosts` or its pins: one trusted for this run alone.
    Seen {
        host: &'a str,
        port: u16,
        fingerprint: &'a str,
        now: u64,
        resolution: u64,
    },
    /// The server's keys are forgotten.
    Forget { host: &'a str, port: u16 },
}

/// The details kept beside a `known_hosts` file.
#[derive(Debug, Clone)]
pub(crate) struct DetailsFile {
    path: PathBuf,
}

impl DetailsFile {
    /// The details kept beside `known_hosts`.
    pub(crate) fn beside(known_hosts: &Path) -> Self {
        let mut name = known_hosts.as_os_str().to_owned();
        name.push(DETAILS_SUFFIX);
        Self {
            path: PathBuf::from(name),
        }
    }

    /// File read and written.
    #[cfg(test)]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The details of every key: the first line of a key when it has more. A file that
    /// cannot be read is said in the log and reads as empty: every key's details unknown.
    pub(crate) fn details(&self) -> HashMap<ServerKey, HostKeyDetails> {
        let read = self.read().unwrap_or_else(|error| {
            log::warn!(
                "{}: the details of the trusted host keys cannot be read: {error}",
                self.path.display()
            );
            Read::default()
        });
        let mut details = HashMap::new();
        for line in read.lines {
            if let Line::Row(row) = line {
                details.entry(row.key()).or_insert_with(|| row.details());
            }
        }
        details
    }

    /// Applies `changes`, and drops the lines of keys `known_hosts` and its pins no longer
    /// trust; the file is replaced whole when that changes it, never written otherwise.
    /// Whether it was written. The lock of the trust files is held by the caller, from its
    /// own write of `known_hosts` to this one.
    ///
    /// # Errors
    ///
    /// `known_hosts`, its pins or the details cannot be read, or the details written: the
    /// file is left as it was.
    pub(crate) fn apply(
        &self,
        _lock: &TrustLock,
        known_hosts: &KnownHosts,
        changes: &[Change<'_>],
    ) -> Result<bool, KnownHostsError> {
        let trusted: HashSet<ServerKey> = known_hosts
            .listed()?
            .into_iter()
            .map(|entry| {
                (
                    entry.host.to_ascii_lowercase(),
                    entry.port,
                    entry.fingerprint,
                )
            })
            .collect();
        let read = self.read().map_err(|source| KnownHostsError::Unreadable {
            path: self.path.clone(),
            source,
        })?;
        let mut lines = read.lines;
        for change in changes {
            change.apply_to(&mut lines, &trusted);
        }
        let mut written = HashSet::new();
        let mut text = String::new();
        for line in &lines {
            let line = match line {
                Line::Row(row) if trusted.contains(&row.key()) && written.insert(row.key()) => {
                    row.line()
                }
                Line::Row(_) => continue,
                Line::Kept(kept) => kept.clone(),
            };
            text.push_str(&line);
            text.push('\n');
        }
        if text == read.text && !read.oversized {
            return Ok(false);
        }
        trust_files::replace(&self.path, &text).map_err(|_| KnownHostsError::WriteFailed {
            path: self.path.clone(),
        })?;
        Ok(true)
    }

    /// The file's lines, at most [`MAX_FILE_BYTES`] of them: none when it does not exist.
    fn read(&self) -> io::Result<Read> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Read::default()),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
            log::warn!(
                "{}: larger than {MAX_FILE_BYTES} bytes, read as empty and replaced at the next write",
                self.path.display()
            );
            return Ok(Read {
                oversized: true,
                ..Read::default()
            });
        }
        let text = String::from_utf8(bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned());
        let lines = text
            .lines()
            .filter(|line| line.len() <= MAX_LINE && !line.trim().is_empty())
            .map(|line| Row::parse(line).map_or_else(|| Line::Kept(line.to_owned()), Line::Row))
            .collect();
        Ok(Read {
            lines,
            text,
            oversized: false,
        })
    }
}

impl Change<'_> {
    /// Applies the change to `lines`, `trusted` the keys `known_hosts` and its pins trust.
    fn apply_to(&self, lines: &mut Vec<Line>, trusted: &HashSet<ServerKey>) {
        match *self {
            Self::Trusted {
                host,
                port,
                fingerprint,
                source,
                now,
            } => {
                let row = row_of(lines, host, port, fingerprint, source);
                row.source = source;
                row.first = row.first.or(Some(now));
                row.last = Some(now);
            }
            Self::Carried {
                host,
                port,
                fingerprint,
                source,
                first,
                last,
            } => {
                let row = row_of(lines, host, port, fingerprint, source);
                row.source = source;
                row.first = first;
                row.last = last;
            }
            Self::Seen {
                host,
                port,
                fingerprint,
                now,
                resolution,
            } => {
                let host = host.to_ascii_lowercase();
                if !trusted.contains(&(host.clone(), port, fingerprint.to_owned())) {
                    return;
                }
                let row = row_of(lines, &host, port, fingerprint, HostKeySource::Unknown);
                let recent = row
                    .last
                    .is_some_and(|last| now >= last && now - last < resolution);
                if !recent {
                    row.last = Some(now);
                }
            }
            Self::Forget { host, port } => {
                let host = host.to_ascii_lowercase();
                lines.retain(
                    |line| !matches!(line, Line::Row(row) if row.host == host && row.port == port),
                );
            }
        }
    }
}

/// The row of a key in `lines`, added from `source` with no dates when there is none.
fn row_of<'a>(
    lines: &'a mut Vec<Line>,
    host: &str,
    port: u16,
    fingerprint: &str,
    source: HostKeySource,
) -> &'a mut Row {
    let host = host.to_ascii_lowercase();
    let index = lines
        .iter()
        .position(|line| matches!(line, Line::Row(row) if row.is(&host, port, fingerprint)))
        .unwrap_or_else(|| {
            lines.push(Line::Row(Row::new(&host, port, fingerprint, source)));
            lines.len() - 1
        });
    match &mut lines[index] {
        Line::Row(row) => row,
        Line::Kept(_) => unreachable!("the index found or pushed is a row"),
    }
}

/// Seconds since 1970, as the file writes a time.
pub(crate) fn unix_seconds(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|since| since.as_secs())
}

/// The time the file wrote as seconds since 1970.
fn time_of(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

/// Now, as the file writes a time.
pub(crate) fn now() -> u64 {
    unix_seconds(SystemTime::now()).unwrap_or_default()
}

/// Applies `changes` to the details beside `known_hosts`, the lock held by the caller; a
/// failure is said in the log and goes no further: the details never fail what wrote the
/// key.
pub(crate) fn record(lock: &TrustLock, known_hosts: &KnownHosts, changes: &[Change<'_>]) {
    if let Err(error) = DetailsFile::beside(known_hosts.path()).apply(lock, known_hosts, changes) {
        log::warn!("the details of a trusted host key are not recorded: {error}");
    }
}

/// Records that `host` on `port` presented `key`, a trusted one, now: later, on a thread of
/// its own, never on the connection's. Nothing when its last seen time is recent, or when
/// the key is trusted for this run alone.
pub(crate) fn seen_later(known_hosts: &KnownHosts, host: &str, port: u16, key: &PublicKey) {
    let (known_hosts, host, fingerprint) = (known_hosts.clone(), host.to_owned(), fingerprint(key));
    let seen = move || {
        let now = now();
        let resolution = LAST_SEEN_RESOLUTION.as_secs();
        // Read without the lock first: a recent time is the usual case, and is left.
        let recorded = DetailsFile::beside(known_hosts.path())
            .details()
            .get(&(host.clone(), port, fingerprint.clone()))
            .and_then(|details| details.last_seen)
            .and_then(unix_seconds);
        if recorded.is_some_and(|last| now >= last && now - last < resolution) {
            return;
        }
        let lock = trust_files::lock();
        record(
            &lock,
            &known_hosts,
            &[Change::Seen {
                host: &host,
                port,
                fingerprint: &fingerprint,
                now,
                resolution,
            }],
        );
    };
    match tokio::runtime::Handle::try_current() {
        Ok(runtime) => drop(runtime.spawn_blocking(seen)),
        Err(_) => seen(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pins::Pins;

    const KEY: &str = include_str!("../tests/fixtures/hostkeys/host-ed25519.pub");
    const OTHER: &str = include_str!("../tests/fixtures/hostkeys/host-ed25519-other.pub");

    fn key(text: &str) -> PublicKey {
        PublicKey::from_openssh(text.trim()).expect("key")
    }

    /// A `known_hosts` in a folder of its own, `web.lab` trusting [`KEY`].
    fn store() -> (tempfile::TempDir, KnownHosts, DetailsFile) {
        let dir = tempfile::tempdir().expect("dir");
        let known = KnownHosts::new(dir.path().join("known_hosts"));
        known.learn("web.lab", 22, &key(KEY)).expect("learn");
        let details = DetailsFile::beside(known.path());
        (dir, known, details)
    }

    fn apply(known: &KnownHosts, details: &DetailsFile, changes: &[Change<'_>]) -> bool {
        details
            .apply(&trust_files::lock(), known, changes)
            .expect("applied")
    }

    fn trusted<'a>(host: &'a str, port: u16, print: &'a str, now: u64) -> Change<'a> {
        Change::Trusted {
            host,
            port,
            fingerprint: print,
            source: HostKeySource::User,
            now,
        }
    }

    fn seen(print: &str, now: u64) -> Change<'_> {
        Change::Seen {
            host: "web.lab",
            port: 22,
            fingerprint: print,
            now,
            resolution: LAST_SEEN_RESOLUTION.as_secs(),
        }
    }

    fn of(details: &DetailsFile, host: &str, port: u16, print: &str) -> Option<HostKeyDetails> {
        details
            .details()
            .get(&(host.to_owned(), port, print.to_owned()))
            .copied()
    }

    fn at(seconds: u64) -> SystemTime {
        time_of(seconds)
    }

    #[test]
    fn a_line_reads_back_as_written_and_unknown_lines_and_attributes_stay() {
        let (_dir, known, details) = store();
        let print = fingerprint(&key(KEY));
        std::fs::write(
            details.path(),
            format!(
                "# a comment\nWEB.lab {print} source=imported first=100 last=200 colour=blue\n\
                 not a line at all\n\n"
            ),
        )
        .expect("seeded");
        assert_eq!(
            of(&details, "web.lab", 22, &print),
            Some(HostKeyDetails {
                source: HostKeySource::Imported,
                first_seen: Some(at(100)),
                last_seen: Some(at(200)),
            })
        );
        assert!(apply(&known, &details, &[seen(&print, 1_000)]));
        assert_eq!(
            std::fs::read_to_string(details.path()).expect("read"),
            format!(
                "# a comment\nweb.lab {print} source=imported first=100 last=1000 colour=blue\n\
                 not a line at all\n"
            ),
            "the comment, the attribute and the line not understood kept"
        );
        assert!(
            !apply(&known, &details, &[]),
            "a file read as written is not written again"
        );
    }

    #[test]
    fn dates_not_known_are_left_out_and_read_as_unknown() {
        let (_dir, known, details) = store();
        let print = fingerprint(&key(KEY));
        std::fs::write(
            details.path(),
            format!("web.lab {print} source=factory first=yesterday\n"),
        )
        .expect("seeded");
        assert_eq!(
            of(&details, "web.lab", 22, &print),
            Some(HostKeyDetails::default()),
            "an unknown source and a date that is none"
        );
        assert_eq!(of(&details, "db.lab", 22, &print), None, "no line");
        assert!(apply(&known, &details, &[]));
        assert_eq!(
            std::fs::read_to_string(details.path()).expect("read"),
            format!("web.lab {print} source=unknown\n")
        );
    }

    #[test]
    fn a_line_too_long_is_dropped_and_a_file_too_large_is_read_as_empty_then_replaced() {
        let (_dir, known, details) = store();
        let print = fingerprint(&key(KEY));
        let long = format!("web.lab {print} source=user {}", "x".repeat(MAX_LINE));
        std::fs::write(details.path(), format!("{long}\n")).expect("seeded");
        assert_eq!(
            of(&details, "web.lab", 22, &print),
            None,
            "too long to read"
        );
        assert!(apply(&known, &details, &[]), "dropped");
        assert_eq!(std::fs::read_to_string(details.path()).expect("read"), "");

        let line = format!("web.lab {print} source=user first=1 last=1\n");
        let lines = usize::try_from(MAX_FILE_BYTES).expect("size") / line.len() + 1;
        std::fs::write(details.path(), line.repeat(lines)).expect("seeded");
        assert_eq!(of(&details, "web.lab", 22, &print), None, "read as empty");
        assert!(apply(
            &known,
            &details,
            &[trusted("web.lab", 22, &print, 50)]
        ));
        assert_eq!(
            std::fs::read_to_string(details.path()).expect("read"),
            format!("web.lab {print} source=user first=50 last=50\n"),
            "replaced"
        );
    }

    #[test]
    fn a_key_trusted_again_keeps_when_it_was_first_trusted() {
        let (_dir, known, details) = store();
        let print = fingerprint(&key(KEY));
        apply(&known, &details, &[trusted("web.lab", 22, &print, 100)]);
        apply(&known, &details, &[trusted("Web.Lab", 22, &print, 200)]);
        assert_eq!(
            of(&details, "web.lab", 22, &print),
            Some(HostKeyDetails {
                source: HostKeySource::User,
                first_seen: Some(at(100)),
                last_seen: Some(at(200)),
            })
        );
    }

    #[test]
    fn details_join_on_host_port_and_fingerprint_and_orphans_go_at_the_next_write() {
        let (_dir, known, details) = store();
        let print = fingerprint(&key(KEY));
        let other = fingerprint(&key(OTHER));
        std::fs::write(
            details.path(),
            format!(
                "web.lab {print} source=user first=1 last=2\n\
                 web.lab {other} source=user first=3 last=4\n\
                 [web.lab]:2222 {print} source=user first=5 last=6\n\
                 gone.lab {print} source=user first=7 last=8\n\
                 web.lab {print} source=imported first=9 last=10\n"
            ),
        )
        .expect("seeded");
        let listed = known.entries().expect("listed");
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].details,
            HostKeyDetails {
                source: HostKeySource::User,
                first_seen: Some(at(1)),
                last_seen: Some(at(2)),
            },
            "the line of that host, port and key; the first of two"
        );
        assert!(apply(&known, &details, &[]));
        assert_eq!(
            std::fs::read_to_string(details.path()).expect("read"),
            format!("web.lab {print} source=user first=1 last=2\n"),
            "another key, another port, a host forgotten and a second line: dropped"
        );
    }

    #[test]
    fn a_key_trusted_without_details_lists_as_unknown() {
        let (_dir, known, _details) = store();
        let listed = known.entries().expect("listed");
        assert_eq!(listed[0].details, HostKeyDetails::default());
        assert_eq!(listed[0].details.source, HostKeySource::Unknown);
        assert!(listed[0].public_key.is_some(), "the key from known_hosts");
    }

    #[test]
    fn last_seen_is_written_once_a_minute_at_most() {
        let (_dir, known, details) = store();
        let print = fingerprint(&key(KEY));
        assert!(
            apply(&known, &details, &[seen(&print, 1_000)]),
            "first seen"
        );
        assert_eq!(
            of(&details, "web.lab", 22, &print),
            Some(HostKeyDetails {
                source: HostKeySource::Unknown,
                first_seen: None,
                last_seen: Some(at(1_000)),
            }),
            "never an invented first seen"
        );
        assert!(
            !apply(&known, &details, &[seen(&print, 1_059)]),
            "less than a minute later: nothing written"
        );
        assert!(
            apply(&known, &details, &[seen(&print, 1_060)]),
            "a minute later"
        );
        assert_eq!(
            of(&details, "web.lab", 22, &print).and_then(|found| found.last_seen),
            Some(at(1_060))
        );
        assert!(
            apply(&known, &details, &[seen(&print, 500)]),
            "a clock set back"
        );
    }

    #[test]
    fn a_key_trusted_for_this_run_alone_records_nothing() {
        let (_dir, known, details) = store();
        let other = fingerprint(&key(OTHER));
        assert!(!apply(&known, &details, &[seen(&other, 1_000)]));
        assert!(!details.path().exists(), "no file for nothing");
    }

    #[test]
    fn a_pin_has_its_details_kept_and_its_last_seen_raised() {
        let (_dir, known, details) = store();
        let print = fingerprint(&key(OTHER));
        Pins::beside(known.path())
            .pin("pinned.lab", 22, &print)
            .expect("pinned");
        apply(
            &known,
            &details,
            &[Change::Carried {
                host: "pinned.lab",
                port: 22,
                fingerprint: &print,
                source: HostKeySource::Imported,
                first: Some(10),
                last: Some(20),
            }],
        );
        apply(
            &known,
            &details,
            &[Change::Seen {
                host: "pinned.lab",
                port: 22,
                fingerprint: &print,
                now: 1_000,
                resolution: 0,
            }],
        );
        assert_eq!(
            of(&details, "pinned.lab", 22, &print),
            Some(HostKeyDetails {
                source: HostKeySource::Imported,
                first_seen: Some(at(10)),
                last_seen: Some(at(1_000)),
            })
        );
    }

    #[test]
    fn forgetting_a_server_removes_its_lines() {
        let (_dir, known, details) = store();
        let print = fingerprint(&key(KEY));
        known.learn("web.lab", 2222, &key(KEY)).expect("learn");
        apply(
            &known,
            &details,
            &[
                trusted("web.lab", 22, &print, 1),
                trusted("web.lab", 2222, &print, 1),
            ],
        );
        assert!(known.forget("WEB.lab", 22).expect("forgotten"));
        assert_eq!(of(&details, "web.lab", 22, &print), None);
        assert!(
            of(&details, "web.lab", 2222, &print).is_some(),
            "another port"
        );
    }

    #[test]
    fn concurrent_writers_under_the_lock_lose_no_line() {
        const WRITERS: usize = 6;
        const SERVERS_PER_WRITER: usize = 10;

        let (_dir, known, details) = store();
        let print = fingerprint(&key(KEY));
        std::thread::scope(|scope| {
            for writer in 0..WRITERS {
                let (known, print) = (&known, &print);
                scope.spawn(move || {
                    for server in 0..SERVERS_PER_WRITER {
                        let host = format!("key-{writer}-{server}.lab");
                        let lock = trust_files::lock();
                        known
                            .learn_locked(&lock, &host, 22, &key(KEY))
                            .expect("learn");
                        record(&lock, known, &[trusted(&host, 22, print, 7)]);
                    }
                });
            }
        });
        let written = details.details();
        for writer in 0..WRITERS {
            for server in 0..SERVERS_PER_WRITER {
                let host = format!("key-{writer}-{server}.lab");
                assert_eq!(
                    written
                        .get(&(host.clone(), 22, print.clone()))
                        .and_then(|found| found.first_seen),
                    Some(at(7)),
                    "{host}"
                );
            }
        }
    }
}
