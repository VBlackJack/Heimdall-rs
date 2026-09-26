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

//! Keys, mouse and paste to bytes.
//!
//! Expected key sequences come from `infocmp -1 xterm-256color` (the ncurses terminfo
//! database, read on 2026-09-26), not from this crate. Modes are always obtained by feeding the
//! application's own escape sequences to a `Terminal`, never built by hand.

use heimdall_term::{
    GridSize, InputMode, Key, KeyLocation, KeyPress, Modifiers, MotionFilter, MouseAction,
    MouseButton, MouseEvent, NamedKey, Terminal, TerminalConfig, encode_focus, encode_key,
    encode_mouse, encode_paste, is_reported, wheel_as_arrows,
};

/// `smkx` of xterm-256color: application cursor keys and keypad.
const SMKX: &[u8] = b"\x1b[?1h\x1b=";

fn mode_after(sequences: &[u8]) -> InputMode {
    let mut terminal = Terminal::new(GridSize { cols: 80, rows: 24 }, TerminalConfig::default());
    terminal.feed(sequences);
    terminal.input_mode()
}

const NONE: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
};
const SHIFT: Modifiers = Modifiers {
    shift: true,
    ctrl: false,
    alt: false,
};
const CTRL: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: false,
};
const ALT: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: true,
};
const CTRL_ALT: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: true,
};
const CTRL_SHIFT: Modifiers = Modifiers {
    shift: true,
    ctrl: true,
    alt: false,
};

fn named(key: NamedKey, modifiers: Modifiers, mode: &InputMode) -> Vec<u8> {
    encode_key(
        &KeyPress {
            key: Key::Named(key),
            text: None,
            physical_digit: None,
            location: KeyLocation::Standard,
            modifiers,
        },
        mode,
    )
    .expect("named keys send something")
}

fn character(
    key: char,
    text: Option<&str>,
    digit: Option<u8>,
    modifiers: Modifiers,
) -> Option<Vec<u8>> {
    encode_key(
        &KeyPress {
            key: Key::Character(key),
            text,
            physical_digit: digit,
            location: KeyLocation::Standard,
            modifiers,
        },
        &InputMode::reset(),
    )
}

// ---- keys against terminfo --------------------------------------------------------------

#[test]
fn keypad_transmit_mode_matches_terminfo() {
    let mode = mode_after(SMKX);
    let cases: [(NamedKey, &[u8]); 16] = [
        (NamedKey::ArrowUp, b"\x1bOA"),     // kcuu1
        (NamedKey::ArrowDown, b"\x1bOB"),   // kcud1
        (NamedKey::Home, b"\x1bOH"),        // khome
        (NamedKey::End, b"\x1bOF"),         // kend
        (NamedKey::Insert, b"\x1b[2~"),     // kich1
        (NamedKey::Delete, b"\x1b[3~"),     // kdch1
        (NamedKey::PageUp, b"\x1b[5~"),     // kpp
        (NamedKey::PageDown, b"\x1b[6~"),   // knp
        (NamedKey::Backspace, b"\x7f"),     // kbs
        (NamedKey::Function(1), b"\x1bOP"), // kf1
        (NamedKey::Function(4), b"\x1bOS"), // kf4
        (NamedKey::Function(5), b"\x1b[15~"),
        (NamedKey::Function(6), b"\x1b[17~"),
        (NamedKey::Function(10), b"\x1b[21~"),
        (NamedKey::Function(11), b"\x1b[23~"),
        (NamedKey::Function(12), b"\x1b[24~"),
    ];
    for (key, expected) in cases {
        assert_eq!(named(key, NONE, &mode), expected, "{key:?}");
    }
    assert_eq!(named(NamedKey::Tab, SHIFT, &mode), b"\x1b[Z"); // kcbt
}

#[test]
fn modified_function_keys_match_terminfo() {
    let mode = mode_after(SMKX);
    let cases: [(NamedKey, Modifiers, &[u8]); 6] = [
        (NamedKey::Function(1), SHIFT, b"\x1b[1;2P"),  // kf13
        (NamedKey::Function(5), SHIFT, b"\x1b[15;2~"), // kf17
        (NamedKey::Function(1), CTRL, b"\x1b[1;5P"),   // kf25
        (NamedKey::Function(12), CTRL, b"\x1b[24;5~"), // kf36
        (NamedKey::Function(4), CTRL_SHIFT, b"\x1b[1;6S"), // kf40
        (NamedKey::Function(9), ALT, b"\x1b[20;3~"),   // kf57
    ];
    for (key, modifiers, expected) in cases {
        assert_eq!(
            named(key, modifiers, &mode),
            expected,
            "{key:?} {modifiers:?}"
        );
    }
}

