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

//! Terminal macros, as the C# ones: what was typed into a session, with the pause before
//! each input, kept to be typed again into any session. An input can wait first for text
//! the session shows, as the C# "expect", for a while, then stop or go on.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::{StoreError, write_atomic};

/// Name of the macros' file, beside the profiles.
pub const MACROS_FILE_NAME: &str = "macros.toml";

/// Version of the macros' file this build writes and reads.
const MACROS_FILE_VERSION: u32 = 1;

/// Milliseconds an expectation waits by default, as the C# `DefaultExpectTimeoutMs`.
pub const EXPECT_TIMEOUT_DEFAULT: u32 = 30_000;

/// Shortest wait of an expectation, in milliseconds, as the C#.
pub const EXPECT_TIMEOUT_MIN: u32 = 100;

/// Longest wait of an expectation, in milliseconds, as the C#.
pub const EXPECT_TIMEOUT_MAX: u32 = 600_000;

/// What a macro does when the text it waits for does not come in time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnTimeout {
    /// Stop there, as the C# default.
    #[default]
    Abort,
    /// Type the input anyway, and go on.
    Continue,
}

/// Text a macro waits for before an input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expect {
    /// The text, or a regular expression.
    pub pattern: String,
    /// `pattern` is a regular expression.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub regex: bool,
    /// How long it is waited for, in milliseconds; held within [`EXPECT_TIMEOUT_MIN`] and
    /// [`EXPECT_TIMEOUT_MAX`] when waited.
    #[serde(default = "default_timeout")]
    pub timeout_ms: u32,
    /// What then.
    #[serde(default)]
    pub on_timeout: OnTimeout,
}

fn default_timeout() -> u32 {
    EXPECT_TIMEOUT_DEFAULT
}

impl Expect {
    /// How long it is waited for, within the C# range.
    #[must_use]
    pub fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(u64::from(
            self.timeout_ms
                .clamp(EXPECT_TIMEOUT_MIN, EXPECT_TIMEOUT_MAX),
        ))
    }
}

/// One input of a macro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroEntry {
    /// What is typed, as the keys sent it: a new line is a carriage return.
    pub input: String,
    /// The pause before it, in milliseconds, as it was recorded.
    #[serde(default)]
    pub delay_ms: u32,
    /// Text waited for before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<Expect>,
}

/// A macro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalMacro {
    /// Its name.
    pub name: String,
    /// Its inputs, in order.
    #[serde(default)]
    pub entries: Vec<MacroEntry>,
}

/// The macros kept, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Macros {
    macros: Vec<TerminalMacro>,
}

#[derive(Serialize, Deserialize, Default)]
struct MacrosFile {
    version: u32,
    #[serde(default, rename = "macro")]
    macros: Vec<TerminalMacro>,
}

impl Macros {
    /// The macros at `path`; none when there is no file.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file is there and cannot be read, does not parse, or
    /// was written by a newer version.
    pub fn load(path: &Path) -> Result<Self, StoreError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(StoreError::Io {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        let file: MacrosFile = toml::from_str(&text).map_err(|source| StoreError::Parse {
            path: path.to_owned(),
            source,
        })?;
        if file.version > MACROS_FILE_VERSION {
            return Err(StoreError::UnsupportedVersion {
                path: path.to_owned(),
                found: file.version,
                expected: MACROS_FILE_VERSION,
            });
        }
        let mut macros = Self::default();
        for found in file.macros {
            macros.put(found);
        }
        Ok(macros)
    }

    /// Writes the macros to `path`, through a temporary file.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when serialisation or any file operation fails.
    pub fn save(&self, path: &Path) -> Result<(), StoreError> {
        let text = toml::to_string_pretty(&MacrosFile {
            version: MACROS_FILE_VERSION,
            macros: self.macros.clone(),
        })?;
        write_atomic(path, &text)
    }

    /// The macros, by name whatever its case.
    #[must_use]
    pub fn all(&self) -> &[TerminalMacro] {
        &self.macros
    }

    /// The macro named `name`, whatever its case.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&TerminalMacro> {
        self.macros
            .iter()
            .find(|found| found.name.eq_ignore_ascii_case(name))
    }

    /// Keeps `kept`, in place of one of the same name whatever its case; the list stays
    /// sorted by name.
    pub fn put(&mut self, kept: TerminalMacro) {
        self.remove(&kept.name);
        self.macros.push(kept);
        self.macros.sort_by_key(|found| found.name.to_lowercase());
    }

    /// Forgets the macro named `name`; whether there was one.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.macros.len();
        self.macros
            .retain(|found| !found.name.eq_ignore_ascii_case(name));
        self.macros.len() != before
    }
}

/// Why an input as written cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    /// It ends with a lone backslash.
    TrailingEscape,
    /// A `\x` is not followed by two hexadecimal digits.
    BadHex,
    /// A backslash is followed by something it does not escape.
    UnknownEscape(char),
}

