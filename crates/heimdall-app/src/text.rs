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
//! The words of a keyboard-interactive question are held to a stricter rule, as the C#
//! `ServerPromptText`: they sit in a Heimdall dialog, so they must not be able to lay
//! themselves out as Heimdall's own text. See [`server_prompt_text`].
//!
//! A report a server wrote (a WHOIS reply, a service banner) keeps its lines: see
//! [`server_report_text`].
//!
//! A command about to be run is shown the other way: nothing removed, nothing cut, every
//! invisible character written out.

use std::fmt::Write as _;
use std::iter::Peekable;
use std::str::Chars;

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

/// Appended to a server prompt cut at [`MAX_SERVER_TEXT_CHARS`].
pub const TRUNCATION_MARKER: &str = "...";

/// `text`, sent by a server in a keyboard-interactive question, made unable to pass for
/// Heimdall's own words: a control character, a line or paragraph separator and any space
/// become one plain space, so words stay apart but no new line or paragraph can start;
/// format characters (bidirectional controls, zero-width characters, the byte order mark,
/// tags) are dropped; runs of spaces become one; the result is trimmed, and cut at
/// [`MAX_SERVER_TEXT_CHARS`] with [`TRUNCATION_MARKER`] after it.
#[must_use]
pub fn server_prompt_text(text: &str) -> String {
    let mut kept = String::with_capacity(text.len().min(MAX_SERVER_TEXT_CHARS));
    let mut pending_space = false;
    for c in text.chars() {
        if c.is_control() || c.is_whitespace() || is_separator(c) {
            pending_space = !kept.is_empty();
        } else if !is_invisible(c) {
            if pending_space {
                kept.push(' ');
                pending_space = false;
            }
            kept.push(c);
        }
    }
    match kept.char_indices().nth(MAX_SERVER_TEXT_CHARS) {
        Some((cut, _)) => {
            let mut capped = kept[..cut].trim_end().to_owned();
            capped.push_str(TRUNCATION_MARKER);
            capped
        }
        None => kept,
    }
}

/// The line and paragraph separators, which start a new line where they stand.
fn is_separator(c: char) -> bool {
    matches!(c, '\u{2028}' | '\u{2029}')
}

/// Whether `c` is one of the [`INVISIBLE`] characters.
pub(crate) fn is_invisible(c: char) -> bool {
    INVISIBLE
        .iter()
        .any(|(first, last)| (*first..=*last).contains(&c))
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
        let hidden = c.is_control() || (c.is_whitespace() && c != ' ') || is_invisible(c);
        if hidden {
            let _ = write!(shown, "\\u{{{:04X}}}", u32::from(c));
        } else {
            shown.push(c);
        }
    }
    shown
}

/// The character that starts a terminal escape sequence.
const ESCAPE: char = '\u{1B}';

/// The 8-bit control sequence introducer, `ESC [` in one character.
const CSI: char = '\u{9B}';

/// The 8-bit introducers of a string sequence: device control, start of string, operating
/// system command, privacy message, application program command.
const STRING_INTRODUCERS: [char; 5] = ['\u{90}', '\u{98}', '\u{9D}', '\u{9E}', '\u{9F}'];

/// What ends a string sequence besides `ESC \`: the bell, and the 8-bit string terminator.
const STRING_TERMINATORS: [char; 2] = ['\u{7}', '\u{9C}'];

/// `text`, a report a server wrote (a WHOIS reply, a service banner), made safe to show and
/// to copy while keeping its lines: a terminal escape sequence is removed whole, not only its
/// escape character (a control sequence to its final byte, an operating system command or
/// another string to its terminator); `\r\n` and a lone `\r` become `\n`; tabs stay; every
/// other control character and every [`INVISIBLE`] one (bidirectional controls, zero-width
/// characters, the byte order mark, tags, the line and paragraph separators) is dropped.
/// Not cut: the caller bounds what it reads.
#[must_use]
pub fn server_report_text(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ESCAPE => match chars.next() {
                Some('[') => skip_control_sequence(&mut chars),
                Some(']' | 'P' | 'X' | '^' | '_') => skip_string(&mut chars),
                // Any other: a sequence of two characters, both dropped.
                _ => {}
            },
            CSI => skip_control_sequence(&mut chars),
            c if STRING_INTRODUCERS.contains(&c) => skip_string(&mut chars),
            '\r' => {
                let _ = chars.next_if_eq(&'\n');
                kept.push('\n');
            }
            '\n' | '\t' => kept.push(c),
            c if c.is_control() || is_invisible(c) => {}
            c => kept.push(c),
        }
    }
    kept
}

