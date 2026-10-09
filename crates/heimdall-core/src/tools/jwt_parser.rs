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

//! JSON Web Tokens read as the C# `JwtParser`, `JwtClaimsEvaluator` and `JwtHmacVerifier`
//! (`Heimdall.Core/Jwt`) read them: three Base64 URL-safe segments, the header and the payload
//! JSON, shown indented; the `exp` claim against the clock; an HS256, HS384 or HS512
//! signature checked against a secret, compared in constant time.
//!
//! The JSON is shown as .NET's `JsonDocument` written back by a `Utf8JsonWriter` with
//! `Indented` and its default encoder writes it: in its own order, numbers as typed, two
//! spaces a level, `": "` after a key, an empty object or array kept on its line; in strings,
//! everything past printable ASCII, and `"`, `&`, `'`, `+`, `<`, `>` and `` ` ``, as
//! `\uXXXX` in upper-case hexadecimal, the control characters .NET names by their names.

use std::fmt::Write as _;

use hmac::{Hmac, KeyInit, Mac};
use serde_json::Value;
use sha2::{Sha256, Sha384, Sha512};

use super::base64_codec;

/// What separates a token's segments.
const SEPARATOR: char = '.';

/// Deepest nesting .NET's `JsonDocument` reads by default (`JsonReaderOptions.MaxDepth`).
const MAX_DEPTH: usize = 64;

/// What one level of the JSON shown is indented by, as .NET's two spaces.
const INDENT: &str = "  ";

/// What separates a key from its value, as .NET's indented writer.
const KEY_SEPARATOR: &str = ": ";

/// The printable ASCII characters .NET's default encoder still escapes: the quotation mark,
/// and the HTML-sensitive ones.
const ESCAPED_ASCII: &[char] = &['"', '&', '\'', '+', '<', '>', '`'];

/// The UTF-16 units that open a surrogate pair.
const HIGH_SURROGATES: std::ops::Range<u16> = 0xD800..0xDC00;

/// Hexadecimal digits of a `\u` escape.
const UNIT_DIGITS: usize = 4;

/// The claim of the expiry.
const EXP_CLAIM: &str = "exp";

/// The header's field of the algorithm.
const ALG_FIELD: &str = "alg";

/// The earliest second .NET's `DateTimeOffset.FromUnixTimeSeconds` takes: year 1, January 1.
pub const MIN_UNIX_SECONDS: i64 = -62_135_596_800;

/// The latest: year 9999, December 31, 23:59:59.
pub const MAX_UNIX_SECONDS: i64 = 253_402_300_799;

/// Why a text is not a token, as the C# `JwtDecodeError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JwtDecodeError {
    /// Not three segments.
    #[error("a JWT has three segments separated by dots")]
    InvalidFormat,
    /// A segment not Base64 URL-safe, or a header or payload not JSON.
    #[error("the JWT is not valid Base64 URL-safe JSON")]
    DecodeFailed,
}

/// A token read, as the C# `JwtDecoded`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JwtDecoded {
    /// The header's JSON, as decoded.
    pub header_json: String,
    /// The payload's JSON, as decoded.
    pub payload_json: String,
    /// The signature's bytes.
    pub signature: Vec<u8>,
    /// The header's segment, as given.
    pub header_raw: String,
    /// The payload's segment, as given.
    pub payload_raw: String,
    /// The signature's segment, as given.
    pub signature_raw: String,
}

impl JwtDecoded {
    /// The header indented, its lines ended by `new_line`, as the C# `PrettyHeaderJson`.
    #[must_use]
    pub fn pretty_header(&self, new_line: &str) -> String {
        pretty_print(&self.header_json, new_line)
    }

    /// The payload indented, as the C# `PrettyPayloadJson`.
    #[must_use]
    pub fn pretty_payload(&self, new_line: &str) -> String {
        pretty_print(&self.payload_json, new_line)
    }

    /// The signature in lower-case hexadecimal, as the C# `SignatureHex`.
    #[must_use]
    pub fn signature_hex(&self) -> String {
        data_encoding::HEXLOWER.encode(&self.signature)
    }
}

