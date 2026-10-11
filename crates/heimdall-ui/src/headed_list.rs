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

//! A list in columns under its header, as a WPF `ListView` with a `GridView`: the rows scroll
//! both ways in their own scroller, and the header, above them, follows them sideways.
//!
//! The header is laid out at its whole width, however wide the pane, and drawn shifted by the
//! rows' horizontal scroll, cut at the pane's edges; it never scrolls up or down. The rows'
//! vertical scrollbar stays at the pane's right edge while the columns scroll under it, as the
//! C# list's.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Id, Operation, Tree, operation, tree};
use iced::advanced::{Clipboard, Renderer as _, Shell, Widget, mouse, overlay, renderer};
use iced::widget::scrollable::{AbsoluteOffset, RelativeOffset};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

/// A header over the rows it heads; see the module.
pub struct HeadedList<'a, Message> {
    header: Element<'a, Message>,
    rows: Element<'a, Message>,
    gap: f32,
}

/// `header` over `rows`, a scroller of them, `gap` between them.
#[must_use]
pub fn headed_list<'a, Message>(
    header: impl Into<Element<'a, Message>>,
    rows: impl Into<Element<'a, Message>>,
    gap: f32,
) -> HeadedList<'a, Message> {
    HeadedList {
        header: header.into(),
        rows: rows.into(),
        gap,
    }
}

/// What the list keeps between frames.
#[derive(Debug, Default, Clone, Copy)]
struct Kept {
    /// How far the rows are scrolled sideways, as last read from their scroller.
    scrolled: f32,
    /// A button pressed on the header and not yet let go: the header follows the pointer
    /// wherever it goes, as a WPF column's gripper captures it.
    held: bool,
}

/// Reads the sideways scroll of the first scroller met.
#[derive(Default)]
struct ReadScroll(Option<f32>);

impl Operation for ReadScroll {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        if self.0.is_none() {
            operate(self);
        }
    }

    fn scrollable(
        &mut self,
        _id: Option<&Id>,
        _bounds: Rectangle,
        _content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn operation::Scrollable,
    ) {
        if self.0.is_none() {
            self.0 = Some(translation.x);
        }
    }
}

/// The header as an operation sees it: a scroller that moves only with the rows.
struct Follower;

impl operation::Scrollable for Follower {
    fn snap_to(&mut self, _offset: RelativeOffset<Option<f32>>) {}

    fn scroll_to(&mut self, _offset: AbsoluteOffset<Option<f32>>) {}

    fn scroll_by(&mut self, _offset: AbsoluteOffset, _bounds: Rectangle, _content: Rectangle) {}
}

/// The part of the header in sight, for a list laid out in `bounds`, its header `height`
/// high.
fn header_view(bounds: Rectangle, height: f32) -> Rectangle {
    Rectangle::new(bounds.position(), Size::new(bounds.width, height))
}

