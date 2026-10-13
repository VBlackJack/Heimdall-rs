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

//! A dialog that keeps its place while it grows, as a WPF window with `SizeToContent`.
//!
//! A dialog centred anew each time it is laid out jumped under the pointer whenever it grew:
//! a validation line appearing raised the whole card by half its height. The C# dialogs are
//! windows opened centred on their owner, which then keep their top-left corner and grow
//! downward. [`Anchored`] centres its dialog the first time it lays it out, then places it
//! by [`place`]: where it was, moved back only as far as it must to stay inside the window.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

/// Where a dialog was placed, and in a window of which size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Its top-left corner.
    pub at: Point,
    /// The window it was placed in.
    pub window: Size,
}

/// Where a dialog of `size` goes in `window`, placed at `previous` the last time it was laid
/// out: centred the first time, and again once the window has another size; else at its
/// place, as a WPF window keeps its top-left corner while it grows, moved back only as far as
/// it must to stay inside the window, and never before its origin.
#[must_use]
pub fn place(previous: Option<Placement>, size: Size, window: Size) -> Placement {
    let at = match previous {
        Some(previous) if previous.window == window => Point::new(
            keep_axis(previous.at.x, size.width, window.width),
            keep_axis(previous.at.y, size.height, window.height),
        ),
        _ => Point::new(
            ((window.width - size.width) / 2.0).max(0.0),
            ((window.height - size.height) / 2.0).max(0.0),
        ),
    };
    Placement { at, window }
}

/// [`place`] along one axis for a dialog placed already at `at`.
fn keep_axis(at: f32, length: f32, bound: f32) -> f32 {
    at.min(bound - length).max(0.0)
}

/// What [`Anchored`] remembers between two layouts: which dialog it placed, and where.
struct State<K> {
    placed: Option<(K, Placement)>,
}

/// `dialog` placed in the area the widget fills; see the module.
pub struct Anchored<'a, Message, K> {
    dialog: Element<'a, Message>,
    key: K,
}

/// `dialog` centred when it opens, then kept at its place while it grows; `key` tells one
/// dialog from another: a dialog of another key is centred anew.
#[must_use]
pub fn anchored<'a, Message, K>(
    dialog: impl Into<Element<'a, Message>>,
    key: K,
) -> Anchored<'a, Message, K> {
    Anchored {
        dialog: dialog.into(),
        key,
    }
}

impl<Message, K> Widget<Message, Theme, iced::Renderer> for Anchored<'_, Message, K>
where
    K: Copy + PartialEq + 'static,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<K>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<K> { placed: None })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.dialog)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.dialog));
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
        // The room a centring container gives its content: the whole area.
        let node =
            self.dialog
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, &limits.loose());
        let window = limits.resolve(Length::Fill, Length::Fill, node.size());
        let state = tree.state.downcast_mut::<State<K>>();
        // Another dialog is centred anew, as a new WPF window is.
        let previous = state
            .placed
            .filter(|(key, _)| *key == self.key)
            .map(|(_, placement)| placement);
        let placement = place(previous, node.size(), window);
        state.placed = Some((self.key, placement));
        layout::Node::with_children(window, vec![node.move_to(placement.at)])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some(dialog) = layout.children().next() {
            self.dialog
                .as_widget_mut()
                .operate(&mut tree.children[0], dialog, renderer, operation);
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
        if let Some(dialog) = layout.children().next() {
            self.dialog.as_widget_mut().update(
                &mut tree.children[0],
                event,
                dialog,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
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
            .map_or(mouse::Interaction::None, |dialog| {
                self.dialog.as_widget().mouse_interaction(
                    &tree.children[0],
                    dialog,
                    cursor,
                    viewport,
                    renderer,
                )
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
        if let Some(dialog) = layout.children().next() {
            self.dialog.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                dialog,
                cursor,
                viewport,
            );
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        let dialog = layout.children().next()?;
        self.dialog.as_widget_mut().overlay(
            &mut tree.children[0],
            dialog,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message: 'a, K> From<Anchored<'a, Message, K>> for Element<'a, Message>
where
    K: Copy + PartialEq + 'static,
{
    fn from(anchored: Anchored<'a, Message, K>) -> Self {
        Element::new(anchored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Size = Size::new(1000.0, 700.0);
    const DIALOG: Size = Size::new(400.0, 300.0);

    #[test]
    fn a_dialog_opens_centred() {
        assert_eq!(
            place(None, DIALOG, WINDOW),
            Placement {
                at: Point::new(300.0, 200.0),
                window: WINDOW,
            }
        );
    }

    #[test]
    fn a_dialog_that_grows_keeps_its_top() {
        let opened = place(None, DIALOG, WINDOW);
        // A validation line under its fields: 24 px taller, its top where it was.
        let grown = place(Some(opened), Size::new(400.0, 324.0), WINDOW);
        assert_eq!(grown.at, Point::new(300.0, 200.0));
        // And the line gone, it shrinks back from the foot, as a WPF window.
        let shrunk = place(Some(grown), DIALOG, WINDOW);
        assert_eq!(shrunk.at, Point::new(300.0, 200.0));
    }

    #[test]
    fn a_dialog_growing_past_the_foot_moves_up_just_enough() {
        let opened = place(None, DIALOG, WINDOW);
        // 560 high from 200 ends at 760: 60 past a window of 700.
        let grown = place(Some(opened), Size::new(400.0, 560.0), WINDOW);
        assert_eq!(grown.at, Point::new(300.0, 140.0));
        // Wider than the room right of it: back just enough on that axis too.
        let wider = place(Some(grown), Size::new(800.0, 560.0), WINDOW);
        assert_eq!(wider.at, Point::new(200.0, 140.0));
    }

    #[test]
    fn a_dialog_taller_than_the_window_starts_at_its_top() {
        let opened = place(None, DIALOG, WINDOW);
        let tall = place(Some(opened), Size::new(400.0, 900.0), WINDOW);
        assert_eq!(tall.at, Point::new(300.0, 0.0));
        assert_eq!(
            place(None, Size::new(1200.0, 900.0), WINDOW).at,
            Point::ORIGIN
        );
    }

    #[test]
    fn a_window_resized_centres_the_dialog_again() {
        let opened = place(None, DIALOG, WINDOW);
        let grown = place(Some(opened), Size::new(400.0, 324.0), WINDOW);
        let resized = Size::new(1200.0, 900.0);
        assert_eq!(
            place(Some(grown), Size::new(400.0, 324.0), resized),
            Placement {
                at: Point::new(400.0, 288.0),
                window: resized,
            }
        );
    }
}
