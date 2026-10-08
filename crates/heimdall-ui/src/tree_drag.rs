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

//! A drag in the sessions tree, as the C# tree's: pressed on a session or a folder, it
//! becomes a drag once the pointer moves; the row under the pointer is where it drops.

use heimdall_app::{DropTarget, Message as AppMessage};
use heimdall_core::profile::ProfileId;
use iced::widget::{column, container, mouse_area, space, stack};
use iced::{Element, Length, Point, Theme, event, mouse, window};

use crate::shell::Message;
use crate::tokens::radius;

/// How far the pointer moves, held down, before a press becomes a drag, in logical pixels,
/// as the system's drag threshold.
pub(crate) const DRAG_THRESHOLD: f32 = 5.0;

/// Height of the line saying where dropped sessions go, before or after a session.
const INSERT_LINE: f32 = 2.0;

/// Width of the drop target's outline.
const TARGET_BORDER: f32 = 1.5;

/// What is dragged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DragSource {
    /// Sessions: the one pressed, or the ones selected together with it.
    Profiles(Vec<ProfileId>),
    /// A folder.
    Folder(String),
}

/// A press in the tree, a drag once the pointer moves.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeDrag {
    /// What it moves.
    pub source: DragSource,
    /// Where the press was.
    start: Point,
    /// The pointer moved far enough: it is a drag.
    pub active: bool,
    /// The row under the pointer.
    pub over: Option<DropTarget>,
}

impl TreeDrag {
    /// A press on `source` at `start`.
    #[must_use]
    pub fn pressed(source: DragSource, start: Point) -> Self {
        Self {
            source,
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

    /// The message dropping it where the pointer is; `None` when it is not a drag or over
    /// no row.
    #[must_use]
    pub fn drop_message(self) -> Option<AppMessage> {
        let onto = self.over.filter(|_| self.active)?;
        Some(match self.source {
            DragSource::Profiles(ids) => AppMessage::DropProfiles { ids, onto },
            DragSource::Folder(path) => AppMessage::DropFolder { path, onto },
        })
    }
}

/// While a press in the tree is held: where the pointer goes, and its release.
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
            Some(Message::TreeDragMoved(position))
        }
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::TreeDragEnd)
        }
        _ => None,
    }
}

/// `row`, which is `target` for a drop: the pointer over it is said, and it is outlined
/// while a drag is over it.
pub fn drop_zone<'a>(
    row: Element<'a, Message>,
    target: DropTarget,
    drag: Option<&TreeDrag>,
) -> Element<'a, Message> {
    let dragging = drag.is_some_and(|drag| drag.active);
    let under = dragging && drag.and_then(|drag| drag.over.as_ref()) == Some(&target);
    let mut zone = mouse_area(row)
        .on_enter(Message::TreeHover(target.clone()))
        .on_exit(Message::TreeHoverLeft(target));
    if dragging {
        zone = zone.interaction(mouse::Interaction::Grabbing);
    }
    if !under {
        return zone.into();
    }
    container(zone)
        .style(|theme: &Theme| container::Style {
            border: iced::Border {
                color: theme.extended_palette().primary.base.color,
                width: TARGET_BORDER,
                radius: radius::SM.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// A session's row while sessions are dragged, as the C# tree's: its upper half places them
/// just before it, its lower half just after; a line says which.
pub fn positioned_zone<'a>(
    row: Element<'a, Message>,
    id: &ProfileId,
    drag: &TreeDrag,
) -> Element<'a, Message> {
    let before = DropTarget::Before(id.clone());
    let after = DropTarget::After(id.clone());
    let line = |shown: bool| {
        container(space().height(INSERT_LINE))
            .width(Length::Fill)
            .style(move |theme: &Theme| container::Style {
                background: shown.then(|| theme.extended_palette().primary.base.color.into()),
                ..container::Style::default()
            })
    };
    let half = |target: DropTarget| {
        mouse_area(space().width(Length::Fill).height(Length::Fill))
            .on_enter(Message::TreeHover(target.clone()))
            .on_exit(Message::TreeHoverLeft(target))
            .interaction(mouse::Interaction::Grabbing)
    };
    let over = drag.over.as_ref();
    stack![
        column![line(over == Some(&before)), row, line(over == Some(&after))],
        column![half(before), half(after)],
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_becomes_a_drag_once_moved_and_drops_where_the_pointer_is() {
        let id = ProfileId::new("web");
        let mut drag = TreeDrag::pressed(DragSource::Profiles(vec![id.clone()]), Point::ORIGIN);
        assert!(
            !drag.moved(Point::new(2.0, 2.0)),
            "a jitter is still a click"
        );
        drag.over = Some(DropTarget::Folder("Lab".to_owned()));
        assert!(drag.clone().drop_message().is_none(), "not a drag yet");
        assert!(drag.moved(Point::new(10.0, 0.0)));
        assert!(!drag.moved(Point::new(20.0, 0.0)), "once");
        assert!(matches!(
            drag.drop_message(),
            Some(AppMessage::DropProfiles { ids, onto: DropTarget::Folder(folder) })
                if ids == [id] && folder == "Lab"
        ));
    }
}
