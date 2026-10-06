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

//! A split tab drawn as the C# `SplitContainerControl`: its panes side by side or stacked,
//! a divider between two sides, each pane under a slim header.
//!
//! The divider is dragged with the layout following live, the share kept by the window
//! and handed to the application once let go; a double click gives the sides half each. A
//! click on the divider gives it the keyboard: the arrows along its axis move it a step,
//! Escape or a click elsewhere gives the keyboard back.
//!
//! A press anywhere in a pane gives that pane the keyboard. It is seen here, before the
//! pane's own widgets: a terminal or a desktop takes its presses for its session, so a
//! `mouse_area` around it would never see them.

use heimdall_app::split::{Axis, MAX_RATIO, MIN_RATIO, Node};
use heimdall_app::{Message as AppMessage, TabId};
use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse::Click;
use iced::advanced::mouse::click::Kind;
use iced::advanced::widget::{Id, Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, overlay, renderer};
use iced::keyboard::key::Named;
use iced::widget::{button, column, container, mouse_area, row, text};
use iced::{Color, Element, Event, Length, Point, Rectangle, Size, Theme, Vector, keyboard, mouse};

use crate::i18n::fl;
use crate::shell::Message;
use crate::tree_view::TreeMenu;

/// Thickness of the divider between two sides, in logical pixels.
pub const DIVIDER: f32 = 6.0;

/// Narrowest a pane is drawn while there is room for it, as the C# pane's minimum.
pub const MIN_PANE_WIDTH: f32 = 120.0;

/// Shortest a pane is drawn while there is room for it, as the C# pane's minimum.
pub const MIN_PANE_HEIGHT: f32 = 80.0;

/// The share an arrow key moves a divider holding the keyboard.
pub const NUDGE: f32 = 0.05;

/// Width of the accent border round the pane with the keyboard, as the C# one's.
pub const FOCUS_EDGE: f32 = 2.0;

/// The outer split's divider: the first numbered, and the only one while a tab takes two
/// panes.
pub const OUTER_DIVIDER: usize = 0;

/// Size of a pane header's text.
const HEADER_TEXT_SIZE: f32 = 12.0;

/// Room inside a pane header.
const HEADER_PADDING: f32 = 2.0;

/// Gap between the marks of a pane header.
const HEADER_SPACING: f32 = 4.0;

/// A split as drawn: its panes numbered first to last, its dividers in the order of a walk
/// from the outer split, each side before the other.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// The pane of this number.
    Pane(usize),
    /// Two sides.
    Split {
        /// How they are placed.
        axis: Axis,
        /// The share the first takes.
        ratio: f32,
        /// The side left or above.
        first: Box<Shape>,
        /// The side right or below.
        second: Box<Shape>,
    },
}

impl Shape {
    /// The shape of `node`; `live`, a divider's number and the share it is being dragged to,
    /// drawn in place of its own.
    #[must_use]
    pub fn of(node: &Node, live: Option<(usize, f32)>) -> Self {
        let (mut panes, mut dividers) = (0, 0);
        Self::numbered(node, live, &mut panes, &mut dividers)
    }

    fn numbered(
        node: &Node,
        live: Option<(usize, f32)>,
        panes: &mut usize,
        dividers: &mut usize,
    ) -> Self {
        match node {
            Node::Leaf(_) => {
                *panes += 1;
                Self::Pane(*panes - 1)
            }
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let number = *dividers;
                *dividers += 1;
                let ratio = match live {
                    Some((dragged, ratio)) if dragged == number => ratio,
                    _ => *ratio,
                };
                Self::Split {
                    axis: *axis,
                    ratio,
                    first: Box::new(Self::numbered(first, live, panes, dividers)),
                    second: Box::new(Self::numbered(second, live, panes, dividers)),
                }
            }
        }
    }
}

/// The split drawn, for a test to find where it is.
#[must_use]
pub fn area_id() -> Id {
    Id::new("split-view")
}

/// A divider as laid out.
#[derive(Debug, Clone, Copy)]
struct Divider {
    axis: Axis,
    ratio: f32,
    /// What it covers.
    bar: Rectangle,
    /// The two sides and itself.
    span: Rectangle,
}

