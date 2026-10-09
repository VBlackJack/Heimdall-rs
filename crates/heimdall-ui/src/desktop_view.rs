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

//! The remote desktop of a tab, whatever protocol draws it, taking the keyboard and the mouse
//! while it is shown. Two ways to show it, as the C# Heimdall's resolution menu has:
//!
//! - Match window: the tab's size is asked of the server, and the desktop drawn pixel for
//!   pixel from the tab's corner. Some servers (xrdp) change the size they paint without
//!   announcing a new desktop: what lies beyond the tab is not theirs any more, and is cut.
//! - Fit to window: nothing is asked; the whole desktop is drawn as large as the tab allows
//!   with its proportions, never larger than itself, centred. For a server that keeps its
//!   size, as a VNC one does.

use std::cell::RefCell;

use heimdall_app::{DesktopInput, DesktopPane, Message as AppMessage, PointerButton, TabId};
use heimdall_rdp::Scancode;
use iced::advanced::Renderer as _;
use iced::advanced::image::{self, FilterMethod, Renderer as _};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::keyboard::key::{Code, Physical};
use iced::{Element, Event, Length, Radians, Rectangle, Size, Theme, keyboard, mouse};
use iced_renderer::fallback;
use iced_renderer::wgpu::primitive::Renderer as _;

use crate::desktop_texture::Desktop;
use crate::keysym::keysym;
use crate::terminal_view::keys::{WindowShortcut, window_shortcut};

pub use scancodes::scancode;

/// Wheel units of one notch, as Windows counts them.
const WHEEL_NOTCH: f32 = 120.0;

/// Per-widget state kept by iced between frames.
#[derive(Default)]
struct State {
    tab: Option<TabId>,
    /// The picture last built, and the generation it shows.
    picture: RefCell<Option<(u64, image::Handle)>>,
    /// The size last asked of the server for the desktop, so a change is asked once.
    reported: Option<(u16, u16)>,
    /// The size last reported for the tab, asked or kept only, so a change is said once.
    shown: Option<(u16, u16)>,
    /// Keys held, with the keysym sent when each went down: a release sends the same one,
    /// or Shift released first would turn a held `!` into a `1` and leave `!` stuck.
    held: Vec<(Physical, u32)>,
}

/// The desktop of one tab.
pub struct DesktopView<'a, M> {
    pane: &'a DesktopPane,
    tab: TabId,
    wrap: fn(AppMessage) -> M,
    release: Option<M>,
    interactive: bool,
    fit: bool,
    density: f32,
}

impl<'a, M> DesktopView<'a, M> {
    /// Shows `pane`, the desktop of `tab`; messages are wrapped with `wrap`.
    #[must_use]
    pub fn new(pane: &'a DesktopPane, tab: TabId, wrap: fn(AppMessage) -> M) -> Self {
        Self {
            pane,
            tab,
            wrap,
            release: None,
            interactive: true,
            fit: false,
            density: 1.0,
        }
    }

    /// The window's screen draws `density` physical pixels per logical one: the desktop is
    /// asked for the tab's size in physical pixels and drawn one of its pixels per physical
    /// one, sharp, as mstsc on a high-density screen.
    #[must_use]
    pub fn density(mut self, density: f32) -> Self {
        if density.is_finite() && density > 0.0 {
            self.density = density;
        }
        self
    }

    /// Fit to window rather than match it: the whole desktop scaled into the tab, and its
    /// size never asked of the server.
    #[must_use]
    pub fn fit(mut self, fit: bool) -> Self {
        self.fit = fit;
        self
    }

    /// What Ctrl+Alt+Home publishes once every key held is let go on the server: the
    /// keyboard given back to the window, as the C# `RdpDefaultShortcuts.ReleaseFocus`.
    #[must_use]
    pub fn on_release(mut self, message: M) -> Self {
        self.release = Some(message);
        self
    }

    /// Whether the desktop takes input. A dialog over it turns it off.
    #[must_use]
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    fn send(&self, shell: &mut Shell<'_, M>, inputs: Vec<DesktopInput>) {
        shell.publish((self.wrap)(AppMessage::DesktopInput {
            tab: self.tab,
            inputs,
        }));
    }

