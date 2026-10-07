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
//!
//! Behind an SSH gateway, the route is gone through first, its questions answered as a
//! tunnel's, and the server's key probed through the gateway; `PuTTY` then goes through a
//! forward that lives as long as it runs, as the C# tunnel does.

use heimdall_core::profile::{ProfileId, SshMode, SshProfile};
use tokio_util::sync::CancellationToken;

use super::tunnels::PendingTunnelKey;
use super::{App, Dialog, Effect, Notice};
use crate::driver::Purpose;
use crate::error::UiError;
use crate::event::ConnectionEvent;
use crate::putty::{self, HostKeyProbe, PuttyRefusal, PuttyStarted};
use crate::putty_driver::{PuttyRouteEvent, PuttyRouteId, PuttyRouteRequest};
use crate::x11_server::X11Outcome;

/// A launch of `PuTTY` through a gateway, from its start to the release of its forward.
#[derive(Debug)]
pub(super) struct PuttyRoute {
    id: PuttyRouteId,
    /// The profile opened, to try again once a key asked about is trusted.
    profile: Box<SshProfile>,
    /// The gateway it goes through.
    gateway: ProfileId,
    /// The gateway's name, as the status bar says it.
    gateway_name: String,
    /// Stops the attempt, then releases the forward.
    cancel: CancellationToken,
    /// The gateways whose saved password this attempt gave.
    answered: Vec<ProfileId>,
}

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

    /// Checks `profile` as the C# does before launching, then probes its server's host key:
    /// directly, or through its gateway.
    pub(super) fn probe_for_putty(&mut self, profile: SshProfile) -> Vec<Effect> {
        if let Err(refusal) = putty::check(&profile) {
            log::warn!("{} is not opened in PuTTY: {refusal:?}", profile.id);
            self.tell(Notice::PuttyRefused(refusal));
            return Vec::new();
        }
        if let Some(gateway) = profile.gateway.clone() {
            return self.open_putty_route(profile, gateway);
        }
        vec![Effect::ProbePuttyHostKey {
            profile: Box::new(profile),
            options: Box::new(self.ssh_options()),
        }]
    }

    /// Starts the launch of `profile` in `PuTTY` through `gateway` and the gateways before it.
    fn open_putty_route(&mut self, profile: SshProfile, gateway: ProfileId) -> Vec<Effect> {
        let route = match self.store.route(Some(&gateway)) {
            Ok(route) => route,
            Err(error) => {
                self.tell(Notice::PuttyRefused(PuttyRefusal::Gateway(UiError::Route(
                    error,
                ))));
                return Vec::new();
            }
        };
        let gateway_name = route
            .last()
            .map_or_else(|| gateway.to_string(), |last| last.name.clone());
        let mut before: Vec<_> = route
            .iter()
            .map(heimdall_core::profile::SshGateway::as_hop)
            .collect();
        let Some(last) = before.pop() else {
            self.tell(Notice::PuttyRefused(PuttyRefusal::Gateway(
                UiError::Cancelled,
            )));
            return Vec::new();
        };
        let id = self.next_putty_route;
        self.next_putty_route = id.next();
        let cancel = CancellationToken::new();
        let request = PuttyRouteRequest {
            before,
            gateway: last,
            launch: putty::plan(&profile, &self.settings, String::new()),
            profile: profile.clone(),
            ssh: self.ssh_options(),
            cancel: cancel.clone(),
        };
        self.putty_routes.push(PuttyRoute {
            id,
            profile: Box::new(profile),
            gateway,
            gateway_name,
            cancel,
            answered: Vec::new(),
        });
        vec![Effect::OpenPuttyRoute {
            id,
            request: Box::new(request),
        }]
    }

    /// Applies what the launch `id` through a gateway reports.
    pub(super) fn putty_route_event(
        &mut self,
        id: PuttyRouteId,
        event: PuttyRouteEvent,
    ) -> Vec<Effect> {
        let Some(index) = self.putty_routes.iter().position(|run| run.id == id) else {
            // Released meanwhile: a question still waiting is declined.
            if let PuttyRouteEvent::Route(ConnectionEvent::Question { question, .. }) = event {
                return vec![super::tunnels::decline(question)];
            }
            return Vec::new();
        };
        match event {
            PuttyRouteEvent::Route(ConnectionEvent::Question { question, kind }) => {
                let run = &self.putty_routes[index];
                let (gateway, answered) = (run.gateway.clone(), run.answered.clone());
                let (answer, given) = self.route_saved_answer(&gateway, &answered, &kind);
                if let Some(profile) = given {
                    self.putty_routes[index].answered.push(profile);
                }
                vec![Effect::Answer { question, answer }]
            }
            PuttyRouteEvent::Route(ConnectionEvent::UnknownHostKey {
                host,
                port,
                fingerprint,
                key,
            }) => {
                let run = self.putty_routes.remove(index);
                let probe = HostKeyProbe::Unknown {
                    host,
                    port,
                    fingerprint,
                    key,
                };
                self.putty_host_key(*run.profile, probe)
            }
            PuttyRouteEvent::Route(ConnectionEvent::Failed(error)) => {
                let run = self.putty_routes.remove(index);
                // A saved password given on the way may be what the gateway refused.
                self.refuse_saved(run.answered);
                self.tell(Notice::PuttyRefused(PuttyRefusal::Gateway(error)));
                Vec::new()
            }
            // The driver goes on by itself on a key trusted: only the others come here.
            PuttyRouteEvent::Route(_) | PuttyRouteEvent::HostKey(HostKeyProbe::Trusted(_)) => {
                Vec::new()
            }
            PuttyRouteEvent::HostKey(probe) => {
                let run = self.putty_routes.remove(index);
                self.putty_host_key(*run.profile, probe)
            }
            PuttyRouteEvent::Launched(result) => {
                let run = &self.putty_routes[index];
                let (name, gateway) = (run.profile.name.clone(), run.gateway_name.clone());
                if result.is_err() {
                    self.putty_routes.remove(index);
                }
                self.say_launched(name, Some(gateway), result);
                Vec::new()
            }
            PuttyRouteEvent::Released => {
                self.putty_routes.remove(index);
                Vec::new()
            }
        }
    }

    /// Releases every forward of `PuTTY` through a gateway, as the application closes.
    pub(super) fn release_putty_routes(&mut self) {
        for run in self.putty_routes.drain(..) {
            run.cancel.cancel();
        }
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
        self.say_launched(name, None, result);
        Vec::new()
    }

    /// Says how `PuTTY` started for the profile `name`, through `gateway` when it went
    /// through one, or why it did not.
    fn say_launched(
        &mut self,
        name: String,
        gateway: Option<String>,
        result: Result<PuttyStarted, PuttyRefusal>,
    ) {
        self.tell(match (result, gateway) {
            (
                Ok(PuttyStarted {
                    x11: Some(X11Outcome::Unavailable),
                }),
                _,
            ) => Notice::X11ServerNotFound,
            (Ok(_), None) => Notice::PuttyLaunched(name),
            (Ok(_), Some(gateway)) => Notice::PuttyLaunchedThrough { name, gateway },
            (Err(refusal), _) => Notice::PuttyRefused(refusal),
        });
    }
}
