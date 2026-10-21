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

//! Cards in rows of equal cells, as the C# `UniformGrid`: every cell is as wide as the
//! others, and as high as the highest of its row.
//!
//! A row of the toolkit cannot do it: cells that fill their height take none when nothing in
//! the row has a height of its own to share, and a cell that shrinks keeps its own. The grid
//! measures each cell at its width, in the height its content takes, then lays the cells of
//! the row out again at the row's height, so a cell that fills its height fills the row.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Shell, Widget, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector, mouse};

/// Cells in `columns` columns, `spacing` apart, the cells of a row as high as the highest.
pub struct EvenGrid<'a, M> {
    columns: usize,
    spacing: f32,
    cells: Vec<Element<'a, M, Theme, iced::Renderer>>,
}

impl<'a, M> EvenGrid<'a, M> {
    /// A grid of `columns` columns (at least one) holding `cells`, row after row.
    #[must_use]
    pub fn new(
        columns: usize,
        spacing: f32,
        cells: Vec<Element<'a, M, Theme, iced::Renderer>>,
    ) -> Self {
        Self {
            columns: columns.max(1),
            spacing,
            cells,
        }
    }
}

/// A count of cells as a length: a grid never holds more than `u16::MAX` of them.
fn count(cells: usize) -> f32 {
    f32::from(u16::try_from(cells).unwrap_or(u16::MAX))
}

/// The width of one cell of a grid of `columns` columns in `available`.
#[must_use]
pub fn cell_width(available: f32, columns: usize, spacing: f32) -> f32 {
    let gaps = spacing * count(columns.saturating_sub(1));
    ((available - gaps) / count(columns.max(1))).max(0.0)
}

impl<M> Widget<M, Theme, iced::Renderer> for EvenGrid<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn children(&self) -> Vec<Tree> {
        self.cells.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.cells);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let available = limits.max();
        let width = cell_width(available.width, self.columns, self.spacing);
        // Measured as their content takes them: a cell that fills its height shrinks to it
        // under compression.
        let measure = layout::Limits::with_compression(
            Size::ZERO,
            Size::new(width, available.height),
            Size::new(false, true),
        );
        let mut nodes = Vec::with_capacity(self.cells.len());
        let mut y = 0.0_f32;
        let rows = self
            .cells
            .chunks_mut(self.columns)
            .zip(tree.children.chunks_mut(self.columns));
        for (row, (cells, trees)) in rows.enumerate() {
            if row > 0 {
                y += self.spacing;
            }
            let height = cells
                .iter_mut()
                .zip(trees.iter_mut())
                .map(|(cell, tree)| {
                    cell.as_widget_mut()
                        .layout(tree, renderer, &measure)
                        .size()
                        .height
                })
                .fold(0.0_f32, f32::max);
            let exact = layout::Limits::new(Size::new(width, height), Size::new(width, height));
            for (column, (cell, tree)) in cells.iter_mut().zip(trees.iter_mut()).enumerate() {
                let x = count(column) * (width + self.spacing);
                nodes.push(
                    cell.as_widget_mut()
                        .layout(tree, renderer, &exact)
                        .move_to(Point::new(x, y)),
                );
            }
            y += height;
        }
        let size = limits.resolve(Length::Fill, Length::Shrink, Size::new(available.width, y));
        layout::Node::with_children(size, nodes)
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
            for ((cell, tree), layout) in self
                .cells
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                cell.as_widget_mut()
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
        for ((cell, tree), layout) in self
            .cells
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            cell.as_widget_mut().update(
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
        self.cells
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((cell, tree), layout)| {
                cell.as_widget()
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
        for ((cell, tree), layout) in self.cells.iter().zip(&tree.children).zip(layout.children()) {
            cell.as_widget()
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
            &mut self.cells,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, M: 'a> From<EvenGrid<'a, M>> for Element<'a, M, Theme, iced::Renderer> {
    fn from(grid: EvenGrid<'a, M>) -> Self {
        Element::new(grid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_is_a_column_of_the_width_less_the_gaps() {
        assert!((cell_width(1000.0, 4, 8.0) - 244.0).abs() < f32::EPSILON);
        assert!((cell_width(100.0, 1, 8.0) - 100.0).abs() < f32::EPSILON);
        assert!(
            cell_width(10.0, 4, 8.0).abs() < f32::EPSILON,
            "never negative"
        );
    }
}
