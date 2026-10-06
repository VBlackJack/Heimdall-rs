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

//! The header row of a list in columns, as the C# `GridView` header: its cells laid out at
//! the widths given, a separator in the gap after each column but the last.
//!
//! A separator is dragged to resize the column on its left, the one on its right giving or
//! taking the room, so the separator stays under the pointer and the row keeps its width;
//! each column keeps its least width. The widths follow live, each move sent; a double
//! click fits the column on the left to its widest cell, as the C# header's grip does. A
//! press on a separator is never a press on a cell: a header sorting on a click does not
//! sort.

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse::Click;
use iced::advanced::mouse::click::Kind;
use iced::advanced::text::{self, LineHeight, Paragraph as _, Renderer as _, Text, Wrapping};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, overlay, renderer};
use iced::{
    Color, Element, Event, Length, Pixels, Point, Rectangle, Size, Theme, Vector, alignment, mouse,
};

/// Width of the band around a separator a press takes, in logical pixels.
pub const SEPARATOR_GRIP: f32 = 8.0;

/// Width of the line a separator is drawn as, in logical pixels.
const SEPARATOR_LINE: f32 = 1.0;

/// Where the middle of each separator is, from the row's left edge: in the gap after each
/// column of `widths` but the last, the first column starting at `inset`.
#[must_use]
pub fn separators(widths: &[f32], inset: f32, spacing: f32) -> Vec<f32> {
    let mut x = inset;
    let mut middles = Vec::with_capacity(widths.len().saturating_sub(1));
    for (index, width) in widths.iter().enumerate() {
        x += width;
        if index + 1 < widths.len() {
            middles.push(x + spacing / 2.0);
            x += spacing;
        }
    }
    middles
}

/// The separator whose grip `x` is in, among those whose middles are `middles`.
#[must_use]
pub fn separator_at(middles: &[f32], x: f32) -> Option<usize> {
    middles
        .iter()
        .position(|middle| (x - middle).abs() <= SEPARATOR_GRIP / 2.0)
}

/// `widths` once `separator` moved by `delta`: the column on its left takes `delta`, the one
/// on its right gives it. Neither is made narrower than its width in `least`, nor, already
/// narrower, any narrower still.
#[must_use]
pub fn dragged(widths: &[f32], least: &[f32], separator: usize, delta: f32) -> Vec<f32> {
    let mut moved = widths.to_vec();
    let (Some(left), Some(right)) = (widths.get(separator), widths.get(separator + 1)) else {
        return moved;
    };
    let least_of = |index: usize| least.get(index).copied().unwrap_or(0.0);
    // The most the left column can give, then take, each kept at its least.
    let shrink = (least_of(separator) - left).min(0.0);
    let grow = (right - least_of(separator + 1)).max(0.0);
    let delta = delta.clamp(shrink, grow);
    moved[separator] = left + delta;
    moved[separator + 1] = right - delta;
    moved
}

/// The texts of a column, each with its size, `None` for the window's: what a double click
/// fits the column to.
type Contents<'a> = Box<dyn Fn(usize) -> Vec<(String, Option<f32>)> + 'a>;

/// A message made from the widths of every column.
type Resized<'a, M> = Box<dyn Fn(Vec<f32>) -> M + 'a>;

/// A separator held.
#[derive(Debug)]
struct Drag {
    /// Its number, from the left.
    separator: usize,
    /// Where the press was along the row.
    from: f32,
    /// The widths when it was pressed.
    widths: Vec<f32>,
    /// The widths last sent.
    sent: Vec<f32>,
}

/// What the header keeps between frames.
#[derive(Debug, Default)]
struct State {
    /// The separator held, while it is.
    drag: Option<Drag>,
    /// The last press on a separator, for a double click.
    last_click: Option<Click>,
}

/// The header row of a list in columns, its separators dragged to resize them.
pub struct ColumnHeader<'a, M> {
    cells: Vec<Element<'a, M>>,
    widths: Vec<f32>,
    least: Vec<f32>,
    inset: f32,
    spacing: f32,
    contents: Option<Contents<'a>>,
    on_resize: Option<Resized<'a, M>>,
}

impl<'a, M> ColumnHeader<'a, M> {
    /// `cells`, one per column, laid out at `widths`, from the left.
    #[must_use]
    pub fn new(cells: Vec<Element<'a, M>>, widths: Vec<f32>) -> Self {
        Self {
            cells,
            widths,
            least: Vec::new(),
            inset: 0.0,
            spacing: 0.0,
            contents: None,
            on_resize: None,
        }
    }

