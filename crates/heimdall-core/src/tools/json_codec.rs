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

//! JSON prettified or minified as the C# `JsonCodec` (`Heimdall.Core/Codecs/JsonCodec.cs`)
//! does it, through .NET's `JsonDocument` and `Utf8JsonWriter` with
//! `UnsafeRelaxedJsonEscaping`: the document read whole, then written again token by token,
//! in its own order, keys repeated or not, numbers as they were typed; indented by two
//! spaces, `": "` after a key, an empty object or array kept on its line.
//!
//! Strings are read and written again as .NET writes them: `"`, `\` and the control
//! characters escaped (`\b`, `\t`, `\n`, `\f`, `\r` by name), and the characters .NET's
//! encoder never lets through unescaped, as `\uXXXX` in upper-case hexadecimal: the format
//! characters, the line and paragraph separators, the private use area, the noncharacters
//! and everything past the Basic Multilingual Plane, as two UTF-16 halves. The rest, `é`,
//! `<`, `&` or `'` among them, is written as it is.
//!
//! The document is checked by `serde_json` first: what it refuses is said with its message
//! and where, as the C# says .NET's.

use std::fmt::Write as _;

/// Largest input formatted, in UTF-8 bytes, as the C# `JsonFormatterToolService.
/// MaxInputSizeBytes`: 5 MB.
pub const MAX_INPUT_BYTES: usize = 5 * 1024 * 1024;

/// Input past which formatting runs away from the window, as the C#
/// `AsyncThresholdBytes`: 100 KB.
pub const ASYNC_THRESHOLD_BYTES: usize = 100 * 1024;

/// What one level of an indented document is indented by, as .NET's two spaces.
const INDENT: &str = "  ";

/// What separates a key from its value, indented and not.
const KEY_SEPARATOR_INDENTED: &str = ": ";
const KEY_SEPARATOR: &str = ":";

/// The highest code point written as one UTF-16 unit.
const BMP_LAST: u32 = 0xFFFF;

/// First code point past the Basic Multilingual Plane, and the bits of a UTF-16 half.
const SUPPLEMENTARY_FIRST: u32 = 0x1_0000;
const SURROGATE_BITS: u32 = 10;
const SURROGATE_MASK: u32 = 0x3FF;
const HIGH_SURROGATE_FIRST: u32 = 0xD800;
const LOW_SURROGATE_FIRST: u32 = 0xDC00;

/// The C0 controls end, and DEL with the C1 controls.
const C0_END: u32 = 0x20;
const C1: std::ops::RangeInclusive<u32> = 0x7F..=0x9F;

/// The format characters (`Cf`) of the Basic Multilingual Plane, which .NET's encoder
/// escapes.
const FORMAT_CHARACTERS: &[std::ops::RangeInclusive<u32>] = &[
    0x00AD..=0x00AD,
    0x0600..=0x0605,
    0x061C..=0x061C,
    0x06DD..=0x06DD,
    0x070F..=0x070F,
    0x0890..=0x0891,
    0x08E2..=0x08E2,
    0x180E..=0x180E,
    0x200B..=0x200F,
    0x202A..=0x202E,
    0x2060..=0x2064,
    0x2066..=0x206F,
    0xFEFF..=0xFEFF,
    0xFFF9..=0xFFFB,
];

/// The line and paragraph separators (`Zl`, `Zp`).
const SEPARATORS: std::ops::RangeInclusive<u32> = 0x2028..=0x2029;

/// The private use area of the Basic Multilingual Plane (`Co`).
const PRIVATE_USE: std::ops::RangeInclusive<u32> = 0xE000..=0xF8FF;

/// The noncharacters of the Basic Multilingual Plane.
const NONCHARACTERS: std::ops::RangeInclusive<u32> = 0xFDD0..=0xFDEF;
const LAST_NONCHARACTERS: std::ops::RangeInclusive<u32> = 0xFFFE..=0xFFFF;