/// The token `input` is, as the C# `JwtParser.TryDecode` (`JwtParser.cs:46-81`): trimmed,
/// three segments, each Base64 URL-safe, the first two JSON; the signature may be empty.
///
/// # Errors
///
/// [`JwtDecodeError::InvalidFormat`] for a blank text or not three segments,
/// [`JwtDecodeError::DecodeFailed`] for a segment that does not decode.
pub fn decode(input: &str) -> Result<JwtDecoded, JwtDecodeError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(JwtDecodeError::InvalidFormat);
    }
    let parts: Vec<&str> = trimmed.split(SEPARATOR).collect();
    let [header, payload, signature] = parts[..] else {
        return Err(JwtDecodeError::InvalidFormat);
    };
    let header_json = decode_base64url_string(header).ok_or(JwtDecodeError::DecodeFailed)?;
    let payload_json = decode_base64url_string(payload).ok_or(JwtDecodeError::DecodeFailed)?;
    let signature_bytes = decode_base64url_bytes(signature).ok_or(JwtDecodeError::DecodeFailed)?;
    if !is_valid_json(&header_json) || !is_valid_json(&payload_json) {
        return Err(JwtDecodeError::DecodeFailed);
    }
    Ok(JwtDecoded {
        header_json,
        payload_json,
        signature: signature_bytes,
        header_raw: header.to_owned(),
        payload_raw: payload.to_owned(),
        signature_raw: signature.to_owned(),
    })
}

/// The bytes a Base64 URL-safe segment stands for, its padding put back, as the C#
/// `DecodeBase64UrlBytes` (`JwtParser.cs:83-107`); `None` when it is not Base64.
#[must_use]
pub fn decode_base64url_bytes(segment: &str) -> Option<Vec<u8>> {
    base64_codec::decode(segment, true).ok()
}

/// The text a Base64 URL-safe segment stands for, read as UTF-8 as .NET's `Encoding.UTF8` reads
/// it, what is not UTF-8 replaced, as the C# `DecodeBase64UrlString`.
#[must_use]
pub fn decode_base64url_string(segment: &str) -> Option<String> {
    decode_base64url_bytes(segment).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Whether `json` is one JSON value .NET's `JsonDocument.Parse` reads: valid, and nested no
/// deeper than [`MAX_DEPTH`].
#[must_use]
pub fn is_valid_json(json: &str) -> bool {
    serde_json::from_str::<serde::de::IgnoredAny>(json).is_ok() && nesting(json) <= MAX_DEPTH
}

/// How deep the objects and arrays of `json` nest.
fn nesting(json: &str) -> usize {
    let (mut depth, mut deepest, mut in_string, mut escaped) = (0_usize, 0_usize, false, false);
    for byte in json.bytes() {
        if in_string {
            match (escaped, byte) {
                (true, _) => escaped = false,
                (false, b'\\') => escaped = true,
                (false, b'"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

/// `json` indented as the C# `PrettyPrintJson` writes it (`JwtParser.cs:115-123`), its lines
/// ended by `new_line`; a text that is not JSON as it is.
#[must_use]
pub fn pretty_print(json: &str, new_line: &str) -> String {
    if !is_valid_json(json) {
        return json.to_owned();
    }
    let mut writer = Writer {
        json,
        at: 0,
        out: String::with_capacity(json.len() * 2),
        new_line,
    };
    writer.value(0);
    writer.out
}

/// The writer of a valid document: where it reads, what it wrote.
struct Writer<'a> {
    json: &'a str,
    at: usize,
    out: String,
    new_line: &'a str,
}

impl Writer<'_> {
    fn peek(&self) -> u8 {
        self.json
            .as_bytes()
            .get(self.at)
            .copied()
            .unwrap_or_default()
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), b' ' | b'\t' | b'\r' | b'\n') {
            self.at += 1;
        }
    }

    fn line(&mut self, depth: usize) {
        self.out.push_str(self.new_line);
        for _ in 0..depth {
            self.out.push_str(INDENT);
        }
    }

    fn value(&mut self, depth: usize) {
        self.skip_space();
        match self.peek() {
            b'{' => self.container(depth, b'}', true),
            b'[' => self.container(depth, b']', false),
            b'"' => {
                let text = self.string();
                write_string(&mut self.out, &text);
            }
            _ => {
                // A number or a literal, written as typed.
                let start = self.at;
                while !matches!(
                    self.peek(),
                    0 | b',' | b']' | b'}' | b' ' | b'\t' | b'\r' | b'\n'
                ) {
                    self.at += 1;
                }
                self.out.push_str(&self.json[start..self.at]);
            }
        }
    }

    /// An object, its keys and values, or an array, its values; empty, on one line.
    fn container(&mut self, depth: usize, close: u8, keyed: bool) {
        let open = self.peek();
        self.at += 1;
        self.skip_space();
        if self.peek() == close {
            self.at += 1;
            self.out.push(char::from(open));
            self.out.push(char::from(close));
            return;
        }
        self.out.push(char::from(open));
        loop {
            self.line(depth + 1);
            if keyed {
                self.skip_space();
                let key = self.string();
                write_string(&mut self.out, &key);
                self.skip_space();
                // The colon.
                self.at += 1;
                self.out.push_str(KEY_SEPARATOR);
            }
            self.value(depth + 1);
            self.skip_space();
            let next = self.peek();
            self.at += 1;
            if next != b',' {
                break;
            }
            self.out.push(',');
        }
        self.line(depth);
        self.out.push(char::from(close));
    }

    /// A string's value, its escapes read; the reader past its closing quote.
    fn string(&mut self) -> String {
        // The opening quote.
        self.at += 1;
        let mut value = String::new();
        let mut start = self.at;
        loop {
            match self.peek() {
                b'"' | 0 => {
                    value.push_str(&self.json[start..self.at]);
                    self.at += 1;
                    return value;
                }
                b'\\' => {
                    value.push_str(&self.json[start..self.at]);
                    self.at += 1;
                    self.escape(&mut value);
                    start = self.at;
                }
                _ => self.at += 1,
            }
        }
    }

    /// The escape after a backslash, read into `value`.
    fn escape(&mut self, value: &mut String) {
        let letter = self.peek();
        self.at += 1;
        match letter {
            b'b' => value.push('\u{8}'),
            b'f' => value.push('\u{c}'),
            b'n' => value.push('\n'),
            b'r' => value.push('\r'),
            b't' => value.push('\t'),
            b'u' => {
                let first = self.unit();
                let mut units = vec![first];
                if HIGH_SURROGATES.contains(&first) && self.json[self.at..].starts_with("\\u") {
                    self.at += 2;
                    units.push(self.unit());
                }
                value.extend(
                    char::decode_utf16(units).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)),
                );
            }
            other => value.push(char::from(other)),
        }
    }

    /// Four hexadecimal digits read as a UTF-16 unit.
    fn unit(&mut self) -> u16 {
        let digits = self
            .json
            .get(self.at..self.at + UNIT_DIGITS)
            .unwrap_or_default();
        self.at += UNIT_DIGITS;
        u16::from_str_radix(digits, 16).unwrap_or_default()
    }
}

