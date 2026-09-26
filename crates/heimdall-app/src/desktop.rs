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

//! The remote desktop of a tab, whatever protocol draws it: its pixels, and what the user
//! does on it, translated for the protocol behind it.

use std::fmt;

use heimdall_rdp::{MouseButton, MousePosition, Operation, Scancode, WheelRotations};
use tokio::sync::mpsc;

/// The pixels of a desktop.
#[derive(Clone)]
pub enum DesktopFramebuffer {
    /// Decoded by an RDP session.
    Rdp(heimdall_rdp::Framebuffer),
}

impl fmt::Debug for DesktopFramebuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (width, height) = self.read(|width, height, _| (width, height));
        write!(f, "DesktopFramebuffer({width}x{height})")
    }
}

impl DesktopFramebuffer {
    /// Calls `read` with the width, height and RGBA pixels, rows top to bottom.
    pub fn read<T>(&self, read: impl FnOnce(u16, u16, &[u8]) -> T) -> T {
        match self {
            Self::Rdp(framebuffer) => framebuffer.read(read),
        }
    }
}

/// A pointer button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    /// Left.
    Left,
    /// Middle, or the wheel pressed.
    Middle,
    /// Right.
    Right,
    /// Back, the first side button.
    Back,
    /// Forward, the second side button.
    Forward,
}

/// What the user did on a desktop, before any protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopInput {
    /// The pointer moved to this pixel of the desktop.
    Move {
        /// Column.
        x: u16,
        /// Row.
        y: u16,
    },
    /// A button went down or up, the pointer at this pixel.
    Button {
        /// Which.
        button: PointerButton,
        /// Down, or up.
        pressed: bool,
        /// Column.
        x: u16,
        /// Row.
        y: u16,
    },
    /// The wheel turned, in units of 120 a notch: positive away from the user, or right.
    Wheel {
        /// Up and down, or left and right.
        vertical: bool,
        /// How far.
        units: i16,
    },
    /// A key went down or up. The view gives what it knows of it: where it is (a PC/AT set 1
    /// scancode) and what it types (an X11 keysym).
    Key {
        /// By its position, whatever its label.
        scancode: Option<Scancode>,
        /// By what it types.
        keysym: Option<u32>,
        /// Down, or up.
        pressed: bool,
    },
}

/// Where the input of a desktop goes.
enum DesktopSink {
    Rdp(mpsc::UnboundedSender<Vec<Operation>>),
}

/// The desktop of a tab, once its session is open.
pub struct DesktopPane {
    /// Its pixels.
    pub framebuffer: DesktopFramebuffer,
    /// Grows each time the desktop changes: tells the view to draw it again.
    pub generation: u64,
    sink: DesktopSink,
}

impl fmt::Debug for DesktopPane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DesktopPane")
            .field("framebuffer", &self.framebuffer)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl DesktopPane {
    /// The desktop of an RDP session.
    pub(crate) fn rdp(
        framebuffer: heimdall_rdp::Framebuffer,
        input: mpsc::UnboundedSender<Vec<Operation>>,
    ) -> Self {
        Self {
            framebuffer: DesktopFramebuffer::Rdp(framebuffer),
            generation: 0,
            sink: DesktopSink::Rdp(input),
        }
    }

    /// Sends `inputs` to the session, in the protocol's terms. A closed session drops its
    /// receiver: the input then goes nowhere, as it should.
    pub(crate) fn send(&self, inputs: &[DesktopInput]) {
        match &self.sink {
            DesktopSink::Rdp(sender) => {
                let operations = rdp_operations(inputs);
                if !operations.is_empty() {
                    let _ = sender.send(operations);
                }
            }
        }
    }
}

fn rdp_button(button: PointerButton) -> MouseButton {
    match button {
        PointerButton::Left => MouseButton::Left,
        PointerButton::Middle => MouseButton::Middle,
        PointerButton::Right => MouseButton::Right,
        PointerButton::Back => MouseButton::X1,
        PointerButton::Forward => MouseButton::X2,
    }
}

/// `inputs` as RDP operations. A key the view could not place gives none: RDP sends keys by
/// position.
fn rdp_operations(inputs: &[DesktopInput]) -> Vec<Operation> {
    let mut operations = Vec::with_capacity(inputs.len() * 2);
    for input in inputs {
        match *input {
            DesktopInput::Move { x, y } => {
                operations.push(Operation::MouseMove(MousePosition { x, y }));
            }
            DesktopInput::Button {
                button,
                pressed: true,
                x,
                y,
            } => {
                operations.push(Operation::MouseMove(MousePosition { x, y }));
                operations.push(Operation::MouseButtonPressed(rdp_button(button)));
            }
            DesktopInput::Button {
                button,
                pressed: false,
                ..
            } => operations.push(Operation::MouseButtonReleased(rdp_button(button))),
            DesktopInput::Wheel { vertical, units } => {
                operations.push(Operation::WheelRotations(WheelRotations {
                    is_vertical: vertical,
                    rotation_units: units,
                }));
            }
            DesktopInput::Key {
                scancode: Some(code),
                pressed,
                ..
            } => operations.push(if pressed {
                Operation::KeyPressed(code)
            } else {
                Operation::KeyReleased(code)
            }),
            DesktopInput::Key { scancode: None, .. } => {}
        }
    }
    operations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_moves_there_first_and_a_release_does_not() {
        let operations = rdp_operations(&[
            DesktopInput::Button {
                button: PointerButton::Back,
                pressed: true,
                x: 3,
                y: 4,
            },
            DesktopInput::Button {
                button: PointerButton::Back,
                pressed: false,
                x: 9,
                y: 9,
            },
        ]);
        assert!(
            matches!(
                operations.as_slice(),
                [
                    Operation::MouseMove(MousePosition { x: 3, y: 4 }),
                    Operation::MouseButtonPressed(MouseButton::X1),
                    Operation::MouseButtonReleased(MouseButton::X1),
                ]
            ),
            "{operations:?}"
        );
    }

    #[test]
    fn keys_go_by_scancode_and_one_without_is_dropped() {
        let enter = Scancode::from_u8(false, 0x1c);
        let operations = rdp_operations(&[
            DesktopInput::Key {
                scancode: Some(enter),
                keysym: Some(0xff0d),
                pressed: true,
            },
            DesktopInput::Key {
                scancode: None,
                keysym: Some(0x20ac),
                pressed: true,
            },
            DesktopInput::Key {
                scancode: Some(enter),
                keysym: None,
                pressed: false,
            },
        ]);
        assert!(
            matches!(
                operations.as_slice(),
                [Operation::KeyPressed(pressed), Operation::KeyReleased(released)]
                    if *pressed == enter && *released == enter
            ),
            "{operations:?}"
        );
    }

    #[test]
    fn the_wheel_keeps_its_axis_and_units() {
        let operations = rdp_operations(&[DesktopInput::Wheel {
            vertical: false,
            units: -240,
        }]);
        assert!(
            matches!(
                operations.as_slice(),
                [Operation::WheelRotations(WheelRotations {
                    is_vertical: false,
                    rotation_units: -240
                })]
            ),
            "{operations:?}"
        );
    }
}
