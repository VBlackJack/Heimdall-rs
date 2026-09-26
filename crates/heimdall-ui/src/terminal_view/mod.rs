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

//! The terminal widget: draws a tab's screen and turns input into application messages.
//!
//! A custom widget rather than a canvas: it needs the input method (dead keys, IME), the
//! clipboard synchronously, and keys, mouse and paste published in the order they happened.
//! A canvas has none of these in iced 0.14, and keys through a global subscription can be
//! dropped when its queue is full.

pub mod keys;
pub mod metrics;
pub mod runs;

use std::cell::RefCell;
use std::time::{Duration, Instant};

use heimdall_app::{Message as AppMessage, PointerInput, TabId};
use heimdall_term::{
    CellPoint, CursorStyle, GridSize, MouseAction, MouseButton, Rgb, Screen, ScreenCursor, Terminal,
};
use iced::advanced::clipboard::Kind;
use iced::advanced::input_method;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Renderer as _, Text};
use iced::advanced::widget::{Tree, tree};
use iced::advanced::{Clipboard, Renderer as _, Shell, Widget};
use iced::font::{Family, Style, Weight};
use iced::keyboard;
use iced::mouse;
use iced::{
    Background, Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme,
    alignment,
};

use crate::terminal_view::keys::{Shortcut, committed_text, key_input, shortcut};
use crate::terminal_view::metrics::{CellMetrics, as_f32};

/// Family name of the embedded terminal font.
pub const FONT_FAMILY: &str = "Source Code Pro";

/// Longest gap between presses counted as a double or triple click.
const MULTI_CLICK: Duration = Duration::from_millis(400);

/// Thickness of beam and underline cursors, and of text decorations, in pixels.
const LINE_THICKNESS: f32 = 2.0;

/// Background of selected cells (Dracula "current line").
const SELECTION_BACKGROUND: Rgb = Rgb {
    r: 0x44,
    g: 0x47,
    b: 0x5a,
};

fn color(rgb: Rgb) -> Color {
    Color::from_rgb8(rgb.r, rgb.g, rgb.b)
}

fn font(bold: bool, italic: bool) -> Font {
    Font {
        family: Family::Name(FONT_FAMILY),
        weight: if bold { Weight::Bold } else { Weight::Normal },
        style: if italic { Style::Italic } else { Style::Normal },
        ..Font::default()
    }
}

/// Per-widget state kept by iced between frames.
#[derive(Default)]
struct State {
    tab: Option<TabId>,
    focused: bool,
    grid: Option<GridSize>,
    held: Option<MouseButton>,
    last_press: Option<(Instant, usize, usize)>,
    clicks: u8,
    last_motion: Option<(usize, usize)>,
    wheel: f32,
    modifiers: keyboard::Modifiers,
    preedit: Option<String>,
    screen: RefCell<Option<Screen>>,
}

/// The terminal of one tab.
pub struct TerminalView<'a, M> {
    terminal: &'a Terminal,
    tab: TabId,
    metrics: CellMetrics,
    wrap: fn(AppMessage) -> M,
}

impl<'a, M> TerminalView<'a, M> {
    /// Shows `terminal`, the terminal of `tab`; messages are wrapped with `wrap`.
    #[must_use]
    pub fn new(terminal: &'a Terminal, tab: TabId, wrap: fn(AppMessage) -> M) -> Self {
        Self {
            terminal,
            tab,
            metrics: CellMetrics::default(),
            wrap,
        }
    }

    fn pointer(&self, state: &State, at: CellPoint, action: MouseAction) -> M {
        (self.wrap)(AppMessage::Pointer {
            tab: self.tab,
            input: PointerInput {
                at,
                action,
                modifiers: heimdall_term::Modifiers {
                    shift: state.modifiers.shift(),
                    ctrl: state.modifiers.control(),
                    alt: state.modifiers.alt(),
                },
                clicks: state.clicks,
            },
        })
    }
}

fn mouse_button(button: mouse::Button) -> Option<MouseButton> {
    match button {
        mouse::Button::Left => Some(MouseButton::Left),
        mouse::Button::Middle => Some(MouseButton::Middle),
        mouse::Button::Right => Some(MouseButton::Right),
        _ => None,
    }
}