/// `text` written in quotes as .NET's default encoder escapes it.
fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' if !ESCAPED_ASCII.contains(&c) => out.push(c),
            _ => {
                let mut units = [0_u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04X}");
                }
            }
        }
    }
    out.push('"');
}

/// What the `exp` claim says, as the C# `JwtExpirationStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expiration {
    /// Past, at this Unix second.
    Expired(i64),
    /// Not yet past, until this Unix second.
    Valid(i64),
    /// No `exp` claim.
    NoExpiry,
    /// An `exp` that is not a whole number of seconds .NET takes, or a payload not an object.
    InvalidClaim,
}

/// What the `exp` claim of `payload_json` says at Unix second `now`, as the C#
/// `JwtClaimsEvaluator.EvaluateExpiration` (`JwtClaimsEvaluator.cs:33-65`): a moment equal
/// to `now` is not past.
#[must_use]
pub fn evaluate_expiration(payload_json: &str, now: i64) -> Expiration {
    let Ok(Value::Object(claims)) = serde_json::from_str::<Value>(payload_json) else {
        return Expiration::InvalidClaim;
    };
    let Some(exp) = claims.get(EXP_CLAIM) else {
        return Expiration::NoExpiry;
    };
    match exp.as_i64() {
        Some(seconds) if (MIN_UNIX_SECONDS..=MAX_UNIX_SECONDS).contains(&seconds) => {
            if seconds < now {
                Expiration::Expired(seconds)
            } else {
                Expiration::Valid(seconds)
            }
        }
        _ => Expiration::InvalidClaim,
    }
}

