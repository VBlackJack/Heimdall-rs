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

//! A row whose items are given their room in an order of their own, then drawn left to
//! right: a short state keeps its word while a long name before it ends with an ellipsis.
//!
//! An iced row gives its room from the left, so the items after a long one get none. Here
//! each item is laid out in the order of its rank, the lowest first, in what the items
//! ranked before it left; items that end with an ellipsis ([`crate::ellipsis`]) then take
//! no more than they are given. The items are drawn in the order they were pushed, one
//! after the other, centred on the row's height.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

/// An item of the row: its rank, the room before it and the item itself.
struct Item<'a, Message> {
    rank: u8,
    gap: f32,
    content: Element<'a, Message>,
}

/// A row giving its room by rank; see the module.
pub struct PriorityRow<'a, Message> {
    spacing: f32,
    items: Vec<Item<'a, Message>>,
}

impl<'a, Message> PriorityRow<'a, Message> {
    /// An empty row, its items `spacing` apart.
    #[must_use]
    pub fn new(spacing: f32) -> Self {
        Self {
            spacing,
            items: Vec::new(),
        }
    }

    /// `content` at the end of the row, given its room at `rank`: the lower, the sooner.
    #[must_use]
    pub fn push(mut self, rank: u8, content: impl Into<Element<'a, Message>>) -> Self {
        let gap = if self.items.is_empty() {
            0.0
        } else {
            self.spacing
        };
        self.items.push(Item {
            rank,
            gap,
            content: content.into(),
        });
        self
    }

    /// `content` at the end of the row, right against the item before it.
    #[must_use]
    pub fn push_glued(mut self, rank: u8, content: impl Into<Element<'a, Message>>) -> Self {
        self.items.push(Item {
            rank,
            gap: 0.0,
            content: content.into(),
        });
        self
    }

    /// `content`, if any, at the end of the row, given its room at `rank`.
    #[must_use]
    pub fn push_maybe(self, rank: u8, content: Option<impl Into<Element<'a, Message>>>) -> Self {
        match content {
            Some(content) => self.push(rank, content),
            None => self,
        }
    }
}

/// The order the items of ranks `ranks` are given their room in: by rank, then as pushed.
#[must_use]
pub fn room_order(ranks: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..ranks.len()).collect();
    order.sort_by_key(|&index| ranks[index]);
    order
}

impl<Message> Widget<Message, Theme, iced::Renderer> for PriorityRow<'_, Message> {
    fn children(&self) -> Vec<Tree> {
        self.items
            .iter()
            .map(|item| Tree::new(&item.content))
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let contents: Vec<&Element<'_, Message>> =
            self.items.iter().map(|item| &item.content).collect();
        tree.diff_children(&contents);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let max = limits.max();
        let gaps: f32 = self.items.iter().map(|item| item.gap).sum();
        let mut left = (max.width - gaps).max(0.0);
        let ranks: Vec<u8> = self.items.iter().map(|item| item.rank).collect();
        let mut nodes: Vec<layout::Node> = (0..self.items.len())
            .map(|_| layout::Node::default())
            .collect();
        for index in room_order(&ranks) {
            let node = self.items[index].content.as_widget_mut().layout(
                &mut tree.children[index],
                renderer,
                &layout::Limits::new(Size::ZERO, Size::new(left, max.height)),
            );
            left = (left - node.size().width).max(0.0);
            nodes[index] = node;
        }
        let height = nodes
            .iter()
            .map(|node| node.size().height)
            .fold(0.0, f32::max);
        let mut x = 0.0;
        for (item, node) in self.items.iter().zip(nodes.iter_mut()) {
            x += item.gap;
            let size = node.size();
            node.move_to_mut(Point::new(x, (height - size.height) / 2.0));
            x += size.width;
        }
        layout::Node::with_children(
            limits.resolve(Length::Shrink, Length::Shrink, Size::new(x, height)),
            nodes,
        )
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
            for ((item, state), layout) in self
                .items
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                item.content
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
        for ((item, state), layout) in self
            .items
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            item.content.as_widget_mut().update(
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
        self.items
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((item, state), layout)| {
                item.content
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
        for ((item, state), layout) in self.items.iter().zip(&tree.children).zip(layout.children())
        {
            item.content
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
        let children = self
            .items
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .filter_map(|((item, state), layout)| {
                item.content
                    .as_widget_mut()
                    .overlay(state, layout, renderer, viewport, translation)
            })
            .collect::<Vec<_>>();
        (!children.is_empty()).then(|| overlay::Group::with_children(children).overlay())
    }
}

impl<'a, Message: 'a> From<PriorityRow<'a, Message>> for Element<'a, Message> {
    fn from(row: PriorityRow<'a, Message>) -> Self {
        Element::new(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lowest_rank_is_served_first_and_equal_ranks_as_pushed() {
        assert_eq!(room_order(&[2, 0, 3, 0, 1]), vec![1, 3, 4, 0, 2]);
        assert!(room_order(&[]).is_empty());
    }
}