/// `cursor` as the header, scrolled by `scrolled`, sees it: shifted with its cells. Beside
/// the part in sight, `view`, at its height, the pointer is not for it, the header's hidden
/// cells there for no one, unless a press on the header is `held`. Above or below it, the
/// pointer is still seen, so that a separator dragged follows it.
fn header_cursor(
    cursor: mouse::Cursor,
    view: Rectangle,
    scrolled: f32,
    held: bool,
) -> mouse::Cursor {
    let Some(position) = cursor.position() else {
        return cursor;
    };
    let beside =
        !held && !view.contains(position) && (view.y..=view.y + view.height).contains(&position.y);
    if beside {
        mouse::Cursor::Unavailable
    } else {
        mouse::Cursor::Available(position + Vector::new(scrolled, 0.0))
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for HeadedList<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Kept>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Kept::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.header), Tree::new(&self.rows)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[self.header.as_widget(), self.rows.as_widget()]);
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
        let max = limits.max();
        // At its whole width: what passes the pane is reached by scrolling.
        let header = self.header.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(Size::ZERO, Size::new(f32::INFINITY, max.height)),
        );
        let top = header.size().height + self.gap;
        let rows = self
            .rows
            .as_widget_mut()
            .layout(
                &mut tree.children[1],
                renderer,
                &layout::Limits::new(
                    Size::ZERO,
                    Size::new(max.width, (max.height - top).max(0.0)),
                ),
            )
            .move_to(Point::new(0.0, top));
        let size = limits.resolve(Length::Fill, Length::Fill, Size::new(max.width, max.height));
        layout::Node::with_children(size, vec![header, rows])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        let scrolled = tree.state.downcast_ref::<Kept>().scrolled;
        let mut children = layout.children();
        let (Some(header), Some(rows)) = (children.next(), children.next()) else {
            return;
        };
        let view = header_view(layout.bounds(), header.bounds().height);
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            // The header said scrolled as the rows are, so that what finds its cells knows
            // which are in sight.
            operation.traverse(&mut |operation| {
                operation.scrollable(
                    None,
                    view,
                    header.bounds(),
                    Vector::new(scrolled, 0.0),
                    &mut Follower,
                );
                operation.traverse(&mut |operation| {
                    self.header.as_widget_mut().operate(
                        &mut tree.children[0],
                        header,
                        renderer,
                        operation,
                    );
                });
            });
            self.rows
                .as_widget_mut()
                .operate(&mut tree.children[1], rows, renderer, operation);
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
        let Kept { scrolled, held } = *tree.state.downcast_ref::<Kept>();
        let mut children = layout.children();
        let (Some(header), Some(rows)) = (children.next(), children.next()) else {
            return;
        };
        let view = header_view(layout.bounds(), header.bounds().height);
        let held = match event {
            Event::Mouse(mouse::Event::ButtonPressed(_)) => cursor.is_over(view),
            _ => held,
        };
        self.header.as_widget_mut().update(
            &mut tree.children[0],
            event,
            header,
            header_cursor(cursor, view, scrolled, held),
            renderer,
            clipboard,
            shell,
            &(view + Vector::new(scrolled, 0.0)),
        );
        self.rows.as_widget_mut().update(
            &mut tree.children[1],
            event,
            rows,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        // The rows' scroll read after they had the event, the header drawn again with them.
        let mut read = ReadScroll::default();
        self.rows
            .as_widget_mut()
            .operate(&mut tree.children[1], rows, renderer, &mut read);
        let now = read.0.unwrap_or_default();
        let kept = tree.state.downcast_mut::<Kept>();
        kept.held = held && !matches!(event, Event::Mouse(mouse::Event::ButtonReleased(_)));
        if now.to_bits() != kept.scrolled.to_bits() {
            kept.scrolled = now;
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
        let Kept { scrolled, held } = *tree.state.downcast_ref::<Kept>();
        let mut children = layout.children();
        let (Some(header), Some(rows)) = (children.next(), children.next()) else {
            return mouse::Interaction::default();
        };
        let view = header_view(layout.bounds(), header.bounds().height);
        let over_header = self.header.as_widget().mouse_interaction(
            &tree.children[0],
            header,
            header_cursor(cursor, view, scrolled, held),
            &(view + Vector::new(scrolled, 0.0)),
            renderer,
        );
        let over_rows = self.rows.as_widget().mouse_interaction(
            &tree.children[1],
            rows,
            cursor,
            viewport,
            renderer,
        );
        over_header.max(over_rows)
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
        let Kept { scrolled, held } = *tree.state.downcast_ref::<Kept>();
        let mut children = layout.children();
        let (Some(header), Some(rows)) = (children.next(), children.next()) else {
            return;
        };
        let view = header_view(layout.bounds(), header.bounds().height);
        renderer.with_layer(view, |renderer| {
            renderer.with_translation(Vector::new(-scrolled, 0.0), |renderer| {
                self.header.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    header,
                    header_cursor(cursor, view, scrolled, held),
                    &(view + Vector::new(scrolled, 0.0)),
                );
            });
        });
        self.rows.as_widget().draw(
            &tree.children[1],
            renderer,
            theme,
            style,
            rows,
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
        let scrolled = tree.state.downcast_ref::<Kept>().scrolled;
        let mut children = layout.children();
        let (header, rows) = (children.next()?, children.next()?);
        let [header_tree, rows_tree] = tree.children.as_mut_slice() else {
            return None;
        };
        let overlays: Vec<_> = [
            self.header.as_widget_mut().overlay(
                header_tree,
                header,
                renderer,
                viewport,
                translation - Vector::new(scrolled, 0.0),
            ),
            self.rows
                .as_widget_mut()
                .overlay(rows_tree, rows, renderer, viewport, translation),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!overlays.is_empty()).then(|| overlay::Group::with_children(overlays).overlay())
    }
}

impl<'a, Message: 'a> From<HeadedList<'a, Message>> for Element<'a, Message> {
    fn from(list: HeadedList<'a, Message>) -> Self {
        Element::new(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_sees_the_pointer_where_its_scrolled_cells_are() {
        let view = Rectangle::new(Point::new(100.0, 50.0), Size::new(300.0, 20.0));
        let at = |x, y| mouse::Cursor::Available(Point::new(x, y));
        assert_eq!(
            header_cursor(at(150.0, 60.0), view, 40.0, false),
            at(190.0, 60.0)
        );
        assert_eq!(
            header_cursor(at(150.0, 90.0), view, 40.0, false),
            at(190.0, 90.0),
            "below it, still followed"
        );
        assert_eq!(
            header_cursor(at(450.0, 60.0), view, 40.0, false),
            mouse::Cursor::Unavailable,
            "beside it, at its height: none of its hidden cells"
        );
        assert_eq!(
            header_cursor(at(450.0, 60.0), view, 40.0, true),
            at(490.0, 60.0),
            "a separator held follows the pointer anywhere"
        );
    }
}
