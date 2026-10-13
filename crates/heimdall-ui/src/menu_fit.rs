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

//! A menu at the pointer kept whole and inside its window, as a WPF `ContextMenu`.
//!
//! An iced 0.14 `pin` lays its content out in the room left right of and below its point:
//! a menu opened near the window's right edge was squeezed until its labels wrapped, and
//! one opened near its foot ran past it, its last entries out of reach. [`MenuFit`] lays
//! the menu out with the whole window as its room, then places it by [`fit`].
//!
//! A WPF menu is also as wide as its longest entry. Entries filling their menu's width have
//! none of their own to give, and an iced 0.14 column as wide as its content collapses them;
//! [`MenuFit`] lays the menu out in two widths of room, [`PROBE`] apart, tells from them what
//! fills its room and what keeps its own width, reads from that how wide the content is
//! ([`natural_width`]), then lays the menu out once more as wide as [`menu_width`] says.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

/// The narrowest a menu is: the width every menu had before menus took their entries'.
pub const MIN_WIDTH: f32 = 270.0;

/// How much wider the second room [`MenuFit`] lays a menu out in is: what fills its room
/// grows by exactly this much, what keeps its own width does not.
const PROBE: f32 = 64.0;

/// How far from [`PROBE`] a growth still counts as filling the room, for rounding.
const PROBE_TOLERANCE: f32 = 0.5;

/// `menu` at `anchor`, the pointer, fitted to the area it fills; see the module.
pub struct MenuFit<'a, Message> {
    menu: Element<'a, Message>,
    anchor: Point,
}

/// `menu` opened at `anchor`, kept whole and inside the area the widget fills.
#[must_use]
pub fn menu_at<'a, Message>(
    menu: impl Into<Element<'a, Message>>,
    anchor: Point,
) -> MenuFit<'a, Message> {
    MenuFit {
        menu: menu.into(),
        anchor,
    }
}

/// Where a menu of `size` opened at `anchor` goes in an area of `bounds` from the origin,
/// as WPF places a `ContextMenu`: right of and below the pointer when it fits; else left
/// of it (its right edge at the pointer) or above it (its foot at the pointer); and when
/// it fits neither way, shifted to stay inside, its left or top edge never before 0.
#[must_use]
pub fn fit(anchor: Point, size: Size, bounds: Size) -> Point {
    Point::new(
        fit_axis(anchor.x, size.width, bounds.width),
        fit_axis(anchor.y, size.height, bounds.height),
    )
}

/// How wide a menu whose content is `natural` wide is drawn in an area `room` wide: as wide
/// as its content, as a WPF menu is as wide as its longest entry, never narrower than
/// [`MIN_WIDTH`] nor wider than the area. Rounded up to a whole pixel: a label laid out in
/// exactly its own measured width could wrap on a rounding.
#[must_use]
pub fn menu_width(natural: f32, room: f32) -> f32 {
    natural.ceil().max(MIN_WIDTH).min(room)
}

/// How wide the content of a menu laid out as `narrow` in some room and as `wide` in a room
/// [`PROBE`] wider is. A node that grew by the probe fills its room: it is as wide as what it
/// holds reaches, plus the margin after it, which is the room a child filling it leaves on
/// its right or, when none fills it, the margin before its first child (a button's padding
/// is the same on both sides); filling and holding nothing, a separator, it gives no width.
/// Any other node keeps its own width. Children are taken where `narrow` placed them, left
/// aligned as menu entries are.
#[must_use]
pub fn natural_width(narrow: &layout::Node, wide: &layout::Node) -> f32 {
    if !fills(narrow, wide) {
        return narrow.size().width;
    }
    let mut reach: f32 = 0.0;
    let mut lead = f32::INFINITY;
    let mut filled: Option<f32> = None;
    for (child, wide_child) in narrow.children().iter().zip(wide.children()) {
        let x = child.bounds().x;
        reach = reach.max(x + natural_width(child, wide_child));
        lead = lead.min(x);
        if fills(child, wide_child) {
            let right = x + child.size().width;
            filled = Some(filled.map_or(right, |found| found.max(right)));
        }
    }
    if lead.is_infinite() {
        return 0.0;
    }
    let trailing = filled.map_or(lead, |right| narrow.size().width - right);
    reach + trailing.max(0.0)
}