/// Skips the rest of a control sequence: its parameter and intermediate bytes, then its
/// final byte.
fn skip_control_sequence(chars: &mut Peekable<Chars<'_>>) {
    while chars
        .next_if(|c| ('\u{20}'..='\u{3F}').contains(c))
        .is_some()
    {}
    let _ = chars.next_if(|c| ('\u{40}'..='\u{7E}').contains(c));
}

/// Skips the rest of a string sequence: to its terminator, or to the end.
fn skip_string(chars: &mut Peekable<Chars<'_>>) {
    while let Some(c) = chars.next() {
        if STRING_TERMINATORS.contains(&c) {
            return;
        }
        if c == ESCAPE {
            let _ = chars.next_if_eq(&'\\');
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_SERVER_TEXT_CHARS, TRUNCATION_MARKER, server_prompt_text, server_report_text,
        server_text, visible_text,
    };

    #[test]
    fn escape_sequences_in_a_server_report_are_removed_whole() {
        assert_eq!(
            server_report_text("a\u{1b}[1;31mred\u{1b}[0m b\u{9b}2Jc"),
            "ared bc"
        );
        assert_eq!(
            server_report_text(
                "\u{1b}]0;title\u{7}x\u{1b}]8;;http://e\u{1b}\\link\u{1b}]8;;\u{1b}\\"
            ),
            "xlink"
        );
        assert_eq!(server_report_text("\u{1b}Pq#0;2\u{1b}\\ok\u{1b}c"), "ok");
        assert_eq!(server_report_text("cut\u{1b}]0;never ended"), "cut");
    }

    #[test]
    fn a_server_report_keeps_its_lines_and_drops_the_invisible() {
        assert_eq!(server_report_text("l1\r\nl2\rl3\n\tx"), "l1\nl2\nl3\n\tx");
        assert_eq!(
            server_report_text("\u{202E}\u{200B}\u{FEFF}a\u{7}\u{0}b\u{2028}"),
            "ab"
        );
        assert_eq!(
            server_report_text("Registrant: Société пароль"),
            "Registrant: Société пароль"
        );
    }

    #[test]
    fn a_server_prompt_cannot_start_a_line_of_its_own() {
        assert_eq!(
            server_prompt_text(
                "Code:\n\nHeimdall: enter the vault master password\u{2028}x\r\ty\u{85}z"
            ),
            "Code: Heimdall: enter the vault master password x y z"
        );
        assert_eq!(
            server_prompt_text("\u{1b}[2J  Verification   code:\u{7}  "),
            "[2J Verification code:"
        );
    }

    #[test]
    fn format_characters_in_a_server_prompt_are_dropped() {
        assert_eq!(
            server_prompt_text("\u{FEFF}Pass\u{202E}drow\u{200B}:\u{E0041}"),
            "Passdrow:"
        );
    }

    #[test]
    fn a_long_server_prompt_is_cut_and_says_so() {
        let cut = server_prompt_text(&"x".repeat(1000));
        assert_eq!(cut, "x".repeat(MAX_SERVER_TEXT_CHARS) + TRUNCATION_MARKER);
        let at_limit = "é".repeat(MAX_SERVER_TEXT_CHARS);
        assert_eq!(
            server_prompt_text(&at_limit),
            at_limit,
            "not cut at the limit"
        );
        let space_at_cut = "x".repeat(MAX_SERVER_TEXT_CHARS - 1) + " yyyy";
        assert_eq!(
            server_prompt_text(&space_at_cut),
            "x".repeat(MAX_SERVER_TEXT_CHARS - 1) + TRUNCATION_MARKER,
            "no space before the marker"
        );
    }

    #[test]
    fn a_server_prompt_keeps_accents_and_other_scripts() {
        assert_eq!(
            server_prompt_text("Mot de passe (é) пароль \u{1F511}:"),
            "Mot de passe (é) пароль \u{1F511}:"
        );
    }

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
