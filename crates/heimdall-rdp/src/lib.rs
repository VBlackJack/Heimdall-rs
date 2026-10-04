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

//! RDP sessions on top of `IronRDP`.
//!
//! [`connect`] opens a connection with Network Level Authentication, trusting a server by
//! the key of its certificate as SSH trusts host keys: recorded on first use with the
//! user's agreement, refused if it changes. [`session::start`] then runs it.

pub mod audio;
pub mod certificate;
mod clipboard;
mod clipboard_files;
mod clipboard_save;
pub mod connect;
pub mod drives;
mod frames;
mod kdc;
pub mod known_hosts;
mod reason;
pub mod session;
mod time_zone;
mod tls;

pub use certificate::{Fingerprint, ServerCertificate};
pub use clipboard::{MAX_IMAGE_BYTES, MAX_REMOTE_TEXT_BYTES};
pub use clipboard_files::{CopyRefusal, MAX_COPY_BYTES, MAX_COPY_ENTRIES};
pub use clipboard_save::{SaveEnd, SaveRefusal};
pub use connect::{
    AskCredentials, Credentials, DESKTOP_SCALE_FACTORS, Opening, RdpConfig, RdpConnection,
    RdpError, Security, Timeouts, Transport, connect, connect_over, connect_through,
    desktop_scale_factor, given,
};
pub use ironrdp::input::{MouseButton, MousePosition, Operation, Scancode, WheelRotations};
pub use known_hosts::{KnownRdpHost, KnownRdpHosts, Verdict};
pub use reason::{Ending, Refusal};
pub use session::{CloseReason, Framebuffer, LocalClipboard, RdpEvent, RdpSession};
pub use time_zone::{TimeZone, Transition};
