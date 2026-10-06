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

//! Where the code editor's parts lie, and how far its text is scrolled across: plain
//! arithmetic, kept apart from the widget so that it can be checked on its own.

use iced::{Point, Rectangle, Size};

/// Room between the editor's border and what it shows, as iced's text editor.
pub const PADDING: f32 = 5.0;

/// Room left of the line numbers.
pub const GUTTER_LEFT: f32 = 4.0;

/// Room between the line numbers and the text.
pub const GUTTER_GAP: f32 = 12.0;

/// Width of the caret.
pub const CARET_WIDTH: f32 = 1.0;

/// Height of the horizontal scrollbar, as iced's scrollables.
pub const SCROLLBAR_HEIGHT: f32 = 10.0;

/// Narrowest the scrollbar's handle gets, so that it can still be grabbed.
pub const MIN_SCROLLER_WIDTH: f32 = 20.0;

/// The editor's parts, in the window's coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Regions {
    /// The line numbers, from the editor's left edge to the text.
    pub gutter: Rectangle,
    /// The text.
    pub text: Rectangle,
    /// The horizontal scrollbar, under the text, when the text is wider than it.
    pub bar: Option<Rectangle>,
}

/// How many digits the number of the last of `line_count` lines has: at least one.
#[must_use]
pub fn digits(line_count: usize) -> u8 {
    let digits = line_count.checked_ilog10().map_or(1, |log| log + 1);
    u8::try_from(digits).unwrap_or(u8::MAX)
}

/// Width of the gutter numbering `line_count` lines, each digit `digit_width` wide.
#[must_use]
pub fn gutter_width(line_count: usize, digit_width: f32) -> f32 {
    GUTTER_LEFT + f32::from(digits(line_count)) * digit_width + GUTTER_GAP
}

/// The parts of an editor drawn in `bounds`, its gutter `gutter_width` wide, with a
/// horizontal scrollbar when `bar`.
#[must_use]
pub fn regions(bounds: Rectangle, gutter_width: f32, bar: bool) -> Regions {
    let inner = Rectangle {
        x: bounds.x + PADDING,
        y: bounds.y + PADDING,
        width: (bounds.width - 2.0 * PADDING).max(0.0),
        height: (bounds.height - 2.0 * PADDING).max(0.0),
    };
    let gutter_width = gutter_width.min(inner.width);
    let bar_height = if bar {
        SCROLLBAR_HEIGHT.min(inner.height)
    } else {
        0.0
    };
    let text = Rectangle {
        x: inner.x + gutter_width,
        y: inner.y,
        width: inner.width - gutter_width,
        height: inner.height - bar_height,
    };
    Regions {
        gutter: Rectangle {
            x: bounds.x,
            y: inner.y,
            width: gutter_width + PADDING,
            height: text.height,
        },
        text,
        bar: bar.then_some(Rectangle {
            x: text.x,
            y: text.y + text.height,
            width: text.width,
            height: bar_height,
        }),
    }
}

/// The size the text is laid out in: the editor's `size` less its padding, its gutter and,
/// when `bar`, its scrollbar.
#[must_use]
pub fn text_size(size: Size, gutter_width: f32, bar: bool) -> Size {
    let bounds = Rectangle::new(Point::ORIGIN, size);
    regions(bounds, gutter_width, bar).text.size()
}

/// The scroll across that shows the caret, `caret` from the start of its line, in a view
/// `view` wide scrolled `scroll` across: unchanged when the caret is in view, else just
/// enough to bring it in.
#[must_use]
pub fn follow(scroll: f32, caret: f32, view: f32) -> f32 {
    if caret < scroll {
        caret
    } else if caret + CARET_WIDTH > scroll + view {
        caret + CARET_WIDTH - view
    } else {
        scroll
    }
    .max(0.0)
}

/// `scroll` kept within what text `content` wide allows in a view `view` wide.
#[must_use]
pub fn clamp(scroll: f32, content: f32, view: f32) -> f32 {
    scroll.clamp(0.0, (content - view).max(0.0))
}

/// The scrollbar's handle in `rail`, for text `content` wide scrolled `scroll` across in a
/// view `view` wide.
#[must_use]
pub fn scroller(rail: Rectangle, scroll: f32, content: f32, view: f32) -> Rectangle {
    if content <= view || content <= 0.0 {
        return rail;
    }
    let width = (rail.width * view / content).clamp(MIN_SCROLLER_WIDTH.min(rail.width), rail.width);
    let ratio = (scroll / (content - view)).clamp(0.0, 1.0);
    Rectangle {
        x: rail.x + (rail.width - width) * ratio,
        width,
        ..rail
    }
}

