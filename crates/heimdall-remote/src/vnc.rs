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

//! VNC (RFB) sessions.
//!
//! Neither of the security types spoken here encrypts: the desktop and what is typed cross
//! the network in clear, and VNC Authentication does not prove who the server is. Reach a
//! VNC server over a network through an SSH tunnel.

mod auth;
mod protocol;
mod screen;
mod session;
mod zrle;

pub use auth::PASSWORD_BYTES;
pub use protocol::{MAX_CUT_TEXT, Rfb, RfbError, RfbEvent, SecurityPolicy, Version};
pub use screen::{MAX_SIDE, Rect, Screen};
pub use session::{
    AskPassword, CloseReason, DEFAULT_CONNECT_TIMEOUT, DEFAULT_HANDSHAKE_TIMEOUT, Framebuffer,
    SessionEnded, Transport, VncConfig, VncConnection, VncError, VncEvent, VncInput, VncSession,
    connect, given_password, handshake, start,
};

/// Port of display 0; display N listens on this plus N.
pub const DEFAULT_VNC_PORT: u16 = 5900;