    /// The least width of each column, from the left.
    #[must_use]
    pub fn least(mut self, least: Vec<f32>) -> Self {
        self.least = least;
        self
    }

    /// The room before the first column, as the rows below leave it.
    #[must_use]
    pub fn inset(mut self, inset: f32) -> Self {
        self.inset = inset;
        self
    }

    /// The gap between two columns, a separator in its middle.
    #[must_use]
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// The texts of a column, from its number, a double click fitting it to the widest.
    #[must_use]
    pub fn contents(mut self, contents: impl Fn(usize) -> Vec<(String, Option<f32>)> + 'a) -> Self {
        self.contents = Some(Box::new(contents));
        self
    }

    /// The message sent with the widths of every column, as each move of a separator held
    /// or a double click on one changes them.
    #[must_use]
    pub fn on_resize(mut self, message: impl Fn(Vec<f32>) -> M + 'a) -> Self {
        self.on_resize = Some(Box::new(message));
        self
    }

    /// The middles of the separators of the header drawn in `bounds`, along the window.
    fn middles(&self, bounds: Rectangle) -> Vec<f32> {
        separators(&self.widths, self.inset, self.spacing)
            .into_iter()
            .map(|middle| bounds.x + middle)
            .collect()
    }

    /// Sends `widths` unless they are those last sent; whether they were sent.
    fn send(&self, widths: Vec<f32>, last: &[f32], shell: &mut Shell<'_, M>) -> bool {
        if widths.as_slice() == last {
            return false;
        }
        if let Some(resized) = &self.on_resize {
            shell.publish(resized(widths));
        }
        true
    }

    /// The widest of the texts of column `index`, as `renderer` lays them on one line.
    fn widest(&self, index: usize, renderer: &iced::Renderer) -> Option<f32> {
        let contents = self.contents.as_ref()?;
        let (font, default) = (renderer.default_font(), renderer.default_size());
        contents(index)
            .iter()
            .map(|(content, size)| {
                <iced::Renderer as text::Renderer>::Paragraph::with_text(Text {
                    content: content.as_str(),
                    bounds: Size::INFINITE,
                    size: size.map_or(default, Pixels),
                    line_height: LineHeight::default(),
                    font,
                    align_x: text::Alignment::Default,
                    align_y: alignment::Vertical::Top,
                    shaping: text::Shaping::default(),
                    wrapping: Wrapping::None,
                })
                .min_width()
                .ceil()
            })
            .reduce(f32::max)
    }

    /// A press at `x` on `separator`: a drag starts, or a double click fits the column on its
    /// left.
    fn press(
        &self,
        state: &mut State,
        (separator, x): (usize, f32),
        position: Point,
        renderer: &iced::Renderer,
        shell: &mut Shell<'_, M>,
    ) {
        let click = Click::new(position, mouse::Button::Left, state.last_click);
        state.last_click = Some(click);
        state.drag = None;
        if click.kind() == Kind::Double {
            if let (Some(widest), Some(width)) =
                (self.widest(separator, renderer), self.widths.get(separator))
            {
                let fitted = dragged(&self.widths, &self.least, separator, widest - width);
                self.send(fitted, &self.widths, shell);
            }
        } else {
            state.drag = Some(Drag {
                separator,
                from: x,
                widths: self.widths.clone(),
                sent: self.widths.clone(),
            });
        }
        shell.capture_event();
        shell.request_redraw();
    }

    /// A separator held answers the pointer moving and its release; whether it did.
    fn drag_event(
        &self,
        state: &mut State,
        event: &Event,
        cursor: mouse::Cursor,
        shell: &mut Shell<'_, M>,
    ) -> bool {
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let (Some(drag), Some(position)) = (state.drag.as_mut(), cursor.position()) else {
                    return false;
                };
                let moved = dragged(
                    &drag.widths,
                    &self.least,
                    drag.separator,
                    position.x - drag.from,
                );
                if self.send(moved.clone(), &drag.sent, shell) {
                    drag.sent = moved;
                }
                true
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.drag.take().is_some()
            }
            _ => false,
        }
    }
}

