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

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

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
        let node = self.menu.as_widget_mut().layout(
            tree,
            renderer,
            &layout::Limits::new(Size::ZERO, limits.max()),
        );
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
