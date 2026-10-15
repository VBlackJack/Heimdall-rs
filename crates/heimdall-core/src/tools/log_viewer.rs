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

//! A log file read for the log viewer, as the C# `LogViewerView`
//! (`Views/Tools/LogViewerView.xaml.cs`) reads it: from a byte offset to the end, the lines
//! split on `\n` with their `\r` dropped, the file read again from its start when it got
//! shorter, the newest 10,000 lines kept.
//!
//! Unlike the C#, which reads the whole file, at most [`MAX_READ_BYTES`] are read at once:
//! the end of a larger file, or of what was appended since the last read. A last line not
//! yet ended is read again once it is, rather than shown twice.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, SystemTime};

use regex::{Regex, RegexBuilder};

/// Lines kept, the newest ones, as the C# `MaxDisplayLines`.
pub const MAX_DISPLAY_LINES: usize = 10_000;

/// How often a tailed file is looked at, as the C# `TailIntervalMs`.
pub const TAIL_INTERVAL: Duration = Duration::from_millis(500);

/// The most bytes read at once: a file, or what was appended to it, larger than this is
/// read from this many bytes before its end, so a multi-gigabyte log never fills memory.
pub const MAX_READ_BYTES: u64 = 16 * 1024 * 1024;

/// The UTF-8 byte order mark.
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// The UTF-16 little-endian byte order mark.
const UTF16LE_BOM: &[u8] = &[0xFF, 0xFE];

/// The longest byte order mark looked for.
const MAX_BOM_LENGTH: usize = 3;

/// Bytes of a UTF-16 code unit.
const UTF16_UNIT: usize = 2;

/// What ASCII puts in place of a byte it does not have, as .NET's `Encoding.ASCII`.
const ASCII_REPLACEMENT: char = '?';

/// The encodings the viewer reads, as the C# encoding list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogEncoding {
    /// UTF-8, the C# default; a byte that is not UTF-8 reads as U+FFFD.
    #[default]
    Utf8,
    /// 7-bit ASCII; a byte above 0x7F reads as `?`, as .NET's.
    Ascii,
    /// UTF-16 little-endian, .NET's `Encoding.Unicode`.
    Utf16Le,
    /// Windows-1252, the Western European Windows code page.
    Windows1252,
}

impl LogEncoding {
    /// Every encoding, in the C# list's order.
    pub const ALL: [Self; 4] = [Self::Utf8, Self::Ascii, Self::Utf16Le, Self::Windows1252];

    /// `bytes`, one line without its end, as text.
    #[must_use]
    pub fn decode(self, bytes: &[u8]) -> String {
        match self {
            Self::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
            Self::Ascii => bytes
                .iter()
                .map(|&byte| {
                    if byte.is_ascii() {
                        char::from(byte)
                    } else {
                        ASCII_REPLACEMENT
                    }
                })
                .collect(),
            Self::Utf16Le => {
                let units: Vec<u16> = bytes
                    .chunks_exact(UTF16_UNIT)
                    .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
                    .collect();
                String::from_utf16_lossy(&units)
            }
            Self::Windows1252 => crate::code_page::decode_windows_1252(bytes),
        }
    }

    /// The bytes that end a line: `\n`, a whole code unit in UTF-16.
    fn newline(self) -> &'static [u8] {
        match self {
            Self::Utf16Le => &[b'\n', 0],
            _ => b"\n",
        }
    }

    /// The bytes of the `\r` a line may end with before its `\n`.
    fn carriage_return(self) -> &'static [u8] {
        match self {
            Self::Utf16Le => &[b'\r', 0],
            _ => b"\r",
        }
    }

    /// Bytes of one code unit: lines start on a multiple of it.
    fn unit(self) -> usize {
        match self {
            Self::Utf16Le => UTF16_UNIT,
            _ => 1,
        }
    }
}

/// The encoding a file's start names by its byte order mark, and the mark's length, as a
/// .NET `StreamReader` finds it before the encoding it was given.
#[must_use]
pub fn byte_order_mark(start: &[u8]) -> Option<(LogEncoding, usize)> {
    if start.starts_with(UTF8_BOM) {
        Some((LogEncoding::Utf8, UTF8_BOM.len()))
    } else if start.starts_with(UTF16LE_BOM) {
        Some((LogEncoding::Utf16Le, UTF16LE_BOM.len()))
    } else {
        None
    }
}

