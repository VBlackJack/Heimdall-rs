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

//! The remote desktop of an RDP tab: drawn at its own size from the top-left corner, and
//! taking the keyboard and the mouse while it is shown.

use std::cell::RefCell;

use heimdall_app::{Message as AppMessage, RdpPane, TabId};
use heimdall_rdp::{MouseButton, MousePosition, Operation, Scancode, WheelRotations};
use iced::advanced::image::{self, FilterMethod, Renderer as _};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::keyboard::key::{Code, Physical};
use iced::{Element, Event, Length, Radians, Rectangle, Size, Theme, keyboard, mouse};

pub use scancodes::scancode;

/// Wheel units of one notch, as Windows counts them.
const WHEEL_NOTCH: f32 = 120.0;

/// Per-widget state kept by iced between frames.
#[derive(Default)]
struct State {
    tab: Option<TabId>,
    /// The picture last built, and the generation it shows.
    picture: RefCell<Option<(u64, image::Handle)>>,
}

/// The desktop of one RDP tab.
pub struct RdpView<'a, M> {
    pane: &'a RdpPane,
    tab: TabId,
    wrap: fn(AppMessage) -> M,
    interactive: bool,
}

impl<'a, M> RdpView<'a, M> {
    /// Shows `pane`, the desktop of `tab`; messages are wrapped with `wrap`.
    #[must_use]
    pub fn new(pane: &'a RdpPane, tab: TabId, wrap: fn(AppMessage) -> M) -> Self {
        Self {
            pane,
            tab,
            wrap,
            interactive: true,
        }
    }

    /// Whether the desktop takes input. A dialog over it turns it off.
    #[must_use]
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    fn send(&self, shell: &mut Shell<'_, M>, operations: Vec<Operation>) {
        shell.publish((self.wrap)(AppMessage::RdpInput {
            tab: self.tab,
            operations,
        }));
    }

    /// Where `position`, in window coordinates, falls on the desktop.
    fn desktop_point(&self, bounds: Rectangle, position: iced::Point) -> MousePosition {
        let (width, height) = self
            .pane
            .framebuffer
            .read(|width, height, _| (width, height));
        let clamp = |offset: f32, size: u16| {
            // Truncation to a pixel is intended; the value is clamped to the desktop first.
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped to 0..size before the conversion"
            )]
            let pixel = offset.clamp(0.0, f32::from(size.saturating_sub(1))) as u16;
            pixel
        };
        MousePosition {
            x: clamp(position.x - bounds.x, width),
            y: clamp(position.y - bounds.y, height),
        }
    }
}

fn mouse_button(button: mouse::Button) -> Option<MouseButton> {
    Some(match button {
        mouse::Button::Left => MouseButton::Left,
        mouse::Button::Middle => MouseButton::Middle,
        mouse::Button::Right => MouseButton::Right,
        mouse::Button::Back => MouseButton::X1,
        mouse::Button::Forward => MouseButton::X2,
        mouse::Button::Other(_) => return None,
    })
}

impl<M> Widget<M, Theme, iced::Renderer> for RdpView<'_, M> {
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

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        if state.tab != Some(self.tab) {
            *state = State {
                tab: Some(self.tab),
                ..State::default()
            };
        }
        if !self.interactive {
            return;
        }
        let bounds = layout.bounds();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(position) = cursor.position_over(bounds) {
                    let at = self.desktop_point(bounds, position);
                    self.send(shell, vec![Operation::MouseMove(at)]);
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                let (Some(position), Some(button)) =
                    (cursor.position_over(bounds), mouse_button(*button))
                else {
                    return;
                };
                let at = self.desktop_point(bounds, position);
                self.send(
                    shell,
                    vec![
                        Operation::MouseMove(at),
                        Operation::MouseButtonPressed(button),
                    ],
                );
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(button)) => {
                if let Some(button) = mouse_button(*button) {
                    self.send(shell, vec![Operation::MouseButtonReleased(button)]);
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if cursor.position_over(bounds).is_none() {
                    return;
                }
                let (lines, vertical) = match delta {
                    mouse::ScrollDelta::Lines { x, y } if y.abs() >= x.abs() => (*y, true),
                    mouse::ScrollDelta::Lines { x, .. } => (*x, false),
                    // A pixel delta, from a touchpad: about one notch per ten pixels.
                    mouse::ScrollDelta::Pixels { x, y } if y.abs() >= x.abs() => (*y / 10.0, true),
                    mouse::ScrollDelta::Pixels { x, .. } => (*x / 10.0, false),
                };
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "clamped to the i16 range first"
                )]
                let units = (lines * WHEEL_NOTCH)
                    .round()
                    .clamp(f32::from(i16::MIN), f32::from(i16::MAX))
                    as i16;
                if units != 0 {
                    self.send(
                        shell,
                        vec![Operation::WheelRotations(WheelRotations {
                            is_vertical: vertical,
                            rotation_units: units,
                        })],
                    );
                }
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { physical_key, .. }) => {
                if let Some(code) = scancode(*physical_key) {
                    self.send(shell, vec![Operation::KeyPressed(code)]);
                    shell.capture_event();
                }
            }
            Event::Keyboard(keyboard::Event::KeyReleased { physical_key, .. }) => {
                if let Some(code) = scancode(*physical_key) {
                    self.send(shell, vec![Operation::KeyReleased(code)]);
                    shell.capture_event();
                }
            }
            _ => {}
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
        let mut picture = state.picture.borrow_mut();
        let current = picture
            .as_ref()
            .filter(|(generation, _)| *generation == self.pane.generation)
            .map(|(_, handle)| handle.clone());
        let handle = current.unwrap_or_else(|| {
            let handle = self.pane.framebuffer.read(|width, height, pixels| {
                image::Handle::from_rgba(u32::from(width), u32::from(height), pixels.to_vec())
            });
            *picture = Some((self.pane.generation, handle.clone()));
            handle
        });
        let (width, height) = self
            .pane
            .framebuffer
            .read(|width, height, _| (width, height));
        let area = Rectangle::new(
            bounds.position(),
            Size::new(f32::from(width), f32::from(height)),
        );
        renderer.draw_image(
            image::Image {
                handle,
                filter_method: FilterMethod::Nearest,
                rotation: Radians(0.0),
                border_radius: iced::border::Radius::default(),
                opacity: 1.0,
                snap: true,
            },
            area,
            bounds,
        );
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }
}

