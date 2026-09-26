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

#[cfg(test)]
mod tests {
    use super::{MAX_SERVER_TEXT_CHARS, server_text};

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
