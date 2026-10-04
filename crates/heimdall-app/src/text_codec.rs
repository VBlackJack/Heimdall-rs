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

//! A server's file as the integrated editor shows it, and back to the same bytes, as the
//! C# `RemoteTextFileCodec`: its byte order mark read and kept, UTF-8 without one, Latin-1
//! when it is not UTF-8. Text left untouched comes back byte for byte.

/// How long the start of a file looked at for binary content is, as the C#.
const SNIFF_LENGTH: usize = 8 * 1024;

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: &[u8] = &[0xFF, 0xFE];
const UTF16BE_BOM: &[u8] = &[0xFE, 0xFF];
const UTF32LE_BOM: &[u8] = &[0xFF, 0xFE, 0x00, 0x00];
const UTF32BE_BOM: &[u8] = &[0x00, 0x00, 0xFE, 0xFF];

/// How a file's text is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    /// UTF-8, with its byte order mark or not.
    Utf8 {
        /// The file starts with the UTF-8 byte order mark.
        bom: bool,
    },
    /// UTF-16, little-endian, with its byte order mark.
    Utf16Le,
    /// UTF-16, big-endian, with its byte order mark.
    Utf16Be,
    /// UTF-32, little-endian, with its byte order mark.
    Utf32Le,
    /// UTF-32, big-endian, with its byte order mark.
    Utf32Be,
    /// ISO-8859-1: a file that is not UTF-8, one character per byte.
    Latin1,
}

/// A file's text and how it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// The text, its byte order mark left out.
    pub text: String,
    /// How it is stored.
    pub encoding: TextEncoding,
}

/// The file's byte order mark names an encoding its bytes do not follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Undecodable(pub TextEncoding);

/// A character the file's encoding cannot store, where it is: line and column from 1, the
/// column in characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unencodable {
    /// Line, from 1.
    pub line: usize,
    /// Column in characters, from 1.
    pub column: usize,
}

/// The text of `bytes`.
///
/// # Errors
///
/// A byte order mark the bytes after it do not follow.
pub fn decode(bytes: &[u8]) -> Result<Decoded, Undecodable> {
    // UTF-32 first: its little-endian mark starts as UTF-16's.
    if let Some(rest) = bytes.strip_prefix(UTF32LE_BOM) {
        return utf32(rest, u32::from_le_bytes, TextEncoding::Utf32Le);
    }
    if let Some(rest) = bytes.strip_prefix(UTF32BE_BOM) {
        return utf32(rest, u32::from_be_bytes, TextEncoding::Utf32Be);
    }
    if let Some(rest) = bytes.strip_prefix(UTF8_BOM) {
        let encoding = TextEncoding::Utf8 { bom: true };
        return std::str::from_utf8(rest)
            .map(|text| Decoded {
                text: text.to_owned(),
                encoding,
            })
            .map_err(|_| Undecodable(encoding));
    }
    if let Some(rest) = bytes.strip_prefix(UTF16LE_BOM) {
        return utf16(rest, u16::from_le_bytes, TextEncoding::Utf16Le);
    }
    if let Some(rest) = bytes.strip_prefix(UTF16BE_BOM) {
        return utf16(rest, u16::from_be_bytes, TextEncoding::Utf16Be);
    }
    Ok(match std::str::from_utf8(bytes) {
        Ok(text) => Decoded {
            text: text.to_owned(),
            encoding: TextEncoding::Utf8 { bom: false },
        },
        Err(_) => Decoded {
            text: bytes.iter().copied().map(char::from).collect(),
            encoding: TextEncoding::Latin1,
        },
    })
}

fn utf16(
    bytes: &[u8],
    unit: fn([u8; 2]) -> u16,
    encoding: TextEncoding,
) -> Result<Decoded, Undecodable> {
    let (units, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(Undecodable(encoding));
    }
    char::decode_utf16(units.iter().map(|pair| unit(*pair)))
        .collect::<Result<String, _>>()
        .map(|text| Decoded { text, encoding })
        .map_err(|_| Undecodable(encoding))
}

fn utf32(
    bytes: &[u8],
    unit: fn([u8; 4]) -> u32,
    encoding: TextEncoding,
) -> Result<Decoded, Undecodable> {
    let (units, rest) = bytes.as_chunks::<4>();
    if !rest.is_empty() {
        return Err(Undecodable(encoding));
    }
    units
        .iter()
        .map(|quad| char::from_u32(unit(*quad)))
        .collect::<Option<String>>()
        .map(|text| Decoded { text, encoding })
        .ok_or(Undecodable(encoding))
}

/// `text` stored as `encoding`, its byte order mark first when it has one.
///
/// # Errors
///
/// A character Latin-1 cannot store: refused, never written as another.
pub fn encode(text: &str, encoding: TextEncoding) -> Result<Vec<u8>, Unencodable> {
    Ok(match encoding {
        TextEncoding::Utf8 { bom } => {
            let mut bytes = if bom { UTF8_BOM.to_vec() } else { Vec::new() };
            bytes.extend_from_slice(text.as_bytes());
            bytes
        }
        TextEncoding::Utf16Le => {
            with_bom(UTF16LE_BOM, text.encode_utf16().flat_map(u16::to_le_bytes))
        }
        TextEncoding::Utf16Be => {
            with_bom(UTF16BE_BOM, text.encode_utf16().flat_map(u16::to_be_bytes))
        }
        TextEncoding::Utf32Le => with_bom(
            UTF32LE_BOM,
            text.chars().flat_map(|c| u32::from(c).to_le_bytes()),
        ),
        TextEncoding::Utf32Be => with_bom(
            UTF32BE_BOM,
            text.chars().flat_map(|c| u32::from(c).to_be_bytes()),
        ),
        TextEncoding::Latin1 => latin1(text)?,
    })
}

