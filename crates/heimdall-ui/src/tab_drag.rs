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
//! pointer moves; let go over another tab, it takes that tab's place; let go over the
//! content, it splits the tab shown, as the C# `ContentDropZone`; let go out of the
//! window, it goes to a window of its own, as the C# drop no target takes. Escape gives
//! the drag up.
//!
//! The pointer is followed out of the window while the button is held: Windows captures
//! it for the window pressed, X11, Wayland and macOS hold it for the window as long as a
//! button is down, so the moves and the release out of it are the window's.

use heimdall_app::TabId;
use heimdall_app::split::{Axis, Placement};
use iced::advanced::widget::Id;
use iced::{Point, Rectangle, Size, Vector, event, mouse, window};

use crate::shell::Message;

/// The middle of the content, as a share of its width or height.
const HALF: f32 = 0.5;

/// How far beyond the window's drawn area, in logical pixels, a tab is let go to go to a
/// window of its own: past the window's own frame, its title bar above, about 31 pixels
/// high on Windows 11, and its borders, so that a tab let go on its own window's frame
/// stays where it was.
pub const DETACH_MARGIN: f32 = 40.0;

/// Where the pointer is on the window a tab detached by a drag opens in, from the window's
/// top left corner, in logical pixels: on its title bar, a little in from its left edge,
/// as if the tab had been carried there by it.
pub const DETACH_GRAB: Vector = Vector::new(120.0, 16.0);

/// A press on a tab, a drag once the pointer moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabDrag {
    /// The tab pressed.
    pub tab: TabId,
    /// Where the press was.
    start: Point,
    /// Where the pointer last was.
    at: Point,
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
            at: start,
            active: false,
        }
    }

    /// The pointer moved to `at`.
    pub fn moved(&mut self, at: Point) {
        self.at = at;
        if self.start.distance(at) >= crate::tree_drag::DRAG_THRESHOLD {
            self.active = true;
        }
    }

    /// Where the pointer last was.
    #[must_use]
    pub fn at(&self) -> Point {
        self.at
    }

    /// Where it goes, let go over `over`: that tab, when it is a drag and another tab.
    #[must_use]
    pub fn onto(self, over: Option<TabId>) -> Option<TabId> {
        over.filter(|over| self.active && *over != self.tab)
    }
}

/// The area a tab is dropped on to split the tab shown: the content, under the tab bar.
#[must_use]
pub fn content_area_id() -> Id {
    crate::screenshot::area_id()
}

/// How a tab let go at `cursor` over the content drawn in `bounds` splits the tab shown;
/// `None` off the content.
///
/// The axis is the C#'s: nearer the top or bottom edge than the left or right one, by
/// share of the height and width, the panes are stacked (the C# `Horizontal`), else side
/// by side (the C# `Vertical`), a tie side by side. The C# always puts the dragged tab
/// second; here the half the cursor is in decides: the top or left half puts it first,
/// the bottom or right half second, as the C# does. The content is so cut by its two
/// diagonals in four zones, each giving the dragged tab the side along its edge.
#[must_use]
pub fn drop_zone(bounds: Rectangle, cursor: Point) -> Option<(Axis, Placement)> {
    if !bounds.contains(cursor) {
        return None;
    }
    let across = (cursor.x - bounds.x) / bounds.width;
    let down = (cursor.y - bounds.y) / bounds.height;
    let side = |share: f32| {
        if share < HALF {
            Placement::First
        } else {
            Placement::Second
        }
    };
    if down.min(1.0 - down) < across.min(1.0 - across) {
        Some((Axis::Stacked, side(down)))
    } else {
        Some((Axis::SideBySide, side(across)))
    }
}

/// The part of the content `bounds` a tab dropped in `zone` takes: the half beside the
/// edge it was dropped near.
#[must_use]
pub fn drop_half(bounds: Rectangle, zone: (Axis, Placement)) -> Rectangle {
    let (axis, placement) = zone;
    match axis {
        Axis::SideBySide => {
            let width = bounds.width * HALF;
            let x = match placement {
                Placement::First => bounds.x,
                Placement::Second => bounds.x + width,
            };
            Rectangle { x, width, ..bounds }
        }
        Axis::Stacked => {
            let height = bounds.height * HALF;
            let y = match placement {
                Placement::First => bounds.y,
                Placement::Second => bounds.y + height,
            };
            Rectangle {
                y,
                height,
                ..bounds
            }
        }
    }
}

/// Whether a tab let go at `at` is out of a window whose drawn area is `window` large, by
/// more than [`DETACH_MARGIN`] past any edge. Both are in the window's logical pixels, as
/// iced reports the pointer and the window's size, each the physical one divided by the
/// screen's scale: the margin is so as wide to the eye on any screen.
#[must_use]
pub fn beyond_window(window: Size, at: Point) -> bool {
    at.x < -DETACH_MARGIN
        || at.y < -DETACH_MARGIN
        || at.x > window.width + DETACH_MARGIN
        || at.y > window.height + DETACH_MARGIN
}

