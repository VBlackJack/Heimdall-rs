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

//! Gestures shared by the window's tests, and the turn they render in.

#![allow(dead_code, reason = "each test file uses part of this module")]

use std::cell::{Cell, RefCell};
use std::ops::{Deref, DerefMut};
use std::sync::{Mutex, MutexGuard, PoisonError};

use heimdall_app::Message as AppMessage;
use heimdall_ui::shell::Message;
use iced::mouse::{Button, Event as MouseEvent, ScrollDelta};
use iced::{Element, Settings, Size};
use iced_test::simulator::Simulator;

/// One test at a time renders in a test binary. Renderers made on several test threads at
/// once ended whole binaries with no test reporting: `STATUS_ACCESS_VIOLATION` on the
/// Windows runner (`rdp_tab`), SIGSEGV on Linux under load (`shell_view`, `rdp_options`).
/// A test that fails still lets the next one draw.
static RENDERING: Mutex<()> = Mutex::new(());

thread_local! {
    /// The turn this thread holds, and how many times it took it: a test makes its
    /// simulators on its own thread, and may hold several at once.
    static HELD: RefCell<Option<MutexGuard<'static, ()>>> = const { RefCell::new(None) };
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// This thread's turn to render, given back when the last one it took is dropped.
pub struct Turn(());

/// The turn to render: waits while another test renders, never on the test itself.
pub fn render_turn() -> Turn {
    if DEPTH.get() == 0 {
        let guard = RENDERING.lock().unwrap_or_else(PoisonError::into_inner);
        HELD.with_borrow_mut(|held| *held = Some(guard));
    }
    DEPTH.set(DEPTH.get() + 1);
    Turn(())
}

impl Drop for Turn {
    fn drop(&mut self) {
        DEPTH.set(DEPTH.get() - 1);
        if DEPTH.get() == 0 {
            HELD.with_borrow_mut(Option::take);
        }
    }
}

/// A simulator holding the turn to render until it is dropped or its messages are taken.
pub struct Drawn<'a> {
    // Declared first: dropped, with its renderer, before the turn is given back.
    ui: Simulator<'a, Message>,
    _turn: Turn,
}

impl Drawn<'_> {
    /// The messages the window sent, the turn given back.
    pub fn into_messages(self) -> impl Iterator<Item = Message> {
        let Drawn { ui, _turn } = self;
        let messages: Vec<Message> = ui.into_messages().collect();
        messages.into_iter()
    }
}

impl<'a> Deref for Drawn<'a> {
    type Target = Simulator<'a, Message>;

    fn deref(&self) -> &Self::Target {
        &self.ui
    }
}

impl DerefMut for Drawn<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.ui
    }
}

/// A simulator of `view` at `size`, made once it is this test's turn to render.
pub fn simulator<'a>(
    settings: Settings,
    size: Size,
    view: impl Into<Element<'a, Message>>,
) -> Drawn<'a> {
    let turn = render_turn();
    Drawn {
        ui: Simulator::with_size(settings, size, view),
        _turn: turn,
    }
}

/// Scrolls the profile form's page, as the wheel does, until the text `label` is in its
/// middle: the form is as high as the C# dialog, whatever the window.
pub fn reveal(ui: &mut Simulator<'_, Message>, label: &str) {
    let page = ui
        .find(heimdall_ui::shell::profile_page_id())
        .expect("the form's page");
    let shown = page.visible_bounds().expect("the page in view");
    let target = ui.find(label).expect(label).bounds();
    ui.point_at(shown.center());
    let _ = ui.simulate([iced::Event::Mouse(MouseEvent::WheelScrolled {
        delta: ScrollDelta::Pixels {
            x: 0.0,
            y: shown.center_y() - target.center_y(),
        },
    })]);
}

/// Windows a double click is tried in. iced tells a double click by the real time between
/// the presses: a runner stalled between them, as the Windows CI runner was on 2026-09-29,
/// makes two single clicks. Each try is a whole double click on a fresh window, so a tree
/// that does not connect on a double click fails every one.
const DOUBLE_CLICK_TRIES: usize = 3;

/// Double-clicks the text `label` as a user does: the pointer on it, two presses and releases
/// in one batch. iced tells a double click by the time between the presses; two separate
/// clicks, each drawn in between, can let more than its window pass on a slow machine.
pub fn double_click(ui: &mut Simulator<'_, Message>, label: &str) {
    let center = ui.find(label).expect(label).bounds().center();
    ui.point_at(center);
    let press = iced::Event::Mouse(MouseEvent::ButtonPressed(Button::Left));
    let release = iced::Event::Mouse(MouseEvent::ButtonReleased(Button::Left));
    let _ = ui.simulate([press.clone(), release.clone(), press, release]);
}

/// The messages of a double click on `label` in the window `window` draws: those of the first
/// try iced took for a double click, one that connected, else those of the last try.
#[allow(dead_code, reason = "not every test file double-clicks")]
pub fn double_click_messages<'a>(window: impl Fn() -> Drawn<'a>, label: &str) -> Vec<Message> {
    let mut messages = Vec::new();
    for _ in 0..DOUBLE_CLICK_TRIES {
        let mut ui = window();
        double_click(&mut ui, label);
        messages = ui.into_messages().collect();
        if messages
            .iter()
            .any(|message| matches!(message, Message::App(AppMessage::ConnectProfile(_))))
        {
            break;
        }
    }
    messages
}