#[test]
fn cursor_keys_follow_the_application_mode_and_modifiers_force_csi() {
    let normal = mode_after(b"");
    let application = mode_after(SMKX);
    assert_eq!(named(NamedKey::ArrowUp, NONE, &normal), b"\x1b[A");
    assert_eq!(named(NamedKey::ArrowUp, NONE, &application), b"\x1bOA");
    assert_eq!(named(NamedKey::ArrowUp, CTRL, &application), b"\x1b[1;5A");
    assert_eq!(named(NamedKey::ArrowLeft, ALT, &normal), b"\x1b[1;3D");
    assert_eq!(named(NamedKey::Home, SHIFT, &normal), b"\x1b[1;2H");
    assert_eq!(named(NamedKey::PageUp, CTRL, &normal), b"\x1b[5;5~");
}

#[test]
fn enter_backspace_space_and_escape() {
    let mode = mode_after(b"");
    assert_eq!(named(NamedKey::Enter, NONE, &mode), b"\r");
    assert_eq!(
        named(NamedKey::Enter, NONE, &mode_after(b"\x1b[20h")),
        b"\r\n"
    );
    assert_eq!(named(NamedKey::Backspace, NONE, &mode), b"\x7f");
    assert_eq!(named(NamedKey::Backspace, CTRL, &mode), b"\x08");
    assert_eq!(named(NamedKey::Backspace, ALT, &mode), b"\x1b\x7f");
    assert_eq!(named(NamedKey::Space, CTRL, &mode), b"\x00");
    assert_eq!(named(NamedKey::Escape, NONE, &mode), b"\x1b");
}

#[test]
fn ctrl_with_letters_and_punctuation_gives_xterms_control_codes() {
    let cases: [(char, u8); 9] = [
        ('a', 0x01),
        ('c', 0x03),
        ('Z', 0x1a),
        ('@', 0x00),
        ('[', 0x1b),
        ('\\', 0x1c),
        (']', 0x1d),
        ('/', 0x1f),
        ('?', 0x7f),
    ];
    for (key, code) in cases {
        assert_eq!(
            character(key, None, None, CTRL),
            Some(vec![code]),
            "Ctrl+{key}"
        );
    }
}

#[test]
fn ctrl_on_the_digit_row_uses_the_physical_key_on_azerty() {
    // AZERTY: the key printed 2 types 'é', the key printed 6 types '-'.
    assert_eq!(character('é', None, Some(2), CTRL), Some(vec![0x00]));
    assert_eq!(character('-', None, Some(6), CTRL), Some(vec![0x1e]));
    assert_eq!(character('è', None, Some(7), CTRL), Some(vec![0x1f]));
}

#[test]
fn altgr_characters_on_azerty_are_sent_as_typed() {
    // Windows reports AltGr as Ctrl+Alt; the text is what the layout produced.
    assert_eq!(
        character('é', Some("~"), Some(2), CTRL_ALT),
        Some(b"~".to_vec())
    );
    assert_eq!(
        character('"', Some("#"), Some(3), CTRL_ALT),
        Some(b"#".to_vec())
    );
    assert_eq!(
        character('\'', Some("{"), Some(4), CTRL_ALT),
        Some(b"{".to_vec())
    );
    assert_eq!(
        character('-', Some("|"), Some(6), CTRL_ALT),
        Some(b"|".to_vec())
    );
    assert_eq!(
        character('à', Some("@"), Some(0), CTRL_ALT),
        Some(b"@".to_vec())
    );
    assert_eq!(
        character('e', Some("\u{20ac}"), None, CTRL_ALT),
        Some("\u{20ac}".as_bytes().to_vec())
    );
}

#[test]
fn ctrl_alt_with_a_letter_is_escape_then_the_control_code() {
    assert_eq!(
        character('a', Some("a"), None, CTRL_ALT),
        Some(vec![0x1b, 0x01])
    );
}

#[test]
fn alt_prefixes_escape_and_text_carries_layout_and_dead_keys() {
    assert_eq!(
        character('x', Some("x"), None, ALT),
        Some(b"\x1bx".to_vec())
    );
    assert_eq!(character('a', Some("A"), None, SHIFT), Some(b"A".to_vec()));
    // Dead key '^' then 'e': the composed character arrives as the text.
    assert_eq!(
        character('e', Some("\u{ea}"), None, NONE),
        Some("\u{ea}".as_bytes().to_vec())
    );
}

