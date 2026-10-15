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

//! Values written into a `PowerShell` script, as the C# `PowerShellSingleQuotedString`
//! (`Heimdall.Core/Security/PowerShellSingleQuotedString.cs`) writes them, and the script
//! handed over as `-EncodedCommand`.
//!
//! `PowerShell` ends a single-quoted string on the ASCII apostrophe and on four typographic
//! single quotes alike, and reads any of them doubled as one literal character: each of the
//! five is doubled, so no value closes its literal and runs the rest as code. The script
//! itself goes as `-EncodedCommand`, Base64 of its UTF-16LE text, so no command line
//! quoting stands between it and `PowerShell`.

use data_encoding::BASE64;

/// The characters `PowerShell` ends a single-quoted string on: the ASCII quote and the four
/// typographic single quotes, as its own `EscapeSingleQuotedStringContent` lists them.
pub const SINGLE_QUOTES: [char; 5] = ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

/// The switch the Base64 script follows.
pub const ENCODED_COMMAND_SWITCH: &str = "-EncodedCommand";

/// `text` as the inside of a single-quoted `PowerShell` string: each quote doubled, as the
/// C# `EscapeContent`.
#[must_use]
pub fn escape_content(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if SINGLE_QUOTES.contains(&c) {
            escaped.push(c);
        }
        escaped.push(c);
    }
    escaped
}

/// `text` as a whole single-quoted `PowerShell` literal, as the C# `Quote`.
#[must_use]
pub fn quote(text: &str) -> String {
    format!("'{}'", escape_content(text))
}

/// `script` as `-EncodedCommand` takes it: the Base64 of its UTF-16LE text.
#[must_use]
pub fn encode_command(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    BASE64.encode(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_single_quote_is_doubled_and_nothing_else_changes() {
        assert_eq!(
            quote("o'b\u{2018}c\u{2019}d\u{201A}e\u{201B}f"),
            "'o''b\u{2018}\u{2018}c\u{2019}\u{2019}d\u{201A}\u{201A}e\u{201B}\u{201B}f'"
        );
        assert_eq!(quote(r#"C:\a "b" $x `y"#), r#"'C:\a "b" $x `y'"#);
        assert_eq!(quote(""), "''");
    }

    #[test]
    fn the_encoded_command_is_base64_of_utf16_little_endian() {
        // "A\u{e9}" is 0x41 0x00 0xE9 0x00 in UTF-16LE.
        assert_eq!(encode_command("A\u{e9}"), "QQDpAA==");
        assert_eq!(encode_command(""), "");
    }
}
