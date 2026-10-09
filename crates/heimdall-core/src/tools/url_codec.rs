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

//! Percent-encoding as the C# `UrlCodec` (`Heimdall.Core/Codecs/UrlCodec.cs`) does it,
//! through .NET's `Uri.EscapeDataString` and `Uri.UnescapeDataString`: every byte of the
//! text's UTF-8 but RFC 3986's unreserved characters escaped, in upper-case hexadecimal;
//! read back, every escaped run that is UTF-8 decoded, and the rest left as it was typed.

/// What introduces an escaped byte.
const ESCAPE: u8 = b'%';

/// Characters of an escaped byte: the sign and two hexadecimal digits.
const ESCAPE_LENGTH: usize = 3;

/// The hexadecimal digits an escaped byte is written with, as .NET writes them.
const HEX_DIGITS: &[u8; 16] = b"0123456789ABCDEF";

/// Bits of a hexadecimal digit.
const NIBBLE_BITS: u8 = 4;

/// The low digit's bits of a byte.
const LOW_NIBBLE: u8 = 0x0F;

/// The base hexadecimal digits are read in.
const HEX_RADIX: u32 = 16;

/// RFC 3986's unreserved characters besides letters and digits: never escaped.
const UNRESERVED_MARKS: &[u8] = b"-._~";

/// The characters the C# keeps as they are outside strict component encoding: those that
/// give a URL its structure, and `%`, so that what is already escaped stays so
/// (`UrlCodec.cs:69-70`).
const STRUCTURAL: &[char] = &[':', '/', '?', '#', '&', '=', '@', '%'];

/// `input` percent-encoded, as the C# `UrlCodec.Encode` (`UrlCodec.cs:23-30`): every
/// reserved character when `component`, else the text between the URL's structural
/// characters, which stay.
#[must_use]
pub fn encode(input: &str, component: bool) -> String {
    if component {
        return escape_data_string(input);
    }
    let mut result = String::with_capacity(input.len() * 2);
    let mut segment = String::new();
    for c in input.chars() {
        if STRUCTURAL.contains(&c) {
            result.push_str(&escape_data_string(&segment));
            segment.clear();
            result.push(c);
        } else {
            segment.push(c);
        }
    }
    result.push_str(&escape_data_string(&segment));
    result
}

/// `input` with its escaped bytes read back, as the C# `UrlCodec.Decode`
/// (`UrlCodec.cs:32-36`), .NET's `Uri.UnescapeDataString`: a `+` stays a `+`, and an
/// escaped run that is not UTF-8 is left escaped.
#[must_use]
pub fn decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut result = String::with_capacity(input.len());
    // A run of escaped bytes, with where it starts in `input`.
    let mut run: Vec<u8> = Vec::new();
    let mut run_start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if let Some(byte) = escaped_at(bytes, index) {
            if run.is_empty() {
                run_start = index;
            }
            run.push(byte);
            index += ESCAPE_LENGTH;
            continue;
        }
        flush(&mut result, &mut run, &input[run_start..index]);
        // Not an escape: the character as typed, whole.
        let next = input[index..].chars().next().map_or(1, char::len_utf8);
        result.push_str(&input[index..index + next]);
        index += next;
    }
    flush(&mut result, &mut run, &input[run_start..]);
    result
}

/// The byte escaped at `index` of `bytes`, when a `%` and two hexadecimal digits are there.
fn escaped_at(bytes: &[u8], index: usize) -> Option<u8> {
    if bytes.get(index) != Some(&ESCAPE) {
        return None;
    }
    let high = hex_value(*bytes.get(index + 1)?)?;
    let low = hex_value(*bytes.get(index + 2)?)?;
    Some((high << NIBBLE_BITS) | low)
}

/// The value of hexadecimal digit `digit`, either case.
fn hex_value(digit: u8) -> Option<u8> {
    char::from(digit)
        .to_digit(HEX_RADIX)
        .and_then(|value| u8::try_from(value).ok())
}

/// Appends the escaped run `run`, typed as `typed`, to `result`: its UTF-8 decoded, each
/// byte that is not left as it was typed. The run is emptied.
fn flush(result: &mut String, run: &mut Vec<u8>, typed: &str) {
    if run.is_empty() {
        return;
    }
    // Each byte of the run was typed as three characters.
    let mut offset = 0;
    for chunk in run.utf8_chunks() {
        result.push_str(chunk.valid());
        offset += chunk.valid().len();
        let invalid = chunk.invalid().len();
        let from = offset * ESCAPE_LENGTH;
        let to = (offset + invalid) * ESCAPE_LENGTH;
        result.push_str(typed.get(from..to).unwrap_or_default());
        offset += invalid;
    }
    run.clear();
}

/// `text` escaped as .NET's `Uri.EscapeDataString`: each byte of its UTF-8 but letters,
/// digits and [`UNRESERVED_MARKS`] as `%` and two upper-case hexadecimal digits.
fn escape_data_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || UNRESERVED_MARKS.contains(&byte) {
            escaped.push(char::from(byte));
        } else {
            escaped.push(char::from(ESCAPE));
            escaped.push(char::from(HEX_DIGITS[usize::from(byte >> NIBBLE_BITS)]));
            escaped.push(char::from(HEX_DIGITS[usize::from(byte & LOW_NIBBLE)]));
        }
    }
    escaped
}
