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
mod diagnose;
mod error;
mod forward;
mod key_file;
mod known_hosts;
mod known_hosts_export;
pub mod known_hosts_import;
pub mod local_forward;
mod options;
mod pins;
mod prompter;
mod run_trust;
mod secret;
mod session;
pub mod socks;

pub use agent::{AgentSurvey, survey as survey_agents};
pub use client::{
    Routed, at_gateway, connect, establish, establish_via, establish_via_keeping_gateway,
    trusted_host_key,
};
pub use connection::{
    ChannelBytes, CommandEnd, Connection, OUTPUT_LIMIT, SubsystemStream, Tunnel, WeakConnection,
};
pub use diagnose::{HopSecrets, Outcome, Step, StepOf, diagnose_route, has_trusted_key};
pub use error::{AuthMethod, ConnectError};
pub use forward::RemoteForward;
pub use key_file::{KeyFile, KeyFileError, KeyFormat};
pub use known_hosts::{
    KnownHostEntry, KnownHosts, KnownHostsError, Verdict, fingerprint, validate_host, verdict,
};
pub use known_hosts_export::KnownHostsExport;
pub use options::{AgentSource, ConnectOptions, TerminalSize};
pub use pins::{Carried, PinVerdict, Pins, carry_over, is_fingerprint, pin_verdict};
pub use prompter::{
    KeyboardInteractivePrompt, KeyboardInteractiveQuestion, PassphraseQuestion, PasswordQuestion,
    Prompter, UsernameQuestion,
};
pub use run_trust::RunTrust;
pub use russh::keys::{Algorithm, PublicKey};
pub use secret::Secret;
pub use session::{SessionClosed, SessionEvent, SessionInput, ShellSession};
