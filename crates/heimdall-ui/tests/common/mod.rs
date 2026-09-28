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

use heimdall_ui::shell::Message;
use iced::mouse::{Button, Event as MouseEvent};
use iced_test::simulator::Simulator;

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