/// What one read of a log file brought.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogChunk {
    /// The file is shorter than the offset read from: it was read again from its start, and
    /// the lines shown before are gone, as the C# clears them.
    pub restarted: bool,
    /// More than [`MAX_READ_BYTES`] were there: the lines before the last ones were skipped.
    pub skipped: bool,
    /// The lines read, at most [`MAX_DISPLAY_LINES`], the newest kept.
    pub lines: Vec<String>,
    /// How many lines were read, those dropped to keep [`MAX_DISPLAY_LINES`] included.
    pub read: usize,
    /// The last line has no end yet: the next read starts at it again.
    pub open_last: bool,
    /// Where the next read starts.
    pub next_offset: u64,
    /// The encoding read with: the one asked, or the one the file's byte order mark names.
    pub encoding: LogEncoding,
    /// The file's length when read.
    pub file_len: u64,
    /// When the file was last written, when the system says.
    pub modified: Option<SystemTime>,
}

/// The lines of the file at `path` from byte `offset`, read in `encoding`, as the C#
/// `LoadFile` (from 0) and `ReadNewContent` (from the last position) read them.
///
/// # Errors
///
/// The file cannot be opened, measured or read.
pub fn read_from(path: &Path, encoding: LogEncoding, offset: u64) -> io::Result<LogChunk> {
    read_capped(path, encoding, offset, MAX_READ_BYTES)
}

/// [`read_from`], reading at most `max_bytes` at once.
fn read_capped(
    path: &Path,
    encoding: LogEncoding,
    offset: u64,
    max_bytes: u64,
) -> io::Result<LogChunk> {
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    let file_len = metadata.len();
    let modified = metadata.modified().ok();
    let restarted = file_len < offset;
    let mut start = if restarted { 0 } else { offset };
    let mut encoding = encoding;
    if start == 0 {
        let mut head = [0_u8; MAX_BOM_LENGTH];
        let read = read_up_to(&mut file, &mut head)?;
        if let Some((named, length)) = byte_order_mark(&head[..read]) {
            encoding = named;
            start = u64::try_from(length).unwrap_or(0);
        }
    }
    let mut chunk = LogChunk {
        restarted,
        skipped: false,
        lines: Vec::new(),
        read: 0,
        open_last: false,
        next_offset: start,
        encoding,
        file_len,
        modified,
    };
    if file_len <= start {
        return Ok(chunk);
    }
    let unit = u64::try_from(encoding.unit()).unwrap_or(1);
    if file_len - start > max_bytes {
        // From a code unit's start, as far from the end as is read at once.
        let from = file_len - max_bytes;
        start = from + (unit - (from - start) % unit) % unit;
        chunk.skipped = true;
    }
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(file_len - start).read_to_end(&mut bytes)?;
    let mut body: &[u8] = &bytes;
    let mut consumed = 0;
    if chunk.skipped {
        // The first line was cut: it starts after the first line end.
        if let Some(end) = find_newline(body, encoding) {
            consumed = end;
            body = &body[end..];
        }
    }
    let (lines, mut closed, mut open_last) = split_lines(body, encoding);
    if open_last && consumed + closed == 0 && u64::try_from(body.len()).unwrap_or(0) >= max_bytes {
        // One line longer than a read: taken as it is, else every look would read it again.
        closed = body.len();
        open_last = false;
    }
    chunk.read = lines.len();
    chunk.open_last = open_last;
    chunk.next_offset = start + u64::try_from(consumed + closed).unwrap_or(0);
    let keep_from = lines.len().saturating_sub(MAX_DISPLAY_LINES);
    chunk.lines = lines.into_iter().skip(keep_from).collect();
    Ok(chunk)
}

/// Reads into `buffer` until it is full or the file ends; how many bytes were read.
fn read_up_to(file: &mut File, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..])? {
            0 => break,
            read => filled += read,
        }
    }
    Ok(filled)
}

/// Where the first line of `bytes` ends: the index after its newline.
fn find_newline(bytes: &[u8], encoding: LogEncoding) -> Option<usize> {
    let newline = encoding.newline();
    let unit = encoding.unit();
    (0..bytes.len())
        .step_by(unit)
        .find(|&index| bytes[index..].starts_with(newline))
        .map(|index| index + newline.len())
}