    fn wheel(&self, shell: &mut Shell<'_, M>, delta: mouse::ScrollDelta) {
        let (lines, vertical) = match delta {
            mouse::ScrollDelta::Lines { x, y } if y.abs() >= x.abs() => (y, true),
            mouse::ScrollDelta::Lines { x, .. } => (x, false),
            // A pixel delta, from a touchpad: about one notch per ten pixels.
            mouse::ScrollDelta::Pixels { x, y } if y.abs() >= x.abs() => (y / 10.0, true),
            mouse::ScrollDelta::Pixels { x, .. } => (x / 10.0, false),
        };
        #[allow(
            clippy::cast_possible_truncation,
            reason = "clamped to the i16 range first"
        )]
        let units = (lines * WHEEL_NOTCH)
            .round()
            .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
        if units != 0 {
            self.send(shell, vec![DesktopInput::Wheel { vertical, units }]);
        }
    }

    /// Reports the size the desktop is shown at, in whole pixels, when it changed: the server
    /// is asked to match it.
    /// Says the tab's size: asked of the server when `ask`, else kept only, so the tab's size
    /// is known whatever the desktop's own.
    fn report_size(
        &self,
        state: &mut State,
        shell: &mut Shell<'_, M>,
        bounds: Rectangle,
        ask: bool,
    ) {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to the u16 range first"
        )]
        // Physical pixels: the desktop is drawn one of its pixels per physical one.
        let size = (
            (bounds.width * self.density).clamp(0.0, f32::from(u16::MAX)) as u16,
            (bounds.height * self.density).clamp(0.0, f32::from(u16::MAX)) as u16,
        );
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        if ask && state.reported != Some(size) {
            state.reported = Some(size);
            state.shown = Some(size);
            shell.publish((self.wrap)(AppMessage::DesktopResize {
                tab: self.tab,
                width: size.0,
                height: size.1,
            }));
        } else if !ask && state.shown != Some(size) {
            state.shown = Some(size);
            shell.publish((self.wrap)(AppMessage::DesktopShown {
                tab: self.tab,
                width: size.0,
                height: size.1,
            }));
        }
    }

    /// A key went down or up: sent by position (RDP) and by what it types (VNC).
    fn key(
        &self,
        held: &mut Vec<(Physical, u32)>,
        shell: &mut Shell<'_, M>,
        key: &keyboard::Key,
        location: keyboard::Location,
        physical_key: Physical,
        pressed: bool,
    ) {
        let found = held
            .iter()
            .position(|(physical, _)| *physical == physical_key);
        let keysym = if pressed {
            let keysym = keysym(key, location);
            if let (Some(keysym), None) = (keysym, found) {
                held.push((physical_key, keysym));
            }
            found.map(|index| held[index].1).or(keysym)
        } else {
            found
                .map(|index| held.remove(index).1)
                .or_else(|| keysym(key, location))
        };
        let code = scancode(physical_key);
        if code.is_none() && keysym.is_none() {
            return;
        }
        self.send(
            shell,
            vec![DesktopInput::Key {
                scancode: code,
                keysym,
                pressed,
            }],
        );
        shell.capture_event();
    }

    /// Where `position`, in window coordinates, falls on the desktop drawn in `bounds`.
    fn desktop_point(&self, bounds: Rectangle, position: iced::Point) -> (u16, u16) {
        let (width, height) = self
            .pane
            .framebuffer
            .read(|width, height, _| (width, height));
        desktop_point(
            bounds,
            (width, height),
            self.placement(),
            self.density,
            position,
        )
    }

    /// How the desktop is placed in its tab: fitted, or pixel for pixel, centred when it
    /// keeps a size of its own as the C# letterbox is.
    fn placement(&self) -> Placement {
        if self.fit {
            Placement::Fitted
        } else if self.pane.has_fixed_size() || self.pane.aspect != heimdall_app::Aspect::Stretch {
            Placement::Centred
        } else {
            Placement::Corner
        }
    }
}

/// Where a desktop is drawn in its tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// As large as the tab allows with its proportions, never larger than itself, centred.
    Fitted,
    /// Pixel for pixel from the tab's corner.
    Corner,
    /// Pixel for pixel, centred; from the corner on a side larger than the tab, the rest
    /// cut.
    Centred,
}

/// Where a desktop of `size` is drawn in `bounds`, on a screen of `density` physical pixels
/// per logical one, and at what scale: logical pixels per desktop pixel.
#[must_use]
pub fn placed(
    bounds: Rectangle,
    size: (u16, u16),
    placement: Placement,
    density: f32,
) -> (Rectangle, f32) {
    // One desktop pixel per physical pixel: its own size, in logical pixels.
    let own = 1.0 / density;
    let native = Size::new(f32::from(size.0) * own, f32::from(size.1) * own);
    match placement {
        Placement::Fitted => fitted(bounds, size, density),
        Placement::Corner => (Rectangle::new(bounds.position(), native), own),
        Placement::Centred => {
            let origin = iced::Point::new(
                bounds.x + ((bounds.width - native.width) / 2.0).max(0.0),
                bounds.y + ((bounds.height - native.height) / 2.0).max(0.0),
            );
            (Rectangle::new(origin, native), own)
        }
    }
}

