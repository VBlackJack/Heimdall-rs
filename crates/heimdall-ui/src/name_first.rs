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

//! A name and the marks that trail it on one line, the name given its width first, as the C#
//! `NameFirstPanel` (`Controls/NameFirstPanel.cs`).
//!
//! The name is laid out against the whole width and keeps what it needs; the marks get what
//! is left, right-aligned, and end with an ellipsis inside it ([`crate::ellipsis`]). When less
//! than the minimum is left, the marks are not laid out, drawn or reached at all: a badge cut
//! down to a sliver says nothing, and the row's tooltip carries its full text.
//!
//! An iced row does the opposite: the items that shrink to their text take their whole width
//! first, and in a narrow sidebar a long "via gateway" badge left the name no width at all.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

/// Added to a name measured in an unbounded width, so that the width it is given back
/// is never a rounding short of it.
const NAME_SLACK: f32 = 1.0;

/// Whether the last layout left the trailing marks enough room to be shown.
struct Shown(bool);

/// What a name of `name_desired` width leaves the marks in `available` width: the name's
/// width, then the room for the marks, or `None` when less than `min_trailing` is left.
/// In an unbounded width (a row measured to find the widest name of a tree that scrolls
/// sideways) the marks are left out: the name alone sets how wide the tree is.
#[must_use]
pub fn split(available: f32, name_desired: f32, min_trailing: f32) -> (f32, Option<f32>) {
    let name = name_desired.min(available);
    if available.is_infinite() {
        return (name, None);
    }
    let remaining = (available - name).max(0.0);
    (name, (remaining >= min_trailing).then_some(remaining))
}

/// A name and trailing marks; see the module.
pub struct NameFirst<'a, Message> {
    name: Element<'a, Message>,
    trailing: Option<Element<'a, Message>>,
    min_trailing: f32,
    width: Length,
}

impl<'a, Message> NameFirst<'a, Message> {
    /// `name`, then `trailing` marks if any, shown only when at least `min_trailing` is left.
    /// The widget is as wide as its content; see [`Self::width`].
    #[must_use]
    pub fn new(
        name: impl Into<Element<'a, Message>>,
        trailing: Option<Element<'a, Message>>,
        min_trailing: f32,
    ) -> Self {
        Self {
            name: name.into(),
            trailing,
            min_trailing,
            width: Length::Shrink,
        }
    }

    /// The widget's width: as wide as its content ([`Length::Shrink`], the default), or the
    /// whole of its room with the marks at its right end ([`Length::Fill`]).
    #[must_use]
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// The name, then the marks when they are there and shown.
    fn parts(&mut self, shown: bool) -> impl Iterator<Item = &mut Element<'a, Message>> {
        std::iter::once(&mut self.name).chain(self.trailing.as_mut().filter(|_| shown))
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for NameFirst<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Shown>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Shown(true))
    }

    fn children(&self) -> Vec<Tree> {
        std::iter::once(&self.name)
            .chain(self.trailing.as_ref())
            .map(Tree::new)
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let contents: Vec<&Element<'_, Message>> = std::iter::once(&self.name)
            .chain(self.trailing.as_ref())
            .collect();
        tree.diff_children(&contents);
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let max = limits.max();
        let name = self.name.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(Size::ZERO, max),
        );
        let (name_width, room) = split(max.width, name.size().width, self.min_trailing);
        let mut trailing = layout::Node::default();
        if let (Some(element), Some(room)) = (self.trailing.as_mut(), room) {
            trailing = element.as_widget_mut().layout(
                &mut tree.children[1],
                renderer,
                &layout::Limits::new(Size::ZERO, Size::new(room, max.height)),
            );
        }
        tree.state.downcast_mut::<Shown>().0 = room.is_some();
        let trailing_width = trailing.size().width;
        let height = name.size().height.max(trailing.size().height);
        // A rounding of the measure must not cut a name that was measured whole.
        let slack = if max.width.is_infinite() {
            NAME_SLACK
        } else {
            0.0
        };
        let used = Size::new(name_width + slack + trailing_width, height);
        let size = limits.resolve(self.width, Length::Shrink, used);
        let name_at = Point::new(0.0, (height - name.size().height) / 2.0);
        let mut nodes = vec![name.move_to(name_at)];
        if self.trailing.is_some() {
            let at = Point::new(
                size.width - trailing_width,
                (height - trailing.size().height) / 2.0,
            );
            nodes.push(trailing.move_to(at));
        }
        layout::Node::with_children(size, nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        let shown = tree.state.downcast_ref::<Shown>().0;
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((element, state), layout) in self
                .parts(shown)
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                element
                    .as_widget_mut()
                    .operate(state, layout, renderer, operation);
            }
        });
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
        let shown = tree.state.downcast_ref::<Shown>().0;
        for ((element, state), layout) in self
            .parts(shown)
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            element.as_widget_mut().update(
                state, event, layout, cursor, renderer, clipboard, shell, viewport,
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
        let shown = tree.state.downcast_ref::<Shown>().0;
        std::iter::once(&self.name)
            .chain(self.trailing.as_ref().filter(|_| shown))
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((element, state), layout)| {
                element
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
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
        let shown = tree.state.downcast_ref::<Shown>().0;
        for ((element, state), layout) in std::iter::once(&self.name)
            .chain(self.trailing.as_ref().filter(|_| shown))
            .zip(&tree.children)
            .zip(layout.children())
        {
            element
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport);
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
        let shown = tree.state.downcast_ref::<Shown>().0;
        let children = self
            .parts(shown)
            .zip(&mut tree.children)
            .zip(layout.children())
            .filter_map(|((element, state), layout)| {
                element
                    .as_widget_mut()
                    .overlay(state, layout, renderer, viewport, translation)
            })
            .collect::<Vec<_>>();
        (!children.is_empty()).then(|| overlay::Group::with_children(children).overlay())
    }
}