/// Where every pane and divider of a shape goes within some bounds.
#[derive(Debug, Default)]
struct Regions {
    panes: Vec<Rectangle>,
    dividers: Vec<Divider>,
}

impl Regions {
    fn of(shape: &Shape, bounds: Rectangle) -> Self {
        let mut regions = Self::default();
        regions.place(shape, bounds);
        regions
    }

    fn place(&mut self, shape: &Shape, bounds: Rectangle) {
        match shape {
            Shape::Pane(_) => self.panes.push(bounds),
            Shape::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let (before, bar, after) = cut(*axis, *ratio, bounds);
                self.dividers.push(Divider {
                    axis: *axis,
                    ratio: *ratio,
                    bar,
                    span: bounds,
                });
                self.place(first, before);
                self.place(second, after);
            }
        }
    }

    /// The divider under `position`.
    fn divider_at(&self, position: Point) -> Option<usize> {
        self.dividers
            .iter()
            .position(|divider| divider.bar.contains(position))
    }
}

/// The room a split's sides share along `axis` within `span`, and the least each takes
/// while there is room for both.
fn room(axis: Axis, span: Rectangle) -> (f32, f32) {
    let (along, least) = match axis {
        Axis::SideBySide => (span.width, MIN_PANE_WIDTH),
        Axis::Stacked => (span.height, MIN_PANE_HEIGHT),
    };
    ((along - DIVIDER).max(0.0), least)
}

/// The length the first side takes of `room` at `ratio`, each side kept to `least` while
/// there is room for both.
fn first_length(ratio: f32, room: f32, least: f32) -> f32 {
    let length = room * ratio;
    if room >= 2.0 * least {
        length.clamp(least, room - least)
    } else {
        length
    }
}

/// `span` cut along `axis` at `ratio`: the first side, the divider, the second side.
fn cut(axis: Axis, ratio: f32, span: Rectangle) -> (Rectangle, Rectangle, Rectangle) {
    let (room, least) = room(axis, span);
    let first = first_length(ratio, room, least);
    let second = room - first;
    match axis {
        Axis::SideBySide => (
            Rectangle::new(span.position(), Size::new(first, span.height)),
            Rectangle::new(
                Point::new(span.x + first, span.y),
                Size::new(DIVIDER, span.height),
            ),
            Rectangle::new(
                Point::new(span.x + first + DIVIDER, span.y),
                Size::new(second, span.height),
            ),
        ),
        Axis::Stacked => (
            Rectangle::new(span.position(), Size::new(span.width, first)),
            Rectangle::new(
                Point::new(span.x, span.y + first),
                Size::new(span.width, DIVIDER),
            ),
            Rectangle::new(
                Point::new(span.x, span.y + first + DIVIDER),
                Size::new(span.width, second),
            ),
        ),
    }
}

/// The share of a divider dragged to `position`, within the C# clamp and the panes' least
/// sizes.
fn ratio_at(divider: &Divider, position: Point) -> f32 {
    let (room, least) = room(divider.axis, divider.span);
    if room <= 0.0 {
        return divider.ratio;
    }
    let offset = match divider.axis {
        Axis::SideBySide => position.x - divider.span.x,
        Axis::Stacked => position.y - divider.span.y,
    } - DIVIDER / 2.0;
    let ratio = (offset / room).clamp(MIN_RATIO, MAX_RATIO);
    if room >= 2.0 * least {
        ratio.clamp(least / room, (room - least) / room)
    } else {
        ratio
    }
}

/// What the split keeps between frames.
#[derive(Debug, Default)]
struct State {
    /// The divider being dragged and the share it is at, while held; whether it moved.
    drag: Option<(usize, f32, bool)>,
    /// The divider holding the keyboard.
    keyboard: Option<usize>,
    /// The last press on a divider, for a double click.
    last_click: Option<Click>,
}

/// A message made from a number.
type Numbered<'a, M> = Box<dyn Fn(usize) -> M + 'a>;
/// A message made from a divider's number and a share.
type Shared<'a, M> = Box<dyn Fn(usize, f32) -> M + 'a>;