/// What formatting gave, as the C# `JsonFormatResult` and its `JsonFormatStatus`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonFormat {
    /// The document written again.
    Success(String),
    /// Nothing but spaces was given.
    Empty,
    /// The text is not JSON.
    ParseError(JsonError),
    /// The text is larger than [`MAX_INPUT_BYTES`].
    InputTooLarge,
}

/// Why a text is not JSON, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    /// What is wrong, as `serde_json` says it, in English as .NET's message is.
    pub message: String,
    /// The line it is on, from 1; `None` when no place is known.
    pub line: Option<usize>,
    /// Its column on that line, from 1.
    pub column: Option<usize>,
}

/// A token of a document checked: the document's own text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token<'a> {
    Open(u8),
    Close(u8),
    Colon,
    Comma,
    String(&'a str),
    Literal(&'a str),
}

/// `input` formatted as the C# `JsonFormatterToolService.FormatAsync`
/// (`JsonFormatterToolService.cs:27-53`) checks it, then as [`format`]: nothing for blank
/// input, refused past [`MAX_INPUT_BYTES`].
#[must_use]
pub fn format_input(input: &str, indented: bool, new_line: &str) -> JsonFormat {
    if input.trim().is_empty() {
        return JsonFormat::Empty;
    }
    if input.len() > MAX_INPUT_BYTES {
        return JsonFormat::InputTooLarge;
    }
    format(input, indented, new_line)
}

/// `input` written again, indented when `indented`, its lines ended with `new_line`, as the
/// C# `JsonCodec.Format` (`JsonCodec.cs:40-80`).
#[must_use]
pub fn format(input: &str, indented: bool, new_line: &str) -> JsonFormat {
    if input.trim().is_empty() {
        return JsonFormat::Empty;
    }
    if let Err(error) = serde_json::from_str::<serde::de::IgnoredAny>(input) {
        return JsonFormat::ParseError(parse_error(&error));
    }
    let mut writer = Writer {
        out: String::with_capacity(input.len()),
        indented,
        new_line,
        depth: 0,
        after_key: false,
    };
    let mut tokens = Tokens { input, at: 0 }.peekable();
    while let Some(token) = tokens.next() {
        match token {
            Token::Open(bracket) => {
                writer.value_start();
                writer.out.push(char::from(bracket));
                if let Some(Token::Close(close)) = tokens.peek().copied() {
                    // An empty container stays on its line, as .NET writes `{}` and `[]`.
                    tokens.next();
                    writer.out.push(char::from(close));
                } else {
                    writer.depth += 1;
                }
            }
            Token::Close(bracket) => {
                writer.depth -= 1;
                writer.line_break();
                writer.out.push(char::from(bracket));
            }
            Token::String(raw) => {
                writer.value_start();
                write_string(&mut writer.out, raw);
            }
            Token::Literal(raw) => {
                writer.value_start();
                writer.out.push_str(raw);
            }
            Token::Colon => {
                writer.out.push_str(if indented {
                    KEY_SEPARATOR_INDENTED
                } else {
                    KEY_SEPARATOR
                });
                writer.after_key = true;
            }
            Token::Comma => writer.out.push(','),
        }
    }
    JsonFormat::Success(writer.out)
}

/// What `serde_json` says of `error`, its place taken out of the message and kept apart.
fn parse_error(error: &serde_json::Error) -> JsonError {
    let line = error.line();
    let column = error.column();
    let said = error.to_string();
    let place = format!(" at line {line} column {column}");
    let message = said.strip_suffix(&place).unwrap_or(&said).to_owned();
    JsonError {
        message,
        line: (line > 0).then_some(line),
        column: (line > 0).then_some(column.max(1)),
    }
}

/// The document being written.
struct Writer<'a> {
    out: String,
    indented: bool,
    new_line: &'a str,
    depth: usize,
    after_key: bool,
}

impl Writer<'_> {
    /// What comes before a value or a key: nothing after a key, else, indented and inside
    /// a container, a line break and the indent.
    fn value_start(&mut self) {
        if self.after_key {
            self.after_key = false;
            return;
        }
        if self.depth > 0 {
            self.line_break();
        }
    }

    /// A line break and the current indent, when indented.
    fn line_break(&mut self) {
        if !self.indented {
            return;
        }
        self.out.push_str(self.new_line);
        for _ in 0..self.depth {
            self.out.push_str(INDENT);
        }
    }
}

