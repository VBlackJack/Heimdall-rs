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

//! The session tabs on one row that scrolls sideways, as the C# `HeaderOnlyTabControlStyle`
//! lays its `TabPanel` in a `ScrollViewer` with `HorizontalScrollBarVisibility="Auto"`.
//!
//! The row never folds onto a second one: past the window's width, its scrollbar shows, the
//! wheel turns it as well as Shift and the wheel, and the tab shown is brought into view
//! whenever another one is shown, as a WPF tab selected brings itself into view.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Id, Operation, Tree, operation};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay, renderer};
use iced::widget::scrollable::{AbsoluteOffset, Direction};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

use crate::styles;

/// `row` in a scroller that keeps the tab of identifier `shown` in view; see the module.
pub struct TabScroller<'a, Message> {
    scrollable: Element<'a, Message>,
    shown: Option<Id>,
    /// Says whether the tabs pass the row's width, and what to reserve beside it.
    overflow: Option<Overflow<'a, Message>>,
}

/// What a scroller says of its overflow: the room a button beside it takes while shown, and
/// the message for a change.
struct Overflow<'a, Message> {
    reserve: f32,
    message: Box<dyn Fn(bool) -> Message + 'a>,
}

/// The tabs of `row`, on one line that scrolls sideways, the one whose container bears the
/// identifier `shown` brought into view each time it changes.
#[must_use]
pub fn tab_scroller<'a, Message: 'a>(
    row: impl Into<Element<'a, Message>>,
    id: Id,
    shown: Option<Id>,
) -> TabScroller<'a, Message> {
    TabScroller {
        scrollable: iced::widget::scrollable(row)
            .id(id)
            // Under the tabs, and only while they pass the row's width, as the C#'s `Auto`.
            .direction(Direction::Horizontal(styles::scrollbar().spacing(0.0)))
            .style(styles::scrollbars)
            .width(Length::Fill)
            .into(),
        shown,
        overflow: None,
    }
}

/// The tab last brought into view.
#[derive(Debug, Default)]
struct Revealed(Option<Id>);

/// Whether a row of `content` width passes a scroller `viewport` wide, `reserve` being the
/// room a button beside the scroller takes while it is shown: judged against the width the
/// row would have without that button, as the C# `UpdateTabStripOverflow`
/// (`MainWindow.TabStrip.cs:100`), with its half-pixel tolerance.
#[must_use]
pub fn overflows(content: f32, viewport: f32, reserve: f32) -> bool {
    content > viewport + reserve + OVERFLOW_TOLERANCE
}

/// Slack before the tabs count as passing the row, as the C# `TabStripOverflowTolerance`.
const OVERFLOW_TOLERANCE: f32 = 0.5;

/// Finds the widths of the scroller, met first, and of its content.
struct Measure(Option<(f32, f32)>);

impl Operation for Measure {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        _id: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        _translation: Vector,
        _state: &mut dyn operation::Scrollable,
    ) {
        if self.0.is_none() {
            self.0 = Some((bounds.width, content_bounds.width));
        }
    }
}

/// Where the offset must go for `tab` to be seen whole in a scroller at `viewport` scrolled
/// by `scrolled`, from the start of its content; `None` when it is seen whole already. A tab
/// wider than the scroller shows its start.
#[must_use]
pub fn reveal_offset(viewport: Rectangle, scrolled: f32, tab: Rectangle) -> Option<f32> {
    let start = tab.x - viewport.x;
    let end = start + tab.width;
    if start < scrolled || tab.width > viewport.width {
        Some(start)
    } else if end > scrolled + viewport.width {
        Some(end - viewport.width)
    } else {
        None
    }
}

/// Finds the scroller's bounds and scroll, and the bounds of the tab sought.
struct Find {
    tab: Id,
    viewport: Option<(Rectangle, f32)>,
    found: Option<Rectangle>,
}

impl Operation for Find {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&self.tab) {
            self.found = Some(bounds);
        }
    }

    fn scrollable(
        &mut self,
        _id: Option<&Id>,
        bounds: Rectangle,
        _content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn operation::Scrollable,
    ) {
        // The scroller's own, met first: the tabs hold none of their own.
        if self.viewport.is_none() {
            self.viewport = Some((bounds, translation.x));
        }
    }
}

/// Scrolls the scroller, met first, to an offset along its row.
struct ScrollTo(f32);

impl Operation for ScrollTo {
    fn traverse(&mut self, _operate: &mut dyn FnMut(&mut dyn Operation)) {}

    fn scrollable(
        &mut self,
        _id: Option<&Id>,
        _bounds: Rectangle,
        _content_bounds: Rectangle,
        _translation: Vector,
        state: &mut dyn operation::Scrollable,
    ) {
        state.scroll_to(AbsoluteOffset {
            x: Some(self.0),
            y: None,
        });
    }
}

impl<'a, Message> TabScroller<'a, Message> {
    /// Says through `message` when the tabs start to pass the row's width, and when they stop,
    /// `reserve` being the room a button beside it takes while shown, nothing while it is not.
    #[must_use]
    pub fn on_overflow(mut self, reserve: f32, message: impl Fn(bool) -> Message + 'a) -> Self {
        self.overflow = Some(Overflow {
            reserve,
            message: Box::new(message),
        });
        self
    }
}