#[test]
fn the_keypad_sends_application_sequences_only_in_keypad_mode() {
    let press = |key| KeyPress {
        key,
        text: None,
        physical_digit: None,
        location: KeyLocation::Numpad,
        modifiers: NONE,
    };
    let application = mode_after(SMKX);
    assert_eq!(
        encode_key(&press(Key::Named(NamedKey::Enter)), &application),
        Some(b"\x1bOM".to_vec()) // kent
    );
    assert_eq!(
        encode_key(&press(Key::Character('5')), &application),
        Some(b"\x1bOu".to_vec())
    );
    assert_eq!(
        encode_key(&press(Key::Character('5')), &mode_after(b"")),
        Some(b"5".to_vec())
    );
}

// ---- mouse ------------------------------------------------------------------------------

fn event(action: MouseAction, col: usize, row: usize, modifiers: Modifiers) -> MouseEvent {
    MouseEvent {
        action,
        col,
        row,
        modifiers,
    }
}

fn report(mode: &InputMode, event: &MouseEvent) -> Option<Vec<u8>> {
    encode_mouse(event, mode, &mut MotionFilter::default())
}

#[test]
fn nothing_is_reported_until_the_application_asks() {
    let mode = mode_after(b"");
    let press = event(MouseAction::Press(MouseButton::Left), 1, 1, NONE);
    assert!(!is_reported(&mode, NONE));
    assert_eq!(report(&mode, &press), None);
}

#[test]
fn sgr_clicks_press_and_release() {
    let mode = mode_after(b"\x1b[?1000h\x1b[?1006h");
    let left = MouseButton::Left;
    assert_eq!(
        report(&mode, &event(MouseAction::Press(left), 4, 2, NONE)),
        Some(b"\x1b[<0;5;3M".to_vec())
    );
    assert_eq!(
        report(&mode, &event(MouseAction::Release(left), 4, 2, NONE)),
        Some(b"\x1b[<0;5;3m".to_vec())
    );
    assert_eq!(
        report(
            &mode,
            &event(MouseAction::Motion { held: Some(left) }, 5, 2, NONE)
        ),
        None,
        "1000 alone does not report motion"
    );
}

#[test]
fn clicks_are_reported_under_drag_mode_alone_as_vim_and_tmux_set_it() {
    // 1002 without 1000: the emulator clears the click flag when setting drag.
    let mode = mode_after(b"\x1b[?1002h\x1b[?1006h");
    let right = MouseButton::Right;
    assert_eq!(
        report(&mode, &event(MouseAction::Press(right), 0, 0, NONE)),
        Some(b"\x1b[<2;1;1M".to_vec())
    );
    assert_eq!(
        report(&mode, &event(MouseAction::WheelUp, 0, 0, NONE)),
        Some(b"\x1b[<64;1;1M".to_vec())
    );
}

#[test]
fn drag_reports_motion_with_a_button_once_per_cell() {
    let mode = mode_after(b"\x1b[?1002h\x1b[?1006h");
    let mut filter = MotionFilter::default();
    let drag = |col| {
        event(
            MouseAction::Motion {
                held: Some(MouseButton::Left),
            },
            col,
            0,
            NONE,
        )
    };
    assert_eq!(
        encode_mouse(&drag(3), &mode, &mut filter),
        Some(b"\x1b[<32;4;1M".to_vec())
    );
    assert_eq!(
        encode_mouse(&drag(3), &mode, &mut filter),
        None,
        "same cell"
    );
    assert_eq!(
        encode_mouse(&drag(4), &mode, &mut filter),
        Some(b"\x1b[<32;5;1M".to_vec())
    );
    let hover = event(MouseAction::Motion { held: None }, 6, 0, NONE);
    assert_eq!(
        encode_mouse(&hover, &mode, &mut filter),
        None,
        "1002 ignores hover"
    );
}

#[test]
fn any_motion_mode_reports_hover_as_button_35() {
    let mode = mode_after(b"\x1b[?1003h\x1b[?1006h");
    let hover = event(MouseAction::Motion { held: None }, 0, 0, NONE);
    assert_eq!(report(&mode, &hover), Some(b"\x1b[<35;1;1M".to_vec()));
}

