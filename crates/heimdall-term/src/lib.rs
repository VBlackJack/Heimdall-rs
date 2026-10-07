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

//! Terminal emulation and local pseudo-terminals, independent of any UI toolkit.
//!
//! [`Terminal`] turns the server's output into a [`Screen`] to draw and replies to send
//! back. [`encode_key`], [`encode_mouse`] and [`encode_paste`] turn user input into bytes
//! for the server, following the modes the application set ([`Terminal::input_mode`]).
//! [`FeedOutput::working_directory`] carries the working folder a shell reports (OSC 7);
//! [`local_folder`] reads it as a folder of this computer.

pub mod keys;
pub mod local;
pub mod mode;
pub mod mouse;
pub mod palette;
pub mod paste;
pub mod plain;
pub mod terminal;
pub mod working_directory;

pub use keys::{Key, KeyLocation, KeyPress, Modifiers, NamedKey, encode_key};
pub use mode::{InputMode, MouseEncoding, MouseTracking};
pub use mouse::{
    MotionFilter, MouseAction, MouseButton, MouseEvent, encode_focus, encode_mouse, is_reported,
    wheel_as_arrows,
};
pub use palette::{Palette, Rgb};
pub use paste::encode_paste;
pub use plain::PlainText;
pub use terminal::{
    CellPixels, CellPoint, CellWidth, ClipboardPolicy, CursorStyle, FeedOutput, FindDirection,
    Found, GridSize, Screen, ScreenCell, ScreenCursor, SelectionKind, Terminal, TerminalConfig,
    TitleChange, Underline,
};
pub use working_directory::{
    MAX_REPORT_LENGTH, WorkingDirectoryScanner, local_folder, unix_folder, windows_folder,
};
