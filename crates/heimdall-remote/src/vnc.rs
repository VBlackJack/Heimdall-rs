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
//! Only the X509 subtypes of `VeNCrypt` encrypt: TLS 1.2 or 1.3, the server's certificate
//! checked before any password is sent. The rest, no authentication and VNC Authentication,
//! directly or inside Tight or `VeNCrypt`, leave the desktop and what is typed to cross the
//! network in clear, and VNC Authentication does not prove who the server is. Reach such a
//! server over a network through an SSH tunnel.

mod auth;
mod clipboard;
mod cursor;
mod hextile;
mod png;
mod protocol;
mod rre;
mod screen;
mod security;
mod session;
mod tight;
mod zrle;

pub use auth::{MAX_PLAIN_PASSWORD, MAX_PLAIN_USERNAME, PASSWORD_BYTES, TooLong};
pub use clipboard::MAX_EXTENDED_CUT_TEXT;
pub use cursor::{Cursor, MAX_CURSOR_SIDE, RemoteCursor};
pub use protocol::{MAX_CUT_TEXT, Quality, Rfb, RfbError, RfbEvent, SecurityPolicy, Version};
pub use screen::{MAX_SIDE, Rect, Screen};
pub use security::{Authentication, Security, SecurityWrapper};
pub use session::{
    AskPassword, CloseReason, DEFAULT_CONNECT_TIMEOUT, DEFAULT_HANDSHAKE_TIMEOUT, Framebuffer,
    SessionEnded, Transport, VncConfig, VncConnection, VncError, VncEvent, VncInput, VncSession,
    connect, given_password, handshake, start,
};

/// Port of display 0; display N listens on this plus N.
pub const DEFAULT_VNC_PORT: u16 = 5900;
