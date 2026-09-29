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

//! Gestures shared by the window's tests.

use heimdall_app::Message as AppMessage;
use heimdall_ui::shell::Message;
use iced::mouse::{Button, Event as MouseEvent};
use iced_test::simulator::Simulator;

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
pub fn double_click_messages<'a>(
    window: impl Fn() -> Simulator<'a, Message>,
    label: &str,
) -> Vec<Message> {
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
