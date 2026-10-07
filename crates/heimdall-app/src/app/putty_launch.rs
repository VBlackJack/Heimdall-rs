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

//! SSH profiles opened in `PuTTY`, as the C# external mode. No tab opens; the status bar
//! says how it went. The server's host key is probed first: a key never seen is asked about
//! in the host key dialog the tunnels use, then probed again; a key that changed, or a
//! server out of reach, stops the launch.

use heimdall_core::profile::{SshMode, SshProfile};

use super::tunnels::PendingTunnelKey;
use super::{App, Dialog, Effect, Notice};
use crate::driver::Purpose;
use crate::error::UiError;
use crate::putty::{self, HostKeyProbe, PuttyRefusal, PuttyStarted};
use crate::x11_server::X11Outcome;

impl App {
    /// Opens `profile` in `PuTTY` when it is a shell set to; `None` when it opens in a tab.
    pub(super) fn open_ssh_external(
        &mut self,
        profile: &SshProfile,
        purpose: Purpose,
    ) -> Option<Vec<Effect>> {
        if purpose != Purpose::Shell || profile.sftp || profile.ssh_mode != SshMode::External {
            return None;
        }
        Some(self.probe_for_putty(profile.clone()))
    }

    /// Checks `profile` as the C# does before launching, then probes its server's host key.
    pub(super) fn probe_for_putty(&mut self, profile: SshProfile) -> Vec<Effect> {
        if let Err(refusal) = putty::check(&profile) {
            log::warn!("{} is not opened in PuTTY: {refusal:?}", profile.id);
            self.tell(Notice::PuttyRefused(refusal));
            return Vec::new();
        }
        vec![Effect::ProbePuttyHostKey {
            profile: Box::new(profile),
            options: Box::new(self.ssh_options()),
        }]
    }

    /// What the probe of `profile`'s host key found: `PuTTY` started on a key trusted, the
    /// user asked about one never seen, the launch stopped otherwise.
    pub(super) fn putty_host_key(
        &mut self,
        profile: SshProfile,
        probe: HostKeyProbe,
    ) -> Vec<Effect> {
        match probe {
            HostKeyProbe::Trusted(host_key) => vec![Effect::LaunchPutty {
                name: profile.name.clone(),
                launch: Box::new(putty::plan(&profile, &self.settings, host_key)),
            }],
            HostKeyProbe::Unknown {
                host,
                port,
                fingerprint,
                key,
            } => {
                // Never queued behind another dialog, never accepted unseen.
                if self.dialog.is_some() {
                    self.tell(Notice::PuttyRefused(PuttyRefusal::HostKey(
                        UiError::Cancelled,
                    )));
                    return Vec::new();
                }
                self.dialog = Some(Dialog::TunnelHostKey {
                    host: host.clone(),
                    port,
                    fingerprint,
                    algorithm: key.algorithm().to_string(),
                });
                self.pending_tunnel_key =
                    Some(PendingTunnelKey::for_putty(profile, host, port, key));
                Vec::new()
            }
            HostKeyProbe::Failed(error) => {
                self.tell(Notice::PuttyRefused(PuttyRefusal::HostKey(error)));
                Vec::new()
            }
        }
    }

    /// `PuTTY` was started for the profile `name`, or why not; without the X server its X11
    /// forwarding asked for, the C# notice.
    pub(super) fn putty_launched(
        &mut self,
        name: String,
        result: Result<PuttyStarted, PuttyRefusal>,
    ) -> Vec<Effect> {
        self.tell(match result {
            Ok(PuttyStarted {
                x11: Some(X11Outcome::Unavailable),
            }) => Notice::X11ServerNotFound,
            Ok(_) => Notice::PuttyLaunched(name),
            Err(refusal) => Notice::PuttyRefused(refusal),
        });
        Vec::new()
    }
}
