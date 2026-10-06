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

/// The Latin letter a shortcut reads from a key: the character when it is one, else the
/// letter printed at that place on a US keyboard, so Ctrl+Shift+C still copies on a
/// Cyrillic or Greek layout.
#[must_use]
pub fn letter(key: &keyboard::Key, physical: Physical) -> Option<char> {
    if let keyboard::Key::Character(c) = key
        && let Some(first) = c.chars().next()
        && first.is_ascii_alphabetic()
    {
        return Some(first.to_ascii_lowercase());
    }
    match physical {
        Physical::Code(Code::KeyA) => Some('a'),
        Physical::Code(Code::KeyC) => Some('c'),
        Physical::Code(Code::KeyD) => Some('d'),
        Physical::Code(Code::KeyF) => Some('f'),
        Physical::Code(Code::KeyL) => Some('l'),
        Physical::Code(Code::KeyS) => Some('s'),
        Physical::Code(Code::KeyU) => Some('u'),
        Physical::Code(Code::KeyV) => Some('v'),
        Physical::Code(Code::KeyW) => Some('w'),
        Physical::Code(Code::KeyX) => Some('x'),
        _ => None,
    }
}

/// The shortcut `key` with `modifiers` stands for, if any. Key repeat is the caller's
/// concern: copy and paste should not repeat, scrolling may.
#[must_use]
pub fn shortcut(
    key: &keyboard::Key,
    physical: Physical,
    modifiers: keyboard::Modifiers,
) -> Option<Shortcut> {
    let (ctrl, shift, alt) = (modifiers.control(), modifiers.shift(), modifiers.alt());
    match key {
        keyboard::Key::Character(_) if ctrl && shift && !alt => match letter(key, physical) {
            Some('c') => Some(Shortcut::Copy),
            Some('v') => Some(Shortcut::Paste),
            _ => None,
        },
        keyboard::Key::Named(Named::Insert) if ctrl && !shift && !alt => Some(Shortcut::Copy),
        keyboard::Key::Named(Named::Insert) if shift && !ctrl && !alt => Some(Shortcut::Paste),
        keyboard::Key::Named(Named::PageUp) if shift && !ctrl && !alt => Some(Shortcut::PageUp),
        keyboard::Key::Named(Named::PageDown) if shift && !ctrl && !alt => Some(Shortcut::PageDown),
        _ => None,
    }
}

/// Whether `key` is Ctrl+V alone, whatever the keyboard's layout: the paste `choice` may
/// take it from the session.
#[must_use]
pub fn is_ctrl_v(key: &keyboard::Key, physical: Physical, modifiers: keyboard::Modifiers) -> bool {
    modifiers.control()
        && !modifiers.shift()
        && !modifiers.alt()
        && !modifiers.logo()
        && letter(key, physical) == Some('v')
}

/// Whether `key` is Ctrl+W alone, whatever the keyboard's layout: in a terminal it is
/// readline's word erase; left by every widget, it closes the session shown, as the C#.
#[must_use]
pub fn is_ctrl_w(key: &keyboard::Key, physical: Physical, modifiers: keyboard::Modifiers) -> bool {
    modifiers.control()
        && !modifiers.shift()
        && !modifiers.alt()
        && !modifiers.logo()
        && letter(key, physical) == Some('w')
}

