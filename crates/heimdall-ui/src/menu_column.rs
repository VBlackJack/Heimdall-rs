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

//! The entries of a menu, as wide as the widest of them, as a WPF menu's `StackPanel`.
//!
//! A WPF menu measures each item at its own size, takes the widest, then arranges every item
//! at that width. Menu entries here fill their width (a button's highlight spans the menu,
//! a separator crosses it), and an iced 0.14 column as wide as its content gives such
//! entries no width at all. [`MenuColumn`] does what the `StackPanel` does: it lays each
//! entry out with its width compressed, which resolves a filling width to the entry's own
//! (`iced_core` `Limits::resolve`), takes the widest within the room, at least its minimum,
//! then lays every entry out again at exactly that width, stacked.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

/// Entries stacked at the width of the widest; see the module.
pub struct MenuColumn<'a, Message, Renderer = iced::Renderer> {
    children: Vec<Element<'a, Message, Theme, Renderer>>,
    min_width: f32,
}

/// A [`MenuColumn`] of the entries given, as `column!` makes a column.
macro_rules! menu_column {
    () => (
        $crate::menu_column::MenuColumn::new()
    );
    ($($x:expr),+ $(,)?) => (
        $crate::menu_column::MenuColumn::with_children([$(iced::Element::from($x)),+])
    );
}
pub(crate) use menu_column;

/// How wide a menu's entries are drawn: as the `widest` of them, at least `min`, at most
/// `room`. Rounded up to a whole pixel: a label laid out in exactly its measured width
/// could wrap on a rounding.
#[must_use]
pub fn column_width(widest: f32, min: f32, room: f32) -> f32 {
    widest.ceil().max(min).min(room)
}

impl<'a, Message, Renderer> MenuColumn<'a, Message, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    /// No entries.
    #[must_use]
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            min_width: 0.0,
        }
    }

    /// The entries `children`.
    #[must_use]
    pub fn with_children(
        children: impl IntoIterator<Item = Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self::new().extend(children)
    }

    /// The narrowest the entries are, however short.
    #[must_use]
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    /// `child` added under the entries.
    #[must_use]
    pub fn push(mut self, child: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        self.children.push(child.into());
        self
    }

    /// `children` added under the entries.
    #[must_use]
    pub fn extend(
        mut self,
        children: impl IntoIterator<Item = Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        self.children.extend(children);
        self
    }
}