/// The kind of a token's `alg`, as the C# `JwtAlgorithmKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JwtAlgorithm {
    /// Anything else, or none given.
    #[default]
    Unknown,
    /// `none`: not signed.
    None,
    /// HS256.
    Hmac256,
    /// HS384.
    Hmac384,
    /// HS512.
    Hmac512,
    /// An `RS` algorithm.
    Rsa,
    /// An `ES` algorithm.
    Ecdsa,
    /// A `PS` algorithm.
    RsaPss,
}

impl JwtAlgorithm {
    /// Whether it is checked with a shared secret here.
    #[must_use]
    pub const fn is_hmac(self) -> bool {
        matches!(self, Self::Hmac256 | Self::Hmac384 | Self::Hmac512)
    }

    /// Whether it takes a public key, which the C# does not check.
    #[must_use]
    pub const fn is_asymmetric(self) -> bool {
        matches!(self, Self::Rsa | Self::Ecdsa | Self::RsaPss)
    }
}

/// The kind of `alg`, as the C# `JwtHmacVerifier.ClassifyAlgorithm` (`JwtHmacVerifier.cs:44-55`):
/// exact names, case counting.
#[must_use]
pub fn classify_algorithm(alg: Option<&str>) -> JwtAlgorithm {
    match alg {
        Some("none") => JwtAlgorithm::None,
        Some("HS256") => JwtAlgorithm::Hmac256,
        Some("HS384") => JwtAlgorithm::Hmac384,
        Some("HS512") => JwtAlgorithm::Hmac512,
        Some(alg) if alg.starts_with("RS") => JwtAlgorithm::Rsa,
        Some(alg) if alg.starts_with("ES") => JwtAlgorithm::Ecdsa,
        Some(alg) if alg.starts_with("PS") => JwtAlgorithm::RsaPss,
        _ => JwtAlgorithm::Unknown,
    }
}

/// The `alg` of a header, as the C# view model's `ExtractAlgorithm`
/// (`JwtParserViewModel.cs:295-301`); `None` when absent or not a string.
///
/// The C# throws for a header that is not an object or an `alg` that is neither a string
/// nor null; both are taken here for no algorithm.
#[must_use]
pub fn extract_algorithm(header_json: &str) -> Option<String> {
    match serde_json::from_str::<Value>(header_json) {
        Ok(Value::Object(mut fields)) => match fields.remove(ALG_FIELD) {
            Some(Value::String(alg)) => Some(alg),
            _ => None,
        },
        _ => None,
    }
}

/// What checking a signature found, as the C# `JwtHmacVerificationResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HmacVerification {
    /// The secret signs the token.
    Valid,
    /// It does not.
    Invalid,
    /// The algorithm is not an HMAC.
    AlgorithmNotHmac,
    /// No secret.
    MalformedInput,
}

/// Whether `secret`, as UTF-8, signs `decoded` by `alg`, as the C# `JwtHmacVerifier.Verify`
/// (`JwtHmacVerifier.cs:57-75`): the HMAC of `header.payload` as given, compared with the
/// signature's bytes in constant time.
#[must_use]
pub fn verify_hmac(decoded: &JwtDecoded, alg: JwtAlgorithm, secret: &str) -> HmacVerification {
    if secret.is_empty() {
        return HmacVerification::MalformedInput;
    }
    let input = format!("{}{SEPARATOR}{}", decoded.header_raw, decoded.payload_raw);
    let key = secret.as_bytes();
    let signed = match alg {
        JwtAlgorithm::Hmac256 => signs::<Hmac<Sha256>>(key, &input, &decoded.signature),
        JwtAlgorithm::Hmac384 => signs::<Hmac<Sha384>>(key, &input, &decoded.signature),
        JwtAlgorithm::Hmac512 => signs::<Hmac<Sha512>>(key, &input, &decoded.signature),
        _ => return HmacVerification::AlgorithmNotHmac,
    };
    if signed {
        HmacVerification::Valid
    } else {
        HmacVerification::Invalid
    }
}

/// Whether `signature` is the code of `input` under `key` by the MAC `M`, compared in
/// constant time.
fn signs<M: Mac + KeyInit>(key: &[u8], input: &str, signature: &[u8]) -> bool {
    let Ok(mut mac) = <M as KeyInit>::new_from_slice(key) else {
        return false;
    };
    Mac::update(&mut mac, input.as_bytes());
    mac.verify_slice(signature).is_ok()
}