/// A shortcut of the window, left uncaptured by the terminal so the window sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowShortcut {
    /// Show the next tab: Ctrl+Tab, Ctrl+Page Down.
    NextTab,
    /// Show the previous tab: Ctrl+Shift+Tab, Ctrl+Page Up.
    PreviousTab,
    /// Close the tab shown: Ctrl+Shift+W.
    CloseTab,
    /// Place the panes of the split tab shown the other way: Ctrl+Shift+O, as the C# one.
    ToggleSplit,
    /// Give the keyboard to the next pane of the split tab shown: Ctrl+Alt+Right, or
    /// Ctrl+F6 where a graphics driver takes Ctrl+Alt and an arrow to turn the screen.
    NextPane,
    /// Give the keyboard to the previous pane of the split tab shown: Ctrl+Alt+Left, or
    /// Ctrl+Shift+F6.
    PreviousPane,
    /// The terminal's text larger, smaller or back to its size: Ctrl +, Ctrl -, Ctrl 0.
    Zoom(Zoom),
    /// Open or close the terminal's search bar: Ctrl+Shift+F.
    Find,
    /// Turn broadcast input on or off: Ctrl+Alt+B, as the C# one.
    Broadcast,
    /// Show the settings: Ctrl+comma, as the C# one.
    Settings,
    /// Copy an image of the session shown to the clipboard: Ctrl+Shift+S, as the C# one.
    Screenshot,
    /// Show the keyboard shortcuts: F1, as the C# one, when no session has the keyboard; a
    /// terminal keeps F1 for its programs. Never a key a terminal leaves to the window.
    Help,
}

/// A change of the terminal's text size, as the C# Heimdall's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zoom {
    /// One point larger.
    In,
    /// One point smaller.
    Out,
    /// Back to the size it had.
    Reset,
}

/// The zoom `key` stands for with Ctrl: + or = larger, - smaller, 0 back, from the main
/// keys or the keypad and whatever the layout (0 is Shift+0 on AZERTY).
fn zoom(key: &keyboard::Key, physical: Physical, shift: bool) -> Option<Zoom> {
    if let keyboard::Key::Character(c) = key {
        match c.as_str() {
            "+" | "=" => return Some(Zoom::In),
            "-" if !shift => return Some(Zoom::Out),
            "0" => return Some(Zoom::Reset),
            _ => {}
        }
    }
    match physical {
        Physical::Code(Code::Digit0 | Code::Numpad0) => Some(Zoom::Reset),
        _ => None,
    }
}

/// Whether `key` with `modifiers` is Ctrl+L, which locks the workspace as in the C#
/// Heimdall. Not a window shortcut: the session still gets it (a shell clears its screen),
/// and without a master password it only does that.
#[must_use]
pub fn is_lock_key(
    key: &keyboard::Key,
    physical: Physical,
    modifiers: keyboard::Modifiers,
) -> bool {
    modifiers.control()
        && !modifiers.shift()
        && !modifiers.alt()
        && letter(key, physical) == Some('l')
}

/// Whether `key` with `modifiers` is Ctrl+F, which goes to the tree's search as in the C#
/// Heimdall. Not a window shortcut: a terminal keeps it (a shell's forward-char), and it
/// reaches the window only when no widget took it.
#[must_use]
pub fn is_search_key(
    key: &keyboard::Key,
    physical: Physical,
    modifiers: keyboard::Modifiers,
) -> bool {
    modifiers.control()
        && !modifiers.shift()
        && !modifiers.alt()
        && letter(key, physical) == Some('f')
}

/// The letter of Ctrl+letter, `key` with `modifiers`, whatever the keyboard's layout: Ctrl
/// alone held, without Shift or Alt.
#[must_use]
pub fn ctrl_letter(
    key: &keyboard::Key,
    physical: Physical,
    modifiers: keyboard::Modifiers,
) -> Option<char> {
    if modifiers.control() && !modifiers.shift() && !modifiers.alt() {
        letter(key, physical)
    } else {
        None
    }
}