/// A split tab's panes, laid out by a [`Shape`].
pub struct SplitView<'a, M> {
    id: Option<Id>,
    shape: Shape,
    panes: Vec<Element<'a, M>>,
    focused: Option<usize>,
    on_focus: Option<Numbered<'a, M>>,
    on_drag: Option<Shared<'a, M>>,
    on_release: Option<Shared<'a, M>>,
    on_reset: Option<Numbered<'a, M>>,
}

impl<'a, M> SplitView<'a, M> {
    /// `panes`, one per pane of `shape`, in their order.
    #[must_use]
    pub fn new(shape: Shape, panes: Vec<Element<'a, M>>) -> Self {
        Self {
            id: None,
            shape,
            panes,
            focused: None,
            on_focus: None,
            on_drag: None,
            on_release: None,
            on_reset: None,
        }
    }

    /// The identifier its area is found by.
    #[must_use]
    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    /// The pane with the keyboard: a press in another one asks for it with `on_focus`.
    #[must_use]
    pub fn focused(mut self, pane: Option<usize>) -> Self {
        self.focused = pane;
        self
    }

    /// The message a press in a pane without the keyboard sends, from its number.
    #[must_use]
    pub fn on_focus(mut self, message: impl Fn(usize) -> M + 'a) -> Self {
        self.on_focus = Some(Box::new(message));
        self
    }

    /// The message each move of a dragged divider sends: its number, the share it is at.
    #[must_use]
    pub fn on_drag(mut self, message: impl Fn(usize, f32) -> M + 'a) -> Self {
        self.on_drag = Some(Box::new(message));
        self
    }

    /// The message a divider let go after a drag, or moved by an arrow key, sends.
    #[must_use]
    pub fn on_release(mut self, message: impl Fn(usize, f32) -> M + 'a) -> Self {
        self.on_release = Some(Box::new(message));
        self
    }

    /// The message a double click on a divider sends, from its number.
    #[must_use]
    pub fn on_reset(mut self, message: impl Fn(usize) -> M + 'a) -> Self {
        self.on_reset = Some(Box::new(message));
        self
    }

    /// A press on a divider: a drag starts, or a double click resets it; the divider takes
    /// the keyboard.
    fn press_divider(
        &self,
        state: &mut State,
        divider: (usize, &Divider),
        position: Point,
        shell: &mut Shell<'_, M>,
    ) {
        let (index, laid) = divider;
        let click = Click::new(position, mouse::Button::Left, state.last_click);
        state.last_click = Some(click);
        state.keyboard = Some(index);
        if click.kind() == Kind::Double {
            state.drag = None;
            if let Some(reset) = &self.on_reset {
                shell.publish(reset(index));
            }
        } else {
            state.drag = Some((index, laid.ratio, false));
        }
        shell.capture_event();
        shell.request_redraw();
    }

    /// The divider dragged, or holding the keyboard, answers a mouse move, its release or an
    /// arrow key; whether it did.
    fn divider_event(
        &self,
        state: &mut State,
        event: &Event,
        regions: &Regions,
        cursor: mouse::Cursor,
        shell: &mut Shell<'_, M>,
    ) -> bool {
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some((index, ratio, _)) = state.drag else {
                    return false;
                };
                let (Some(position), Some(divider)) =
                    (cursor.position(), regions.dividers.get(index))
                else {
                    return false;
                };
                let moved = ratio_at(divider, position);
                if (moved - ratio).abs() > f32::EPSILON {
                    state.drag = Some((index, moved, true));
                    if let Some(drag) = &self.on_drag {
                        shell.publish(drag(index, moved));
                    }
                }
                true
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let Some((index, ratio, moved)) = state.drag.take() else {
                    return false;
                };
                // Committed once let go, and only when it moved.
                if moved && let Some(release) = &self.on_release {
                    shell.publish(release(index, ratio));
                }
                true
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(named),
                ..
            }) => {
                let Some(index) = state.keyboard else {
                    return false;
                };
                let Some(divider) = regions.dividers.get(index) else {
                    state.keyboard = None;
                    return false;
                };
                let step = match (divider.axis, named) {
                    (Axis::SideBySide, Named::ArrowLeft) | (Axis::Stacked, Named::ArrowUp) => {
                        -NUDGE
                    }
                    (Axis::SideBySide, Named::ArrowRight) | (Axis::Stacked, Named::ArrowDown) => {
                        NUDGE
                    }
                    (_, Named::Escape) => {
                        state.keyboard = None;
                        return true;
                    }
                    _ => return false,
                };
                if let Some(release) = &self.on_release {
                    let ratio = (divider.ratio + step).clamp(MIN_RATIO, MAX_RATIO);
                    shell.publish(release(index, ratio));
                }
                true
            }
            _ => false,
        }
    }
}

