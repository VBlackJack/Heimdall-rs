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

//! A tooltip that a press puts away, as a WPF tooltip closes when its control is clicked.
//!
//! An iced 0.14 tooltip stays open for as long as the pointer is over its content, a press
//! or not: over a tree row it hid the row's neighbours after a click, and over the search
//! box it hid the filters under it while the user typed. Wrapped in [`PressHides`], it is
//! put away by a press of any button over it, and shown again only once the pointer has
//! left and come back.
//!
//! What the content draws above the window of its own, a menu say, goes with the tooltip:
//! wrap content that has none.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay, renderer};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector, touch};

/// `tooltip`, put away by a press over it; see the module.
pub struct PressHides<'a, Message> {
    tooltip: Element<'a, Message>,
}

/// `tooltip` put away by a press over it until the pointer leaves it.
#[must_use]
pub fn hide_on_press<'a, Message>(
    tooltip: impl Into<Element<'a, Message>>,
) -> PressHides<'a, Message> {
    PressHides {
        tooltip: tooltip.into(),
    }
}

/// Whether a press put the tooltip away.
#[derive(Debug, Default)]
struct Pressed(bool);

impl<Message> Widget<Message, Theme, iced::Renderer> for PressHides<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Pressed>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Pressed::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.tooltip)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.tooltip));
    }

    fn size(&self) -> Size<Length> {
        self.tooltip.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.tooltip.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.tooltip
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
        self.tooltip
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
        let over = cursor.is_over(layout.bounds());
        let pressed = tree.state.downcast_mut::<Pressed>();
        let press = matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(_))
                | Event::Touch(touch::Event::FingerPressed { .. })
        );
        if press && over && !pressed.0 {
            pressed.0 = true;
            shell.invalidate_layout();
            shell.request_redraw();
        } else if !over && pressed.0 {
            // Left: the next time the pointer comes, the tooltip comes with it.
            pressed.0 = false;
        }
        self.tooltip.as_widget_mut().update(
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
        self.tooltip.as_widget().mouse_interaction(
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
        self.tooltip.as_widget().draw(
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
        if tree.state.downcast_ref::<Pressed>().0 {
            return None;
        }
        self.tooltip.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message: 'a> From<PressHides<'a, Message>> for Element<'a, Message> {
    fn from(hides: PressHides<'a, Message>) -> Self {
        Element::new(hides)
    }
}
