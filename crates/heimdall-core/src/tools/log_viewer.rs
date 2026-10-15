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

//! Log file inspection and tailing engine.
//!
//! Provides chunked reading, continuous tail monitoring, encoding detection,
//! and regex/string filtering for system and application log files.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Maximum number of lines preserved in the in-memory log buffer.
pub const MAX_DISPLAY_LINES: usize = 10_000;

/// Supported character encodings for log file reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogEncoding {
    /// UTF-8 encoding (default).
    #[default]
    Utf8,
    /// 7-bit ASCII encoding.
    Ascii,
    /// UTF-16 Little Endian encoding.
    Utf16Le,
    /// Windows-1252 / ISO-8859-1 ANSI encoding.
    Windows1252,
}

impl LogEncoding {
    /// Human-readable label for the encoding.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Ascii => "ASCII",
            Self::Utf16Le => "UTF-16 LE",
            Self::Windows1252 => "Windows-1252",
        }
    }
}

/// A single decoded log line with metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// Line index (1-based).
    pub line_number: usize,
    /// Raw text content of the line.
    pub text: String,
    /// Whether the line indicates an error condition.
    pub is_error: bool,
    /// Whether the line indicates a warning condition.
    pub is_warning: bool,
}

impl LogLine {
    /// Constructs a new log line and inspects text for common log levels.
    #[must_use]
    pub fn new(line_number: usize, text: String) -> Self {
        let upper = text.to_uppercase();
        let is_error = upper.contains("ERROR")
            || upper.contains("FATAL")
            || upper.contains("CRITICAL")
            || upper.contains("EXCEPTION")
            || upper.contains("FAIL");
        let is_warning = !is_error && (upper.contains("WARN") || upper.contains("WARNING"));

        Self {
            line_number,
            text,
            is_error,
            is_warning,
        }
    }
}

/// Filter options for log lines inspection.
#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    /// Query string or regex pattern.
    pub query: String,
    /// Case-sensitive matching flag.
    pub case_sensitive: bool,
    /// Regex evaluation flag.
    pub use_regex: bool,
}

impl LogFilter {
    /// Tests if a candidate line passes the filter query.
    #[must_use]
    pub fn matches(&self, line: &str) -> bool {
        let trimmed = self.query.trim();
        if trimmed.is_empty() {
            return true;
        }

        if self.use_regex {
            let pattern = if self.case_sensitive {
                regex::Regex::new(trimmed)
            } else {
                regex::RegexBuilder::new(trimmed)
                    .case_insensitive(true)
                    .build()
            };

            pattern.map_or(false, |re| re.is_match(line))
        } else if self.case_sensitive {
            line.contains(trimmed)
        } else {
            line.to_lowercase().contains(&trimmed.to_lowercase())
        }
    }

    /// Returns references to all log lines matching this filter.
    #[must_use]
    pub fn filter_lines<'a>(&self, lines: &'a [LogLine]) -> Vec<&'a LogLine> {
        lines.iter().filter(|l| self.matches(&l.text)).collect()
    }
}

/// Tailing file reader that tracks byte position and decodes incoming log text.
#[derive(Debug)]
pub struct LogTailReader {
    path: PathBuf,
    encoding: LogEncoding,
    last_position: u64,
    total_lines: usize,
}

impl LogTailReader {
    /// Creates a reader for the log file at `path`.
    #[must_use]
    pub fn new(path: PathBuf, encoding: LogEncoding) -> Self {
        Self {
            path,
            encoding,
            last_position: 0,
            total_lines: 0,
        }
    }