impl<M> Widget<M, Theme, iced::Renderer> for TerminalView<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    #[allow(clippy::too_many_lines, reason = "one match over the input events")]
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();
        if state.tab != Some(self.tab) {
            // Another tab in the same place: nothing carries over, and it gets the focus.
            *state = State {
                tab: Some(self.tab),
                focused: true,
                ..State::default()
            };
        }
        let grid = self.metrics.grid(bounds.size());
        if state.grid != Some(grid) {
            state.grid = Some(grid);
            shell.publish((self.wrap)(AppMessage::Resize {
                tab: self.tab,
                grid,
                cell: self.metrics.pixels(),
            }));
        }
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                let Some(position) = cursor.position_over(bounds) else {
                    state.focused = false;
                    return;
                };
                state.focused = true;
                let Some(button) = mouse_button(*button) else {
                    return;
                };
                let at = self.metrics.cell_at(bounds, grid, position);
                let now = Instant::now();
                state.clicks = match state.last_press {
                    Some((when, row, col))
                        if now.duration_since(when) < MULTI_CLICK
                            && (row, col) == (at.row, at.col) =>
                    {
                        state.clicks % 3 + 1
                    }
                    _ => 1,
                };
                state.last_press = Some((now, at.row, at.col));
                state.held = Some(button);
                state.last_motion = Some((at.row, at.col));
                shell.publish(self.pointer(state, at, MouseAction::Press(button)));
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(button)) => {
                let Some(button) = mouse_button(*button).filter(|b| state.held == Some(*b)) else {
                    return;
                };
                state.held = None;
                let position = cursor.position().unwrap_or(bounds.position());
                let at = self.metrics.cell_at(bounds, grid, position);
                shell.publish(self.pointer(state, at, MouseAction::Release(button)));
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state.held.is_none() && !bounds.contains(*position) {
                    return;
                }
                let at = self.metrics.cell_at(bounds, grid, *position);
                if state.last_motion == Some((at.row, at.col)) {
                    return;
                }
                state.last_motion = Some((at.row, at.col));
                let held = state.held;
                shell.publish(self.pointer(state, at, MouseAction::Motion { held }));
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(position) = cursor.position_over(bounds) else {
                    return;
                };
                state.wheel += match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => *y / self.metrics.height,
                };
                let at = self.metrics.cell_at(bounds, grid, position);
                while state.wheel.abs() >= 1.0 {
                    let action = if state.wheel > 0.0 {
                        state.wheel -= 1.0;
                        MouseAction::WheelUp
                    } else {
                        state.wheel += 1.0;
                        MouseAction::WheelDown
                    };
                    state.clicks = 1;
                    shell.publish(self.pointer(state, at, action));
                }
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                physical_key,
                location,
                modifiers,
                text,
                repeat,
                ..
            }) if state.focused => {
                if let Some(action) = shortcut(key, *modifiers) {
                    let page = i32::try_from(grid.rows).unwrap_or(i32::MAX);
                    let message = match action {
                        Shortcut::Copy | Shortcut::Paste if *repeat => None,
                        Shortcut::Copy => Some(AppMessage::Copy(self.tab)),
                        Shortcut::Paste => Some(AppMessage::ClipboardText {
                            tab: self.tab,
                            text: clipboard.read(Kind::Standard),
                        }),
                        Shortcut::PageUp => Some(AppMessage::ScrollHistory {
                            tab: self.tab,
                            lines: page,
                        }),
                        Shortcut::PageDown => Some(AppMessage::ScrollHistory {
                            tab: self.tab,
                            lines: -page,
                        }),
                    };
                    if let Some(message) = message {
                        shell.publish((self.wrap)(message));
                    }
                    shell.capture_event();
                    return;
                }
                if let Some(input) =
                    key_input(key, *physical_key, *location, *modifiers, text.as_deref())
                {
                    shell.publish((self.wrap)(AppMessage::Key {
                        tab: self.tab,
                        input,
                    }));
                    shell.capture_event();
                }
            }
            Event::InputMethod(input_method::Event::Commit(committed)) if state.focused => {
                state.preedit = None;
                if let Some(input) = committed_text(committed) {
                    shell.publish((self.wrap)(AppMessage::Key {
                        tab: self.tab,
                        input,
                    }));
                }
                shell.capture_event();
            }
            Event::InputMethod(input_method::Event::Preedit(content, _)) if state.focused => {
                state.preedit = (!content.is_empty()).then(|| content.clone());
            }
            _ => {}
        }

        if state.focused {
            let cursor_cell = state
                .screen
                .borrow()
                .as_ref()
                .and_then(|screen| screen.cursor)
                .map_or(Rectangle::new(bounds.position(), Size::ZERO), |c| {
                    Rectangle::new(
                        self.metrics.origin(bounds, c.row, c.col),
                        Size::new(self.metrics.width, self.metrics.height),
                    )
                });
            shell.request_input_method(&input_method::InputMethod::Enabled {
                cursor: cursor_cell,
                purpose: input_method::Purpose::Terminal,
                preedit: state.preedit.as_ref().map(|content| input_method::Preedit {
                    content: content.clone(),
                    selection: None,
                    text_size: Some(Pixels(self.metrics.font_size)),
                }),
            });
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State>();
        let mut slot = state.screen.borrow_mut();
        let screen = match slot.as_mut() {
            Some(screen) => {
                self.terminal.snapshot_into(screen);
                screen
            }
            None => slot.insert(self.terminal.snapshot()),
        };
        fill(renderer, bounds, screen.background);
        for row in 0..screen.rows {
            let cells = &screen.cells[row * screen.cols..(row + 1) * screen.cols];
            for run in runs::backgrounds(cells, screen.background, SELECTION_BACKGROUND) {
                fill(
                    renderer,
                    self.cells_rect(bounds, row, run.col, run.cells),
                    run.color,
                );
            }
            for run in runs::texts(cells) {
                self.text(renderer, bounds, row, &run);
            }
            for run in runs::lines(cells) {
                let area = self.cells_rect(bounds, row, run.col, run.cells);
                let y = if run.underline {
                    area.y + area.height - LINE_THICKNESS
                } else {
                    area.y + area.height / 2.0
                };
                fill(
                    renderer,
                    Rectangle::new(Point::new(area.x, y), Size::new(area.width, 1.0)),
                    run.color,
                );
            }
        }
        if let Some(cursor) = screen.cursor {
            self.cursor(renderer, bounds, screen, cursor, state.focused);
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }
}