/// The tokens of a document `serde_json` accepted.
struct Tokens<'a> {
    input: &'a str,
    at: usize,
}

impl<'a> Iterator for Tokens<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        let bytes = self.input.as_bytes();
        while self
            .at
            .checked_sub(0)
            .and_then(|at| bytes.get(at))
            .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        {
            self.at += 1;
        }
        let start = self.at;
        let byte = *bytes.get(start)?;
        self.at += 1;
        let token = match byte {
            b'{' | b'[' => Token::Open(byte),
            b'}' | b']' => Token::Close(byte),
            b':' => Token::Colon,
            b',' => Token::Comma,
            b'"' => {
                let mut escaped = false;
                while let Some(&next) = bytes.get(self.at) {
                    self.at += 1;
                    if escaped {
                        escaped = false;
                    } else if next == b'\\' {
                        escaped = true;
                    } else if next == b'"' {
                        break;
                    }
                }
                Token::String(&self.input[start..self.at])
            }
            _ => {
                while bytes.get(self.at).is_some_and(|next| {
                    !matches!(
                        next,
                        b' ' | b'\t' | b'\r' | b'\n' | b',' | b':' | b'}' | b']' | b'{' | b'['
                    )
                }) {
                    self.at += 1;
                }
                Token::Literal(&self.input[start..self.at])
            }
        };
        Some(token)
    }
}

/// The string token `raw`, quotes included, read and written again as .NET writes it; kept
/// as it was typed if it cannot be read, a lone UTF-16 half escaped in it.
fn write_string(out: &mut String, raw: &str) {
    let Ok(value) = serde_json::from_str::<String>(raw) else {
        out.push_str(raw);
        return;
    };
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if escaped_by_dotnet(u32::from(c)) => escape(out, u32::from(c)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Whether .NET's relaxed encoder writes the code point `code` escaped.
fn escaped_by_dotnet(code: u32) -> bool {
    code < C0_END
        || C1.contains(&code)
        || code > BMP_LAST
        || SEPARATORS.contains(&code)
        || PRIVATE_USE.contains(&code)
        || NONCHARACTERS.contains(&code)
        || LAST_NONCHARACTERS.contains(&code)
        || FORMAT_CHARACTERS.iter().any(|range| range.contains(&code))
}

/// `code` written as `\uXXXX`, two of them past the Basic Multilingual Plane.
fn escape(out: &mut String, code: u32) {
    if code > BMP_LAST {
        let offset = code - SUPPLEMENTARY_FIRST;
        escape(out, HIGH_SURROGATE_FIRST + (offset >> SURROGATE_BITS));
        escape(out, LOW_SURROGATE_FIRST + (offset & SURROGATE_MASK));
        return;
    }
    let _ = write!(out, "\\u{code:04X}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nested_document_is_indented_as_dotnet_writes_it() {
        let JsonFormat::Success(out) = format("{\"a\":[1,{\"b\":null}],\"c\":{}}", true, "\n")
        else {
            panic!("formatted");
        };
        assert_eq!(
            out,
            "{\n  \"a\": [\n    1,\n    {\n      \"b\": null\n    }\n  ],\n  \"c\": {}\n}"
        );
    }

    #[test]
    fn keys_keep_their_order_and_repeats() {
        assert_eq!(
            format("{\"z\":1,\"a\":2,\"z\":3}", false, "\n"),
            JsonFormat::Success("{\"z\":1,\"a\":2,\"z\":3}".to_owned())
        );
    }

    #[test]
    fn strings_are_escaped_as_dotnet_escapes_them() {
        assert_eq!(
            format("\"\\u00e9\\/\\u0001\\u2028\\ud83d\\ude00\"", false, "\n"),
            JsonFormat::Success("\"é/\\u0001\\u2028\\uD83D\\uDE00\"".to_owned())
        );
    }
}