/// While a press on a tab is held: where the pointer goes, and its release.
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
            Some(Message::TabDragMoved(position))
        }
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::TabDragEnd)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Content 1000 wide and 600 high, under a tab bar.
    const CONTENT: Rectangle = Rectangle {
        x: 300.0,
        y: 40.0,
        width: 1000.0,
        height: 600.0,
    };

    fn at(across: f32, down: f32) -> Option<(Axis, Placement)> {
        drop_zone(
            CONTENT,
            Point::new(
                CONTENT.x + CONTENT.width * across,
                CONTENT.y + CONTENT.height * down,
            ),
        )
    }

    #[test]
    fn the_nearest_edge_gives_the_axis_and_its_half_the_side() {
        assert_eq!(at(0.5, 0.05), Some((Axis::Stacked, Placement::First)));
        assert_eq!(at(0.5, 0.95), Some((Axis::Stacked, Placement::Second)));
        assert_eq!(at(0.05, 0.5), Some((Axis::SideBySide, Placement::First)));
        assert_eq!(at(0.95, 0.5), Some((Axis::SideBySide, Placement::Second)));
        // 150 pixels from the left and 120 from the top: nearer the top by pixels, the
        // left by share, which the C# compares.
        assert_eq!(at(0.25, 0.2), Some((Axis::Stacked, Placement::First)));
        assert_eq!(at(0.15, 0.2), Some((Axis::SideBySide, Placement::First)));
    }

    #[test]
    fn a_tie_and_the_middle_are_side_by_side_as_the_csharp() {
        assert_eq!(at(0.3, 0.3), Some((Axis::SideBySide, Placement::First)));
        assert_eq!(at(0.5, 0.5), Some((Axis::SideBySide, Placement::Second)));
        assert_eq!(at(0.75, 0.75), Some((Axis::SideBySide, Placement::Second)));
    }

    #[test]
    fn off_the_content_nothing_splits() {
        assert_eq!(at(0.5, -0.01), None, "over the tab bar");
        assert_eq!(at(-0.01, 0.5), None, "over the tree");
        assert_eq!(at(1.0, 0.5), None);
        let flat = Rectangle {
            height: 0.0,
            ..CONTENT
        };
        assert_eq!(drop_zone(flat, Point::new(400.0, 40.0)), None);
    }

    /// A window 1100 wide and 700 high, in logical pixels.
    const WINDOW: Size = Size::new(1100.0, 700.0);

    #[test]
    fn inside_the_window_or_on_its_frame_a_tab_stays() {
        assert!(!beyond_window(WINDOW, Point::new(550.0, 350.0)));
        assert!(!beyond_window(WINDOW, Point::ORIGIN));
        assert!(!beyond_window(WINDOW, Point::new(1100.0, 700.0)));
        // Just out, within the margin: on the title bar or a border.
        let just = DETACH_MARGIN - 1.0;
        assert!(!beyond_window(WINDOW, Point::new(-just, 350.0)), "left");
        assert!(
            !beyond_window(WINDOW, Point::new(550.0, -just)),
            "title bar"
        );
        assert!(
            !beyond_window(WINDOW, Point::new(1100.0 + just, 350.0)),
            "right"
        );
        assert!(
            !beyond_window(WINDOW, Point::new(550.0, 700.0 + just)),
            "bottom"
        );
        // On the margin itself, still the window's.
        assert!(!beyond_window(WINDOW, Point::new(-DETACH_MARGIN, 350.0)));
    }

    #[test]
    fn beyond_the_margin_on_any_side_a_tab_detaches() {
        let past = DETACH_MARGIN + 1.0;
        assert!(beyond_window(WINDOW, Point::new(-past, 350.0)), "left");
        assert!(beyond_window(WINDOW, Point::new(550.0, -past)), "above");
        assert!(
            beyond_window(WINDOW, Point::new(1100.0 + past, 350.0)),
            "right"
        );
        assert!(
            beyond_window(WINDOW, Point::new(550.0, 700.0 + past)),
            "below"
        );
        assert!(beyond_window(WINDOW, Point::new(-past, -past)), "a corner");
    }

    #[test]
    fn the_margin_is_in_logical_pixels_whatever_the_screen_scale() {
        // At 150%, a window 1650 by 1050 physical pixels is reported 1100 by 700, and the
        // pointer 45 physical pixels past its right edge 30: within the margin.
        let scale = 1.5;
        let window = Size::new(1650.0 / scale, 1050.0 / scale);
        assert_eq!(window, WINDOW);
        let near = Point::new((1650.0 + 45.0) / scale, 350.0);
        assert!(!beyond_window(window, near));
        // 90 physical pixels past it, 60 logical: beyond.
        let far = Point::new((1650.0 + 90.0) / scale, 350.0);
        assert!(beyond_window(window, far));
        // At 100%, the same 45 physical pixels are beyond: the margin grows with the
        // screen's scale.
        let unscaled = Size::new(1650.0, 1050.0);
        assert!(beyond_window(unscaled, Point::new(1650.0 + 45.0, 350.0)));
    }

    #[test]
    fn the_half_taken_is_beside_the_edge() {
        let left = drop_half(CONTENT, (Axis::SideBySide, Placement::First));
        assert_eq!(
            left,
            Rectangle {
                width: 500.0,
                ..CONTENT
            }
        );
        let right = drop_half(CONTENT, (Axis::SideBySide, Placement::Second));
        assert_eq!(
            right,
            Rectangle {
                x: 800.0,
                width: 500.0,
                ..CONTENT
            }
        );
        let top = drop_half(CONTENT, (Axis::Stacked, Placement::First));
        assert_eq!(
            top,
            Rectangle {
                height: 300.0,
                ..CONTENT
            }
        );
        let bottom = drop_half(CONTENT, (Axis::Stacked, Placement::Second));
        assert_eq!(
            bottom,
            Rectangle {
                y: 340.0,
                height: 300.0,
                ..CONTENT
            }
        );
    }
}
