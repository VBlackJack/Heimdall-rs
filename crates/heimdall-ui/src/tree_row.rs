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

//! What the C# tree draws behind a row: the highlight under the pointer, the accent's tint
//! and edge on a session selected, the focus colour's edge on the folder the keyboard is on,
//! and the indent guide of each open folder the row is in.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::Widget;
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, mouse, overlay, renderer};
use iced::{Color, Element, Event, Length, Point, Rectangle, Size, Theme, Vector, window};

use crate::shell::Message;

/// Width of the edge at the left of a row marked, as the C# rows' left border.
pub const EDGE_WIDTH: f32 = 3.0;

/// Opacity of the accent behind a session selected, as the C# `TreeRowSelectedBrush`.
const SELECTED_ALPHA: f32 = 0.18;

/// Width of an open folder's indent guide, as the C# `IndentGuide` border.
pub const GUIDE_WIDTH: f32 = 1.0;

/// Opacity of the guide, as the C# `TreeViewIndentGuideBrush`: the border's colour at 35%.
const GUIDE_ALPHA: f32 = 0.35;

/// How a row is marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Not at all.
    None,
    /// A session selected: the accent, faint behind it and full at its edge.
    Selected,
    /// The folder the keyboard is on: the highlight behind it, the focus colour at its edge.
    Cursor,
}

/// `content` with the C# row's background.
pub struct RowChrome<'a> {
    content: Element<'a, Message>,
    mark: Mark,
}

impl<'a> RowChrome<'a> {
    /// `content` marked `mark`.
    pub fn new(content: impl Into<Element<'a, Message>>, mark: Mark) -> Self {
        Self {
            content: content.into(),
            mark,
        }
    }
}

/// Whether the pointer was over the row when it was last drawn.
#[derive(Debug, Default)]
struct Hover {
    drawn: Option<bool>,
}

/// `bounds` filled with `color`.
fn fill(renderer: &mut iced::Renderer, bounds: Rectangle, color: Color) {
    use iced::advanced::Renderer as _;
    renderer.fill_quad(
        renderer::Quad {
            bounds,
            ..renderer::Quad::default()
        },
        color,
    );
}

impl Widget<Message, Theme, iced::Renderer> for RowChrome<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Hover>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Hover::default())
    }

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
        // Drawn again when the pointer comes or goes, as a button is.
        let over = cursor.is_over(layout.bounds());
        let hover = tree.state.downcast_mut::<Hover>();
        if let Event::Window(window::Event::RedrawRequested(_)) = event {
            hover.drawn = Some(over);
        } else if hover.drawn.is_some_and(|drawn| drawn != over) {
            shell.request_redraw();
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
        let bounds = layout.bounds();
        let palette = theme.extended_palette();
        // The C# `HighlightBrush`, under the pointer and on the keyboard's folder.
        if cursor.is_over(bounds) || self.mark == Mark::Cursor {
            fill(renderer, bounds, palette.background.weak.color);
        }
        let edge = Rectangle::new(
            Point::new(bounds.x, bounds.y),
            Size::new(EDGE_WIDTH, bounds.height),
        );
        match self.mark {
            Mark::None => {}
            Mark::Selected => {
                let accent = palette.primary.base.color;
                fill(renderer, bounds, accent.scale_alpha(SELECTED_ALPHA));
                fill(renderer, edge, accent);
            }
            // The C# `FocusIndicatorBrush`.
            Mark::Cursor => fill(renderer, edge, crate::themes::colors_of(theme).cyan),
        }
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

impl<'a> From<RowChrome<'a>> for Element<'a, Message> {
    fn from(chrome: RowChrome<'a>) -> Self {
        Element::new(chrome)
    }
}

/// `content` with the indent guides of the open folders it is in: a vertical line down its
/// left side for each, as the C# tree draws one beside every open folder's items.
pub struct Guided<'a> {
    content: Element<'a, Message>,
    /// Where each guide is, from the row's left edge.
    offsets: Vec<f32>,
    /// How far above the row each guide starts: the gap to the row before, so that the
    /// guides of rows one under the other join.
    reach: f32,
}

impl<'a> Guided<'a> {
    /// `content` with a guide at each of `offsets`, reaching `reach` above it.
    pub fn new(content: impl Into<Element<'a, Message>>, offsets: Vec<f32>, reach: f32) -> Self {
        Self {
            content: content.into(),
            offsets,
            reach,
        }
    }
}

/// Where the guides of a row laid out at `bounds` are drawn: one at each of `offsets` from
/// its left edge, from `reach` above it to its bottom.
#[must_use]
pub fn guide_bounds(bounds: Rectangle, offsets: &[f32], reach: f32) -> Vec<Rectangle> {
    offsets
        .iter()
        .map(|offset| {
            Rectangle::new(
                Point::new(bounds.x + offset, bounds.y - reach),
                Size::new(GUIDE_WIDTH, bounds.height + reach),
            )
        })
        .collect()
}

impl Widget<Message, Theme, iced::Renderer> for Guided<'_> {
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
        // The C# `BorderColor`, faded.
        let color = theme
            .extended_palette()
            .background
            .strong
            .color
            .scale_alpha(GUIDE_ALPHA);
        for guide in guide_bounds(layout.bounds(), &self.offsets, self.reach) {
            fill(renderer, guide, color);
        }
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

impl<'a> From<Guided<'a>> for Element<'a, Message> {
    fn from(guided: Guided<'a>) -> Self {
        Element::new(guided)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guide_runs_down_the_whole_row_and_the_gap_above_it_to_join_the_one_before() {
        let row = Rectangle::new(Point::new(10.0, 100.0), Size::new(200.0, 24.0));
        let guides = guide_bounds(row, &[15.0, 31.0], 2.0);
        assert_eq!(
            guides,
            vec![
                Rectangle::new(Point::new(25.0, 98.0), Size::new(GUIDE_WIDTH, 26.0)),
                Rectangle::new(Point::new(41.0, 98.0), Size::new(GUIDE_WIDTH, 26.0)),
            ]
        );
        assert!(guide_bounds(row, &[], 2.0).is_empty(), "a row in no folder");
    }
}