fn fill(renderer: &mut iced::Renderer, area: Rectangle, rgb: Rgb) {
    renderer.fill_quad(
        Quad {
            bounds: area,
            border: Border::default(),
            ..Quad::default()
        },
        Background::Color(color(rgb)),
    );
}

impl<M> TerminalView<'_, M> {
    fn cells_rect(&self, bounds: Rectangle, row: usize, col: usize, cells: usize) -> Rectangle {
        Rectangle::new(
            self.metrics.origin(bounds, row, col),
            Size::new(as_f32(cells) * self.metrics.width, self.metrics.height),
        )
    }

    fn text(
        &self,
        renderer: &mut iced::Renderer,
        bounds: Rectangle,
        row: usize,
        run: &runs::TextRun,
    ) {
        let area = self.cells_rect(bounds, row, run.col, run.cells);
        renderer.fill_text(
            Text {
                content: run.content.clone(),
                bounds: area.size(),
                size: Pixels(self.metrics.font_size),
                line_height: text::LineHeight::Absolute(Pixels(self.metrics.height)),
                font: font(run.bold, run.italic),
                align_x: text::Alignment::Left,
                align_y: alignment::Vertical::Top,
                shaping: if run.ascii {
                    text::Shaping::Basic
                } else {
                    text::Shaping::Advanced
                },
                wrapping: text::Wrapping::None,
            },
            area.position(),
            color(run.fg),
            bounds,
        );
    }

    fn cursor(
        &self,
        renderer: &mut iced::Renderer,
        bounds: Rectangle,
        screen: &Screen,
        cursor: ScreenCursor,
        focused: bool,
    ) {
        let cells = if cursor.wide { 2 } else { 1 };
        let area = self.cells_rect(bounds, cursor.row, cursor.col, cells);
        let style = if focused {
            cursor.style
        } else {
            CursorStyle::HollowBlock
        };
        match style {
            CursorStyle::Block => {
                fill(renderer, area, cursor.color);
                if let Some(cell) = screen.cell(cursor.row, cursor.col)
                    && cell.ch != ' '
                {
                    let mut content = String::from(cell.ch);
                    content.extend(cell.zerowidth.iter());
                    let run = runs::TextRun {
                        col: cursor.col,
                        cells,
                        content,
                        fg: screen.background,
                        bold: cell.bold,
                        italic: cell.italic,
                        ascii: cell.ch.is_ascii(),
                    };
                    self.text(renderer, bounds, cursor.row, &run);
                }
            }
            CursorStyle::Beam => fill(
                renderer,
                Rectangle::new(area.position(), Size::new(LINE_THICKNESS, area.height)),
                cursor.color,
            ),
            CursorStyle::Underline => fill(
                renderer,
                Rectangle::new(
                    Point::new(area.x, area.y + area.height - LINE_THICKNESS),
                    Size::new(area.width, LINE_THICKNESS),
                ),
                cursor.color,
            ),
            CursorStyle::HollowBlock => renderer.fill_quad(
                Quad {
                    bounds: area,
                    border: Border {
                        color: color(cursor.color),
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..Quad::default()
                },
                Background::Color(Color::TRANSPARENT),
            ),
        }
    }
}

impl<'a, M: 'a> From<TerminalView<'a, M>> for Element<'a, M> {
    fn from(view: TerminalView<'a, M>) -> Self {
        Element::new(view)
    }
}