impl<Message, Renderer> Default for MenuColumn<'_, Message, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for MenuColumn<'_, Message, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let room = limits.max().width;
        // Measured: each entry at its own width, a filling one at what it holds.
        let measure =
            layout::Limits::new(Size::ZERO, Size::new(room, f32::INFINITY)).width(Length::Shrink);
        let widest = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .map(|(child, tree)| {
                child
                    .as_widget_mut()
                    .layout(tree, renderer, &measure)
                    .size()
                    .width
            })
            .fold(0.0, f32::max);
        // At least what the parent asks: an inner section is as wide as its menu.
        let width = column_width(widest, self.min_width.max(limits.min().width), room);
        // Arranged: every entry at that width, stacked.
        let arrange = layout::Limits::new(Size::new(width, 0.0), Size::new(width, f32::INFINITY));
        let mut height = 0.0;
        let nodes = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .map(|(child, tree)| {
                let node = child
                    .as_widget_mut()
                    .layout(tree, renderer, &arrange)
                    .move_to(Point::new(0.0, height));
                height += node.size().height;
                node
            })
            .collect();
        let size = limits.resolve(Length::Shrink, Length::Shrink, Size::new(width, height));
        layout::Node::with_children(size, nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, state), layout) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                child
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
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        for ((child, tree), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
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
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, tree), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(tree, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        for ((child, tree), layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .filter(|(_, layout)| layout.bounds().intersects(viewport))
        {
            child
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message: 'a, Renderer> From<MenuColumn<'a, Message, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(column: MenuColumn<'a, Message, Renderer>) -> Self {
        Element::new(column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::{container, row, space};

    /// What the tests lay out: the null renderer, which draws nothing and measures text as
    /// empty, so labels are stood in for by spaces of their width.
    type Entry = Element<'static, (), Theme, ()>;

    /// The room of a window.
    const ROOM: f32 = 1000.0;

    /// The minimum of a menu.
    const MIN: f32 = 194.0;

    /// A label of `width`.
    fn label(width: f32) -> Entry {
        space().width(width).height(20.0).into()
    }

    /// A menu entry as the tree's: a label padded inside a box filling the menu's width.
    fn entry(width: f32) -> Entry {
        container(label(width))
            .padding(8.0)
            .width(Length::Fill)
            .into()
    }

    /// A separator: a line crossing the menu, of no width of its own.
    fn separator() -> Entry {
        space().width(Length::Fill).height(1.0).into()
    }

    /// `menu` laid out in a room of `room`: its size, and its entries' widths.
    fn lay_out(menu: MenuColumn<'static, (), ()>, room: f32) -> (Size, Vec<f32>) {
        let mut menu: Entry = menu.into();
        let mut tree = Tree::new(&menu);
        let node = menu.as_widget_mut().layout(
            &mut tree,
            &(),
            &layout::Limits::new(Size::ZERO, Size::new(room, f32::INFINITY)),
        );
        let widths = node
            .children()
            .iter()
            .map(|child| child.size().width)
            .collect();
        (node.size(), widths)
    }

    #[test]
    fn a_menu_is_as_wide_as_its_longest_entry_and_every_entry_spans_it() {
        let menu = menu_column![entry(120.0), separator(), entry(400.0)].min_width(MIN);
        let (size, widths) = lay_out(menu, ROOM);
        assert!((size.width - 416.0).abs() < f32::EPSILON, "{size:?}");
        assert!((size.height - 2.0 * 36.0 - 1.0).abs() < f32::EPSILON);
        assert!(
            widths
                .iter()
                .all(|width| (width - 416.0).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn a_filling_label_counts_its_own_width() {
        // A label told to fill its width, as a header filling its row.
        let header: Entry = container(label(300.0)).width(Length::Fill).into();
        let (size, _) = lay_out(menu_column![header, separator()].min_width(MIN), ROOM);
        assert!((size.width - 300.0).abs() < f32::EPSILON, "{size:?}");
    }

    #[test]
    fn a_row_with_a_spacer_and_a_glyph_on_the_right_is_not_window_wide() {
        // A label, a spacer pushing a submenu arrow of 16 to the right edge, as the C#
        // MenuItem template's last column.
        let submenu: Entry = container(
            row![label(250.0), space().width(Length::Fill), label(16.0)].width(Length::Fill),
        )
        .padding(8.0)
        .width(Length::Fill)
        .into();
        let (size, widths) = lay_out(menu_column![submenu, entry(100.0)].min_width(MIN), ROOM);
        assert!((size.width - 282.0).abs() < f32::EPSILON, "{size:?}");
        assert!(
            widths
                .iter()
                .all(|width| (width - 282.0).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn a_menu_of_short_entries_keeps_its_minimum() {
        let (size, widths) = lay_out(
            menu_column![entry(40.0), separator(), entry(60.0)].min_width(MIN),
            ROOM,
        );
        assert!((size.width - MIN).abs() < f32::EPSILON, "{size:?}");
        assert!(
            widths
                .iter()
                .all(|width| (width - MIN).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn a_menu_is_never_wider_than_its_room() {
        let (size, widths) = lay_out(menu_column![entry(1400.0)].min_width(MIN), ROOM);
        assert!((size.width - ROOM).abs() < f32::EPSILON, "{size:?}");
        assert!((widths[0] - ROOM).abs() < f32::EPSILON);
        // Under the minimum too in a room narrower than it.
        let (size, _) = lay_out(menu_column![entry(10.0)].min_width(MIN), 150.0);
        assert!((size.width - 150.0).abs() < f32::EPSILON, "{size:?}");
    }

    #[test]
    fn an_inner_section_is_as_wide_as_its_menu() {
        let section = menu_column![entry(50.0), entry(80.0)];
        let (size, widths) = lay_out(menu_column![section, entry(300.0)].min_width(MIN), ROOM);
        assert!((size.width - 316.0).abs() < f32::EPSILON, "{size:?}");
        assert!(
            widths
                .iter()
                .all(|width| (width - 316.0).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn a_width_is_a_whole_pixel() {
        assert!((column_width(300.2, MIN, ROOM) - 301.0).abs() < f32::EPSILON);
    }
}
