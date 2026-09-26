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

//! Keys to bytes, the way xterm sends them (`TERM=xterm-256color`).
//!
//! The text the operating system produced for a key is authoritative for printable
//! characters: a layout's own characters, dead-key compositions and `AltGr` combinations
//! reach the server as typed. On Windows `AltGr` arrives as Ctrl+Alt, so Ctrl+Alt with a
//! printable non-letter is taken as `AltGr` rather than as a control sequence.

use crate::mode::InputMode;

const ESC: u8 = 0x1b;
const DEL: u8 = 0x7f;
const BACKSPACE: u8 = 0x08;
const NUL: u8 = 0x00;
const CSI: &[u8] = b"\x1b[";
const SS3: &[u8] = b"\x1bO";

/// Offset xterm adds to 1 for each modifier in `CSI 1;m X`.
const SHIFT_PARAMETER: u8 = 1;
const ALT_PARAMETER: u8 = 2;
const CTRL_PARAMETER: u8 = 4;

/// Mask turning an ASCII letter into its control code.
const CONTROL_MASK: u8 = 0x1f;

/// Control codes of the non-letter Ctrl combinations, keyed by digit of the physical row.
const CONTROL_DIGITS: [(u8, u8); 7] = [
    (2, NUL),
    (3, ESC),
    (4, 0x1c),
    (5, 0x1d),
    (6, 0x1e),
    (7, 0x1f),
    (8, DEL),
];

/// Number sent for F5 to F12 in `CSI n ~`; the gaps are xterm's.
const FUNCTION_CODES_5_TO_12: [u8; 8] = [15, 17, 18, 19, 20, 21, 23, 24];

/// Final byte of F1 to F4.
const FUNCTION_FINALS_1_TO_4: [u8; 4] = *b"PQRS";

/// Modifier keys held with a key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Shift.
    pub shift: bool,
    /// Control.
    pub ctrl: bool,
    /// Alt (Option).
    pub alt: bool,
}

impl Modifiers {
    fn parameter(self) -> u8 {
        1 + u8::from(self.shift) * SHIFT_PARAMETER
            + u8::from(self.alt) * ALT_PARAMETER
            + u8::from(self.ctrl) * CTRL_PARAMETER
    }
}

/// Keys that are not characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedKey {
    /// Enter or Return.
    Enter,
    /// Tab.
    Tab,
    /// Backspace.
    Backspace,
    /// Escape.
    Escape,
    /// Space bar.
    Space,
    /// Up arrow.
    ArrowUp,
    /// Down arrow.
    ArrowDown,
    /// Left arrow.
    ArrowLeft,
    /// Right arrow.
    ArrowRight,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Insert.
    Insert,
    /// Delete.
    Delete,
    /// Function key F1 to F12.
    Function(u8),
}

/// The key pressed, as the layout names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A named key.
    Named(NamedKey),
    /// A character key: the character the layout gives it without modifiers.
    Character(char),
}

/// Where the key is on the keyboard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KeyLocation {
    /// The main block.
    #[default]
    Standard,
    /// The numeric keypad.
    Numpad,
}

/// One key press, as the UI toolkit reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress<'a> {
    /// The key.
    pub key: Key,
    /// Text the operating system produced, layout, shift, `AltGr` and dead keys applied.
    pub text: Option<&'a str>,
    /// Digit printed on the physical key when it is on the digit row, whatever the layout
    /// types there (`AZERTY` types `&é"'(-è_çà`).
    pub physical_digit: Option<u8>,
    /// Main block or keypad.
    pub location: KeyLocation,
    /// Modifiers held.
    pub modifiers: Modifiers,
}

/// Bytes a key press sends, or `None` when it sends nothing.
#[must_use]
pub fn encode_key(press: &KeyPress<'_>, mode: &InputMode) -> Option<Vec<u8>> {
    if press.location == KeyLocation::Numpad
        && mode.app_keypad
        && let Some(bytes) = keypad(press.key)
    {
        return Some(bytes);
    }
    match press.key {
        Key::Named(named) => named_key(named, press.modifiers, mode),
        Key::Character(character) => character_key(character, press),
    }
}

fn with_alt(alt: bool, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    if alt {
        out.push(ESC);
    }
    out.extend_from_slice(bytes);
    out
}

fn sequence(prefix: &[u8], body: &str) -> Vec<u8> {
    let mut out = prefix.to_vec();
    out.extend_from_slice(body.as_bytes());
    out
}

/// Cursor-style keys: `SS3 X` or `CSI X`, or `CSI 1;m X` with modifiers.
fn cursor_key(final_byte: char, modifiers: Modifiers, app_cursor: bool) -> Vec<u8> {
    let parameter = modifiers.parameter();
    if parameter > 1 {
        sequence(CSI, &format!("1;{parameter}{final_byte}"))
    } else if app_cursor {
        sequence(SS3, &final_byte.to_string())
    } else {
        sequence(CSI, &final_byte.to_string())
    }
}

