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

//! Connection settings and their defaults.

use crate::run_trust::RunTrust;
use heimdall_core::settings::AgentPreference;
use std::path::PathBuf;
use std::time::Duration;

/// Time allowed for the TCP connection plus the key exchange. No human wait falls inside it.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Time allowed for one answer from the user. Below the 120 s `LoginGraceTime` default of
/// OpenSSH, so the client gives up before the server silently drops the connection.
pub const DEFAULT_PROMPT_TIMEOUT: Duration = Duration::from_secs(110);

/// Interval between keepalive requests once authenticated.
pub const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);

/// Unanswered keepalives before the connection is considered dead.
pub const DEFAULT_KEEPALIVE_MAX: usize = 3;

/// `TERM` announced to the server.
pub const DEFAULT_TERMINAL_TYPE: &str = "xterm-256color";

/// Terminal size used until the UI reports the real one.
pub const DEFAULT_TERMINAL_SIZE: TerminalSize = TerminalSize {
    cols: 80,
    rows: 24,
    pixel_width: 0,
    pixel_height: 0,
};

/// Size of a terminal, in character cells and optionally in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    /// Columns.
    pub cols: u32,
    /// Rows.
    pub rows: u32,
    /// Width in pixels, 0 when unknown.
    pub pixel_width: u32,
    /// Height in pixels, 0 when unknown.
    pub pixel_height: u32,
}

/// Where to look for an SSH agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSource {
    /// Do not use an agent.
    Disabled,
    /// The platform's usual places: `SSH_AUTH_SOCK` on Unix; on Windows `SSH_AUTH_SOCK` when
    /// it names a pipe, then the OpenSSH agent pipe and Pageant in the order, or alone, as
    /// the preference says. Every one reachable offers its keys.
    Auto(AgentPreference),
    /// A Unix socket or a Windows named pipe given explicitly.
    Path(PathBuf),
    /// Several agents given explicitly, their keys offered in this order.
    Paths(Vec<PathBuf>),
}

/// Settings of one connection.
#[derive(Debug, Clone)]
pub struct ConnectOptions {
    /// Limit on TCP connect plus key exchange.
    pub connect_timeout: Duration,
    /// Limit on each answer from the user.
    pub prompt_timeout: Duration,
    /// Keepalive interval once authenticated.
    pub keepalive_interval: Duration,
    /// Unanswered keepalives tolerated.
    pub keepalive_max: usize,
    /// `TERM` announced to the server.
    pub terminal_type: String,
    /// Size of the PTY at creation.
    pub initial_size: TerminalSize,
    /// The `known_hosts` file owned by Heimdall-rs.
    pub known_hosts: PathBuf,
    /// Where to look for an SSH agent.
    pub agent: AgentSource,
    /// Forward the agent to the server's shell (`ssh -A`), as the C# "Forward SSH agent".
    pub forward_agent: bool,
    /// Compress the traffic of every connection on the way (`ssh -C`), as the C# "Enable
    /// compression".
    pub compression: bool,
    /// Keys trusted for this run only, counted as recorded beside `known_hosts`.
    pub run_trust: RunTrust,
}

impl ConnectOptions {
    /// Defaults, with host keys recorded in `known_hosts`.
    #[must_use]
    pub fn new(known_hosts: PathBuf) -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            prompt_timeout: DEFAULT_PROMPT_TIMEOUT,
            keepalive_interval: DEFAULT_KEEPALIVE_INTERVAL,
            keepalive_max: DEFAULT_KEEPALIVE_MAX,
            terminal_type: DEFAULT_TERMINAL_TYPE.to_owned(),
            initial_size: DEFAULT_TERMINAL_SIZE,
            known_hosts,
            agent: AgentSource::Auto(AgentPreference::default()),
            run_trust: RunTrust::default(),
            forward_agent: false,
            compression: false,
        }
    }
}