impl<'a, M: 'a> From<RdpView<'a, M>> for Element<'a, M, Theme, iced::Renderer> {
    fn from(view: RdpView<'a, M>) -> Self {
        Element::new(view)
    }
}

/// PC/AT set 1 scancodes, by the position of the key, whatever its label: the server maps
/// them with its own keyboard layout.
mod scancodes {
    use super::{Code, Physical, Scancode};

    /// The scancode of a physical key, when it has one the server knows.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one table row per key")]
    pub fn scancode(key: Physical) -> Option<Scancode> {
        let Physical::Code(code) = key else {
            return None;
        };
        let (extended, value) = match code {
            Code::Escape => (false, 0x01),
            Code::Digit1 => (false, 0x02),
            Code::Digit2 => (false, 0x03),
            Code::Digit3 => (false, 0x04),
            Code::Digit4 => (false, 0x05),
            Code::Digit5 => (false, 0x06),
            Code::Digit6 => (false, 0x07),
            Code::Digit7 => (false, 0x08),
            Code::Digit8 => (false, 0x09),
            Code::Digit9 => (false, 0x0A),
            Code::Digit0 => (false, 0x0B),
            Code::Minus => (false, 0x0C),
            Code::Equal => (false, 0x0D),
            Code::Backspace => (false, 0x0E),
            Code::Tab => (false, 0x0F),
            Code::KeyQ => (false, 0x10),
            Code::KeyW => (false, 0x11),
            Code::KeyE => (false, 0x12),
            Code::KeyR => (false, 0x13),
            Code::KeyT => (false, 0x14),
            Code::KeyY => (false, 0x15),
            Code::KeyU => (false, 0x16),
            Code::KeyI => (false, 0x17),
            Code::KeyO => (false, 0x18),
            Code::KeyP => (false, 0x19),
            Code::BracketLeft => (false, 0x1A),
            Code::BracketRight => (false, 0x1B),
            Code::Enter => (false, 0x1C),
            Code::ControlLeft => (false, 0x1D),
            Code::KeyA => (false, 0x1E),
            Code::KeyS => (false, 0x1F),
            Code::KeyD => (false, 0x20),
            Code::KeyF => (false, 0x21),
            Code::KeyG => (false, 0x22),
            Code::KeyH => (false, 0x23),
            Code::KeyJ => (false, 0x24),
            Code::KeyK => (false, 0x25),
            Code::KeyL => (false, 0x26),
            Code::Semicolon => (false, 0x27),
            Code::Quote => (false, 0x28),
            Code::Backquote => (false, 0x29),
            Code::ShiftLeft => (false, 0x2A),
            Code::Backslash => (false, 0x2B),
            Code::KeyZ => (false, 0x2C),
            Code::KeyX => (false, 0x2D),
            Code::KeyC => (false, 0x2E),
            Code::KeyV => (false, 0x2F),
            Code::KeyB => (false, 0x30),
            Code::KeyN => (false, 0x31),
            Code::KeyM => (false, 0x32),
            Code::Comma => (false, 0x33),
            Code::Period => (false, 0x34),
            Code::Slash => (false, 0x35),
            Code::ShiftRight => (false, 0x36),
            Code::NumpadMultiply => (false, 0x37),
            Code::AltLeft => (false, 0x38),
            Code::Space => (false, 0x39),
            Code::CapsLock => (false, 0x3A),
            Code::F1 => (false, 0x3B),
            Code::F2 => (false, 0x3C),
            Code::F3 => (false, 0x3D),
            Code::F4 => (false, 0x3E),
            Code::F5 => (false, 0x3F),
            Code::F6 => (false, 0x40),
            Code::F7 => (false, 0x41),
            Code::F8 => (false, 0x42),
            Code::F9 => (false, 0x43),
            Code::F10 => (false, 0x44),
            Code::NumLock => (false, 0x45),
            Code::ScrollLock => (false, 0x46),
            Code::Numpad7 => (false, 0x47),
            Code::Numpad8 => (false, 0x48),
            Code::Numpad9 => (false, 0x49),
            Code::NumpadSubtract => (false, 0x4A),
            Code::Numpad4 => (false, 0x4B),
            Code::Numpad5 => (false, 0x4C),
            Code::Numpad6 => (false, 0x4D),
            Code::NumpadAdd => (false, 0x4E),
            Code::Numpad1 => (false, 0x4F),
            Code::Numpad2 => (false, 0x50),
            Code::Numpad3 => (false, 0x51),
            Code::Numpad0 => (false, 0x52),
            Code::NumpadDecimal => (false, 0x53),
            Code::IntlBackslash => (false, 0x56),
            Code::F11 => (false, 0x57),
            Code::F12 => (false, 0x58),
            Code::NumpadEnter => (true, 0x1C),
            Code::ControlRight => (true, 0x1D),
            Code::NumpadDivide => (true, 0x35),
            Code::PrintScreen => (true, 0x37),
            Code::AltRight => (true, 0x38),
            Code::Home => (true, 0x47),
            Code::ArrowUp => (true, 0x48),
            Code::PageUp => (true, 0x49),
            Code::ArrowLeft => (true, 0x4B),
            Code::ArrowRight => (true, 0x4D),
            Code::End => (true, 0x4F),
            Code::ArrowDown => (true, 0x50),
            Code::PageDown => (true, 0x51),
            Code::Insert => (true, 0x52),
            Code::Delete => (true, 0x53),
            Code::SuperLeft => (true, 0x5B),
            Code::SuperRight => (true, 0x5C),
            Code::ContextMenu => (true, 0x5D),
            _ => return None,
        };
        Some(Scancode::from_u8(extended, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_their_set_1_scancodes_by_position() {
        let code = |code| scancode(Physical::Code(code));
        assert_eq!(code(Code::KeyA), Some(Scancode::from_u8(false, 0x1E)));
        assert_eq!(code(Code::KeyQ), Some(Scancode::from_u8(false, 0x10)));
        assert_eq!(code(Code::Enter), Some(Scancode::from_u8(false, 0x1C)));
        assert_eq!(code(Code::NumpadEnter), Some(Scancode::from_u8(true, 0x1C)));
        assert_eq!(code(Code::ArrowLeft), Some(Scancode::from_u8(true, 0x4B)));
        assert_eq!(code(Code::Delete), Some(Scancode::from_u8(true, 0x53)));
        assert_eq!(code(Code::F12), Some(Scancode::from_u8(false, 0x58)));
        assert_eq!(code(Code::Pause), None, "Pause needs its own sequence");
    }

    #[test]
    fn no_two_keys_share_a_scancode() {
        use std::collections::HashSet;
        let all = [
            Code::Escape,
            Code::Digit1,
            Code::Digit0,
            Code::Minus,
            Code::Equal,
            Code::Backspace,
            Code::Tab,
            Code::KeyQ,
            Code::KeyP,
            Code::BracketLeft,
            Code::BracketRight,
            Code::Enter,
            Code::ControlLeft,
            Code::KeyA,
            Code::KeyL,
            Code::Semicolon,
            Code::Quote,
            Code::Backquote,
            Code::ShiftLeft,
            Code::Backslash,
            Code::KeyZ,
            Code::KeyM,
            Code::Comma,
            Code::Period,
            Code::Slash,
            Code::ShiftRight,
            Code::NumpadMultiply,
            Code::AltLeft,
            Code::Space,
            Code::CapsLock,
            Code::F1,
            Code::F10,
            Code::NumLock,
            Code::ScrollLock,
            Code::Numpad7,
            Code::NumpadSubtract,
            Code::NumpadAdd,
            Code::Numpad0,
            Code::NumpadDecimal,
            Code::IntlBackslash,
            Code::F11,
            Code::F12,
            Code::NumpadEnter,
            Code::ControlRight,
            Code::NumpadDivide,
            Code::PrintScreen,
            Code::AltRight,
            Code::Home,
            Code::ArrowUp,
            Code::PageUp,
            Code::ArrowLeft,
            Code::ArrowRight,
            Code::End,
            Code::ArrowDown,
            Code::PageDown,
            Code::Insert,
            Code::Delete,
            Code::SuperLeft,
            Code::SuperRight,
            Code::ContextMenu,
        ];
        let mut seen = HashSet::new();
        for key in all {
            let code = scancode(Physical::Code(key)).expect("mapped");
            assert!(seen.insert(code), "{key:?} shares {code:?}");
        }
    }
}