/// Editing keys: `CSI n ~` or `CSI n;m ~`.
fn tilde_key(code: u8, modifiers: Modifiers) -> Vec<u8> {
    let parameter = modifiers.parameter();
    if parameter > 1 {
        sequence(CSI, &format!("{code};{parameter}~"))
    } else {
        sequence(CSI, &format!("{code}~"))
    }
}

fn named_key(key: NamedKey, modifiers: Modifiers, mode: &InputMode) -> Option<Vec<u8>> {
    let alt = modifiers.alt;
    Some(match key {
        NamedKey::Enter => {
            let enter: &[u8] = if mode.line_feed_new_line {
                b"\r\n"
            } else {
                b"\r"
            };
            with_alt(alt, enter)
        }
        NamedKey::Tab if modifiers.shift => sequence(CSI, "Z"),
        NamedKey::Tab => with_alt(alt, b"\t"),
        NamedKey::Backspace if modifiers.ctrl => with_alt(alt, &[BACKSPACE]),
        NamedKey::Backspace => with_alt(alt, &[DEL]),
        NamedKey::Escape => with_alt(alt, &[ESC]),
        NamedKey::Space if modifiers.ctrl => with_alt(alt, &[NUL]),
        NamedKey::Space => with_alt(alt, b" "),
        NamedKey::ArrowUp => cursor_key('A', modifiers, mode.app_cursor),
        NamedKey::ArrowDown => cursor_key('B', modifiers, mode.app_cursor),
        NamedKey::ArrowRight => cursor_key('C', modifiers, mode.app_cursor),
        NamedKey::ArrowLeft => cursor_key('D', modifiers, mode.app_cursor),
        NamedKey::Home => cursor_key('H', modifiers, mode.app_cursor),
        NamedKey::End => cursor_key('F', modifiers, mode.app_cursor),
        NamedKey::Insert => tilde_key(2, modifiers),
        NamedKey::Delete => tilde_key(3, modifiers),
        NamedKey::PageUp => tilde_key(5, modifiers),
        NamedKey::PageDown => tilde_key(6, modifiers),
        NamedKey::Function(number @ 1..=4) => {
            let final_byte = char::from(FUNCTION_FINALS_1_TO_4[usize::from(number - 1)]);
            let parameter = modifiers.parameter();
            if parameter > 1 {
                sequence(CSI, &format!("1;{parameter}{final_byte}"))
            } else {
                sequence(SS3, &final_byte.to_string())
            }
        }
        NamedKey::Function(number @ 5..=12) => {
            tilde_key(FUNCTION_CODES_5_TO_12[usize::from(number - 5)], modifiers)
        }
        NamedKey::Function(_) => return None,
    })
}

/// Control code of a character under Ctrl, the xterm way.
fn control_code(character: char) -> Option<u8> {
    match character {
        'a'..='z' | 'A'..='Z' => u8::try_from(character).ok().map(|byte| byte & CONTROL_MASK),
        '@' | ' ' => Some(NUL),
        '[' => Some(ESC),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '/' | '_' | '-' => Some(0x1f),
        '?' => Some(DEL),
        _ => None,
    }
}

fn control_digit(digit: u8) -> Option<u8> {
    CONTROL_DIGITS
        .iter()
        .find(|(candidate, _)| *candidate == digit)
        .map(|(_, code)| *code)
}

fn printable(text: Option<&str>) -> Option<&str> {
    text.filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
}

fn character_key(character: char, press: &KeyPress<'_>) -> Option<Vec<u8>> {
    let modifiers = press.modifiers;
    let text = printable(press.text);

    // AltGr arrives as Ctrl+Alt on Windows: a printable non-letter is what the user typed.
    if modifiers.ctrl
        && modifiers.alt
        && let Some(typed) = text
        && !typed.chars().all(|c| c.is_ascii_alphabetic())
    {
        return Some(typed.as_bytes().to_vec());
    }

    if modifiers.ctrl {
        let code = press
            .physical_digit
            .and_then(control_digit)
            .or_else(|| control_code(character));
        if let Some(code) = code {
            return Some(with_alt(modifiers.alt, &[code]));
        }
    }

    let typed = text.map_or_else(|| character.to_string(), str::to_owned);
    if typed.chars().any(char::is_control) {
        return None;
    }
    Some(with_alt(modifiers.alt && !modifiers.ctrl, typed.as_bytes()))
}

/// Keypad keys in application mode (DECKPAM).
fn keypad(key: Key) -> Option<Vec<u8>> {
    let final_byte = match key {
        Key::Named(NamedKey::Enter) => 'M',
        Key::Character(digit @ '0'..='9') => {
            let offset = u8::try_from(digit).ok()? - b'0';
            char::from(b'p' + offset)
        }
        Key::Character('.' | ',') => 'n',
        Key::Character('+') => 'k',
        Key::Character('-') => 'm',
        Key::Character('*') => 'j',
        Key::Character('/') => 'o',
        _ => return None,
    };
    Some(sequence(SS3, &final_byte.to_string()))
}
