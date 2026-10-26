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

//! A button whose menu opens under it, as the C# `ContextMenu` of a toolbar button does
//! (`Placement` bottom of its `PlacementTarget`), not at the pointer, where it would cover
//! the button it belongs to.
//!
//! [`below`] wraps the button: a left press on it publishes [`Message::OpenTreeMenuBelow`]
//! with the button's bounds, whose bottom-left corner the menu opens at and which [`crate::menu_fit`] then fits to the window.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

use crate::shell::Message;
use crate::tree_view::TreeMenu;

/// `content`, a button, opening `menu` under itself when pressed.
#[must_use]
pub fn below<'a>(content: impl Into<Element<'a, Message>>, menu: TreeMenu) -> Element<'a, Message> {
    Element::new(Below {
        content: content.into(),
        menu,
    })
}

/// Where a menu opened by a button of `bounds` goes: its bottom-left corner.
#[must_use]
pub fn under(bounds: Rectangle) -> Point {
    Point::new(bounds.x, bounds.y + bounds.height)
}

/// A button and the menu that opens under it.
struct Below<'a> {
    content: Element<'a, Message>,
    menu: TreeMenu,
}

impl Widget<Message, Theme, iced::Renderer> for Below<'_> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && cursor.is_over(layout.bounds())
        {
            shell.publish(Message::OpenTreeMenuBelow(
                self.menu.clone(),
                layout.bounds(),
            ));
            shell.capture_event();
            return;
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
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
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_opens_at_the_bottom_left_corner_of_its_button() {
        let button = Rectangle::new(Point::new(200.0, 120.0), Size::new(32.0, 32.0));
        assert_eq!(under(button), Point::new(200.0, 152.0));
    }
}