/// Whether a node laid out as `narrow`, then as `wide` in a room [`PROBE`] wider, fills it.
fn fills(narrow: &layout::Node, wide: &layout::Node) -> bool {
    (wide.size().width - narrow.size().width - PROBE).abs() < PROBE_TOLERANCE
}

/// [`fit`] along one axis.
fn fit_axis(anchor: f32, length: f32, bound: f32) -> f32 {
    if anchor + length <= bound {
        anchor
    } else if anchor - length >= 0.0 {
        anchor - length
    } else {
        (bound - length).max(0.0)
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for MenuFit<'_, Message> {
    fn tag(&self) -> tree::Tag {
        self.menu.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.menu.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.menu.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.menu.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let limits = limits.width(Length::Fill).height(Length::Fill);
        // The whole area as room, not what is left past the pointer: the menu keeps its width.
        let room = limits.max();
        let mut lay_out = |width: f32| {
            self.menu.as_widget_mut().layout(
                tree,
                renderer,
                &layout::Limits::new(Size::ZERO, Size::new(width, room.height)),
            )
        };
        let node = if room.width.is_finite() {
            // As wide as its longest entry, as a WPF menu; see the module.
            let narrow = lay_out(room.width);
            let wide = lay_out(room.width + PROBE);
            lay_out(menu_width(natural_width(&narrow, &wide), room.width))
        } else {
            lay_out(room.width)
        };
        let size = limits.resolve(Length::Fill, Length::Fill, node.size());
        let at = fit(self.anchor, node.size(), size);
        layout::Node::with_children(size, vec![node.move_to(at)])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some(menu) = layout.children().next() {
            self.menu
                .as_widget_mut()
                .operate(tree, menu, renderer, operation);
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Some(menu) = layout.children().next() {
            self.menu.as_widget_mut().update(
                tree, event, menu, cursor, renderer, clipboard, shell, viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        layout
            .children()
            .next()
            .map_or(mouse::Interaction::None, |menu| {
                self.menu
                    .as_widget()
                    .mouse_interaction(tree, menu, cursor, viewport, renderer)
            })
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let (Some(menu), Some(clipped)) = (
            layout.children().next(),
            layout.bounds().intersection(viewport),
        ) else {
            return;
        };
        self.menu
            .as_widget()
            .draw(tree, renderer, theme, style, menu, cursor, &clipped);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        let menu = layout.children().next()?;
        self.menu
            .as_widget_mut()
            .overlay(tree, menu, renderer, viewport, translation)
    }
}

impl<'a, Message: 'a> From<MenuFit<'a, Message>> for Element<'a, Message> {
    fn from(fitted: MenuFit<'a, Message>) -> Self {
        Element::new(fitted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Size = Size::new(1000.0, 700.0);
    const MENU: Size = Size::new(270.0, 300.0);

    /// Room of a probe: the first layout's width.
    const ROOM: f32 = 1000.0;

    /// A node `width` wide at `x`, holding `children`.
    fn node(x: f32, width: f32, children: Vec<layout::Node>) -> layout::Node {
        layout::Node::with_children(Size::new(width, 20.0), children).move_to(Point::new(x, 0.0))
    }

    /// A menu as the tree's are laid out in `room`: a card with a padding of 4 around a
    /// column filling it, of a button filling it with a label of `label` and a padding of
    /// 8, and of a separator.
    fn menu(room: f32, label: f32) -> layout::Node {
        let column = room - 8.0;
        let button = node(0.0, column, vec![node(8.0, label, vec![])]);
        let separator = node(0.0, column, vec![]);
        node(0.0, room, vec![node(4.0, column, vec![button, separator])])
    }

    #[test]
    fn a_menu_is_as_wide_as_its_longest_entry() {
        // The label 300, its button's padding 8 each side, the card's 4 each side.
        let natural = natural_width(&menu(ROOM, 300.0), &menu(ROOM + PROBE, 300.0));
        assert!((natural - 324.0).abs() < f32::EPSILON, "{natural}");
        assert!((menu_width(natural, ROOM) - 324.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_menu_of_short_entries_keeps_the_width_it_had() {
        let natural = natural_width(&menu(ROOM, 80.0), &menu(ROOM + PROBE, 80.0));
        assert!((natural - 104.0).abs() < f32::EPSILON, "{natural}");
        assert!((menu_width(natural, ROOM) - MIN_WIDTH).abs() < f32::EPSILON);
    }

    #[test]
    fn a_menu_is_never_wider_than_its_window() {
        assert!((menu_width(1400.0, ROOM) - ROOM).abs() < f32::EPSILON);
        // Under the minimum too in a window narrower than it.
        assert!((menu_width(100.0, 200.0) - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_menu_width_is_a_whole_pixel() {
        assert!((menu_width(300.2, ROOM) - 301.0).abs() < f32::EPSILON);
    }

    #[test]
    fn an_entry_keeping_its_own_width_counts_whole() {
        // A header in a card that fills its room: the header is 350 wide whatever the room.
        let laid_out = |room: f32| node(0.0, room, vec![node(4.0, 350.0, vec![])]);
        let natural = natural_width(&laid_out(ROOM), &laid_out(ROOM + PROBE));
        assert!((natural - 358.0).abs() < f32::EPSILON, "{natural}");
    }

    #[test]
    fn a_menu_that_fits_opens_right_of_and_below_the_pointer() {
        assert_eq!(
            fit(Point::new(100.0, 50.0), MENU, WINDOW),
            Point::new(100.0, 50.0)
        );
    }

    #[test]
    fn a_menu_short_of_room_on_the_right_opens_left_of_the_pointer() {
        assert_eq!(
            fit(Point::new(860.0, 50.0), MENU, WINDOW),
            Point::new(590.0, 50.0)
        );
    }

    #[test]
    fn a_menu_short_of_room_below_opens_above_the_pointer() {
        assert_eq!(
            fit(Point::new(100.0, 600.0), MENU, WINDOW),
            Point::new(100.0, 300.0)
        );
    }

    #[test]
    fn a_menu_short_of_room_both_ways_is_shifted_inside() {
        // 270 wide in 400: neither right of 250 nor left of it; its right edge at the window's.
        let small = Size::new(400.0, 350.0);
        assert_eq!(
            fit(Point::new(250.0, 200.0), MENU, small),
            Point::new(130.0, 50.0)
        );
    }

    #[test]
    fn a_menu_larger_than_the_window_starts_at_its_origin() {
        let tiny = Size::new(200.0, 100.0);
        assert_eq!(fit(Point::new(150.0, 60.0), MENU, tiny), Point::ORIGIN);
    }

    #[test]
    fn a_menu_reaching_exactly_an_edge_stays_where_it_was_opened() {
        assert_eq!(
            fit(Point::new(730.0, 400.0), MENU, WINDOW),
            Point::new(730.0, 400.0)
        );
        // One pixel past the edge and it flips; with the pointer as far from the origin as
        // the menu is long, it then starts exactly at 0.
        let edge = Size::new(540.0, 600.0);
        assert_eq!(
            fit(Point::new(270.0, 300.0), MENU, edge),
            Point::new(270.0, 300.0)
        );
        assert_eq!(
            fit(Point::new(271.0, 301.0), MENU, edge),
            Point::new(1.0, 1.0)
        );
        assert_eq!(
            fit(Point::new(270.0, 300.0), MENU, Size::new(539.0, 599.0)),
            Point::ORIGIN
        );
    }
}
