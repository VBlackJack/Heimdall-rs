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

//! SSH sessions, tunnels and jump hosts.
//!
//! [`connect`] opens an interactive shell to an [`SshProfile`](heimdall_core::profile::SshProfile).
//! It never asks a human anything during the key exchange: an unknown host key ends the
//! attempt with [`ConnectError::UnknownHostKey`], the caller asks the user, records the key
//! with [`KnownHosts::learn`], and connects again. Questions asked during authentication go
//! through a [`Prompter`], raced against cancellation and a deadline.

mod agent;
mod auth;
mod client;
mod connection;
mod error;
mod key_file;
mod known_hosts;
mod options;
mod prompter;
mod secret;
mod session;

pub use client::{connect, establish, establish_via};
pub use connection::{Connection, SubsystemStream};
pub use error::{AuthMethod, ConnectError};
pub use key_file::{KeyFile, KeyFileError, KeyFormat};
pub use known_hosts::{KnownHosts, KnownHostsError, Verdict, fingerprint, validate_host, verdict};
pub use options::{AgentSource, ConnectOptions, TerminalSize};
pub use prompter::{
    KeyboardInteractivePrompt, KeyboardInteractiveQuestion, PassphraseQuestion, PasswordQuestion,
    Prompter, UsernameQuestion,
};
pub use russh::keys::{Algorithm, PublicKey};
pub use secret::Secret;
pub use session::{SessionClosed, SessionEvent, SessionInput, ShellSession};