/// `bytes` split into lines, each without its `\r\n` or `\n`; how many bytes the ended
/// lines take; and whether the last line has no end yet.
fn split_lines(bytes: &[u8], encoding: LogEncoding) -> (Vec<String>, usize, bool) {
    let mut lines = Vec::new();
    let mut rest = bytes;
    let mut closed = 0;
    while let Some(end) = find_newline(rest, encoding) {
        let line = &rest[..end - encoding.newline().len()];
        let line = line
            .strip_suffix(encoding.carriage_return())
            .unwrap_or(line);
        lines.push(encoding.decode(line));
        closed += end;
        rest = &rest[end..];
    }
    let open_last = !rest.is_empty();
    if open_last {
        let line = rest
            .strip_suffix(encoding.carriage_return())
            .unwrap_or(rest);
        lines.push(encoding.decode(line));
    }
    (lines, closed, open_last)
}

/// How a line reads: an error, a warning, or neither, by the words it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// It names an error, a failure, a fatal or critical condition, or an exception.
    Error,
    /// It names a warning.
    Warning,
    /// Anything else.
    Normal,
}

/// The words that make a line an error.
const ERROR_WORDS: [&str; 5] = ["ERROR", "FATAL", "CRITICAL", "EXCEPTION", "FAIL"];

/// The word that makes a line a warning.
const WARNING_WORD: &str = "WARN";

/// How `line` reads.
#[must_use]
pub fn severity(line: &str) -> Severity {
    let upper = line.to_uppercase();
    if ERROR_WORDS.iter().any(|word| upper.contains(word)) {
        Severity::Error
    } else if upper.contains(WARNING_WORD) {
        Severity::Warning
    } else {
        Severity::Normal
    }
}

/// The filter typed: a regular expression, as the C# reads it, or plain text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilterSpec {
    /// What was typed.
    pub query: String,
    /// Case sensitive, as the C# check box; ignored by default.
    pub case_sensitive: bool,
    /// Read as a regular expression; else as the text itself.
    pub regex: bool,
}

/// Why a filter could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidFilter;

/// The bound on a compiled filter's size, so a pattern typed cannot take the memory.
const FILTER_SIZE_LIMIT: usize = 1024 * 1024;

