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
//!
//! Files imported are read as the C# reads each: a `.rdp` file as .NET's `ReadAllText`, a
//! `MobaXterm` file in Windows-1252 without a byte order mark.

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

/// The text of a file as .NET's `File.ReadAllText` reads it, which the C# `.rdp` import
/// calls (`RdpImportService.cs:156`): a UTF-8, UTF-16 or UTF-32 byte order mark read and
/// left out, UTF-8 without one, and bytes that are not UTF-8 read as U+FFFD, as .NET's
/// default UTF-8 decoder replaces them. A Remote Desktop file saved by mstsc is UTF-16LE
/// with its mark.
///
/// # Errors
///
/// A byte order mark the bytes after it do not follow, as a UTF-16 file cut in the middle
/// of a character: refused, where .NET would put U+FFFD in place of what it cannot read.
pub fn decode_as_read_all_text(bytes: &[u8]) -> Result<String, Undecodable> {
    let decoded = decode(bytes)?;
    Ok(match decoded.encoding {
        TextEncoding::Latin1 => String::from_utf8_lossy(bytes).into_owned(),
        _ => decoded.text,
    })
}

/// Characters of Windows-1252 from 0x80 to 0x9F, where it is not Latin-1. The five bytes
/// it leaves undefined (0x81, 0x8D, 0x8F, 0x90, 0x9D) are the control characters of the
/// same value, as the Windows code page table maps them.
const WINDOWS_1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

/// First byte of [`WINDOWS_1252_HIGH`].
const WINDOWS_1252_HIGH_START: u8 = 0x80;

/// The text of a file written by a Windows program in the system's code page, as the C#
/// `MobaXterm` import reads one (`SettingsViewModel.cs:3023-3026`): UTF-8 after a UTF-8
/// byte order mark, which is left out, bytes that are not UTF-8 there read as U+FFFD;
/// Windows-1252 without one.
///
/// Unlike the C#, a file without a mark that is UTF-8 throughout is read as UTF-8: such a
/// file was read that way before, and Windows-1252 text that is also valid UTF-8 needs an
/// accented capital followed by a symbol (0xC3 then 0xA9) at every non-ASCII byte, which
/// session names and hosts do not hold.
#[must_use]
pub fn decode_windows_1252_fallback(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(UTF8_BOM) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    bytes
        .iter()
        .map(|&byte| {
            byte.checked_sub(WINDOWS_1252_HIGH_START)
                .and_then(|index| WINDOWS_1252_HIGH.get(usize::from(index)))
                .copied()
                .unwrap_or(char::from(byte))
        })
        .collect()
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
    use super::{
        Decoded, TextEncoding, Undecodable, Unencodable, decode, decode_as_read_all_text,
        decode_windows_1252_fallback, encode, looks_binary,
    };

    /// `full address:s:dc.lab` and a line end, as mstsc saves it: UTF-16LE after its mark.
    const MSTSC_UTF16LE: &[u8] = &[
        0xFF, 0xFE, b'f', 0, b'u', 0, b'l', 0, b'l', 0, b' ', 0, b'a', 0, b'd', 0, b'd', 0, b'r',
        0, b'e', 0, b's', 0, b's', 0, b':', 0, b's', 0, b':', 0, b'd', 0, b'c', 0, b'.', 0, b'l',
        0, b'a', 0, b'b', 0, b'\r', 0, b'\n', 0,
    ];

    #[test]
    fn a_remote_desktop_file_saved_by_mstsc_in_utf16le_is_read_without_its_mark() {
        assert_eq!(
            decode_as_read_all_text(MSTSC_UTF16LE).as_deref(),
            Ok("full address:s:dc.lab\r\n")
        );
    }

    #[test]
    fn a_remote_desktop_file_is_read_whatever_mark_dotnet_knows() {
        let text = "username:s:LAB\\\u{e9}lise\r\n";
        for encoding in [
            TextEncoding::Utf8 { bom: true },
            TextEncoding::Utf8 { bom: false },
            TextEncoding::Utf16Le,
            TextEncoding::Utf16Be,
            TextEncoding::Utf32Le,
            TextEncoding::Utf32Be,
        ] {
            let bytes = encode(text, encoding).expect("encoded");
            assert_eq!(
                decode_as_read_all_text(&bytes).as_deref(),
                Ok(text),
                "{encoding:?}"
            );
        }
    }

    #[test]
    fn a_remote_desktop_file_without_a_mark_that_is_not_utf8_reads_as_dotnet_replaces_it() {
        assert_eq!(
            decode_as_read_all_text(b"domain:s:caf\xE9\r\n").as_deref(),
            Ok("domain:s:caf\u{FFFD}\r\n"),
            "U+FFFD, as .NET's UTF-8 decoder, never Latin-1"
        );
    }

    #[test]
    fn a_remote_desktop_file_its_mark_does_not_describe_is_refused_not_misread() {
        let mut cut = MSTSC_UTF16LE.to_vec();
        cut.pop();
        assert_eq!(
            decode_as_read_all_text(&cut),
            Err(Undecodable(TextEncoding::Utf16Le)),
            "cut in the middle of a character"
        );
        assert_eq!(
            decode_as_read_all_text(&[0xFE, 0xFF, 0xDC, 0x00]),
            Err(Undecodable(TextEncoding::Utf16Be)),
            "a surrogate alone"
        );
    }

    #[test]
    fn a_mobaxterm_file_without_a_mark_is_windows_1252_as_the_csharp_reads_it() {
        assert_eq!(
            decode_windows_1252_fallback(b"caf\xE9 \x80 \x93x\x94 \x9C \x81"),
            "caf\u{e9} \u{20AC} \u{201C}x\u{201D} \u{153} \u{81}"
        );
        assert_eq!(
            decode_windows_1252_fallback(b"[Bookmarks]\r\n"),
            "[Bookmarks]\r\n"
        );
    }

    #[test]
    fn a_mobaxterm_file_after_a_utf8_mark_or_utf8_throughout_is_utf8() {
        assert_eq!(
            decode_windows_1252_fallback(b"\xEF\xBB\xBFcaf\xC3\xA9"),
            "caf\u{e9}",
            "the mark left out"
        );
        assert_eq!(
            decode_windows_1252_fallback(b"\xEF\xBB\xBFcaf\xE9"),
            "caf\u{FFFD}",
            "after the mark, as the C# UTF-8 decoder replaces what it cannot read"
        );
        assert_eq!(decode_windows_1252_fallback(b"caf\xC3\xA9"), "caf\u{e9}");
    }

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