impl<'a, Message: 'a> From<NameFirst<'a, Message>> for Element<'a, Message> {
    fn from(widget: NameFirst<'a, Message>) -> Self {
        Element::new(widget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ellipsis::fit;

    /// The narrowest the marks are shown at in the tests, as the tree's token.
    const MIN: f32 = 56.0;

    /// Widest the badge grows in the tests, as the tree's token.
    const BADGE_MAX: f32 = 140.0;

    /// Width of a character in the measures below.
    const ADVANCE: f32 = 7.0;

    #[allow(
        clippy::cast_precision_loss,
        reason = "test lines are a few characters"
    )]
    fn width_of(line: &str) -> f32 {
        line.chars().count() as f32 * ADVANCE
    }

    /// The name's width and the badge's shown text in a row `available` wide, the marks
    /// beside the badge `marks` wide: the tree's two nested panels.
    fn lay_out(available: f32, name: &str, marks: f32, badge: &str) -> (f32, Option<String>) {
        let (name_width, room) = split(available, width_of(name), MIN);
        let Some(room) = room else {
            return (name_width, None);
        };
        let (_, badge_room) = split(room, marks, MIN);
        let shown = badge_room.map(|room| fit(badge, room.min(BADGE_MAX), width_of).into_owned());
        (name_width, shown)
    }

    #[test]
    fn the_name_keeps_what_it_needs_while_it_fits_the_row() {
        let name = "Test Server 01";
        for available in [160.0, 200.0, 260.0, 400.0] {
            let (name_width, _) = lay_out(available, name, 0.0, "via Heimdall TestEnv Gateway");
            assert!(
                (name_width - width_of(name)).abs() < f32::EPSILON,
                "{available}"
            );
        }
    }

    #[test]
    fn a_name_wider_than_the_row_takes_it_all_and_the_marks_go() {
        let (name_width, badge) = lay_out(120.0, &"n".repeat(40), 0.0, "via Gateway");
        assert!((name_width - 120.0).abs() < f32::EPSILON);
        assert_eq!(badge, None);
    }

    #[test]
    fn a_long_badge_ends_with_an_ellipsis_in_what_the_name_leaves() {
        let name = "Test Server 01";
        let badge = "via Heimdall TestEnv Gateway";
        let (_, shown) = lay_out(200.0, name, 0.0, badge);
        let shown = shown.expect("room for the badge");
        assert!(shown.ends_with("..."), "{shown}");
        assert!(width_of(&shown) <= 200.0 - width_of(name));
        // Wide enough for all of it, it is whole.
        let (_, whole) = lay_out(400.0, name, 0.0, "via Gateway");
        assert_eq!(whole.as_deref(), Some("via Gateway"));
        // Never past the token's maximum, whatever the room.
        let (_, capped) = lay_out(600.0, name, 0.0, &"g".repeat(40));
        assert!(width_of(&capped.expect("room")) <= BADGE_MAX);
    }

    #[test]
    fn below_the_minimum_the_badge_is_not_shown_at_all() {
        let name = "Test Server 01";
        let name_width = width_of(name);
        let badge = "via Heimdall TestEnv Gateway";
        let (_, at_min) = lay_out(name_width + MIN, name, 0.0, badge);
        assert!(at_min.is_some());
        let (_, below) = lay_out(name_width + MIN - 1.0, name, 0.0, badge);
        assert_eq!(below, None);
    }

    #[test]
    fn the_marks_beside_the_badge_are_served_before_it() {
        let name = "Test Server 01";
        // 40 of marks leave the badge under the minimum in a room of 90.
        let (_, shown) = lay_out(width_of(name) + 90.0, name, 40.0, "via Gateway");
        assert_eq!(shown, None);
        let (_, shown) = lay_out(width_of(name) + 120.0, name, 40.0, "via Gateway");
        assert!(shown.is_some());
    }

    #[test]
    fn an_unbounded_width_measures_the_name_alone() {
        let (name, room) = split(f32::INFINITY, 80.0, MIN);
        assert!((name - 80.0).abs() < f32::EPSILON);
        assert_eq!(room, None);
    }
}