/// Where a desktop of `size` is drawn in `bounds`: as large as fits with its proportions,
/// never larger than itself (one desktop pixel per physical pixel of a screen of `density`),
/// centred; and the scale it is drawn at.
#[must_use]
pub fn fitted(bounds: Rectangle, (width, height): (u16, u16), density: f32) -> (Rectangle, f32) {
    let (width, height) = (f32::from(width.max(1)), f32::from(height.max(1)));
    let scale = (bounds.width / width)
        .min(bounds.height / height)
        .min(1.0 / density);
    let size = Size::new(width * scale, height * scale);
    let origin = iced::Point::new(
        bounds.x + (bounds.width - size.width) / 2.0,
        bounds.y + (bounds.height - size.height) / 2.0,
    );
    (Rectangle::new(origin, size), scale)
}

/// Where `position`, in window coordinates, falls on a desktop of `size` drawn in `bounds`:
/// scaled back, and clamped to the desktop.
#[must_use]
pub fn desktop_point(
    bounds: Rectangle,
    size: (u16, u16),
    placement: Placement,
    density: f32,
    position: iced::Point,
) -> (u16, u16) {
    let (area, scale) = placed(bounds, size, placement, density);
    let clamp = |offset: f32, extent: u16| {
        // Truncation to a pixel is intended; the value is clamped to the desktop first.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to 0..extent before the conversion"
        )]
        let pixel = (offset / scale).clamp(0.0, f32::from(extent.saturating_sub(1))) as u16;
        pixel
    };
    (
        clamp(position.x - area.x, size.0),
        clamp(position.y - area.y, size.1),
    )
}

fn mouse_button(button: mouse::Button) -> Option<PointerButton> {
    Some(match button {
        mouse::Button::Left => PointerButton::Left,
        mouse::Button::Middle => PointerButton::Middle,
        mouse::Button::Right => PointerButton::Right,
        mouse::Button::Back => PointerButton::Back,
        mouse::Button::Forward => PointerButton::Forward,
        mouse::Button::Other(_) => return None,
    })
}

impl<M: Clone> DesktopView<'_, M> {
    /// A key pressed or let go while the desktop takes the keyboard: the server's, but for
    /// the window's own keys.
    fn keyboard(
        &self,
        held: &mut Vec<(Physical, u32)>,
        shell: &mut Shell<'_, M>,
        event: &keyboard::Event,
    ) {
        match event {
            // F11 is the window's: full screen, as in the C# Heimdall.
            keyboard::Event::KeyPressed { physical_key, .. }
            | keyboard::Event::KeyReleased { physical_key, .. }
                if *physical_key == Physical::Code(Code::F11) => {}
            // Quick Connect is the window's, as the C# keyboard hook takes Ctrl+K from its
            // RDP control: left uncaptured, and its K never sent, down or up. The status
            // copied for a screen reader too: it must work wherever the keyboard is.
            keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                ..
            }
            | keyboard::Event::KeyReleased {
                key,
                physical_key,
                modifiers,
                ..
            } if matches!(
                window_shortcut(key, *physical_key, *modifiers),
                Some(WindowShortcut::QuickConnect | WindowShortcut::CopyStatus)
            ) => {}
            keyboard::Event::KeyPressed {
                modified_key,
                physical_key,
                location,
                modifiers,
                ..
            } => {
                if self.gives_back(held, shell, *modifiers, *physical_key) {
                    return;
                }
                self.key(held, shell, modified_key, *location, *physical_key, true);
            }
            keyboard::Event::KeyReleased {
                modified_key,
                physical_key,
                location,
                ..
            } => {
                self.key(held, shell, modified_key, *location, *physical_key, false);
            }
            keyboard::Event::ModifiersChanged(_) => {}
        }
    }

    /// Every key `held` let go on the server, so none stays down there.
    fn release_held(&self, held: &mut Vec<(Physical, u32)>, shell: &mut Shell<'_, M>) {
        let releases: Vec<DesktopInput> = held
            .drain(..)
            .map(|(physical, keysym)| DesktopInput::Key {
                scancode: scancode(physical),
                keysym: Some(keysym),
                pressed: false,
            })
            .collect();
        if !releases.is_empty() {
            self.send(shell, releases);
        }
    }

    /// Ctrl+Alt+Home: every key the server holds down is let go, or Ctrl and Alt would stay,
    /// then the keyboard goes back to the window. Whether it was that.
    fn gives_back(
        &self,
        held: &mut Vec<(Physical, u32)>,
        shell: &mut Shell<'_, M>,
        modifiers: keyboard::Modifiers,
        physical_key: Physical,
    ) -> bool {
        let Some(release) = &self.release else {
            return false;
        };
        if !(modifiers.control() && modifiers.alt() && physical_key == Physical::Code(Code::Home)) {
            return false;
        }
        self.release_held(held, shell);
        shell.publish(release.clone());
        shell.capture_event();
        true
    }
}

