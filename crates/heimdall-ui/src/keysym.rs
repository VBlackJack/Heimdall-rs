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

//! X11 keysyms, as VNC sends keys: by what a key types, laid out by the client's keyboard.
//!
//! Values from X11's `keysymdef.h`. A printable character is its own keysym in Latin-1, and
//! `0x0100_0000` plus its code point beyond.

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Location};

/// Offset of the keysyms that stand for a Unicode code point.
const UNICODE_KEYSYMS: u32 = 0x0100_0000;

/// The keysym of a key: `key` is what it types, `location` which of a left or right pair it
/// is. `None` for a key with no keysym, or text of more than one character.
#[must_use]
pub fn keysym(key: &Key, location: Location) -> Option<u32> {
    match key {
        Key::Named(named) => named_keysym(*named, location),
        Key::Character(text) => {
            let mut characters = text.chars();
            let (Some(character), None) = (characters.next(), characters.next()) else {
                return None;
            };
            Some(character_keysym(character))
        }
        Key::Unidentified => None,
    }
}

/// The keysym of a printable character.
#[must_use]
pub fn character_keysym(character: char) -> u32 {
    let code = u32::from(character);
    match code {
        0x20..=0x7e | 0xa0..=0xff => code,
        _ => UNICODE_KEYSYMS + code,
    }
}

fn named_keysym(named: Named, location: Location) -> Option<u32> {
    let right = location == Location::Right;
    Some(match named {
        Named::Enter => 0xff0d,
        Named::Tab => 0xff09,
        Named::Space => 0x0020,
        Named::Backspace => 0xff08,
        Named::Escape => 0xff1b,
        Named::Delete => 0xffff,
        Named::Insert => 0xff63,
        Named::Home => 0xff50,
        Named::End => 0xff57,
        Named::PageUp => 0xff55,
        Named::PageDown => 0xff56,
        Named::ArrowLeft => 0xff51,
        Named::ArrowUp => 0xff52,
        Named::ArrowRight => 0xff53,
        Named::ArrowDown => 0xff54,
        Named::Shift if right => 0xffe2,
        Named::Shift => 0xffe1,
        Named::Control if right => 0xffe4,
        Named::Control => 0xffe3,
        Named::Alt if right => 0xffea,
        Named::Alt => 0xffe9,
        Named::Super | Named::Meta if right => 0xffec,
        Named::Super | Named::Meta => 0xffeb,
        // ISO_Level3_Shift: what AltGr is to X.
        Named::AltGraph => 0xfe03,
        Named::CapsLock => 0xffe5,
        Named::NumLock => 0xff7f,
        Named::ScrollLock => 0xff14,
        Named::PrintScreen => 0xff61,
        Named::Pause => 0xff13,
        Named::ContextMenu => 0xff67,
        Named::F1 => 0xffbe,
        Named::F2 => 0xffbf,
        Named::F3 => 0xffc0,
        Named::F4 => 0xffc1,
        Named::F5 => 0xffc2,
        Named::F6 => 0xffc3,
        Named::F7 => 0xffc4,
        Named::F8 => 0xffc5,
        Named::F9 => 0xffc6,
        Named::F10 => 0xffc7,
        Named::F11 => 0xffc8,
        Named::F12 => 0xffc9,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn character(text: &str) -> Option<u32> {
        keysym(&Key::Character(text.into()), Location::Standard)
    }

    #[test]
    fn latin_1_characters_are_their_own_keysym_and_others_are_offset() {
        assert_eq!(character("a"), Some(0x61));
        assert_eq!(character("A"), Some(0x41));
        assert_eq!(character("~"), Some(0x7e));
        // AZERTY keys that type Latin-1 letters.
        assert_eq!(character("\u{e9}"), Some(0xe9));
        assert_eq!(character("\u{e7}"), Some(0xe7));
        // The euro sign, beyond Latin-1.
        assert_eq!(character("\u{20ac}"), Some(0x0100_20ac));
        // Two characters are not one key.
        assert_eq!(character("ab"), None);
        assert_eq!(character(""), None);
    }

    #[test]
    fn named_keys_have_their_x11_keysyms() {
        let named = |named, location| keysym(&Key::Named(named), location);
        assert_eq!(named(Named::Enter, Location::Standard), Some(0xff0d));
        assert_eq!(named(Named::Escape, Location::Standard), Some(0xff1b));
        assert_eq!(named(Named::ArrowDown, Location::Standard), Some(0xff54));
        assert_eq!(named(Named::F12, Location::Standard), Some(0xffc9));
        assert_eq!(named(Named::Space, Location::Standard), Some(0x20));
        assert_eq!(named(Named::AltGraph, Location::Standard), Some(0xfe03));
    }

    #[test]
    fn left_and_right_modifiers_differ() {
        let named = |named, location| keysym(&Key::Named(named), location);
        assert_eq!(named(Named::Shift, Location::Left), Some(0xffe1));
        assert_eq!(named(Named::Shift, Location::Right), Some(0xffe2));
        assert_eq!(named(Named::Control, Location::Left), Some(0xffe3));
        assert_eq!(named(Named::Control, Location::Right), Some(0xffe4));
        assert_eq!(named(Named::Alt, Location::Right), Some(0xffea));
        assert_eq!(named(Named::Super, Location::Left), Some(0xffeb));
    }

    #[test]
    fn keys_without_a_keysym_give_none() {
        assert_eq!(keysym(&Key::Unidentified, Location::Standard), None);
        assert_eq!(
            keysym(&Key::Named(Named::MediaPlay), Location::Standard),
            None
        );
    }
}
