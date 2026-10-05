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

//! A drag in a Files tab's panes, as the C# Files tab's: pressed on an entry, it becomes a
//! drag once the pointer moves; where the pointer is let go, a pane or a folder in it, is
//! where the entries go.

use heimdall_app::files::Side;
use heimdall_app::{FilesMessage, Message as AppMessage, TabId};
use iced::widget::mouse_area;
use iced::{Element, Point, event, mouse, window};

use crate::shell::Message;

/// How far the pointer moves, held down, before a press becomes a drag, in logical pixels,
/// as the system's drag threshold.
const DRAG_THRESHOLD: f32 = 5.0;

/// Where the pointer is in a Files tab: a pane, and the entry under it if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    /// The tab.
    pub tab: TabId,
    /// The pane.
    pub side: Side,
    /// The entry, by its place; `None` between entries.
    pub index: Option<usize>,
}

/// A press on an entry, a drag once the pointer moves.
#[derive(Debug, Clone, PartialEq)]
pub struct FilesDrag {
    /// Where the press was: on an entry.
    pub from: Spot,
    start: Point,
    /// The pointer moved far enough: it is a drag.
    pub active: bool,
    /// Where the pointer is now, in the same tab.
    pub over: Option<Spot>,
}

impl FilesDrag {
    /// A press on `from`, at `start`.
    #[must_use]
    pub fn pressed(from: Spot, start: Point) -> Self {
        Self {
            from,
            start,
            active: false,
            over: None,
        }
    }

    /// The pointer moved to `at`: whether the press just became a drag.
    pub fn moved(&mut self, at: Point) -> bool {
        if self.active || self.start.distance(at) < DRAG_THRESHOLD {
            return false;
        }
        self.active = true;
        true
    }

    /// The message dropping the entries where the pointer is; `None` when it is not a drag
    /// or over no pane of its tab.
    #[must_use]
    pub fn drop_message(self) -> Option<AppMessage> {
        let onto = self
            .over
            .filter(|over| self.active && over.tab == self.from.tab)?;
        Some(AppMessage::Files(FilesMessage::DropEntries {
            tab: onto.tab,
            from: self.from.side,
            onto: onto.side,
            into: onto.index,
        }))
    }
}

/// While a press in a pane is held: where the pointer goes, and its release.
#[must_use]
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
            Some(Message::FilesDragMoved(position))
        }
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::FilesDragEnd)
        }
        _ => None,
    }
}

/// `content`, at `spot`: the pointer coming over it and leaving it is said.
pub fn spot<'a>(content: impl Into<Element<'a, Message>>, at: Spot) -> Element<'a, Message> {
    mouse_area(content)
        .on_enter(Message::FilesHover(at))
        .on_exit(Message::FilesHoverLeft(at))
        .into()
}
