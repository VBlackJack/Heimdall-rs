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

//! iced keyboard events to terminal input, and the shortcuts the application keeps.
//!
//! iced 0.14 reports `key` without modifiers, `text` with every modifier applied (so
//! `AltGr` and dead-key compositions are in `text`), and the physical key separately.

use heimdall_app::KeyInput;
use heimdall_term::{Key, KeyLocation, Modifiers, NamedKey};
use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Location};

/// A shortcut handled by the application instead of the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    /// Copy the selection: Ctrl+Shift+C, Ctrl+Insert.
    Copy,
    /// Paste: Ctrl+Shift+V, Shift+Insert.
    Paste,
    /// Scroll the history one page up: Shift+Page Up.
    PageUp,
    /// Scroll the history one page down: Shift+Page Down.
    PageDown,
}

/// The shortcut `key` with `modifiers` stands for, if any. Key repeat is the caller's
/// concern: copy and paste should not repeat, scrolling may.
#[must_use]
pub fn shortcut(key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Option<Shortcut> {
    let (ctrl, shift, alt) = (modifiers.control(), modifiers.shift(), modifiers.alt());
    match key {
        keyboard::Key::Character(c) if ctrl && shift && !alt => {
            match c.as_str().to_ascii_lowercase().as_str() {
                "c" => Some(Shortcut::Copy),
                "v" => Some(Shortcut::Paste),
                _ => None,
            }
        }
        keyboard::Key::Named(Named::Insert) if ctrl && !shift && !alt => Some(Shortcut::Copy),
        keyboard::Key::Named(Named::Insert) if shift && !ctrl && !alt => Some(Shortcut::Paste),
        keyboard::Key::Named(Named::PageUp) if shift && !ctrl && !alt => Some(Shortcut::PageUp),
        keyboard::Key::Named(Named::PageDown) if shift && !ctrl && !alt => Some(Shortcut::PageDown),
        _ => None,
    }
}

fn named(key: Named) -> Option<NamedKey> {
    Some(match key {
        Named::Enter => NamedKey::Enter,
        Named::Tab => NamedKey::Tab,
        Named::Backspace => NamedKey::Backspace,
        Named::Escape => NamedKey::Escape,
        Named::Space => NamedKey::Space,
        Named::ArrowUp => NamedKey::ArrowUp,
        Named::ArrowDown => NamedKey::ArrowDown,
        Named::ArrowLeft => NamedKey::ArrowLeft,
        Named::ArrowRight => NamedKey::ArrowRight,
        Named::Home => NamedKey::Home,
        Named::End => NamedKey::End,
        Named::PageUp => NamedKey::PageUp,
        Named::PageDown => NamedKey::PageDown,
        Named::Insert => NamedKey::Insert,
        Named::Delete => NamedKey::Delete,
        Named::F1 => NamedKey::Function(1),
        Named::F2 => NamedKey::Function(2),
        Named::F3 => NamedKey::Function(3),
        Named::F4 => NamedKey::Function(4),
        Named::F5 => NamedKey::Function(5),
        Named::F6 => NamedKey::Function(6),
        Named::F7 => NamedKey::Function(7),
        Named::F8 => NamedKey::Function(8),
        Named::F9 => NamedKey::Function(9),
        Named::F10 => NamedKey::Function(10),
        Named::F11 => NamedKey::Function(11),
        Named::F12 => NamedKey::Function(12),
        _ => return None,
    })
}

fn digit(physical: Physical) -> Option<u8> {
    let Physical::Code(code) = physical else {
        return None;
    };
    Some(match code {
        Code::Digit0 => 0,
        Code::Digit1 => 1,
        Code::Digit2 => 2,
        Code::Digit3 => 3,
        Code::Digit4 => 4,
        Code::Digit5 => 5,
        Code::Digit6 => 6,
        Code::Digit7 => 7,
        Code::Digit8 => 8,
        Code::Digit9 => 9,
        _ => return None,
    })
}

/// Terminal input for a key press; `None` for keys that type nothing on their own (a
/// modifier alone, a dead key waiting for the next one).
#[must_use]
pub fn key_input(
    key: &keyboard::Key,
    physical: Physical,
    location: Location,
    modifiers: keyboard::Modifiers,
    text: Option<&str>,
) -> Option<KeyInput> {
    let key = match key {
        keyboard::Key::Named(name) => Key::Named(named(*name)?),
        keyboard::Key::Character(c) => Key::Character(c.chars().next()?),
        keyboard::Key::Unidentified => {
            // Dead keys arrive here without text; a composed character arrives with it.
            let character = text?.chars().next()?;
            Key::Character(character)
        }
    };
    Some(KeyInput {
        key,
        text: text.map(str::to_owned),
        physical_digit: digit(physical),
        location: if location == Location::Numpad {
            KeyLocation::Numpad
        } else {
            KeyLocation::Standard
        },
        modifiers: Modifiers {
            shift: modifiers.shift(),
            ctrl: modifiers.control(),
            alt: modifiers.alt(),
        },
    })
}

/// Terminal input for text committed by an input method (a composed or converted string).
#[must_use]
pub fn committed_text(text: &str) -> Option<KeyInput> {
    Some(KeyInput {
        key: Key::Character(text.chars().next()?),
        text: Some(text.to_owned()),
        physical_digit: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::default(),
    })
}

#[cfg(test)]
mod tests {
    use heimdall_term::{Key, KeyLocation, NamedKey};
    use iced::keyboard::key::{Code, Named, Physical};
    use iced::keyboard::{self, Location, Modifiers};

    use super::{Shortcut, key_input, shortcut};

    fn character(c: &str) -> keyboard::Key {
        keyboard::Key::Character(c.into())
    }

    #[test]
    fn azerty_altgr_keeps_the_typed_text_and_the_physical_digit() {
        // AltGr+2 on AZERTY: key 'é' without modifiers, text '~', Ctrl+Alt held.
        let input = key_input(
            &character("é"),
            Physical::Code(Code::Digit2),
            Location::Standard,
            Modifiers::CTRL | Modifiers::ALT,
            Some("~"),
        )
        .expect("types something");
        assert_eq!(input.key, Key::Character('é'));
        assert_eq!(input.text.as_deref(), Some("~"));
        assert_eq!(input.physical_digit, Some(2));
        assert!(input.modifiers.ctrl && input.modifiers.alt);
    }

    #[test]
    fn keypad_keys_carry_their_location() {
        let input = key_input(
            &character("5"),
            Physical::Code(Code::Numpad5),
            Location::Numpad,
            Modifiers::empty(),
            Some("5"),
        )
        .expect("types");
        assert_eq!(input.location, KeyLocation::Numpad);
        assert_eq!(input.physical_digit, None);
    }

    #[test]
    fn named_keys_map_and_lone_modifiers_type_nothing() {
        let f5 = key_input(
            &keyboard::Key::Named(Named::F5),
            Physical::Code(Code::F5),
            Location::Standard,
            Modifiers::empty(),
            None,
        )
        .expect("types");
        assert_eq!(f5.key, Key::Named(NamedKey::Function(5)));
        assert!(
            key_input(
                &keyboard::Key::Named(Named::Shift),
                Physical::Code(Code::ShiftLeft),
                Location::Left,
                Modifiers::SHIFT,
                None,
            )
            .is_none()
        );
    }

    #[test]
    fn a_dead_key_types_nothing_and_its_composition_types_the_result() {
        let dead = key_input(
            &keyboard::Key::Unidentified,
            Physical::Code(Code::BracketLeft),
            Location::Standard,
            Modifiers::empty(),
            None,
        );
        assert!(dead.is_none());
        let composed = key_input(
            &keyboard::Key::Unidentified,
            Physical::Code(Code::KeyE),
            Location::Standard,
            Modifiers::empty(),
            Some("ê"),
        )
        .expect("types");
        assert_eq!(composed.text.as_deref(), Some("ê"));
    }

    #[test]
    fn application_shortcuts() {
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(shortcut(&character("C"), ctrl_shift), Some(Shortcut::Copy));
        assert_eq!(shortcut(&character("v"), ctrl_shift), Some(Shortcut::Paste));
        assert_eq!(
            shortcut(&character("c"), Modifiers::CTRL),
            None,
            "Ctrl+C is SIGINT"
        );
        assert_eq!(
            shortcut(&keyboard::Key::Named(Named::Insert), Modifiers::SHIFT),
            Some(Shortcut::Paste)
        );
        assert_eq!(
            shortcut(&keyboard::Key::Named(Named::PageUp), Modifiers::SHIFT),
            Some(Shortcut::PageUp)
        );
        assert_eq!(
            shortcut(&keyboard::Key::Named(Named::PageUp), Modifiers::empty()),
            None
        );
    }
}