impl FilterSpec {
    /// The expression it compiles to, once per change as the C# `ApplyFilter`; `None` when
    /// nothing is typed, which keeps every line.
    ///
    /// # Errors
    ///
    /// [`InvalidFilter`] when a regular expression does not compile.
    pub fn compile(&self) -> Result<Option<Regex>, InvalidFilter> {
        let query = self.query.trim();
        if query.is_empty() {
            return Ok(None);
        }
        let pattern = if self.regex {
            query.to_owned()
        } else {
            regex::escape(query)
        };
        RegexBuilder::new(&pattern)
            .case_insensitive(!self.case_sensitive)
            .size_limit(FILTER_SIZE_LIMIT)
            .build()
            .map(Some)
            .map_err(|_| InvalidFilter)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn file_with(bytes: &[u8]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        file.write_all(bytes).expect("written");
        file.flush().expect("flushed");
        file
    }

    fn append(file: &mut tempfile::NamedTempFile, bytes: &[u8]) {
        file.write_all(bytes).expect("appended");
        file.flush().expect("flushed");
    }

    #[test]
    fn lines_split_on_lf_and_crlf_and_the_offset_follows_the_ended_lines() {
        let file = file_with(b"one\r\ntwo\nthree\r\n");
        let chunk = read_from(file.path(), LogEncoding::Utf8, 0).expect("read");
        assert_eq!(chunk.lines, ["one", "two", "three"]);
        assert_eq!(chunk.read, 3);
        assert!(!chunk.open_last);
        assert_eq!(chunk.next_offset, 16);
        assert!(!chunk.restarted && !chunk.skipped);
    }

    #[test]
    fn a_line_not_yet_ended_is_read_again_once_it_is_never_twice() {
        let mut file = file_with(b"first\nsec");
        let chunk = read_from(file.path(), LogEncoding::Utf8, 0).expect("read");
        assert_eq!(chunk.lines, ["first", "sec"]);
        assert!(chunk.open_last);
        assert_eq!(chunk.next_offset, 6, "the open line is read again");
        append(&mut file, b"ond\nthird\n");
        let next = read_from(file.path(), LogEncoding::Utf8, chunk.next_offset).expect("read");
        assert_eq!(next.lines, ["second", "third"]);
        assert!(!next.open_last);
        assert_eq!(next.next_offset, 19);
        let idle = read_from(file.path(), LogEncoding::Utf8, next.next_offset).expect("read");
        assert!(idle.lines.is_empty());
        assert_eq!(idle.next_offset, 19);
    }

    #[test]
    fn a_file_that_got_shorter_is_read_again_from_its_start() {
        let file = file_with(b"new\n");
        let chunk = read_from(file.path(), LogEncoding::Utf8, 100).expect("read");
        assert!(chunk.restarted);
        assert_eq!(chunk.lines, ["new"]);
        assert_eq!(chunk.next_offset, 4);
    }

    #[test]
    fn a_large_file_is_read_from_near_its_end_only_whole_lines_kept() {
        // 25 lines of 10 bytes, read 95 bytes at a time: the cut line is dropped.
        let text: String = (0..25).map(|index| format!("line {index:04}\n")).collect();
        let file = file_with(text.as_bytes());
        let chunk = read_capped(file.path(), LogEncoding::Utf8, 0, 95).expect("read");
        assert!(chunk.skipped);
        assert_eq!(
            chunk.lines,
            (16..25)
                .map(|index| format!("line {index:04}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(chunk.next_offset, chunk.file_len);
    }

    #[test]
    fn the_newest_lines_are_kept_when_more_are_read() {
        let count = MAX_DISPLAY_LINES + 5;
        let text: String = (0..count).map(|index| format!("{index}\n")).collect();
        let file = file_with(text.as_bytes());
        let chunk = read_from(file.path(), LogEncoding::Utf8, 0).expect("read");
        assert_eq!(chunk.read, count);
        assert_eq!(chunk.lines.len(), MAX_DISPLAY_LINES);
        assert_eq!(chunk.lines.first().map(String::as_str), Some("5"));
    }

    #[test]
    fn a_line_longer_than_a_read_is_taken_once() {
        let file = file_with(&[b'x'; 64]);
        let chunk = read_capped(file.path(), LogEncoding::Utf8, 0, 32).expect("read");
        assert_eq!(chunk.lines.len(), 1);
        assert!(!chunk.open_last);
        assert_eq!(chunk.next_offset, 64);
    }

    #[test]
    fn the_byte_order_mark_names_the_encoding_and_is_not_shown() {
        let file = file_with(b"\xEF\xBB\xBFcaf\xC3\xA9\n");
        let chunk = read_from(file.path(), LogEncoding::Windows1252, 0).expect("read");
        assert_eq!(chunk.encoding, LogEncoding::Utf8);
        assert_eq!(chunk.lines, ["caf\u{e9}"]);
        let utf16 = file_with(b"\xFF\xFEa\x00\r\x00\n\x00b\x00\n\x00");
        let chunk = read_from(utf16.path(), LogEncoding::Utf8, 0).expect("read");
        assert_eq!(chunk.encoding, LogEncoding::Utf16Le);
        assert_eq!(chunk.lines, ["a", "b"]);
        assert_eq!(chunk.next_offset, 12);
    }

    #[test]
    fn each_encoding_reads_its_bytes() {
        assert_eq!(LogEncoding::Ascii.decode(b"caf\xE9"), "caf?");
        assert_eq!(
            LogEncoding::Windows1252.decode(b"\x80 caf\xE9"),
            "\u{20AC} caf\u{e9}"
        );
        assert_eq!(LogEncoding::Utf8.decode(b"caf\xE9"), "caf\u{FFFD}");
        assert_eq!(LogEncoding::Utf16Le.decode(b"a\x00\xE9\x00"), "a\u{e9}");
    }

    #[test]
    fn severity_comes_from_the_words_of_the_line() {
        assert_eq!(severity("[ERROR] bind failed"), Severity::Error);
        assert_eq!(severity("[warn] memory"), Severity::Warning);
        assert_eq!(severity("[INFO] started"), Severity::Normal);
    }

    #[test]
    fn a_filter_is_plain_text_or_a_regex_ignoring_case_unless_asked() {
        let plain = FilterSpec {
            query: "a.b".to_owned(),
            ..FilterSpec::default()
        };
        let regex = plain.compile().expect("valid").expect("a filter");
        assert!(regex.is_match("xA.Bx"));
        assert!(!regex.is_match("aXb"), "a dot typed is a dot");
        let pattern = FilterSpec {
            query: r"status=\d{3}".to_owned(),
            case_sensitive: true,
            regex: true,
        };
        let regex = pattern.compile().expect("valid").expect("a filter");
        assert!(regex.is_match("status=200"));
        assert!(!regex.is_match("STATUS=200"));
        let broken = FilterSpec {
            query: "(".to_owned(),
            regex: true,
            ..FilterSpec::default()
        };
        assert!(broken.compile().is_err());
        assert!(FilterSpec::default().compile().expect("valid").is_none());
    }
}
