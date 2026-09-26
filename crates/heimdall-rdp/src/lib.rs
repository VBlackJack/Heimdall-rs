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

pub mod certificate;
pub mod connect;
mod frames;
pub mod known_hosts;
pub mod session;
mod tls;

pub use certificate::{Fingerprint, ServerCertificate};
pub use connect::{RdpConfig, RdpConnection, RdpError, Security, Timeouts, connect, connect_over};
pub use ironrdp::input::{MouseButton, MousePosition, Operation, Scancode, WheelRotations};
pub use known_hosts::{KnownRdpHosts, Verdict};
pub use session::{CloseReason, Framebuffer, RdpEvent, RdpSession};