#[test]
fn x10_encoding_release_is_button_three_and_positions_stop_at_223() {
    let mode = mode_after(b"\x1b[?1000h");
    let left = MouseButton::Left;
    assert_eq!(
        report(&mode, &event(MouseAction::Press(left), 0, 0, NONE)),
        Some(vec![0x1b, b'[', b'M', 32, 33, 33])
    );
    assert_eq!(
        report(&mode, &event(MouseAction::Release(left), 0, 0, NONE)),
        Some(vec![0x1b, b'[', b'M', 35, 33, 33])
    );
    assert!(report(&mode, &event(MouseAction::Press(left), 222, 0, NONE)).is_some());
    assert_eq!(
        report(&mode, &event(MouseAction::Press(left), 223, 0, NONE)),
        None
    );
}

#[test]
fn utf8_encoding_carries_positions_beyond_223() {
    let mode = mode_after(b"\x1b[?1000h\x1b[?1005h");
    let bytes = report(
        &mode,
        &event(MouseAction::Press(MouseButton::Left), 299, 0, NONE),
    )
    .expect("reported");
    let mut expected = vec![0x1b, b'[', b'M', 32];
    expected.extend_from_slice("\u{14c}".as_bytes()); // 32 + 300
    expected.push(33);
    assert_eq!(bytes, expected);
}

#[test]
fn shift_leaves_the_mouse_to_local_selection_and_modifiers_are_encoded() {
    let mode = mode_after(b"\x1b[?1000h\x1b[?1006h");
    let press = MouseAction::Press(MouseButton::Left);
    assert!(!is_reported(&mode, SHIFT));
    assert_eq!(report(&mode, &event(press, 0, 0, SHIFT)), None);
    assert_eq!(
        report(&mode, &event(press, 0, 0, CTRL)),
        Some(b"\x1b[<16;1;1M".to_vec())
    );
    assert_eq!(
        report(&mode, &event(press, 0, 0, ALT)),
        Some(b"\x1b[<8;1;1M".to_vec())
    );
}

#[test]
fn the_wheel_sends_arrows_on_the_alternate_screen_only_without_mouse_tracking() {
    let pager = mode_after(b"\x1b[?1049h");
    assert_eq!(
        wheel_as_arrows(true, 3, &pager),
        Some(b"\x1b[A\x1b[A\x1b[A".to_vec())
    );
    let pager_app = mode_after(b"\x1b[?1049h\x1b[?1h");
    assert_eq!(
        wheel_as_arrows(false, 1, &pager_app),
        Some(b"\x1bOB".to_vec())
    );
    assert_eq!(
        wheel_as_arrows(true, 1, &mode_after(b"")),
        None,
        "primary screen"
    );
    assert_eq!(
        wheel_as_arrows(true, 1, &mode_after(b"\x1b[?1049h\x1b[?1000h")),
        None,
        "tracked mouse"
    );
}

#[test]
fn focus_is_reported_only_when_asked() {
    assert_eq!(encode_focus(true, &mode_after(b"")), None);
    let mode = mode_after(b"\x1b[?1004h");
    assert_eq!(encode_focus(true, &mode), Some(b"\x1b[I".to_vec()));
    assert_eq!(encode_focus(false, &mode), Some(b"\x1b[O".to_vec()));
}

// ---- paste ------------------------------------------------------------------------------

#[test]
fn a_bracketed_paste_is_wrapped_and_cannot_close_its_own_bracket() {
    let mode = mode_after(b"\x1b[?2004h");
    let hostile = "echo safe\x1b[201~\nrm -rf ~\n";
    let bytes = encode_paste(hostile, &mode);
    assert!(bytes.starts_with(b"\x1b[200~"));
    assert!(bytes.ends_with(b"\x1b[201~"));
    let inner = &bytes[6..bytes.len() - 6];
    assert!(!inner.contains(&0x1b), "no escape inside: {inner:?}");
    assert_eq!(inner, b"echo safe[201~\rrm -rf ~\r");
}

#[test]
fn line_endings_become_one_carriage_return_and_controls_are_dropped() {
    let mode = mode_after(b"");
    assert_eq!(encode_paste("a\r\nb\nc\rd", &mode), b"a\rb\rc\rd");
    assert_eq!(encode_paste("x\ty", &mode), b"x\ty");
    assert_eq!(encode_paste("a\x7fb\u{9b}c\x07d", &mode), b"abcd");
    assert_eq!(encode_paste("caf\u{e9}", &mode), "caf\u{e9}".as_bytes());
}