impl<M> Widget<M, Theme, iced::Renderer> for ColumnHeader<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
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
        let tallest = limits.max().height;
        let mut x = self.inset;
        let mut height: f32 = 0.0;
        let mut nodes = Vec::with_capacity(self.cells.len());
        for ((cell, tree), width) in self
            .cells
            .iter_mut()
            .zip(&mut tree.children)
            .zip(self.widths.iter().copied())
        {
            let node = cell
                .as_widget_mut()
                .layout(
                    tree,
                    renderer,
                    &layout::Limits::new(Size::ZERO, Size::new(width, tallest)),
                )
                .move_to(Point::new(x, 0.0));
            height = height.max(node.size().height);
            nodes.push(node);
            x += width + self.spacing;
        }
        let size = limits.resolve(Length::Fill, Length::Shrink, Size::new(x, height));
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
        let state = tree.state.downcast_mut::<State>();
        if self.drag_event(state, event, cursor, shell) {
            shell.capture_event();
            shell.request_redraw();
            return;
        }
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && let Some(position) = cursor.position_over(layout.bounds())
        {
            let middles = self.middles(layout.bounds());
            if let Some(separator) = separator_at(&middles, position.x) {
                self.press(state, (separator, position.x), position, renderer, shell);
                return;
            }
        }
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
        let state = tree.state.downcast_ref::<State>();
        let over = cursor
            .position_over(layout.bounds())
            .and_then(|at| separator_at(&self.middles(layout.bounds()), at.x));
        if state.drag.is_some() || over.is_some() {
            return mouse::Interaction::ResizingHorizontally;
        }
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
        use iced::advanced::Renderer as _;

        for ((cell, tree), layout) in self.cells.iter().zip(&tree.children).zip(layout.children()) {
            cell.as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
        }
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let over = cursor
            .position_over(bounds)
            .and_then(|at| separator_at(&self.middles(bounds), at.x));
        let palette = theme.extended_palette();
        for (index, middle) in self.middles(bounds).into_iter().enumerate() {
            let held = state
                .drag
                .as_ref()
                .is_some_and(|drag| drag.separator == index)
                || over == Some(index);
            let color: Color = if held {
                palette.primary.strong.color
            } else {
                palette.background.strong.color
            };
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle::new(
                        Point::new(middle - SEPARATOR_LINE / 2.0, bounds.y),
                        Size::new(SEPARATOR_LINE, bounds.height),
                    ),
                    ..renderer::Quad::default()
                },
                color,
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

impl<'a, M: 'a> From<ColumnHeader<'a, M>> for Element<'a, M, Theme, iced::Renderer> {
    fn from(header: ColumnHeader<'a, M>) -> Self {
        Element::new(header)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_separator_sits_in_the_middle_of_the_gap_after_each_column_but_the_last() {
        assert_eq!(
            separators(&[100.0, 50.0, 30.0], 10.0, 8.0),
            [114.0, 172.0],
            "10 + 100 + 4, then 10 + 100 + 8 + 50 + 4"
        );
        assert!(separators(&[100.0], 10.0, 8.0).is_empty(), "one column");
        assert!(separators(&[], 10.0, 8.0).is_empty());
    }

    #[test]
    fn a_press_takes_a_separator_within_its_grip_only() {
        let middles = [114.0, 172.0];
        assert_eq!(separator_at(&middles, 114.0), Some(0));
        assert_eq!(
            separator_at(&middles, 114.0 + SEPARATOR_GRIP / 2.0),
            Some(0)
        );
        assert_eq!(separator_at(&middles, 168.5), Some(1));
        assert_eq!(separator_at(&middles, 109.0), None, "on the column");
        assert_eq!(separator_at(&middles, 140.0), None);
    }

    #[test]
    fn a_dragged_separator_gives_its_left_column_what_the_right_one_gives_up() {
        let widths = [300.0, 90.0, 130.0];
        let least = [160.0, 40.0, 60.0];
        assert_eq!(dragged(&widths, &least, 1, 20.0), [300.0, 110.0, 110.0]);
        assert_eq!(dragged(&widths, &least, 1, -30.0), [300.0, 60.0, 160.0]);
        assert_eq!(dragged(&widths, &least, 0, 25.0), [325.0, 65.0, 130.0]);
        assert_eq!(
            dragged(&widths, &least, 2, 10.0),
            widths,
            "no column after the last"
        );
    }

    #[test]
    fn a_dragged_separator_keeps_both_columns_at_their_least() {
        let widths = [300.0, 90.0, 130.0];
        let least = [160.0, 40.0, 60.0];
        assert_eq!(dragged(&widths, &least, 1, -500.0), [300.0, 40.0, 180.0]);
        assert_eq!(dragged(&widths, &least, 1, 500.0), [300.0, 160.0, 60.0]);
        assert_eq!(dragged(&widths, &least, 0, -500.0), [160.0, 230.0, 130.0]);
        // A column narrower than its least, the pane being narrow, is not made narrower.
        assert_eq!(dragged(&[120.0, 90.0], &least, 0, -10.0), [120.0, 90.0]);
        assert_eq!(dragged(&[120.0, 90.0], &least, 0, 10.0), [130.0, 80.0]);
    }
}