/// The window shortcut `key` with `modifiers` stands for, if any.
#[must_use]
pub fn window_shortcut(
    key: &keyboard::Key,
    physical: Physical,
    modifiers: keyboard::Modifiers,
) -> Option<WindowShortcut> {
    let (ctrl, shift, alt) = (modifiers.control(), modifiers.shift(), modifiers.alt());
    if ctrl && alt && !shift {
        return match key {
            keyboard::Key::Named(Named::ArrowRight) => Some(WindowShortcut::NextPane),
            keyboard::Key::Named(Named::ArrowLeft) => Some(WindowShortcut::PreviousPane),
            // The character itself, never the key's place: AltGr is Ctrl+Alt, and AltGr+B
            // types a character on some layouts.
            keyboard::Key::Character(c) if c.eq_ignore_ascii_case("b") => {
                Some(WindowShortcut::Broadcast)
            }
            _ => None,
        };
    }
    if !ctrl || alt {
        return None;
    }
    match key {
        keyboard::Key::Named(Named::F6) if shift => Some(WindowShortcut::PreviousPane),
        keyboard::Key::Named(Named::F6) => Some(WindowShortcut::NextPane),
        keyboard::Key::Named(Named::Tab) if shift => Some(WindowShortcut::PreviousTab),
        keyboard::Key::Named(Named::Tab) => Some(WindowShortcut::NextTab),
        keyboard::Key::Named(Named::PageUp) if !shift => Some(WindowShortcut::PreviousTab),
        keyboard::Key::Named(Named::PageDown) if !shift => Some(WindowShortcut::NextTab),
        keyboard::Key::Character(_) if shift && letter(key, physical) == Some('w') => {
            Some(WindowShortcut::CloseTab)
        }
        keyboard::Key::Character(_) if shift && letter(key, physical) == Some('f') => {
            Some(WindowShortcut::Find)
        }
        keyboard::Key::Character(_) if shift && letter(key, physical) == Some('s') => {
            Some(WindowShortcut::Screenshot)
        }
        keyboard::Key::Character(_) if shift && letter(key, physical) == Some('o') => {
            Some(WindowShortcut::ToggleSplit)
        }
        // The character, wherever the layout puts it: the C# reads the comma key.
        keyboard::Key::Character(c) if !shift && c.as_str() == "," => {
            Some(WindowShortcut::Settings)
        }
        _ => zoom(key, physical, shift).map(WindowShortcut::Zoom),
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
    let key = if let Some(typed) = keypad_text(location, text) {
        // The key is named as if Num Lock were off (1 is End): what it typed is the text.
        Key::Character(typed)
    } else {
        match key {
            keyboard::Key::Named(name) => Key::Named(named(*name)?),
            keyboard::Key::Character(c) => Key::Character(c.chars().next()?),
            keyboard::Key::Unidentified => {
                // Dead keys arrive here without text; a composed character arrives with it.
                let character = text?.chars().next()?;
                Key::Character(character)
            }
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

/// The printable character a keypad key typed: a digit with Num Lock on, or an operator.
/// `None` off the keypad, and for keys that type no character (Num Lock off, Enter).
///
/// The toolkit names a key by `key_without_modifiers`, which on Windows reads the keypad as if
/// Num Lock were off: the 1 of the keypad arrives as End, with "1" as its text.
fn keypad_text(location: Location, text: Option<&str>) -> Option<char> {
    if location != Location::Numpad {
        return None;
    }
    let typed = text?.chars().next()?;
    (!typed.is_control()).then_some(typed)
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

    /// A physical key no shortcut reads.
    const ANY_PLACE: Physical = Physical::Code(Code::F24);

    use super::{
        Shortcut, WindowShortcut, Zoom, is_ctrl_w, is_lock_key, is_search_key, key_input, shortcut,
        window_shortcut,
    };

    #[test]
    fn ctrl_w_alone_is_known_by_its_key_whatever_the_layout() {
        let w = Physical::Code(Code::KeyW);
        assert!(is_ctrl_w(&character("w"), w, Modifiers::CTRL));
        assert!(
            is_ctrl_w(&character("\u{0446}"), w, Modifiers::CTRL),
            "the W key of a Cyrillic keyboard"
        );
        assert!(!is_ctrl_w(
            &character("w"),
            w,
            Modifiers::CTRL | Modifiers::SHIFT
        ));
        assert!(!is_ctrl_w(&character("w"), w, Modifiers::empty()));
    }

    #[test]
    fn ctrl_comma_shows_the_settings() {
        assert_eq!(
            window_shortcut(&character(","), ANY_PLACE, Modifiers::CTRL),
            Some(WindowShortcut::Settings)
        );
        // On AZERTY the comma is where M is on QWERTY: the character decides.
        assert_eq!(
            window_shortcut(&character(","), Physical::Code(Code::KeyM), Modifiers::CTRL),
            Some(WindowShortcut::Settings)
        );
        for modifiers in [
            Modifiers::empty(),
            Modifiers::CTRL | Modifiers::SHIFT,
            Modifiers::CTRL | Modifiers::ALT,
        ] {
            assert_eq!(
                window_shortcut(&character(","), ANY_PLACE, modifiers),
                None,
                "{modifiers:?}"
            );
        }
    }

    #[test]
    fn ctrl_alt_arrows_and_ctrl_f6_move_between_the_panes() {
        let (right, left, f6) = (
            keyboard::Key::Named(Named::ArrowRight),
            keyboard::Key::Named(Named::ArrowLeft),
            keyboard::Key::Named(Named::F6),
        );
        let ctrl_alt = Modifiers::CTRL | Modifiers::ALT;
        assert_eq!(
            window_shortcut(&right, ANY_PLACE, ctrl_alt),
            Some(WindowShortcut::NextPane)
        );
        assert_eq!(
            window_shortcut(&left, ANY_PLACE, ctrl_alt),
            Some(WindowShortcut::PreviousPane)
        );
        assert_eq!(
            window_shortcut(&f6, ANY_PLACE, Modifiers::CTRL),
            Some(WindowShortcut::NextPane)
        );
        assert_eq!(
            window_shortcut(&f6, ANY_PLACE, Modifiers::CTRL | Modifiers::SHIFT),
            Some(WindowShortcut::PreviousPane)
        );
        // The shell's own: word moves with Ctrl, F6 alone for its programs.
        assert_eq!(window_shortcut(&right, ANY_PLACE, Modifiers::CTRL), None);
        assert_eq!(
            window_shortcut(&left, ANY_PLACE, ctrl_alt | Modifiers::SHIFT),
            None
        );
        assert_eq!(window_shortcut(&f6, ANY_PLACE, Modifiers::empty()), None);
        assert_eq!(window_shortcut(&f6, ANY_PLACE, ctrl_alt), None);
    }

    #[test]
    fn ctrl_alt_b_toggles_broadcast_but_never_takes_a_character_typed_with_altgr() {
        let ctrl_alt = Modifiers::CTRL | Modifiers::ALT;
        assert_eq!(
            window_shortcut(&character("b"), ANY_PLACE, ctrl_alt),
            Some(WindowShortcut::Broadcast)
        );
        assert_eq!(
            window_shortcut(&character("B"), ANY_PLACE, ctrl_alt),
            Some(WindowShortcut::Broadcast)
        );
        assert_eq!(
            window_shortcut(&character("{"), Physical::Code(Code::KeyB), ctrl_alt),
            None,
            "AltGr+B typing a brace keeps typing it"
        );
        assert_eq!(
            window_shortcut(&character("b"), ANY_PLACE, ctrl_alt | Modifiers::SHIFT),
            None
        );
        assert_eq!(
            window_shortcut(&character("b"), ANY_PLACE, Modifiers::CTRL),
            None,
            "Ctrl alone"
        );
        assert_eq!(window_shortcut(&character("c"), ANY_PLACE, ctrl_alt), None);
    }

    #[test]
    fn ctrl_shift_s_copies_the_session_as_an_image_on_any_layout() {
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(
            window_shortcut(&character("S"), ANY_PLACE, ctrl_shift),
            Some(WindowShortcut::Screenshot)
        );
        assert_eq!(
            window_shortcut(
                &character("\u{044b}"),
                Physical::Code(Code::KeyS),
                ctrl_shift
            ),
            Some(WindowShortcut::Screenshot),
            "the S key of a Cyrillic keyboard"
        );
        assert_eq!(
            window_shortcut(&character("s"), ANY_PLACE, Modifiers::CTRL),
            None,
            "Ctrl+S is the session's, or the editor's save"
        );
    }

    #[test]
    fn ctrl_shift_f_opens_the_terminal_search_on_any_layout() {
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(
            window_shortcut(&character("F"), ANY_PLACE, ctrl_shift),
            Some(WindowShortcut::Find)
        );
        assert_eq!(
            window_shortcut(
                &character("\u{0430}"),
                Physical::Code(Code::KeyF),
                ctrl_shift
            ),
            Some(WindowShortcut::Find),
            "the F key of a Cyrillic keyboard"
        );
        assert_eq!(
            window_shortcut(&character("f"), ANY_PLACE, Modifiers::CTRL),
            None,
            "Ctrl+F is the tree's search"
        );
        assert_eq!(
            window_shortcut(&character("f"), ANY_PLACE, ctrl_shift | Modifiers::ALT),
            None
        );
    }

    #[test]
    fn ctrl_plus_minus_and_zero_zoom_on_any_layout_and_only_with_ctrl() {
        let zoom = |key: &str, physical: Physical, modifiers: Modifiers| match window_shortcut(
            &character(key),
            physical,
            modifiers,
        ) {
            Some(WindowShortcut::Zoom(zoom)) => Some(zoom),
            _ => None,
        };
        assert_eq!(zoom("+", ANY_PLACE, Modifiers::CTRL), Some(Zoom::In));
        assert_eq!(
            zoom("+", ANY_PLACE, Modifiers::CTRL | Modifiers::SHIFT),
            Some(Zoom::In),
            "Shift+= on a US keyboard"
        );
        assert_eq!(zoom("=", ANY_PLACE, Modifiers::CTRL), Some(Zoom::In));
        assert_eq!(zoom("-", ANY_PLACE, Modifiers::CTRL), Some(Zoom::Out));
        assert_eq!(
            zoom("-", ANY_PLACE, Modifiers::CTRL | Modifiers::SHIFT),
            None,
            "Ctrl+Shift+- is not a zoom"
        );
        assert_eq!(zoom("0", ANY_PLACE, Modifiers::CTRL), Some(Zoom::Reset));
        assert_eq!(
            zoom("à", Physical::Code(Code::Digit0), Modifiers::CTRL),
            Some(Zoom::Reset),
            "the 0 key of an AZERTY keyboard"
        );
        assert_eq!(
            zoom("x", Physical::Code(Code::Numpad0), Modifiers::CTRL),
            Some(Zoom::Reset)
        );
        assert_eq!(zoom("+", ANY_PLACE, Modifiers::empty()), None, "typed");
        assert_eq!(zoom("-", ANY_PLACE, Modifiers::CTRL | Modifiers::ALT), None);
        assert_eq!(zoom("9", ANY_PLACE, Modifiers::CTRL), None);
    }

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
    fn a_keypad_digit_named_as_its_num_lock_off_key_types_the_digit() {
        // Windows, Num Lock on: the key is End, the text is the digit.
        let input = key_input(
            &keyboard::Key::Named(Named::End),
            Physical::Code(Code::Numpad1),
            Location::Numpad,
            Modifiers::empty(),
            Some("1"),
        )
        .expect("types");
        assert_eq!(input.key, Key::Character('1'));
        assert_eq!(input.location, KeyLocation::Numpad);
    }

    #[test]
    fn a_keypad_key_that_types_no_character_keeps_its_name() {
        // Num Lock off: End, no text.
        let end = key_input(
            &keyboard::Key::Named(Named::End),
            Physical::Code(Code::Numpad1),
            Location::Numpad,
            Modifiers::empty(),
            None,
        )
        .expect("a key");
        assert_eq!(end.key, Key::Named(NamedKey::End));
        // Keypad Enter types a carriage return, a control character.
        let enter = key_input(
            &keyboard::Key::Named(Named::Enter),
            Physical::Code(Code::NumpadEnter),
            Location::Numpad,
            Modifiers::empty(),
            Some("\r"),
        )
        .expect("a key");
        assert_eq!(enter.key, Key::Named(NamedKey::Enter));
    }

    #[test]
    fn off_the_keypad_the_key_name_wins_over_its_text() {
        let input = key_input(
            &keyboard::Key::Named(Named::End),
            Physical::Code(Code::End),
            Location::Standard,
            Modifiers::empty(),
            Some("1"),
        )
        .expect("a key");
        assert_eq!(input.key, Key::Named(NamedKey::End));
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
        assert_eq!(
            shortcut(&character("C"), ANY_PLACE, ctrl_shift),
            Some(Shortcut::Copy)
        );
        assert_eq!(
            shortcut(&character("v"), ANY_PLACE, ctrl_shift),
            Some(Shortcut::Paste)
        );
        assert_eq!(
            shortcut(&character("c"), ANY_PLACE, Modifiers::CTRL),
            None,
            "Ctrl+C is SIGINT"
        );
        assert_eq!(
            shortcut(
                &keyboard::Key::Named(Named::Insert),
                ANY_PLACE,
                Modifiers::SHIFT
            ),
            Some(Shortcut::Paste)
        );
        assert_eq!(
            shortcut(
                &keyboard::Key::Named(Named::PageUp),
                ANY_PLACE,
                Modifiers::SHIFT
            ),
            Some(Shortcut::PageUp)
        );
        assert_eq!(
            shortcut(
                &keyboard::Key::Named(Named::PageUp),
                ANY_PLACE,
                Modifiers::empty()
            ),
            None
        );
    }

    #[test]
    fn window_shortcuts_need_ctrl_and_leave_plain_keys_to_the_terminal() {
        let tab = keyboard::Key::Named(Named::Tab);
        assert_eq!(
            window_shortcut(&tab, ANY_PLACE, Modifiers::CTRL),
            Some(WindowShortcut::NextTab)
        );
        assert_eq!(
            window_shortcut(&tab, ANY_PLACE, Modifiers::CTRL | Modifiers::SHIFT),
            Some(WindowShortcut::PreviousTab)
        );
        assert_eq!(
            window_shortcut(&tab, ANY_PLACE, Modifiers::empty()),
            None,
            "Tab completes"
        );
        assert_eq!(
            window_shortcut(
                &keyboard::Key::Named(Named::PageUp),
                ANY_PLACE,
                Modifiers::CTRL
            ),
            Some(WindowShortcut::PreviousTab)
        );
        assert_eq!(
            window_shortcut(
                &character("W"),
                ANY_PLACE,
                Modifiers::CTRL | Modifiers::SHIFT
            ),
            Some(WindowShortcut::CloseTab)
        );
        assert_eq!(
            window_shortcut(&character("w"), ANY_PLACE, Modifiers::CTRL),
            None,
            "Ctrl+W deletes a word in the shell"
        );
        assert_eq!(
            window_shortcut(&tab, ANY_PLACE, Modifiers::CTRL | Modifiers::ALT),
            None,
            "AltGr is Ctrl+Alt on Windows"
        );
    }

    #[test]
    fn ctrl_f_is_the_search_key_and_the_terminal_keeps_it() {
        assert!(is_search_key(&character("f"), ANY_PLACE, Modifiers::CTRL));
        assert!(!is_search_key(&character("g"), ANY_PLACE, Modifiers::CTRL));
        assert!(is_search_key(
            &keyboard::Key::Character("\u{0430}".into()),
            Physical::Code(Code::KeyF),
            Modifiers::CTRL
        ));
        assert!(!is_search_key(
            &character("f"),
            ANY_PLACE,
            Modifiers::empty()
        ));
        assert!(!is_search_key(
            &character("F"),
            ANY_PLACE,
            Modifiers::CTRL | Modifiers::SHIFT
        ));
        assert!(!is_search_key(
            &character("f"),
            ANY_PLACE,
            Modifiers::CTRL | Modifiers::ALT
        ));
        assert_eq!(
            window_shortcut(&character("f"), ANY_PLACE, Modifiers::CTRL),
            None,
            "not a window shortcut: the terminal keeps Ctrl+F"
        );
    }

    #[test]
    fn ctrl_l_is_the_lock_key_on_any_layout_and_only_alone() {
        assert!(is_lock_key(&character("l"), ANY_PLACE, Modifiers::CTRL));
        assert!(is_lock_key(
            &keyboard::Key::Character("\u{0434}".into()),
            Physical::Code(Code::KeyL),
            Modifiers::CTRL
        ));
        assert!(!is_lock_key(&character("l"), ANY_PLACE, Modifiers::empty()));
        assert!(!is_lock_key(
            &character("L"),
            ANY_PLACE,
            Modifiers::CTRL | Modifiers::SHIFT
        ));
        assert!(
            !is_lock_key(&character("l"), ANY_PLACE, Modifiers::CTRL | Modifiers::ALT),
            "AltGr is Ctrl+Alt on Windows"
        );
        assert!(!is_lock_key(&character("k"), ANY_PLACE, Modifiers::CTRL));
        assert_eq!(
            window_shortcut(&character("l"), ANY_PLACE, Modifiers::CTRL),
            None,
            "the terminal keeps Ctrl+L"
        );
    }

    #[test]
    fn shortcuts_read_the_key_place_on_a_non_latin_layout() {
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        // Russian layout: the C key types 's' (Cyrillic es), V 'm', W 'ts'.
        let russian = |c: &str| keyboard::Key::Character(c.into());
        assert_eq!(
            shortcut(&russian("\u{0441}"), Physical::Code(Code::KeyC), ctrl_shift),
            Some(Shortcut::Copy)
        );
        assert_eq!(
            shortcut(&russian("\u{043c}"), Physical::Code(Code::KeyV), ctrl_shift),
            Some(Shortcut::Paste)
        );
        assert_eq!(
            window_shortcut(&russian("\u{0446}"), Physical::Code(Code::KeyW), ctrl_shift),
            Some(WindowShortcut::CloseTab)
        );
        // A Latin letter wins over its place: AZERTY moves letters, and the shortcut
        // follows the letter typed, not where its key sits.
        assert_eq!(
            shortcut(&character("q"), Physical::Code(Code::KeyC), ctrl_shift),
            None
        );
    }

    #[test]
    fn the_other_shortcut_keys() {
        assert_eq!(
            shortcut(
                &keyboard::Key::Named(Named::Insert),
                ANY_PLACE,
                Modifiers::CTRL
            ),
            Some(Shortcut::Copy)
        );
        assert_eq!(
            shortcut(
                &keyboard::Key::Named(Named::PageDown),
                ANY_PLACE,
                Modifiers::SHIFT
            ),
            Some(Shortcut::PageDown)
        );
        assert_eq!(
            window_shortcut(
                &keyboard::Key::Named(Named::PageDown),
                ANY_PLACE,
                Modifiers::CTRL
            ),
            Some(WindowShortcut::NextTab)
        );
        assert_eq!(
            shortcut(
                &character("c"),
                ANY_PLACE,
                Modifiers::CTRL | Modifiers::SHIFT | Modifiers::ALT
            ),
            None,
            "AltGr+Shift types a character"
        );
    }

    #[test]
    fn committed_text_types_the_whole_composition() {
        let input = super::committed_text("\u{3053}\u{3093}").expect("types");
        assert_eq!(input.text.as_deref(), Some("\u{3053}\u{3093}"));
        assert!(super::committed_text("").is_none());
    }
}
