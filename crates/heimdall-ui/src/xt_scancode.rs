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

//! XT scancodes of the keys, by their position whatever their label, as noVNC's
//! `xtscancodes.js` gives them (PC/AT set 1, an extended one as `0xE0xx`): what a VNC server
//! taking QEMU's extended key events is sent, so it types with its own keyboard layout.
//!
//! Its own table, not RDP's: noVNC names keys RDP sends otherwise (Print Screen as `SysRq`,
//! Pause as one code) and more of them (F13 to F24, the media and browser keys, the
//! Japanese and Korean ones).

use iced::keyboard::key::{Code, Physical};

/// The XT scancode of a physical key, when noVNC's table has one.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one table row per key")]
pub fn xt_scancode(key: Physical) -> Option<u16> {
    let Physical::Code(code) = key else {
        return None;
    };
    Some(match code {
        Code::Again => 0xe005,
        Code::AltLeft => 0x38,
        Code::AltRight => 0xe038,
        Code::ArrowDown => 0xe050,
        Code::ArrowLeft => 0xe04b,
        Code::ArrowRight => 0xe04d,
        Code::ArrowUp => 0xe048,
        Code::AudioVolumeDown => 0xe02e,
        Code::AudioVolumeMute => 0xe020,
        Code::AudioVolumeUp => 0xe030,
        Code::Backquote => 0x29,
        Code::Backslash => 0x2b,
        Code::Backspace => 0x0e,
        Code::BracketLeft => 0x1a,
        Code::BracketRight => 0x1b,
        Code::BrowserBack => 0xe06a,
        Code::BrowserFavorites => 0xe066,
        Code::BrowserForward => 0xe069,
        Code::BrowserHome => 0xe032,
        Code::BrowserRefresh => 0xe067,
        Code::BrowserSearch => 0xe065,
        Code::BrowserStop => 0xe068,
        Code::CapsLock => 0x3a,
        Code::Comma => 0x33,
        Code::ContextMenu => 0xe05d,
        Code::ControlLeft => 0x1d,
        Code::ControlRight => 0xe01d,
        Code::Convert => 0x79,
        Code::Copy => 0xe078,
        Code::Cut => 0xe03c,
        Code::Delete => 0xe053,
        Code::Digit0 => 0x0b,
        Code::Digit1 => 0x02,
        Code::Digit2 => 0x03,
        Code::Digit3 => 0x04,
        Code::Digit4 => 0x05,
        Code::Digit5 => 0x06,
        Code::Digit6 => 0x07,
        Code::Digit7 => 0x08,
        Code::Digit8 => 0x09,
        Code::Digit9 => 0x0a,
        Code::Eject => 0xe07d,
        Code::End => 0xe04f,
        Code::Enter => 0x1c,
        Code::Equal => 0x0d,
        Code::Escape => 0x01,
        Code::F1 => 0x3b,
        Code::F2 => 0x3c,
        Code::F3 => 0x3d,
        Code::F4 => 0x3e,
        Code::F5 => 0x3f,
        Code::F6 => 0x40,
        Code::F7 => 0x41,
        Code::F8 => 0x42,
        Code::F9 => 0x43,
        Code::F10 => 0x44,
        Code::F11 => 0x57,
        Code::F12 => 0x58,
        Code::F13 => 0x5d,
        Code::F14 => 0x5e,
        Code::F15 => 0x5f,
        Code::F16 => 0x55,
        Code::F17 => 0xe003,
        Code::F18 => 0xe077,
        Code::F19 => 0xe004,
        Code::F20 => 0x5a,
        Code::F21 => 0x74,
        Code::F22 => 0xe079,
        Code::F23 => 0x6d,
        Code::F24 => 0x6f,
        Code::Find => 0xe041,
        Code::Help => 0xe075,
        Code::Hiragana | Code::Lang4 => 0x77,
        Code::Home => 0xe047,
        Code::Insert => 0xe052,
        Code::IntlBackslash => 0x56,
        Code::IntlRo => 0x73,
        Code::IntlYen => 0x7d,
        Code::KanaMode => 0x70,
        Code::Katakana | Code::Lang3 => 0x78,
        Code::KeyA => 0x1e,
        Code::KeyB => 0x30,
        Code::KeyC => 0x2e,
        Code::KeyD => 0x20,
        Code::KeyE => 0x12,
        Code::KeyF => 0x21,
        Code::KeyG => 0x22,
        Code::KeyH => 0x23,
        Code::KeyI => 0x17,
        Code::KeyJ => 0x24,
        Code::KeyK => 0x25,
        Code::KeyL => 0x26,
        Code::KeyM => 0x32,
        Code::KeyN => 0x31,
        Code::KeyO => 0x18,
        Code::KeyP => 0x19,
        Code::KeyQ => 0x10,
        Code::KeyR => 0x13,
        Code::KeyS => 0x1f,
        Code::KeyT => 0x14,
        Code::KeyU => 0x16,
        Code::KeyV => 0x2f,
        Code::KeyW => 0x11,
        Code::KeyX => 0x2d,
        Code::KeyY => 0x15,
        Code::KeyZ => 0x2c,
        Code::Lang1 => 0x72,
        Code::Lang2 => 0x71,
        Code::Lang5 => 0x76,
        Code::LaunchApp1 => 0xe06b,
        Code::LaunchApp2 => 0xe021,
        Code::LaunchMail => 0xe06c,
        Code::MediaPlayPause => 0xe022,
        Code::MediaSelect => 0xe06d,
        Code::MediaStop => 0xe024,
        Code::MediaTrackNext => 0xe019,
        Code::MediaTrackPrevious => 0xe010,
        // noVNC's MetaLeft and MetaRight: the Windows keys.
        Code::SuperLeft => 0xe05b,
        Code::SuperRight => 0xe05c,
        Code::Minus => 0x0c,
        Code::NonConvert => 0x7b,
        Code::NumLock => 0x45,
        Code::Numpad0 => 0x52,
        Code::Numpad1 => 0x4f,
        Code::Numpad2 => 0x50,
        Code::Numpad3 => 0x51,
        Code::Numpad4 => 0x4b,
        Code::Numpad5 => 0x4c,
        Code::Numpad6 => 0x4d,
        Code::Numpad7 => 0x47,
        Code::Numpad8 => 0x48,
        Code::Numpad9 => 0x49,
        Code::NumpadAdd => 0x4e,
        Code::NumpadComma => 0x7e,
        Code::NumpadDecimal => 0x53,
        Code::NumpadDivide => 0xe035,
        Code::NumpadEnter => 0xe01c,
        Code::NumpadEqual => 0x59,
        Code::NumpadMultiply => 0x37,
        Code::NumpadParenLeft => 0xe076,
        Code::NumpadParenRight => 0xe07b,
        Code::NumpadSubtract => 0x4a,
        Code::Open => 0x64,
        Code::PageDown => 0xe051,
        Code::PageUp => 0xe049,
        Code::Paste => 0x65,
        Code::Pause => 0xe046,
        Code::Period => 0x34,
        Code::Power => 0xe05e,
        Code::PrintScreen => 0x54,
        Code::Props => 0xe006,
        Code::Quote => 0x28,
        Code::ScrollLock => 0x46,
        Code::Semicolon => 0x27,
        Code::ShiftLeft => 0x2a,
        Code::ShiftRight => 0x36,
        Code::Slash => 0x35,
        Code::Sleep => 0xe05f,
        Code::Space => 0x39,
        Code::Suspend => 0xe025,
        Code::Tab => 0x0f,
        Code::Undo => 0xe007,
        Code::WakeUp => 0xe063,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xt(code: Code) -> Option<u16> {
        xt_scancode(Physical::Code(code))
    }

    #[test]
    fn keys_map_to_novnc_s_xt_scancodes() {
        assert_eq!(xt(Code::KeyA), Some(0x1e));
        assert_eq!(xt(Code::KeyQ), Some(0x10), "by position: A on AZERTY");
        assert_eq!(xt(Code::AltRight), Some(0xe038), "AltGr");
        assert_eq!(xt(Code::Numpad7), Some(0x47));
        assert_eq!(xt(Code::NumpadEnter), Some(0xe01c));
        assert_eq!(xt(Code::F1), Some(0x3b));
        assert_eq!(xt(Code::F17), Some(0xe003));
        assert_eq!(xt(Code::PrintScreen), Some(0x54), "noVNC's SysRq");
        assert_eq!(xt(Code::Pause), Some(0xe046));
        assert_eq!(xt(Code::SuperLeft), Some(0xe05b));
        assert_eq!(xt(Code::Fn), None, "no code noVNC knows");
    }

    #[test]
    fn the_set_1_keys_rdp_sends_have_the_same_codes_but_print_screen() {
        use crate::desktop_view::scancode;
        let keys = [
            Code::Escape,
            Code::Digit1,
            Code::KeyZ,
            Code::Enter,
            Code::ShiftRight,
            Code::NumpadMultiply,
            Code::F10,
            Code::F12,
            Code::IntlBackslash,
            Code::ControlRight,
            Code::NumpadDivide,
            Code::Home,
            Code::Delete,
            Code::SuperRight,
            Code::ContextMenu,
        ];
        for key in keys {
            let (extended, code) = scancode(Physical::Code(key)).expect("RDP's").as_u8();
            let prefix = if extended { 0xe000 } else { 0 };
            assert_eq!(xt(key), Some(prefix | u16::from(code)), "{key:?}");
        }
    }
}
