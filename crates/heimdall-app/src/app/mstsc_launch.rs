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

//! RDP profiles opened in Remote Desktop Connection, as the C# external mode: those set to,
//! and those behind an RD Gateway, which the built-in client does not go through. No tab
//! opens; the status bar says how it went.
//!
//! Behind an SSH gateway, the route is gone through first, its questions answered as a
//! tunnel's, and Remote Desktop Connection goes through a forward that lives as long as it
//! runs, as the C# tunnel does.

use heimdall_core::profile::{ProfileId, RdpProfile};
use tokio_util::sync::CancellationToken;

use super::tunnels::PendingTunnelKey;
use super::{App, Dialog, Effect, Notice};
use crate::error::UiError;
use crate::event::ConnectionEvent;
use crate::mstsc_driver::{MstscRouteEvent, MstscRouteId, MstscRouteRequest};
use crate::rdp_external::{self, ExternalRefusal};

/// A launch of Remote Desktop Connection through a gateway, from its start to the release
/// of its forward.
#[derive(Debug)]
pub(super) struct MstscRoute {
    id: MstscRouteId,
    /// The profile opened, its options the session's, to try again once a gateway's key
    /// asked about is trusted.
    profile: Box<RdpProfile>,
    /// The gateway it goes through.
    gateway: ProfileId,
    /// The gateway's name, as the status bar says it.
    gateway_name: String,
    /// Stops the attempt, then releases the forward.
    cancel: CancellationToken,
    /// The gateways whose saved password this attempt gave.
    answered: Vec<ProfileId>,
}

/// The launches of Remote Desktop Connection through gateways, until their forwards are
/// released.
#[derive(Debug, Default)]
pub(super) struct MstscRoutes {
    /// Those under way or running.
    runs: Vec<MstscRoute>,
    /// The identifier of the next one.
    next: MstscRouteId,
}

impl App {
    /// Opens `profile`, its options already the session's, in Remote Desktop Connection when
    /// it is set to, or names an RD Gateway; `None` when it opens in a tab. One through an
    /// SSH gateway goes through it, as the C# tunnels the external client.
    pub(super) fn open_rdp_external(&mut self, profile: &RdpProfile) -> Option<Vec<Effect>> {
        let gateway = profile.extras.rd_gateway();
        if !profile.extras.external && gateway.is_none() {
            return None;
        }
        if let Some(ssh_gateway) = profile.gateway.clone() {
            return Some(self.open_mstsc_route(profile.clone(), ssh_gateway));
        }
        Some(vec![Effect::LaunchRdpExternal {
            name: profile.name.clone(),
            // Said only when the gateway, not the profile, chose the external client.
            gateway: gateway
                .filter(|_| !profile.extras.external)
                .map(str::to_owned),
            content: rdp_external::rdp_file(profile),
        }])
    }

    /// The RDP profile `name` was opened in Remote Desktop Connection, or why not.
    pub(super) fn rdp_external_launched(
        &mut self,
        name: String,
        gateway: Option<String>,
        result: Result<(), ExternalRefusal>,
    ) -> Vec<Effect> {
        self.tell(match result {
            Ok(()) => Notice::RdpExternalLaunched { name, gateway },
            Err(refusal) => Notice::RdpExternalRefused(refusal),
        });
        Vec::new()
    }

    /// Starts the launch of `profile` in Remote Desktop Connection through `gateway` and the
    /// gateways before it.
    pub(super) fn open_mstsc_route(
        &mut self,
        profile: RdpProfile,
        gateway: ProfileId,
    ) -> Vec<Effect> {
        let route = match self.store.route(Some(&gateway)) {
            Ok(route) => route,
            Err(error) => {
                self.tell(Notice::RdpExternalRefused(ExternalRefusal::Gateway(
                    UiError::Route(error),
                )));
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
            self.tell(Notice::RdpExternalRefused(ExternalRefusal::Gateway(
                UiError::Cancelled,
            )));
            return Vec::new();
        };
        let id = self.mstsc_routes.next;
        self.mstsc_routes.next = id.next();
        let cancel = CancellationToken::new();
        let request = MstscRouteRequest {
            before,
            gateway: last,
            profile: profile.clone(),
            ssh: self.ssh_options(),
            cancel: cancel.clone(),
        };
        self.mstsc_routes.runs.push(MstscRoute {
            id,
            profile: Box::new(profile),
            gateway,
            gateway_name,
            cancel,
            answered: Vec::new(),
        });
        vec![Effect::OpenMstscRoute {
            id,
            request: Box::new(request),
        }]
    }

    /// Applies what the launch `id` through a gateway reports.
    pub(super) fn mstsc_route_event(
        &mut self,
        id: MstscRouteId,
        event: MstscRouteEvent,
    ) -> Vec<Effect> {
        let Some(index) = self.mstsc_routes.runs.iter().position(|run| run.id == id) else {
            // Released meanwhile: a question still waiting is declined.
            if let MstscRouteEvent::Route(ConnectionEvent::Question { question, .. }) = event {
                return vec![super::tunnels::decline(question)];
            }
            return Vec::new();
        };
        match event {
            MstscRouteEvent::Route(ConnectionEvent::Question { question, kind }) => {
                let run = &self.mstsc_routes.runs[index];
                let (gateway, answered) = (run.gateway.clone(), run.answered.clone());
                let (answer, given) = self.route_saved_answer(&gateway, &answered, &kind);
                if let Some(profile) = given {
                    self.mstsc_routes.runs[index].answered.push(profile);
                }
                vec![Effect::Answer { question, answer }]
            }
            MstscRouteEvent::Route(ConnectionEvent::UnknownHostKey {
                host,
                port,
                fingerprint,
                key,
            }) => {
                let run = self.mstsc_routes.runs.remove(index);
                // Never queued behind another dialog, never accepted unseen.
                if self.dialog.is_some() {
                    self.tell(Notice::RdpExternalRefused(ExternalRefusal::Gateway(
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
                    Some(PendingTunnelKey::for_mstsc(*run.profile, host, port, key));
                Vec::new()
            }
            MstscRouteEvent::Route(ConnectionEvent::Failed(error)) => {
                let run = self.mstsc_routes.runs.remove(index);
                // A saved password given on the way may be what the gateway refused.
                self.refuse_saved(run.answered);
                self.tell(Notice::RdpExternalRefused(ExternalRefusal::Gateway(error)));
                Vec::new()
            }
            MstscRouteEvent::Route(_) => Vec::new(),
            MstscRouteEvent::Launched(result) => {
                let run = &self.mstsc_routes.runs[index];
                let (name, gateway) = (run.profile.name.clone(), run.gateway_name.clone());
                if result.is_err() {
                    self.mstsc_routes.runs.remove(index);
                }
                self.tell(match result {
                    Ok(()) => Notice::RdpExternalLaunchedThrough { name, gateway },
                    Err(refusal) => Notice::RdpExternalRefused(refusal),
                });
                Vec::new()
            }
            MstscRouteEvent::Released => {
                self.mstsc_routes.runs.remove(index);
                Vec::new()
            }
        }
    }

    /// Releases every forward of Remote Desktop Connection through a gateway, as the
    /// application closes.
    pub(super) fn release_mstsc_routes(&mut self) {
        for run in self.mstsc_routes.runs.drain(..) {
            run.cancel.cancel();
        }
    }
}