impl<Message> TabScroller<'_, Message> {
    /// Brings the tab shown into view in the scroller laid out at `node`.
    fn reveal(&mut self, tree: &mut Tree, renderer: &iced::Renderer, node: &layout::Node) {
        let Some(tab) = self.shown.clone() else {
            return;
        };
        let mut find = Find {
            tab,
            viewport: None,
            found: None,
        };
        self.scrollable
            .as_widget_mut()
            .operate(tree, Layout::new(node), renderer, &mut find);
        let (Some((viewport, scrolled)), Some(bounds)) = (find.viewport, find.found) else {
            return;
        };
        if let Some(offset) = reveal_offset(viewport, scrolled, bounds) {
            self.scrollable.as_widget_mut().operate(
                tree,
                Layout::new(node),
                renderer,
                &mut ScrollTo(offset),
            );
        }
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for TabScroller<'_, Message> {
    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        iced::advanced::widget::tree::Tag::of::<Revealed>()
    }

    fn state(&self) -> iced::advanced::widget::tree::State {
        iced::advanced::widget::tree::State::new(Revealed::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.scrollable)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.scrollable));
    }

    fn size(&self) -> Size<Length> {
        self.scrollable.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = self
            .scrollable
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let revealed = tree.state.downcast_mut::<Revealed>();
        if revealed.0 != self.shown {
            revealed.0.clone_from(&self.shown);
            self.reveal(&mut tree.children[0], renderer, &node);
        }
        node
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.scrollable
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
        // The wheel turned over the row scrolls it sideways, the only way it goes: its turn
        // given to both axes, so that iced's swap of them under Shift changes nothing.
        let sideways = match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if cursor.is_over(layout.bounds()) =>
            {
                match *delta {
                    mouse::ScrollDelta::Lines { x: 0.0, y } => {
                        Some(Event::Mouse(mouse::Event::WheelScrolled {
                            delta: mouse::ScrollDelta::Lines { x: y, y },
                        }))
                    }
                    mouse::ScrollDelta::Pixels { x: 0.0, y } => {
                        Some(Event::Mouse(mouse::Event::WheelScrolled {
                            delta: mouse::ScrollDelta::Pixels { x: y, y },
                        }))
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        self.scrollable.as_widget_mut().update(
            &mut tree.children[0],
            sideways.as_ref().unwrap_or(event),
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        // Told on the frame drawn, once the row is laid out; only when it differs from what the
        // caller shows.
        if let Some(overflow) = &self.overflow
            && matches!(
                event,
                Event::Window(iced::window::Event::RedrawRequested(_))
            )
        {
            let mut measure = Measure(None);
            self.scrollable.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                &mut measure,
            );
            if let Some((row, content)) = measure.0 {
                let now = overflows(content, row, overflow.reserve);
                // The button is shown exactly while a room is reserved for it.
                if now != (overflow.reserve > 0.0) {
                    shell.publish((overflow.message)(now));
                }
            }
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
        self.scrollable.as_widget().mouse_interaction(
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
        self.scrollable.as_widget().draw(
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
        self.scrollable.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message: 'a> From<TabScroller<'a, Message>> for Element<'a, Message> {
    fn from(scroller: TabScroller<'a, Message>) -> Self {
        Element::new(scroller)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Point;

    fn rectangle(x: f32, width: f32) -> Rectangle {
        Rectangle::new(Point::new(x, 0.0), Size::new(width, 30.0))
    }

    #[test]
    fn a_tab_seen_whole_leaves_the_row_where_it_is() {
        let viewport = rectangle(100.0, 400.0);
        assert_eq!(reveal_offset(viewport, 0.0, rectangle(150.0, 100.0)), None);
        assert_eq!(
            reveal_offset(viewport, 200.0, rectangle(350.0, 100.0)),
            None
        );
    }

    #[test]
    fn the_tabs_overflow_past_the_row_less_the_room_of_the_button_when_shown() {
        use super::overflows;
        assert!(!overflows(400.0, 400.0, 0.0), "just fitting");
        assert!(!overflows(400.4, 400.0, 0.0), "within the half pixel");
        assert!(overflows(401.0, 400.0, 0.0));
        // The button shown has narrowed the row to 400, 436 wide without it: tabs that fit
        // there do not hide the button, and it does not blink on and off.
        assert!(!overflows(430.0, 400.0, 36.0));
        assert!(overflows(437.0, 400.0, 36.0));
    }

    #[test]
    fn a_tab_past_the_end_is_brought_to_the_end() {
        let viewport = rectangle(100.0, 400.0);
        // From 500 to 650 in the content; 400 wide: scrolled to 250.
        assert_eq!(
            reveal_offset(viewport, 0.0, rectangle(600.0, 150.0)),
            Some(250.0)
        );
    }

    #[test]
    fn a_tab_before_the_start_is_brought_to_the_start() {
        let viewport = rectangle(100.0, 400.0);
        assert_eq!(
            reveal_offset(viewport, 300.0, rectangle(150.0, 100.0)),
            Some(50.0)
        );
    }

    #[test]
    fn a_tab_wider_than_the_row_shows_its_start() {
        let viewport = rectangle(0.0, 100.0);
        assert_eq!(
            reveal_offset(viewport, 0.0, rectangle(20.0, 300.0)),
            Some(20.0)
        );
    }
}
