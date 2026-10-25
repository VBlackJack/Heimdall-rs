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

//! A column whose items are all as wide as the widest, or as a given minimum.
//!
//! The session tree scrolls sideways when a name is wider than the sidebar (an intentional
//! divergence from the C# tree, which turned its horizontal bar off in `0f5cd2f1`). Its rows
//! fill their width, so they must share one: the widest row's, or the sidebar's when none is
//! wider, so that a selected row is lit across the whole scrolled width. An iced column in a
//! sideways scroll gives a row that fills no more than what the rows before it measured.
//!
//! Each item is measured as its content takes it, then laid out again at the common width.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Shell, Widget, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector, mouse};

/// Items one under the other, `spacing` apart, all as wide as the widest or `min_width`.
pub struct EvenWidth<'a, M> {
    spacing: f32,
    min_width: f32,
    bar_height: f32,
    items: Vec<Element<'a, M, Theme, iced::Renderer>>,
}

impl<'a, M> EvenWidth<'a, M> {
    /// `items` in a column, `spacing` apart, at least `min_width` wide. When they are wider
    /// than that, a sideways scroll bar of `bar_height` is drawn over the foot of the column:
    /// the column is that much higher, so the last item stays clear of the bar.
    #[must_use]
    pub fn new(
        spacing: f32,
        min_width: f32,
        bar_height: f32,
        items: Vec<Element<'a, M, Theme, iced::Renderer>>,
    ) -> Self {
        Self {
            spacing,
            min_width,
            bar_height,
            items,
        }
    }
}

/// The height added under the items: `bar_height` when they are wider than `min_width`, so
/// that a sideways bar is shown, else nothing.
#[must_use]
pub fn foot(width: f32, min_width: f32, bar_height: f32) -> f32 {
    if width > min_width { bar_height } else { 0.0 }
}

/// The width every item is given: the widest measured, but not under `min_width`.
#[must_use]
pub fn common_width(measured: impl IntoIterator<Item = f32>, min_width: f32) -> f32 {
    measured.into_iter().fold(min_width, f32::max)
}

impl<M> Widget<M, Theme, iced::Renderer> for EvenWidth<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn children(&self) -> Vec<Tree> {
        self.items.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.items);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        // Measured as their content takes them: an item that fills its width shrinks to it.
        let measure =
            layout::Limits::with_compression(Size::ZERO, Size::INFINITE, Size::new(true, false));
        let measured: Vec<f32> = self
            .items
            .iter_mut()
            .zip(tree.children.iter_mut())
            .map(|(item, tree)| {
                item.as_widget_mut()
                    .layout(tree, renderer, &measure)
                    .size()
                    .width
            })
            .collect();
        let width = common_width(measured, self.min_width);
        let exact = layout::Limits::new(Size::new(width, 0.0), Size::new(width, f32::INFINITY));
        let mut nodes = Vec::with_capacity(self.items.len());
        let mut y = 0.0_f32;
        for (index, (item, tree)) in self
            .items
            .iter_mut()
            .zip(tree.children.iter_mut())
            .enumerate()
        {
            if index > 0 {
                y += self.spacing;
            }
            let node = item
                .as_widget_mut()
                .layout(tree, renderer, &exact)
                .move_to(Point::new(0.0, y));
            y += node.size().height;
            nodes.push(node);
        }
        y += foot(width, self.min_width, self.bar_height);
        layout::Node::with_children(Size::new(width, y), nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((item, tree), layout) in self
                .items
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                item.as_widget_mut()
                    .operate(tree, layout, renderer, operation);
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
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        for ((item, tree), layout) in self
            .items
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            item.as_widget_mut().update(
                tree, event, layout, cursor, renderer, clipboard, shell, viewport,
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
        self.items
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((item, tree), layout)| {
                item.as_widget()
                    .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        for ((item, tree), layout) in self.items.iter().zip(&tree.children).zip(layout.children()) {
            item.as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, iced::Renderer>> {
        overlay::from_children(
            &mut self.items,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, M: 'a> From<EvenWidth<'a, M>> for Element<'a, M, Theme, iced::Renderer> {
    fn from(column: EvenWidth<'a, M>) -> Self {
        Element::new(column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_is_as_wide_as_the_widest_when_it_passes_the_sidebar() {
        assert!((common_width([120.0, 340.0, 90.0], 260.0) - 340.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_foot_is_kept_only_while_the_sideways_bar_is_shown() {
        assert!((foot(340.0, 260.0, 12.0) - 12.0).abs() < f32::EPSILON);
        assert!(foot(260.0, 260.0, 12.0).abs() < f32::EPSILON);
    }

    #[test]
    fn rows_narrower_than_the_sidebar_fill_it_so_no_bar_shows() {
        assert!((common_width([120.0, 200.0], 260.0) - 260.0).abs() < f32::EPSILON);
        assert!((common_width([], 260.0) - 260.0).abs() < f32::EPSILON);
    }
}
