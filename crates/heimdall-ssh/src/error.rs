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

//! Why a connection did not open.
//!
//! The variants carry data, not sentences: the UI turns them into localised text.

use std::io;

use russh::keys::PublicKey;
use thiserror::Error;

use crate::key_file::KeyFileError;
use crate::known_hosts::KnownHostsError;

/// An authentication method that was attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    /// A key held by the SSH agent.
    Agent,
    /// The profile's key file.
    KeyFile,
    /// Keyboard-interactive.
    KeyboardInteractive,
    /// Password.
    Password,
}

/// Why [`crate::connect`] failed.
#[derive(Debug, Error)]
pub enum ConnectError {
    /// The host name cannot be used safely.
    #[error("invalid host name")]
    InvalidHost,
    /// The TCP connection failed.
    #[error("network: {0}")]
    Network(#[source] io::Error),
    /// TCP connect plus key exchange took longer than allowed.
    #[error("connection timed out")]
    Timeout,
    /// No key is recorded for this host. Ask the user, then call
    /// [`crate::KnownHosts::learn`] and connect again.
    #[error("unknown host key")]
    UnknownHostKey {
        /// Host as normalised for the `known_hosts` file.
        host: String,
        /// Port.
        port: u16,
        /// Key the server presented.
        key: Box<PublicKey>,
    },
    /// The server presented a different key of the algorithm recorded for it.
    #[error("host key changed")]
    HostKeyChanged {
        /// Host.
        host: String,
        /// Port.
        port: u16,
        /// SHA-256 fingerprint of the recorded key.
        recorded: String,
        /// SHA-256 fingerprint of the presented key.
        offered: String,
    },
    /// The server no longer offers any algorithm recorded for it.
    #[error("host key algorithm mismatch")]
    HostKeyAlgorithmMismatch {
        /// Host.
        host: String,
        /// Port.
        port: u16,
        /// Algorithms recorded in `known_hosts`.
        recorded: Vec<String>,
    },
    /// The server presented a host certificate; certificate authorities are not supported.
    #[error("host certificate refused")]
    HostCertificateRefused,
    /// The `known_hosts` file could not be used.
    #[error(transparent)]
    KnownHosts(KnownHostsError),
    /// The profile's key file could not be used.
    #[error(transparent)]
    KeyFile(#[from] KeyFileError),
    /// Every method the server allows was tried and refused.
    #[error("authentication failed")]
    AuthenticationFailed {
        /// Methods attempted, in order.
        tried: Vec<AuthMethod>,
    },
    /// The server closed the connection.
    #[error("disconnected by the server")]
    Disconnected {
        /// Message the server sent with its disconnect, if any.
        server_message: Option<String>,
    },
    /// The user cancelled, or the connection was closed while waiting for the user.
    #[error("cancelled")]
    Cancelled,
    /// The user did not answer in time.
    #[error("no answer in time")]
    PromptTimedOut,
    /// The server refused to allocate a terminal.
    #[error("terminal refused")]
    PtyRefused,
    /// The server refused to start a shell.
    #[error("shell refused")]
    ShellRefused,
    /// The server refused to start a subsystem, such as `sftp`.
    #[error("subsystem {name} refused")]
    SubsystemRefused {
        /// Subsystem asked for.
        name: String,
    },
    /// A gateway on the way refused to open a connection onward, to the next gateway or to
    /// the server: forwarding is off there, or the next host cannot be reached from it.
    #[error("the gateway refused to reach {host}:{port}")]
    JumpRefused {
        /// Host the gateway was asked to reach.
        host: String,
        /// Port.
        port: u16,
    },
    /// Any other SSH protocol error.
    #[error("ssh: {0}")]
    Protocol(#[source] russh::Error),
}
