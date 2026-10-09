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

//! Base64 as the C# `Base64Codec` (`Heimdall.Core/Codecs/Base64Codec.cs`) writes and reads
//! it, through .NET's `Convert`: the standard alphabet, padded, a line break every 76
//! characters (`Base64FormattingOptions.InsertLineBreaks`); URL-safe, `-` and `_` for `+`
//! and `/` and no padding, the line breaks kept.
//!
//! Reading is .NET's `Convert.FromBase64String`: spaces, tabs and line breaks are skipped,
//! the padding must be whole, and the bits past the last byte are not checked.

use std::sync::LazyLock;

use data_encoding::{BASE64, Encoding, Specification};

/// Characters on a line, as .NET's `InsertLineBreaks` cuts them.
pub const LINE_LENGTH: usize = 76;

/// What ends a line, as .NET writes it on every system.
pub const LINE_BREAK: &str = "\r\n";

/// The padding character.
const PADDING: char = '=';

/// What .NET's reading skips.
const SKIPPED: &str = " \t\r\n";

/// The standard alphabet's two last symbols, and the URL-safe ones taking their place.
const STANDARD_62: char = '+';
const STANDARD_63: char = '/';
const URL_SAFE_62: char = '-';
const URL_SAFE_63: char = '_';

/// Length of a block of symbols, padded.
const BLOCK: usize = 4;

/// The reading of .NET's `Convert.FromBase64String`.
static DOTNET_READING: LazyLock<Encoding> = LazyLock::new(|| {
    let mut specification = Specification::new();
    specification
        .symbols
        .push_str(&BASE64.specification().symbols);
    specification.padding = Some(PADDING);
    specification.ignore.push_str(SKIPPED);
    specification.check_trailing_bits = false;
    specification
        .encoding()
        .expect("the standard alphabet with its padding is a valid specification")
});

/// Why a text is not Base64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the text is not valid Base64")]
pub struct InvalidBase64;

/// `data` in Base64, as the C# `Base64Codec.Encode` (`Base64Codec.cs:21-35`); URL-safe when
/// `url_safe`.
#[must_use]
pub fn encode(data: &[u8], url_safe: bool) -> String {
    let encoded = with_line_breaks(&BASE64.encode(data));
    if !url_safe {
        return encoded;
    }
    encoded
        .replace(STANDARD_62, &URL_SAFE_62.to_string())
        .replace(STANDARD_63, &URL_SAFE_63.to_string())
        .trim_end_matches(PADDING)
        .to_owned()
}

/// The bytes `text` stands for, as the C# `Base64Codec.Decode` (`Base64Codec.cs:37-56`);
/// URL-safe when `url_safe`, its padding put back.
///
/// The C# counts the URL-safe padding on the text as typed, line breaks included, so a text
/// of an odd number of line breaks, as its own encoding of more than 57 bytes can be, is
/// refused. The padding is counted here on the symbols alone.
///
/// # Errors
///
/// [`InvalidBase64`] when `text` is not Base64, as .NET's `FormatException`.
pub fn decode(text: &str, url_safe: bool) -> Result<Vec<u8>, InvalidBase64> {
    if !url_safe {
        return DOTNET_READING
            .decode(text.as_bytes())
            .map_err(|_| InvalidBase64);
    }
    let mut standard: String = text
        .chars()
        .map(|c| match c {
            URL_SAFE_62 => STANDARD_62,
            URL_SAFE_63 => STANDARD_63,
            other => other,
        })
        .collect();
    let symbols = standard.chars().filter(|c| !SKIPPED.contains(*c)).count();
    // As the C#: two symbols past a block take two padding characters, three take one.
    let padding = match symbols % BLOCK {
        2 => 2,
        3 => 1,
        _ => 0,
    };
    standard.extend(std::iter::repeat_n(PADDING, padding));
    DOTNET_READING
        .decode(standard.as_bytes())
        .map_err(|_| InvalidBase64)
}

/// `encoded` cut into lines of [`LINE_LENGTH`], as .NET's `InsertLineBreaks`: no break
/// after the last line.
fn with_line_breaks(encoded: &str) -> String {
    let mut lines =
        String::with_capacity(encoded.len() + encoded.len() / LINE_LENGTH * LINE_BREAK.len());
    // Base64 is ASCII: a line is as many bytes as characters.
    for (index, line) in encoded.as_bytes().chunks(LINE_LENGTH).enumerate() {
        if index > 0 {
            lines.push_str(LINE_BREAK);
        }
        lines.push_str(std::str::from_utf8(line).unwrap_or_default());
    }
    lines
}
