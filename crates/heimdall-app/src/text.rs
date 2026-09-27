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

//! Text chosen by a server (titles, prompts, disconnect messages) made safe to show.
//!
//! Control characters and Unicode bidirectional controls are removed: the latter can make a
//! prompt read differently from what it is (Trojan Source, CVE-2021-42574). Length is capped.
//!
//! A command about to be run is shown the other way: nothing removed, nothing cut, every
//! invisible character written out.

use std::fmt::Write as _;

/// Longest server text shown, in characters.
pub const MAX_SERVER_TEXT_CHARS: usize = 256;

/// Unicode bidirectional formatting characters.
const BIDI_CONTROLS: [char; 12] = [
    '\u{061C}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}',
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
];

/// `text` without control or bidi characters, at most [`MAX_SERVER_TEXT_CHARS`] long.
#[must_use]
pub fn server_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() && !BIDI_CONTROLS.contains(c))
        .take(MAX_SERVER_TEXT_CHARS)
        .collect()
}

/// Characters that show nothing, or change how what follows shows: format characters
/// (Unicode category Cf: bidirectional controls, zero-width spaces and joiners, the byte
/// order mark, tags) and the line and paragraph separators.
const INVISIBLE: [(char, char); 16] = [
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

/// `text` with every character that would not show as itself written out as `\u{...}`:
/// controls, format and separator characters, and any space other than the plain one.
/// Nothing is removed and nothing is cut, so what is shown is what there is, for a command
/// someone is about to agree to run.
#[must_use]
pub fn visible_text(text: &str) -> String {
    let mut shown = String::with_capacity(text.len());
    for c in text.chars() {
        let hidden = c.is_control()
            || (c.is_whitespace() && c != ' ')
            || INVISIBLE
                .iter()
                .any(|(first, last)| (*first..=*last).contains(&c));
        if hidden {
            let _ = write!(shown, "\\u{{{:04X}}}", u32::from(c));
        } else {
            shown.push(c);
        }
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::{MAX_SERVER_TEXT_CHARS, server_text, visible_text};

    #[test]
    fn invisible_characters_are_written_out_and_nothing_is_dropped() {
        assert_eq!(
            visible_text("a\tb\nc\u{202E}d\u{200B}e\u{A0}f\u{E0041}"),
            r"a\u{0009}b\u{000A}c\u{202E}d\u{200B}e\u{00A0}f\u{E0041}"
        );
        let long = "x".repeat(10_000) + "tail";
        assert!(visible_text(&long).ends_with("tail"), "nothing is cut");
        assert_eq!(visible_text(r#"C:\a b\x.exe "é""#), r#"C:\a b\x.exe "é""#);
    }

    #[test]
    fn controls_and_bidi_overrides_are_removed() {
        assert_eq!(server_text("Pass\u{202E}drow\x1b[2J:\u{7}"), "Passdrow[2J:");
    }

    #[test]
    fn long_text_is_capped() {
        assert_eq!(
            server_text(&"x".repeat(1000)).chars().count(),
            MAX_SERVER_TEXT_CHARS
        );
    }

    #[test]
    fn accents_and_other_scripts_are_kept() {
        assert_eq!(
            server_text("Mot de passe (é) пароль"),
            "Mot de passe (é) пароль"
        );
    }
}
