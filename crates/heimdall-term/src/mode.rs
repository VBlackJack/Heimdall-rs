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

//! What the input encoders need to know about the terminal's current modes.

use alacritty_terminal::term::TermMode;

/// Which mouse events the application asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseTracking {
    /// Not reported: the mouse selects text locally.
    Off,
    /// Presses, releases and wheel (DEC mode 1000).
    Clicks,
    /// Also motion while a button is held (1002).
    Drag,
    /// Also every motion (1003).
    Motion,
}

/// How mouse events are encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEncoding {
    /// `CSI M` followed by three bytes (the X10 default).
    X10,
    /// Like X10, each value encoded as UTF-8 (1005).
    Utf8,
    /// `CSI < b ; x ; y M` or `m` (1006).
    Sgr,
}

/// Terminal modes that change what a key, a click or a paste sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent DEC modes, each on or off"
)]
pub struct InputMode {
    /// Cursor keys send `SS3` instead of `CSI` (DECCKM).
    pub app_cursor: bool,
    /// The numeric keypad sends application sequences (DECKPAM).
    pub app_keypad: bool,
    /// Enter sends CR LF (LNM).
    pub line_feed_new_line: bool,
    /// Pastes are wrapped in `ESC[200~ .. ESC[201~` (2004).
    pub bracketed_paste: bool,
    /// Focus changes are reported (1004).
    pub focus_reports: bool,
    /// The alternate screen is shown.
    pub alternate_screen: bool,
    /// The wheel sends arrow keys on the alternate screen when the mouse is not tracked (1007).
    pub alternate_scroll: bool,
    /// Which mouse events are reported.
    pub mouse: MouseTracking,
    /// How they are encoded.
    pub mouse_encoding: MouseEncoding,
}

impl InputMode {
    /// The modes of a terminal just reset: nothing reported, cursor keys in normal mode.
    #[must_use]
    pub fn reset() -> Self {
        Self::from(TermMode::default())
    }
}

impl From<TermMode> for InputMode {
    fn from(mode: TermMode) -> Self {
        // Modes 1000, 1002 and 1003 replace each other: setting 1002 removes the click flag
        // and sets only the drag flag, yet clicks are still to be reported.
        let mouse = if mode.contains(TermMode::MOUSE_MOTION) {
            MouseTracking::Motion
        } else if mode.contains(TermMode::MOUSE_DRAG) {
            MouseTracking::Drag
        } else if mode.contains(TermMode::MOUSE_REPORT_CLICK) {
            MouseTracking::Clicks
        } else {
            MouseTracking::Off
        };
        let mouse_encoding = if mode.contains(TermMode::SGR_MOUSE) {
            MouseEncoding::Sgr
        } else if mode.contains(TermMode::UTF8_MOUSE) {
            MouseEncoding::Utf8
        } else {
            MouseEncoding::X10
        };
        Self {
            app_cursor: mode.contains(TermMode::APP_CURSOR),
            app_keypad: mode.contains(TermMode::APP_KEYPAD),
            line_feed_new_line: mode.contains(TermMode::LINE_FEED_NEW_LINE),
            bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
            focus_reports: mode.contains(TermMode::FOCUS_IN_OUT),
            alternate_screen: mode.contains(TermMode::ALT_SCREEN),
            alternate_scroll: mode.contains(TermMode::ALTERNATE_SCROLL),
            mouse,
            mouse_encoding,
        }
    }
}