/// `input` as the macro editor shows it, as the C# `MacroInputEscaper`: a backslash, a new
/// line, a carriage return and a tab written `\\`, `\n`, `\r` and `\t`, any other control
/// character `\xNN`.
#[must_use]
pub fn written_input(input: &str) -> String {
    let mut written = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\\' => written.push_str("\\\\"),
            '\r' => written.push_str("\\r"),
            '\n' => written.push_str("\\n"),
            '\t' => written.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(written, "\\x{:02X}", u32::from(c));
            }
            c => written.push(c),
        }
    }
    written
}

/// The input written as [`written_input`] writes it.
///
/// # Errors
///
/// [`InputError`] when a backslash starts nothing it escapes.
pub fn read_input(written: &str) -> Result<String, InputError> {
    let mut input = String::with_capacity(written.len());
    let mut chars = written.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            input.push(c);
            continue;
        }
        match chars.next() {
            None => return Err(InputError::TrailingEscape),
            Some('\\') => input.push('\\'),
            Some('r') => input.push('\r'),
            Some('n') => input.push('\n'),
            Some('t') => input.push('\t'),
            Some('x') => {
                let digits: String = chars.by_ref().take(2).collect();
                let value = (digits.len() == 2)
                    .then(|| u8::from_str_radix(&digits, 16).ok())
                    .flatten()
                    .ok_or(InputError::BadHex)?;
                input.push(char::from(value));
            }
            Some(other) => return Err(InputError::UnknownEscape(other)),
        }
    }
    Ok(input)
}

/// The macros' file beside `profiles_file`.
#[must_use]
pub fn macros_path(profiles_file: &Path) -> PathBuf {
    profiles_file.with_file_name(MACROS_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(input: &str, delay_ms: u32) -> MacroEntry {
        MacroEntry {
            input: input.to_owned(),
            delay_ms,
            expect: None,
        }
    }

    #[test]
    fn macros_are_kept_by_name_and_read_back() {
        let dir = tempfile::tempdir().expect("dir");
        let path = macros_path(&dir.path().join("profiles.toml"));
        assert_eq!(Macros::load(&path).expect("none"), Macros::default());
        let mut macros = Macros::default();
        macros.put(TerminalMacro {
            name: "uptime".to_owned(),
            entries: vec![entry("uptime\r", 0)],
        });
        macros.put(TerminalMacro {
            name: "Deploy".to_owned(),
            entries: vec![
                MacroEntry {
                    expect: Some(Expect {
                        pattern: r"\$ $".to_owned(),
                        regex: true,
                        timeout_ms: 5000,
                        on_timeout: OnTimeout::Continue,
                    }),
                    ..entry("cd /srv\r", 300)
                },
                entry("\u{1b}[A\r", 1200),
            ],
        });
        let names: Vec<&str> = macros
            .all()
            .iter()
            .map(|found| found.name.as_str())
            .collect();
        assert_eq!(names, ["Deploy", "uptime"], "by name");
        macros.save(&path).expect("save");
        assert_eq!(Macros::load(&path).expect("load"), macros);

        // The same name, whatever its case, replaces.
        macros.put(TerminalMacro {
            name: "UPTIME".to_owned(),
            entries: Vec::new(),
        });
        assert_eq!(macros.all().len(), 2);
        assert!(
            macros
                .get("uptime")
                .is_some_and(|found| found.entries.is_empty())
        );
        assert!(macros.remove("deploy"));
        assert!(!macros.remove("deploy"));
    }

    #[test]
    fn an_input_is_written_with_its_control_characters_escaped_and_read_back() {
        let input = "cd C:\\tmp\r\u{1b}[A\tx\n";
        let written = written_input(input);
        assert_eq!(written, r"cd C:\\tmp\r\x1B[A\tx\n");
        assert_eq!(read_input(&written), Ok(input.to_owned()));
        assert_eq!(read_input(r"ok\"), Err(InputError::TrailingEscape));
        assert_eq!(read_input(r"\xZ1"), Err(InputError::BadHex));
        assert_eq!(read_input(r"\x1"), Err(InputError::BadHex));
        assert_eq!(read_input(r"\q"), Err(InputError::UnknownEscape('q')));
        assert_eq!(read_input("plain"), Ok("plain".to_owned()));
    }

    #[test]
    fn an_expectation_waits_within_the_csharp_range() {
        let expect = |timeout_ms| Expect {
            pattern: "$".to_owned(),
            regex: false,
            timeout_ms,
            on_timeout: OnTimeout::Abort,
        };
        assert_eq!(expect(1).timeout().as_millis(), 100);
        assert_eq!(expect(2000).timeout().as_millis(), 2000);
        assert_eq!(expect(u32::MAX).timeout().as_millis(), 600_000);
        let read: Expect = toml::from_str("pattern = \"$\"").expect("read");
        assert_eq!(read.timeout_ms, EXPECT_TIMEOUT_DEFAULT);
        assert_eq!(read.on_timeout, OnTimeout::Abort);
    }
}
