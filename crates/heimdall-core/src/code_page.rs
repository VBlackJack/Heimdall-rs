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

//! Text in the code pages of Western European Windows, read as .NET's `Encoding.GetEncoding`
//! reads them: Windows-1252, its ANSI code page, and 850, the OEM code page its console
//! programs write in.

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

/// The character Windows-1252 writes as `byte`: Latin-1 outside 0x80 to 0x9F.
#[must_use]
pub fn windows_1252_char(byte: u8) -> char {
    byte.checked_sub(WINDOWS_1252_HIGH_START)
        .and_then(|index| WINDOWS_1252_HIGH.get(usize::from(index)))
        .copied()
        .unwrap_or(char::from(byte))
}

/// `bytes` read as Windows-1252: every byte one character.
#[must_use]
pub fn decode_windows_1252(bytes: &[u8]) -> String {
    bytes.iter().copied().map(windows_1252_char).collect()
}

/// Characters of code page 850 from 0x80 to 0xFF; below, it is ASCII.
const CP850_HIGH: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}',
    '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}',
    '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00F8}', '\u{00A3}', '\u{00D8}', '\u{00D7}', '\u{0192}',
    '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}',
    '\u{00BF}', '\u{00AE}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{00C1}', '\u{00C2}', '\u{00C0}',
    '\u{00A9}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{00A2}', '\u{00A5}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{00E3}', '\u{00C3}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{00A4}',
    '\u{00F0}', '\u{00D0}', '\u{00CA}', '\u{00CB}', '\u{00C8}', '\u{0131}', '\u{00CD}', '\u{00CE}',
    '\u{00CF}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{00A6}', '\u{00CC}', '\u{2580}',
    '\u{00D3}', '\u{00DF}', '\u{00D4}', '\u{00D2}', '\u{00F5}', '\u{00D5}', '\u{00B5}', '\u{00FE}',
    '\u{00DE}', '\u{00DA}', '\u{00DB}', '\u{00D9}', '\u{00FD}', '\u{00DD}', '\u{00AF}', '\u{00B4}',
    '\u{00AD}', '\u{00B1}', '\u{2017}', '\u{00BE}', '\u{00B6}', '\u{00A7}', '\u{00F7}', '\u{00B8}',
    '\u{00B0}', '\u{00A8}', '\u{00B7}', '\u{00B9}', '\u{00B3}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

/// First byte of [`CP850_HIGH`].
const CP850_HIGH_START: u8 = 0x80;

/// `bytes` read as code page 850, the OEM code page of Western European Windows, which a
/// console program such as `schtasks` writes in when its output goes to a pipe.
#[must_use]
pub fn decode_cp850(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| {
            byte.checked_sub(CP850_HIGH_START)
                .and_then(|index| CP850_HIGH.get(usize::from(index)))
                .copied()
                .unwrap_or(char::from(byte))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_high_range_is_windows_1252_and_the_rest_latin_1() {
        assert_eq!(
            decode_windows_1252(b"caf\xE9 \x80 \x93x\x94 \x9C \x81 A"),
            "caf\u{e9} \u{20AC} \u{201C}x\u{201D} \u{153} \u{81} A"
        );
    }

    #[test]
    fn code_page_850_reads_the_french_console_letters() {
        assert_eq!(
            decode_cp850(b"Pr\x88t \x82t\x82 \x85 A"),
            "Pr\u{ea}t \u{e9}t\u{e9} \u{e0} A"
        );
    }
}