impl<M: Clone> Widget<M, Theme, iced::Renderer> for DesktopView<'_, M> {
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
        let bounds = layout.bounds();
        // Fitted, the server keeps its own size, once it has had the tab's when its profile
        // asks for it once; a desktop of a size of its own is never asked another.
        let ask = self.pane.asks_tab_size() && (!self.fit || self.pane.wants_first_size());
        self.report_size(state, shell, bounds, ask);
        if !self.interactive {
            // Keys held when a dialog or Quick Connect came over it are let go on the server:
            // their releases go to the dialog.
            self.release_held(&mut state.held, shell);
            return;
        }
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(position) = cursor.position_over(bounds) {
                    let (x, y) = self.desktop_point(bounds, position);
                    self.send(shell, vec![DesktopInput::Move { x, y }]);
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                let (Some(position), Some(button)) =
                    (cursor.position_over(bounds), mouse_button(*button))
                else {
                    return;
                };
                let (x, y) = self.desktop_point(bounds, position);
                self.send(
                    shell,
                    vec![DesktopInput::Button {
                        button,
                        pressed: true,
                        x,
                        y,
                    }],
                );
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(button)) => {
                if let Some(button) = mouse_button(*button) {
                    // Released where the pointer is, even off the desktop: clamped to it.
                    let (x, y) = cursor
                        .position()
                        .map_or((0, 0), |position| self.desktop_point(bounds, position));
                    self.send(
                        shell,
                        vec![DesktopInput::Button {
                            button,
                            pressed: false,
                            x,
                            y,
                        }],
                    );
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if cursor.position_over(bounds).is_some() {
                    self.wheel(shell, *delta);
                    shell.capture_event();
                }
            }
            Event::Keyboard(event) => self.keyboard(&mut state.held, shell, event),
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
        let size = self
            .pane
            .framebuffer
            .read(|width, height, _| (width, height));
        let (area, _) = placed(bounds, size, self.placement(), self.density);
        // The GPU renderer keeps the desktop in a texture it rewrites; see `desktop_texture`.
        if matches!(renderer, fallback::Renderer::Primary(_)) {
            let desktop = Desktop {
                tab: self.tab,
                framebuffer: self.pane.framebuffer.clone(),
                generation: self.pane.generation,
            };
            renderer.with_layer(bounds, |renderer| renderer.draw_primitive(area, desktop));
            return;
        }
        let state = tree.state.downcast_ref::<State>();
        let mut picture = state.picture.borrow_mut();
        let current = picture
            .as_ref()
            .filter(|(generation, _)| *generation == self.pane.generation)
            .map(|(_, handle)| handle.clone());
        let handle = current.unwrap_or_else(|| {
            let handle = self.pane.framebuffer.read(|width, height, pixels| {
                // Opaque whatever alpha the decoder left, as the GPU path draws it: a new
                // desktop is zeros, which would let the window show through.
                let mut opaque = pixels.to_vec();
                for pixel in opaque.as_chunks_mut::<4>().0 {
                    pixel[3] = u8::MAX;
                }
                image::Handle::from_rgba(u32::from(width), u32::from(height), opaque)
            });
            *picture = Some((self.pane.generation, handle.clone()));
            handle
        });
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

impl<'a, M: Clone + 'a> From<DesktopView<'a, M>> for Element<'a, M, Theme, iced::Renderer> {
    fn from(view: DesktopView<'a, M>) -> Self {
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
    fn a_desktop_larger_than_its_tab_is_fitted_and_pointed_at_where_it_is_drawn() {
        let bounds = Rectangle::new(iced::Point::new(10.0, 20.0), Size::new(800.0, 600.0));
        // Twice as large, and wider than the tab's proportions: the width decides.
        let (area, scale) = fitted(bounds, (1920, 1080), 1.0);
        assert!((scale - 800.0 / 1920.0).abs() < f32::EPSILON);
        assert!((area.width - 800.0).abs() < 0.01);
        assert!((area.height - 450.0).abs() < 0.01);
        assert!((area.x - 10.0).abs() < 0.01);
        assert!((area.y - 95.0).abs() < 0.01, "centred: {area:?}");
        // The middle of what is drawn is the middle of the desktop.
        let middle = iced::Point::new(area.x + area.width / 2.0, area.y + area.height / 2.0);
        assert_eq!(
            desktop_point(bounds, (1920, 1080), Placement::Fitted, 1.0, middle),
            (960, 540)
        );
        // Beside the picture, the pointer is clamped to its edge.
        assert_eq!(
            desktop_point(
                bounds,
                (1920, 1080),
                Placement::Fitted,
                1.0,
                iced::Point::new(12.0, 21.0)
            ),
            (4, 0)
        );
        // Matched, the same point is the desktop's pixel under it, from the corner.
        assert_eq!(
            desktop_point(bounds, (1920, 1080), Placement::Corner, 1.0, middle),
            (400, 300)
        );
    }

    #[test]
    fn a_fixed_desktop_shown_pixel_for_pixel_is_centred_and_cut_where_larger() {
        let bounds = Rectangle::new(iced::Point::new(10.0, 20.0), Size::new(800.0, 600.0));
        // Narrower and shorter: centred both ways, unscaled.
        let (area, scale) = placed(bounds, (400, 300), Placement::Centred, 1.0);
        assert!((scale - 1.0).abs() < f32::EPSILON);
        assert!(
            (area.x - 210.0).abs() < 0.01 && (area.y - 170.0).abs() < 0.01,
            "{area:?}"
        );
        // Wider than the tab, shorter: from the left edge, centred up and down.
        let (area, _) = placed(bounds, (1024, 300), Placement::Centred, 1.0);
        assert!(
            (area.x - 10.0).abs() < 0.01 && (area.y - 170.0).abs() < 0.01,
            "{area:?}"
        );
        assert!(
            (area.width - 1024.0).abs() < 0.01,
            "unscaled, cut: {area:?}"
        );
        // The pointer lands where the desktop is drawn.
        assert_eq!(
            desktop_point(
                bounds,
                (400, 300),
                Placement::Centred,
                1.0,
                iced::Point::new(210.0, 170.0)
            ),
            (0, 0)
        );
    }

    #[test]
    fn a_desktop_smaller_than_its_tab_keeps_its_size_centred() {
        let bounds = Rectangle::new(iced::Point::ORIGIN, Size::new(1600.0, 900.0));
        let (area, scale) = fitted(bounds, (1024, 768), 1.0);
        assert!((scale - 1.0).abs() < f32::EPSILON, "never enlarged");
        assert!(
            (area.x - 288.0).abs() < 0.01 && (area.y - 66.0).abs() < 0.01,
            "{area:?}"
        );
        assert_eq!(
            desktop_point(
                bounds,
                (1024, 768),
                Placement::Fitted,
                1.0,
                iced::Point::new(288.0 + 100.0, 66.0 + 50.0)
            ),
            (100, 50)
        );
        let (area, scale) = placed(bounds, (1024, 768), Placement::Corner, 1.0);
        assert!((scale - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            area.position(),
            bounds.position(),
            "matched: from the corner"
        );
        // A tab matched by the server: drawn pixel for pixel where the tab starts.
        let (area, scale) = fitted(bounds, (1600, 900), 1.0);
        assert!((scale - 1.0).abs() < f32::EPSILON);
        assert_eq!(area, bounds);
    }

    #[test]
    fn on_a_dense_screen_one_desktop_pixel_takes_one_physical_pixel() {
        // A 150 % screen: the tab's 800 by 600 logical pixels are 1200 by 900 physical.
        let bounds = Rectangle::new(iced::Point::ORIGIN, Size::new(800.0, 600.0));
        let (area, scale) = placed(bounds, (1200, 900), Placement::Corner, 1.5);
        assert!((scale - 1.0 / 1.5).abs() < f32::EPSILON, "{scale}");
        assert!(
            (area.width - 800.0).abs() < 0.01 && (area.height - 600.0).abs() < 0.01,
            "the whole tab, sharp: {area:?}"
        );
        // The pointer at the tab's middle is the desktop's middle.
        let middle = iced::Point::new(400.0, 300.0);
        assert_eq!(
            desktop_point(bounds, (1200, 900), Placement::Corner, 1.5, middle),
            (600, 450)
        );
        // Fitted, a desktop already that small is never enlarged past one pixel per pixel.
        let (_, scale) = fitted(bounds, (600, 450), 1.5);
        assert!((scale - 1.0 / 1.5).abs() < f32::EPSILON, "{scale}");
    }

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