fn with_bom(bom: &[u8], rest: impl Iterator<Item = u8>) -> Vec<u8> {
    bom.iter().copied().chain(rest).collect()
}

fn latin1(text: &str) -> Result<Vec<u8>, Unencodable> {
    let mut bytes = Vec::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            bytes.push(b'\n');
        }
        for (column, character) in line.chars().enumerate() {
            let Ok(byte) = u8::try_from(u32::from(character)) else {
                return Err(Unencodable {
                    line: index + 1,
                    column: column + 1,
                });
            };
            bytes.push(byte);
        }
    }
    Ok(bytes)
}

/// Whether `bytes`, the start of a file, look like a program, an image or an archive
/// rather than text: a NUL in the first 8 KiB, as the C#, unless a UTF-16 or UTF-32 byte
/// order mark says the NULs are text.
#[must_use]
pub fn looks_binary(bytes: &[u8]) -> bool {
    let wide = [UTF32LE_BOM, UTF32BE_BOM, UTF16LE_BOM, UTF16BE_BOM]
        .iter()
        .any(|bom| bytes.starts_with(bom));
    !wide && bytes.iter().take(SNIFF_LENGTH).any(|&byte| byte == 0)
}

#[cfg(test)]
mod tests {
    use super::{Decoded, TextEncoding, Undecodable, Unencodable, decode, encode, looks_binary};

    fn round_trip(bytes: &[u8]) -> Decoded {
        let decoded = decode(bytes).expect("decoded");
        assert_eq!(
            encode(&decoded.text, decoded.encoding).expect("encoded"),
            bytes,
            "{bytes:?} back as it was"
        );
        decoded
    }

    #[test]
    fn each_encoding_is_read_and_written_back_byte_for_byte() {
        let text = "Grüße\r\nline 2\n";
        let utf8 = text.as_bytes().to_vec();
        assert_eq!(
            round_trip(&utf8).encoding,
            TextEncoding::Utf8 { bom: false }
        );
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(text.as_bytes());
        let decoded = round_trip(&bom);
        assert_eq!(decoded.encoding, TextEncoding::Utf8 { bom: true });
        assert_eq!(decoded.text, text, "the mark is not text");

        for encoding in [
            TextEncoding::Utf16Le,
            TextEncoding::Utf16Be,
            TextEncoding::Utf32Le,
            TextEncoding::Utf32Be,
        ] {
            let bytes = encode("é𝄞\r\n", encoding).expect("encoded");
            let decoded = round_trip(&bytes);
            assert_eq!(
                (decoded.text.as_str(), decoded.encoding),
                ("é𝄞\r\n", encoding)
            );
        }
        assert_eq!(round_trip(b"").encoding, TextEncoding::Utf8 { bom: false });
    }

    #[test]
    fn a_file_that_is_not_utf8_is_latin1_and_comes_back_unchanged() {
        let bytes: Vec<u8> = (0..=255).collect();
        let decoded = round_trip(&bytes);
        assert_eq!(decoded.encoding, TextEncoding::Latin1);
        assert_eq!(decoded.text.chars().nth(0xE9), Some('é'));
    }

    #[test]
    fn a_character_latin1_cannot_store_is_refused_where_it_is() {
        assert_eq!(
            encode("café\nok €", TextEncoding::Latin1),
            Err(Unencodable { line: 2, column: 4 })
        );
        assert_eq!(
            encode("café", TextEncoding::Latin1),
            Ok(vec![b'c', b'a', b'f', 0xE9])
        );
    }

    #[test]
    fn a_mark_its_bytes_do_not_follow_is_refused() {
        assert_eq!(
            decode(&[0xEF, 0xBB, 0xBF, 0xFF]),
            Err(Undecodable(TextEncoding::Utf8 { bom: true }))
        );
        assert_eq!(
            decode(&[0xFF, 0xFE, 0x41]),
            Err(Undecodable(TextEncoding::Utf16Le)),
            "an odd length"
        );
        assert_eq!(
            decode(&[0xFF, 0xFE, 0x00, 0xD8, 0x41, 0x00]),
            Err(Undecodable(TextEncoding::Utf16Le)),
            "a surrogate alone"
        );
        assert_eq!(
            decode(&[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x11, 0x00, 0x00]),
            Err(Undecodable(TextEncoding::Utf32Be)),
            "past the last character"
        );
    }

    #[test]
    fn a_nul_near_the_start_says_binary_unless_the_text_is_wide() {
        assert!(looks_binary(b"\x7FELF\x02\x01\x01\x00"));
        assert!(!looks_binary(b"plain text\n"));
        let mut late = vec![b'a'; 8 * 1024];
        late.push(0);
        assert!(!looks_binary(&late), "past what is looked at");
        let wide = encode("text", TextEncoding::Utf16Le).expect("encoded");
        assert!(!looks_binary(&wide));
    }
}