/// The scroll across that puts the left edge of a handle `width` wide at `left` in `rail`.
#[must_use]
pub fn scroll_at(rail: Rectangle, width: f32, left: f32, content: f32, view: f32) -> f32 {
    let travel = rail.width - width;
    if travel <= 0.0 {
        return 0.0;
    }
    ((left - rail.x) / travel).clamp(0.0, 1.0) * (content - view).max(0.0)
}

#[cfg(test)]
mod tests {
    use iced::{Point, Rectangle, Size};

    use super::{
        CARET_WIDTH, GUTTER_GAP, GUTTER_LEFT, PADDING, SCROLLBAR_HEIGHT, clamp, digits, follow,
        gutter_width, regions, scroll_at, scroller,
    };

    #[test]
    fn the_gutter_widens_with_the_digits_of_the_last_line_number() {
        assert_eq!(digits(0), 1, "an empty text still shows line 1");
        assert_eq!(digits(1), 1);
        assert_eq!(digits(9), 1);
        assert_eq!(digits(10), 2);
        assert_eq!(digits(99), 2);
        assert_eq!(digits(100), 3);
        assert_eq!(digits(1_000_000), 7);
        let digit = 8.0;
        assert!((gutter_width(9, digit) - (GUTTER_LEFT + digit + GUTTER_GAP)).abs() < f32::EPSILON);
        assert!(gutter_width(10, digit) > gutter_width(9, digit));
        assert!(gutter_width(100, digit) > gutter_width(99, digit));
        assert!((gutter_width(10, digit) - gutter_width(99, digit)).abs() < f32::EPSILON);
    }

    #[test]
    fn the_caret_is_followed_only_when_it_leaves_the_view() {
        let view = 100.0;
        assert!(follow(0.0, 50.0, view).abs() < f32::EPSILON, "in view");
        assert!((follow(0.0, 150.0, view) - (150.0 + CARET_WIDTH - view)).abs() < f32::EPSILON);
        assert!(
            (follow(200.0, 120.0, view) - 120.0).abs() < f32::EPSILON,
            "left of it"
        );
        assert!(
            (follow(30.0, 80.0, view) - 30.0).abs() < f32::EPSILON,
            "kept"
        );
        assert!(follow(500.0, 0.0, view).abs() < f32::EPSILON, "Home");
    }

    #[test]
    fn the_scroll_stays_within_the_text() {
        assert!((clamp(50.0, 300.0, 100.0) - 50.0).abs() < f32::EPSILON);
        assert!((clamp(250.0, 300.0, 100.0) - 200.0).abs() < f32::EPSILON);
        assert!(clamp(-5.0, 300.0, 100.0).abs() < f32::EPSILON);
        assert!(
            clamp(40.0, 80.0, 100.0).abs() < f32::EPSILON,
            "narrower than the view"
        );
    }

    #[test]
    fn the_handle_and_the_scroll_match() {
        let rail = Rectangle::new(Point::new(10.0, 0.0), Size::new(200.0, 10.0));
        let start = scroller(rail, 0.0, 400.0, 100.0);
        assert!((start.x - rail.x).abs() < f32::EPSILON);
        assert!((start.width - 50.0).abs() < f32::EPSILON, "a quarter shown");
        let end = scroller(rail, 300.0, 400.0, 100.0);
        assert!((end.x + end.width - (rail.x + rail.width)).abs() < f32::EPSILON);
        let middle = scroller(rail, 150.0, 400.0, 100.0);
        let back = scroll_at(rail, middle.width, middle.x, 400.0, 100.0);
        assert!((back - 150.0).abs() < 0.01);
        assert_eq!(scroller(rail, 0.0, 50.0, 100.0), rail, "nothing to scroll");
    }

    #[test]
    fn the_gutter_text_and_scrollbar_share_the_editor() {
        let bounds = Rectangle::new(Point::new(0.0, 0.0), Size::new(400.0, 300.0));
        let parts = regions(bounds, 30.0, true);
        assert!((parts.text.x - (PADDING + 30.0)).abs() < f32::EPSILON);
        assert!((parts.gutter.x + parts.gutter.width - parts.text.x).abs() < f32::EPSILON);
        let bar = parts.bar.expect("a scrollbar");
        assert!((bar.height - SCROLLBAR_HEIGHT).abs() < f32::EPSILON);
        assert!((bar.y - (parts.text.y + parts.text.height)).abs() < f32::EPSILON);
        assert!(regions(bounds, 30.0, false).bar.is_none());
    }
}
