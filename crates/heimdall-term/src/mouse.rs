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

//! Mouse events to bytes, when the application tracks the mouse; wheel to arrow keys on the
//! alternate screen when it does not; focus changes.
//!
//! Shift held means "select locally": the event is not reported, and shift is never part
//! of an encoded event.

use crate::keys::Modifiers;
use crate::mode::{InputMode, MouseEncoding, MouseTracking};

const LEFT_CODE: u32 = 0;
const MIDDLE_CODE: u32 = 1;
const RIGHT_CODE: u32 = 2;
/// Button code of a release in X10 and UTF-8 encodings, and of motion with no button.
const NO_BUTTON_CODE: u32 = 3;
const WHEEL_UP_CODE: u32 = 64;
const WHEEL_DOWN_CODE: u32 = 65;
const MOTION_FLAG: u32 = 32;
const ALT_FLAG: u32 = 8;
const CTRL_FLAG: u32 = 16;

/// Offset added to every value in the X10 and UTF-8 encodings.
const X10_OFFSET: u32 = 32;
/// Largest byte value X10 can carry.
const X10_MAX_VALUE: u32 = 255;
/// Largest value the UTF-8 encoding (1005) carries: two-byte UTF-8.
const UTF8_MAX_VALUE: u32 = 2047;

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Primary.
    Left,
    /// Middle or wheel click.
    Middle,
    /// Secondary.
    Right,
}

impl MouseButton {
    fn code(self) -> u32 {
        match self {
            Self::Left => LEFT_CODE,
            Self::Middle => MIDDLE_CODE,
            Self::Right => RIGHT_CODE,
        }
    }
}

/// What the mouse did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    /// A button went down.
    Press(MouseButton),
    /// A button went up.
    Release(MouseButton),
    /// The pointer moved, possibly with a button held.
    Motion {
        /// Button held during the motion.
        held: Option<MouseButton>,
    },
    /// One wheel notch up.
    WheelUp,
    /// One wheel notch down.
    WheelDown,
}

/// A mouse event over the terminal, in viewport cells counted from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
    /// What happened.
    pub action: MouseAction,
    /// Column under the pointer.
    pub col: usize,
    /// Row under the pointer.
    pub row: usize,
    /// Modifiers held.
    pub modifiers: Modifiers,
}

/// Whether the application receives this event rather than local selection handling it.
#[must_use]
pub fn is_reported(mode: &InputMode, modifiers: Modifiers) -> bool {
    mode.mouse != MouseTracking::Off && !modifiers.shift
}

/// Remembers the last cell a motion was reported for, so a pointer moving within one cell
/// does not flood the application.
#[derive(Debug, Clone, Copy, Default)]
pub struct MotionFilter {
    last: Option<(usize, usize)>,
}

/// Bytes reporting `event` to the application, or `None` when it is not reported: tracking
/// off, shift held, a motion the mode does not ask for, the same cell as the last motion, or
/// a position the encoding cannot express.
#[must_use]
pub fn encode_mouse(
    event: &MouseEvent,
    mode: &InputMode,
    filter: &mut MotionFilter,
) -> Option<Vec<u8>> {
    if !is_reported(mode, event.modifiers) {
        return None;
    }
    let modifier_flags =
        u32::from(event.modifiers.alt) * ALT_FLAG + u32::from(event.modifiers.ctrl) * CTRL_FLAG;
    let (code, released) = match event.action {
        MouseAction::Press(button) => (button.code(), false),
        MouseAction::Release(button) => match mode.mouse_encoding {
            MouseEncoding::Sgr => (button.code(), true),
            MouseEncoding::X10 | MouseEncoding::Utf8 => (NO_BUTTON_CODE, true),
        },
        MouseAction::WheelUp => (WHEEL_UP_CODE, false),
        MouseAction::WheelDown => (WHEEL_DOWN_CODE, false),
        MouseAction::Motion { held } => {
            let wanted = match mode.mouse {
                MouseTracking::Motion => true,
                MouseTracking::Drag => held.is_some(),
                MouseTracking::Clicks | MouseTracking::Off => false,
            };
            if !wanted || filter.last == Some((event.col, event.row)) {
                return None;
            }
            filter.last = Some((event.col, event.row));
            (
                held.map_or(NO_BUTTON_CODE, MouseButton::code) + MOTION_FLAG,
                false,
            )
        }
    };
    let code = code + modifier_flags;
    let col = u32::try_from(event.col).ok()?.checked_add(1)?;
    let row = u32::try_from(event.row).ok()?.checked_add(1)?;
    match mode.mouse_encoding {
        MouseEncoding::Sgr => {
            let final_byte = if released { 'm' } else { 'M' };
            Some(format!("\x1b[<{code};{col};{row}{final_byte}").into_bytes())
        }
        MouseEncoding::X10 => {
            let mut out = b"\x1b[M".to_vec();
            for value in [code, col, row] {
                let shifted = value + X10_OFFSET;
                if shifted > X10_MAX_VALUE {
                    return None;
                }
                out.push(u8::try_from(shifted).ok()?);
            }
            Some(out)
        }
        MouseEncoding::Utf8 => {
            let mut out = b"\x1b[M".to_vec();
            for value in [code, col, row] {
                let shifted = value + X10_OFFSET;
                if shifted > UTF8_MAX_VALUE {
                    return None;
                }
                let mut buffer = [0; 4];
                out.extend_from_slice(char::from_u32(shifted)?.encode_utf8(&mut buffer).as_bytes());
            }
            Some(out)
        }
    }
}

/// Arrow keys for wheel notches on the alternate screen when the mouse is not tracked
/// (`less`, `man`, a pager); `None` when the wheel should scroll the history instead.
#[must_use]
pub fn wheel_as_arrows(up: bool, notches: u32, mode: &InputMode) -> Option<Vec<u8>> {
    if !(mode.alternate_screen && mode.alternate_scroll && mode.mouse == MouseTracking::Off) {
        return None;
    }
    let arrow: &[u8] = match (mode.app_cursor, up) {
        (true, true) => b"\x1bOA",
        (true, false) => b"\x1bOB",
        (false, true) => b"\x1b[A",
        (false, false) => b"\x1b[B",
    };
    Some(arrow.repeat(usize::try_from(notches).ok()?))
}

/// Focus report, when the application asked for them (1004).
#[must_use]
pub fn encode_focus(focused: bool, mode: &InputMode) -> Option<Vec<u8>> {
    mode.focus_reports.then(|| {
        if focused {
            b"\x1b[I".to_vec()
        } else {
            b"\x1b[O".to_vec()
        }
    })
}