impl<M> Widget<M, Theme, iced::Renderer> for SplitView<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        self.panes.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.panes);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.max();
        let regions = Regions::of(&self.shape, Rectangle::new(Point::ORIGIN, size));
        let children = self
            .panes
            .iter_mut()
            .zip(&mut tree.children)
            .zip(regions.panes)
            .map(|((pane, tree), region)| {
                pane.as_widget_mut()
                    .layout(
                        tree,
                        renderer,
                        &layout::Limits::new(Size::ZERO, region.size()),
                    )
                    .move_to(region.position())
            })
            .collect();
        layout::Node::with_children(size, children)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(self.id.as_ref(), layout.bounds());
        operation.traverse(&mut |operation| {
            for ((pane, tree), layout) in self
                .panes
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                pane.as_widget_mut()
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
        let regions = Regions::of(&self.shape, layout.bounds());
        if self.divider_event(state, event, &regions, cursor, shell) {
            shell.capture_event();
            shell.request_redraw();
            return;
        }
        if let Event::Mouse(mouse::Event::ButtonPressed(button)) = event {
            let position = cursor.position_over(layout.bounds());
            let divider = position.and_then(|at| Some((regions.divider_at(at)?, at)));
            if let (mouse::Button::Left, Some((index, at))) = (button, divider) {
                self.press_divider(state, (index, &regions.dividers[index]), at, shell);
                return;
            }
            state.keyboard = None;
            // Before the pane's widgets, which may take the press for their session.
            let pane = position.and_then(|at| regions.panes.iter().position(|r| r.contains(at)));
            if let (Some(pane), Some(focus)) = (pane, &self.on_focus)
                && Some(pane) != self.focused
            {
                shell.publish(focus(pane));
            }
        }
        for ((pane, tree), layout) in self
            .panes
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            pane.as_widget_mut().update(
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
        let regions = Regions::of(&self.shape, layout.bounds());
        let divider = state
            .drag
            .map(|(index, ..)| index)
            .or_else(|| cursor.position().and_then(|at| regions.divider_at(at)))
            .and_then(|index| regions.dividers.get(index));
        if let Some(divider) = divider {
            return match divider.axis {
                Axis::SideBySide => mouse::Interaction::ResizingHorizontally,
                Axis::Stacked => mouse::Interaction::ResizingVertically,
            };
        }
        self.panes
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((pane, tree), layout)| {
                pane.as_widget()
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

        for ((pane, tree), layout) in self.panes.iter().zip(&tree.children).zip(layout.children()) {
            pane.as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
        }
        let state = tree.state.downcast_ref::<State>();
        let regions = Regions::of(&self.shape, layout.bounds());
        let palette = theme.extended_palette();
        for (index, divider) in regions.dividers.iter().enumerate() {
            let held = state.drag.is_some_and(|(dragged, ..)| dragged == index)
                || state.keyboard == Some(index)
                || cursor.is_over(divider.bar);
            let color: Color = if held {
                palette.primary.strong.color
            } else {
                palette.background.strong.color
            };
            renderer.fill_quad(
                renderer::Quad {
                    bounds: divider.bar,
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
            &mut self.panes,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, M: 'a> From<SplitView<'a, M>> for Element<'a, M, Theme, iced::Renderer> {
    fn from(view: SplitView<'a, M>) -> Self {
        Element::new(view)
    }
}

/// A pane of a split tab: a slim header with `marks` before `label`, its close button at
/// its end and the tab's menu, as a pane's, on a right click, over `content`; an accent border round it
/// while it has the keyboard, as the C# pane's.
pub fn pane<'a>(
    tab: TabId,
    marks: Vec<Element<'a, Message>>,
    label: iced::widget::Row<'a, Message>,
    content: Element<'a, Message>,
    focused: bool,
) -> Element<'a, Message> {
    let close = button(text(fl!("ui-tab-close-button")).size(HEADER_TEXT_SIZE))
        .style(button::text)
        .padding([0.0, HEADER_PADDING])
        .on_press(Message::App(AppMessage::Split(
            heimdall_app::split::SplitMessage::ClosePane(tab),
        )));
    let header = mouse_area(
        container(
            row(marks)
                .push(label)
                .push(iced::widget::space::horizontal())
                .push(close)
                .spacing(HEADER_SPACING)
                .align_y(iced::Alignment::Center),
        )
        .padding(HEADER_PADDING)
        .width(Length::Fill)
        .style(container::rounded_box),
    )
    .on_right_press(Message::OpenTreeMenu(TreeMenu::Pane(tab)));
    container(column![header, container(content).height(Length::Fill)])
        .padding(FOCUS_EDGE)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |theme: &Theme| container::Style {
            border: iced::Border {
                color: if focused {
                    theme.extended_palette().primary.strong.color
                } else {
                    Color::TRANSPARENT
                },
                width: FOCUS_EDGE,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn divider(axis: Axis, ratio: f32) -> Divider {
        let span = Rectangle::new(Point::ORIGIN, Size::new(1000.0 + DIVIDER, 600.0 + DIVIDER));
        let (_, bar, _) = cut(axis, ratio, span);
        Divider {
            axis,
            ratio,
            bar,
            span,
        }
    }

    #[test]
    fn a_split_shares_its_room_between_the_divider_and_its_sides() {
        let span = Rectangle::new(Point::ORIGIN, Size::new(1000.0 + DIVIDER, 400.0));
        let (first, bar, second) = cut(Axis::SideBySide, 0.25, span);
        assert!((first.width - 250.0).abs() < 0.01);
        assert!((bar.x - 250.0).abs() < 0.01 && (bar.width - DIVIDER).abs() < 0.01);
        assert!((second.width - 750.0).abs() < 0.01);
        assert!((first.height - 400.0).abs() < 0.01);
    }

    #[test]
    fn a_pane_keeps_its_least_size_while_there_is_room() {
        let span = Rectangle::new(Point::ORIGIN, Size::new(400.0 + DIVIDER, 300.0));
        let (first, _, second) = cut(Axis::SideBySide, 0.1, span);
        assert!((first.width - MIN_PANE_WIDTH).abs() < 0.01, "{first:?}");
        assert!((second.width - (400.0 - MIN_PANE_WIDTH)).abs() < 0.01);
        let tight = Rectangle::new(Point::ORIGIN, Size::new(500.0, 100.0 + DIVIDER));
        let (above, _, _) = cut(Axis::Stacked, 0.5, tight);
        assert!(
            (above.height - 50.0).abs() < 0.01,
            "too little room: shared"
        );
    }

    #[test]
    fn a_dragged_divider_follows_the_pointer_within_the_clamp() {
        let side = divider(Axis::SideBySide, 0.5);
        let half = Point::new(500.0 + DIVIDER / 2.0, 10.0);
        assert!((ratio_at(&side, half) - 0.5).abs() < 0.001);
        assert!(
            (ratio_at(&side, Point::new(0.0, 10.0)) - 0.12).abs() < 0.001,
            "120 of 1000"
        );
        let stacked = divider(Axis::Stacked, 0.5);
        let lowest = ratio_at(&stacked, Point::new(10.0, 10_000.0));
        assert!(
            (lowest - (600.0 - MIN_PANE_HEIGHT) / 600.0).abs() < 0.001,
            "the pane below keeps its least height, within the clamp: {lowest}"
        );
        // Wide enough that a least width leaves more than the C# clamp: the clamp holds.
        let span = Rectangle::new(Point::ORIGIN, Size::new(3000.0 + DIVIDER, 600.0));
        let (_, bar, _) = cut(Axis::SideBySide, 0.5, span);
        let wide = Divider {
            axis: Axis::SideBySide,
            ratio: 0.5,
            bar,
            span,
        };
        let far = ratio_at(&wide, Point::new(10_000.0, 10.0));
        assert!((far - MAX_RATIO).abs() < 0.001, "the C# clamp: {far}");
    }
}
