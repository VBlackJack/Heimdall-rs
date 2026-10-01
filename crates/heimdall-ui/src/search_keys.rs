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

//! The keys the tree's search answers while it has the keyboard, as the C# filter box:
//! Escape empties it, Down moves to the first profile found. Enter is the field's own
//! submit.
//!
//! The field's focus is read when the key arrives, before the field sees it: a text field
//! loses its focus on Escape, and asking afterwards, or through a task, would come too late
//! or a frame later for every Down pressed anywhere in the window.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{Id, Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell, mouse, overlay, renderer};
use iced::keyboard::{self, key::Named};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

/// The search field `content`, publishing `escape` and `down` for those keys pressed while
/// it has the keyboard. `escape` is `None` while there is nothing to empty: the field then
/// takes Escape as any field does.
pub struct SearchKeys<'a, Message> {
    content: Element<'a, Message>,
    escape: Option<Message>,
    down: Message,
}

impl<'a, Message> SearchKeys<'a, Message> {
    /// `content` answering Escape with `escape` and Down with `down` while focused.
    pub fn new(
        content: impl Into<Element<'a, Message>>,
        escape: Option<Message>,
        down: Message,
    ) -> Self {
        Self {
            content: content.into(),
            escape,
            down,
        }
    }
}

/// Whether a focusable widget under the one operated on has the keyboard.
#[derive(Default)]
struct HasFocus(bool);

impl Operation for HasFocus {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        self.0 |= state.is_focused();
    }
}

impl<Message: Clone> Widget<Message, Theme, iced::Renderer> for SearchKeys<'_, Message> {
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
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(named),
            modifiers,
            ..
        }) = event
            && modifiers.is_empty()
        {
            let answer = match named {
                Named::Escape => self.escape.clone(),
                Named::ArrowDown => Some(self.down.clone()),
                _ => None,
            };
            if let Some(message) = answer {
                let mut focus = HasFocus::default();
                self.operate(tree, layout, renderer, &mut focus);
                if focus.0 {
                    shell.publish(message);
                    shell.capture_event();
                    return;
                }
            }
        }
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

impl<'a, Message: Clone + 'a> From<SearchKeys<'a, Message>> for Element<'a, Message> {
    fn from(keys: SearchKeys<'a, Message>) -> Self {
        Element::new(keys)
    }
}
