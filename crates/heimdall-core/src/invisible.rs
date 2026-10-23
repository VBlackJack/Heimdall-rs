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

//! Characters that show nothing, or change how what follows shows. One list for the whole
//! application: the text a server or a terminal sends is cleaned with it, and the commands
//! of the library are refused when they hold one, so an import cannot bring what a capture
//! removes.

/// Whether `c` is one of the [`INVISIBLE`] characters.
#[must_use]
pub fn is_invisible(c: char) -> bool {
    INVISIBLE
        .iter()
        .any(|(first, last)| (*first..=*last).contains(&c))
}

/// Format characters (Unicode category Cf: bidirectional controls, zero-width spaces and
/// joiners, the byte order mark, tags), the soft hyphen, and the line and paragraph
/// separators.
pub const INVISIBLE: [(char, char); 16] = [
    ('\u{00AD}', '\u{00AD}'),
    ('\u{0600}', '\u{0605}'),
    ('\u{061C}', '\u{061C}'),
    ('\u{06DD}', '\u{06DD}'),
    ('\u{070F}', '\u{070F}'),
    ('\u{0890}', '\u{0891}'),
    ('\u{08E2}', '\u{08E2}'),
    ('\u{180E}', '\u{180E}'),
    ('\u{200B}', '\u{200F}'),
    ('\u{2028}', '\u{202E}'),
    ('\u{2060}', '\u{2064}'),
    ('\u{2066}', '\u{206F}'),
    ('\u{FEFF}', '\u{FEFF}'),
    ('\u{FFF9}', '\u{FFFB}'),
    ('\u{E0001}', '\u{E0001}'),
    ('\u{E0020}', '\u{E007F}'),
];

#[cfg(test)]
mod tests {
    use super::is_invisible;

    #[test]
    fn format_and_direction_characters_are_invisible_and_text_is_not() {
        for c in [
            '\u{061C}',
            '\u{00AD}',
            '\u{E0041}',
            '\u{202E}',
            '\u{200B}',
            '\u{FEFF}',
        ] {
            assert!(is_invisible(c), "{c:?}");
        }
        for c in ['a', ' ', '-', '\u{e9}', '\u{1F600}'] {
            assert!(!is_invisible(c), "{c:?}");
        }
    }
}
