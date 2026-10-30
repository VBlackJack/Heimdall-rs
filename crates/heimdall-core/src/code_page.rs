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

use zeroize::Zeroizing;

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

/// Characters of code page 437, the OEM code page of the United States, from 0x80 to 0xFF
/// (the Unicode consortium's CP437 mapping); below, it is ASCII.
const CP437_HIGH: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}',
    '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}',
    '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}',
    '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}',
    '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}',
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}',
    '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}',
    '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}',
    '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

/// First byte of the high half of an OEM code page table.
const OEM_HIGH_START: u8 = 0x80;

/// The most bytes one character of an OEM table takes in UTF-8 (the box drawing ones).
const OEM_MAX_UTF8_LEN: usize = 3;

/// The character an OEM code page `table` (its half from 0x80) writes as `byte`: ASCII below.
fn oem_char(table: &[char; 128], byte: u8) -> char {
    byte.checked_sub(OEM_HIGH_START)
        .and_then(|index| table.get(usize::from(index)))
        .copied()
        .unwrap_or(char::from(byte))
}

/// `bytes` read as code page 850, the OEM code page of Western European Windows, which a
/// console program such as `schtasks` writes in when its output goes to a pipe.
#[must_use]
pub fn decode_cp850(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| oem_char(&CP850_HIGH, byte))
        .collect()
}

/// `bytes` read as code page 437, the OEM code page of the United States.
#[must_use]
pub fn decode_cp437(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| oem_char(&CP437_HIGH, byte))
        .collect()
}

/// What a command printed, as the C# `CommandCredentialProvider.DecodeOutput` reads it: strict
/// UTF-8 first, else the OEM console code page. Rust has no pure way to ask Windows for that
/// code page, so it is taken to be 850, the one of Western European Windows (the French one).
///
/// The text may be a password: it is written once into a buffer sized for the worst case, so
/// it never grows (a reallocation would leave an unwiped copy) and wipes on drop.
#[must_use]
pub fn decode_command_output(bytes: &[u8]) -> Zeroizing<String> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        let mut out = Zeroizing::new(String::with_capacity(text.len()));
        out.push_str(text);
        return out;
    }
    let mut out = Zeroizing::new(String::with_capacity(
        bytes.len().saturating_mul(OEM_MAX_UTF8_LEN),
    ));
    for &byte in bytes {
        out.push(oem_char(&CP850_HIGH, byte));
    }
    out
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

    #[test]
    fn every_entry_of_each_oem_table_is_distinct() {
        for table in [&CP850_HIGH, &CP437_HIGH] {
            let mut seen = std::collections::HashSet::new();
            assert!(table.iter().all(|c| seen.insert(*c)));
        }
    }

    #[test]
    fn code_page_850_anchors() {
        let anchors = [
            (0x82, '\u{e9}'),
            (0x85, '\u{e0}'),
            (0x87, '\u{e7}'),
            (0x8A, '\u{e8}'),
            (0x9B, '\u{f8}'),
            (0xD5, '\u{131}'),
            (0xFF, '\u{00A0}'),
        ];
        for (byte, expected) in anchors {
            assert_eq!(oem_char(&CP850_HIGH, byte), expected, "{byte:#04x}");
        }
    }

    #[test]
    fn code_page_437_anchors() {
        assert_eq!(oem_char(&CP437_HIGH, 0x9B), '\u{a2}');
        assert_eq!(oem_char(&CP437_HIGH, 0xE1), '\u{df}');
        assert_eq!(oem_char(&CP437_HIGH, 0xFF), '\u{00A0}');
        for byte in 0xB0..=0xDF {
            let c = oem_char(&CP437_HIGH, byte);
            assert!(('\u{2500}'..='\u{259F}').contains(&c), "{byte:#04x}");
        }
        assert_eq!(decode_cp437(b"Pr\x88t"), "Pr\u{ea}t");
    }

    #[test]
    fn the_two_oem_tables_agree_on_the_shared_letters() {
        for byte in 0x80..=0x9A {
            assert_eq!(oem_char(&CP437_HIGH, byte), oem_char(&CP850_HIGH, byte));
        }
    }

    #[test]
    fn valid_utf8_command_output_is_untouched() {
        assert_eq!(
            &*decode_command_output("p\u{e9}ss \u{20AC}".as_bytes()),
            "p\u{e9}ss \u{20AC}"
        );
        assert_eq!(&*decode_command_output(b""), "");
    }

    #[test]
    fn an_accented_cp850_password_is_decoded() {
        assert_eq!(
            &*decode_command_output(b"m\x82t\x85\x87\n"),
            "m\u{e9}t\u{e0}\u{e7}\n"
        );
    }

    #[test]
    fn mixed_invalid_input_falls_back_whole() {
        // Valid UTF-8 (e acute) next to a lone 0x82: the whole text is code page 850.
        assert_eq!(
            &*decode_command_output(b"\xC3\xA9\x82"),
            "\u{251C}\u{ae}\u{e9}"
        );
    }
}