    /// Opens a log reader for the given path using UTF-8 encoding.
    ///
    /// # Errors
    ///
    /// Returns error if path does not exist.
    pub fn open(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Err(format!("File not found: {}", path.display()));
        }
        Ok(Self::new(path.to_path_buf(), LogEncoding::Utf8))
    }

    /// Returns the current byte offset position.
    #[must_use]
    pub fn position(&self) -> u64 {
        self.last_position
    }

    /// Sets the byte offset position to start reading from.
    pub fn seek_to(&mut self, pos: u64) {
        self.last_position = pos;
    }

    /// Reads up to `max_lines` from the log file.
    ///
    /// # Errors
    ///
    /// Returns error if file cannot be opened or read.
    pub fn read_initial(&mut self, max_lines: usize) -> Result<Vec<LogLine>, String> {
        let lines = self.read_next()?;
        if lines.len() > max_lines {
            let start = lines.len() - max_lines;
            Ok(lines[start..].to_vec())
        } else {
            Ok(lines)
        }
    }

    /// Reads newly appended lines and returns them along with the new file position.
    ///
    /// # Errors
    ///
    /// Returns error if file cannot be read.
    pub fn read_update(&mut self) -> Result<(Vec<LogLine>, u64), String> {
        let lines = self.read_next()?;
        Ok((lines, self.last_position))
    }

    /// Returns the target log file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Resets reading position back to the beginning of the file.
    pub fn reset(&mut self) {
        self.last_position = 0;
        self.total_lines = 0;
    }

    /// Reads all lines from the current file position up to EOF.
    ///
    /// # Errors
    ///
    /// Returns error if the file cannot be opened or read.
    pub fn read_next(&mut self) -> Result<Vec<LogLine>, String> {
        let mut file =
            File::open(&self.path).map_err(|err| format!("{}: {}", self.path.display(), err))?;

        let meta = file
            .metadata()
            .map_err(|err| format!("Failed to read metadata: {err}"))?;
        let len = meta.len();

        // Handle file truncation / log rotation
        if len < self.last_position {
            self.last_position = 0;
            self.total_lines = 0;
        }

        file.seek(SeekFrom::Start(self.last_position))
            .map_err(|err| format!("Failed to seek: {err}"))?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|err| format!("Failed to read log content: {err}"))?;

        self.last_position = file
            .stream_position()
            .map_err(|err| format!("Failed to determine file position: {err}"))?;

        let text = self.decode_buffer(&buffer);
        let mut lines = Vec::new();

        for raw_line in text.lines() {
            self.total_lines += 1;
            lines.push(LogLine::new(self.total_lines, raw_line.to_owned()));
        }

        Ok(lines)
    }

    fn decode_buffer(&self, buf: &[u8]) -> String {
        match self.encoding {
            LogEncoding::Utf8 | LogEncoding::Ascii => String::from_utf8_lossy(buf).into_owned(),
            LogEncoding::Utf16Le => {
                let u16s: Vec<u16> = buf
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                String::from_utf16_lossy(&u16s)
            }
            LogEncoding::Windows1252 => buf.iter().map(|&b| b as char).collect(),
        }
    }
}

/// Builds a text export of lines matching `filter`.
#[must_use]
pub fn export_log_text(lines: &[LogLine], filter: &LogFilter) -> String {
    let mut out = String::new();
    for line in lines {
        if filter.matches(&line.text) {
            out.push_str(&format!("{:>6}: {}\n", line.line_number, line.text));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_line_severity() {
        let err = LogLine::new(1, "2026-10-15 [ERROR] Failed to bind port 80".to_owned());
        assert!(err.is_error);
        assert!(!err.is_warning);

        let warn = LogLine::new(2, "2026-10-15 [WARN] High memory usage".to_owned());
        assert!(!warn.is_error);
        assert!(warn.is_warning);

        let info = LogLine::new(3, "2026-10-15 [INFO] Server started".to_owned());
        assert!(!info.is_error);
        assert!(!info.is_warning);
    }

    #[test]
    fn test_log_filter_plain_case_insensitive() {
        let filter = LogFilter {
            query: "failed".to_owned(),
            case_sensitive: false,
            use_regex: false,
        };
        assert!(filter.matches("Authentication FAILED for user root"));
        assert!(!filter.matches("Authentication success"));
    }

    #[test]
    fn test_log_filter_regex() {
        let filter = LogFilter {
            query: r"status=\d{3}".to_owned(),
            case_sensitive: false,
            use_regex: true,
        };
        assert!(filter.matches("HTTP GET /api status=200 ok"));
        assert!(filter.matches("HTTP POST /login status=403 forbidden"));
        assert!(!filter.matches("No HTTP response"));
    }
}
