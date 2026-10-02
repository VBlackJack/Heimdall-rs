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

//! Keeping SSH sessions up, as the C# Heimdall does: protocol keep-alives at the settings'
//! interval on every connection, and the `TMOUT` reset of an idle SSH shell.
//!
//! The reset is a bare carriage return written into the shell so a remote `TMOUT` does not
//! log the user out. It is a keystroke, so it waits for a shell nobody typed into for a whole
//! interval: one written while the user types would submit a half-typed line. A local shell
//! or a `WinRM` session has no remote `TMOUT`, and gets none.

use std::time::Duration;

use heimdall_ssh::ConnectOptions;

use super::{App, Phase, Tab, TabProfile};
use crate::driver::Purpose;

/// What the `TMOUT` reset writes: Enter, on an empty line.
const TMOUT_RESET: &[u8] = b"\r";

impl App {
    /// The options every SSH connection shares: known hosts, the agent, this run's trust and
    /// the keep-alive interval of the settings.
    pub(super) fn ssh_options(&self) -> ConnectOptions {
        let mut options = ConnectOptions::new(self.config.known_hosts.clone());
        options.agent = self.config.agent.clone();
        options.run_trust = self.run_trust.clone();
        options.keepalive_interval =
            Duration::from_secs(u64::from(self.settings.ssh_keep_alive_interval));
        options
    }

    /// How often idle SSH shells are looked at for their `TMOUT` reset; `None` when none is
    /// open or the settings turn it off.
    #[must_use]
    pub fn tmout_reset_interval(&self) -> Option<Duration> {
        let seconds = self.settings.ssh_tmout_reset_interval;
        (seconds > 0 && self.tabs.iter().any(resets_tmout))
            .then(|| Duration::from_secs(u64::from(seconds)))
    }

    /// A `TMOUT` reset tick: Enter to each SSH shell nobody typed into for the interval.
    pub(super) fn tmout_reset_tick(&self) {
        let Some(interval) = self.tmout_reset_interval() else {
            return;
        };
        for tab in self.tabs.iter().filter(|tab| resets_tmout(tab)) {
            if tab.idle_for(interval)
                && let Some(sink) = &tab.sink
            {
                // Not the user's input: the shell stays idle for the next tick.
                let _ = sink.write(TMOUT_RESET.to_vec());
            }
        }
    }
}

/// Whether `tab` is a connected SSH shell, the only session with a remote `TMOUT`.
fn resets_tmout(tab: &Tab) -> bool {
    tab.phase == Phase::Connected
        && tab.purpose == Purpose::Shell
        && matches!(&tab.profile, TabProfile::Ssh(profile) if !profile.sftp)
}
