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

//! A tab dragged along the tab bar, as the C# tab: pressed, it becomes a drag once the
//! pointer moves; let go over another tab, it takes that tab's place.

use heimdall_app::TabId;
use iced::{Point, event, mouse, window};

use crate::shell::Message;

/// A press on a tab, a drag once the pointer moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabDrag {
    /// The tab pressed.
    pub tab: TabId,
    /// Where the press was.
    start: Point,
    /// The pointer moved far enough: it is a drag.
    pub active: bool,
}

impl TabDrag {
    /// A press on `tab` at `start`.
    #[must_use]
    pub fn pressed(tab: TabId, start: Point) -> Self {
        Self {
            tab,
            start,
            active: false,
        }
    }

    /// The pointer moved to `at`.
    pub fn moved(&mut self, at: Point) {
        if self.start.distance(at) >= crate::tree_drag::DRAG_THRESHOLD {
            self.active = true;
        }
    }

    /// Where it goes, let go over `over`: that tab, when it is a drag and another tab.
    #[must_use]
    pub fn onto(self, over: Option<TabId>) -> Option<TabId> {
        over.filter(|over| self.active && *over != self.tab)
    }
}

/// While a press on a tab is held: where the pointer goes, and its release.
#[expect(
    clippy::needless_pass_by_value,
    reason = "the signature `event::listen_with` takes"
)]
pub fn drag_event(
    event: iced::Event,
    _status: event::Status,
    _window: window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::TabDragMoved(position))
        }
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::TabDragEnd)
        }
        _ => None,
    }
}
