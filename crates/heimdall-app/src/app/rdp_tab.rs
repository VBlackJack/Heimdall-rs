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

//! RDP tabs: opening one, the certificate decision, forgetting a server, and input.

use std::path::PathBuf;

use heimdall_core::profile::{ProfileId, RdpProfile, SshGateway};
use heimdall_rdp::{Fingerprint, KnownRdpHosts};
use heimdall_ssh::ConnectOptions;
use tokio_util::sync::CancellationToken;

use super::{App, Effect, KeyTrust, Phase, Tab, TabProfile};
use crate::desktop::{DesktopInput, DesktopPane};
use crate::driver::Purpose;
use crate::error::UiError;
use crate::event::ConnectionEvent;
use crate::ids::{AttemptId, TabId};
use crate::rdp_driver::{DEFAULT_DESKTOP, RdpRequest};

/// File of trusted RDP servers, beside the SSH `known_hosts`.
const KNOWN_RDP_HOSTS_FILE_NAME: &str = "known_rdp_hosts";

/// Applies an event only an RDP attempt sends.
pub(super) fn apply(tab: &mut Tab, event: ConnectionEvent) {
    match event {
        ConnectionEvent::UnknownRdpCertificate {
            host,
            port,
            fingerprint,
        } => {
            tab.retry = None;
            tab.prompts.clear();
            tab.pending_rdp_key = Some(fingerprint);
            tab.phase = Phase::HostKey {
                host,
                port,
                fingerprint: fingerprint.to_string(),
            };
        }
        ConnectionEvent::RdpReady {
            framebuffer,
            input,
            size,
            clipboard,
        } => {
            // Back: the attempts stop.
            tab.retry = None;
            tab.phase = Phase::Connected;
            tab.desktop = Some(Box::new(DesktopPane::rdp(
                framebuffer,
                input,
                size,
                clipboard,
            )));
        }
        ConnectionEvent::DesktopFrame => {
            if let Some(pane) = tab.desktop.as_mut() {
                pane.generation = pane.generation.wrapping_add(1);
            }
        }
        _ => {}
    }
}

impl App {
    fn known_rdp_hosts(&self) -> PathBuf {
        self.config
            .known_hosts
            .with_file_name(KNOWN_RDP_HOSTS_FILE_NAME)
    }

    /// What connecting to `profile` needs, its SSH gateways included; an error when they
    /// cannot be followed.
    fn rdp_request(
        &self,
        profile: &RdpProfile,
        accepted: Option<Fingerprint>,
        cancel: CancellationToken,
    ) -> Result<RdpRequest, UiError> {
        let route = self
            .store
            .route(profile.gateway.as_ref())
            .map_err(UiError::Route)?;
        let mut ssh = ConnectOptions::new(self.config.known_hosts.clone());
        ssh.agent = self.config.agent.clone();
        ssh.run_trust = self.run_trust.clone();
        Ok(RdpRequest {
            profile: profile.clone(),
            known_hosts: self.known_rdp_hosts(),
            accepted,
            trusted_for_run: self
                .rdp_run_trust
                .iter()
                .filter(|(host, port, _)| *host == profile.host && *port == profile.port)
                .map(|(_, _, key)| *key)
                .collect(),
            desktop: DEFAULT_DESKTOP,
            route: route.iter().map(SshGateway::as_hop).collect(),
            ssh,
            cancel,
        })
    }

    /// Opens an RDP tab for a saved profile.
    pub(super) fn open_rdp(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(profile) = self.rdp_profiles().iter().find(|p| &p.id == id).cloned() else {
            return Vec::new();
        };
        self.open_rdp_profile(profile)
    }

    /// Opens an RDP tab for `profile`.
    pub(super) fn open_rdp_profile(&mut self, profile: RdpProfile) -> Vec<Effect> {
        let tab_id = TabId::fresh();
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = self.rdp_request(&profile, None, cancel.clone());
        let mut tab = Tab::new(
            tab_id,
            TabProfile::Rdp(profile),
            Purpose::Rdp,
            self.viewport,
            attempt,
            cancel,
        );
        tab.files = None;
        let effects = match request {
            Ok(request) => vec![Effect::ConnectRdp {
                tab: tab_id,
                attempt,
                request: Box::new(request),
            }],
            Err(error) => {
                tab.phase = Phase::Failed(error);
                Vec::new()
            }
        };
        self.tabs.push(tab);
        self.active = Some(tab_id);
        effects
    }

    /// Connects `tab` again, with `accepted` as the key the user just agreed to.
    pub(super) fn reconnect_rdp(
        &mut self,
        tab_id: TabId,
        accepted: Option<Fingerprint>,
    ) -> Vec<Effect> {
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        let TabProfile::Rdp(profile) = tab.profile.clone() else {
            return Vec::new();
        };
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        tab.attempt = attempt;
        tab.cancel = cancel.clone();
        tab.phase = Phase::Connecting;
        tab.desktop = None;
        match self.rdp_request(&profile, accepted, cancel) {
            Ok(request) => vec![Effect::ConnectRdp {
                tab: tab_id,
                attempt,
                request: Box::new(request),
            }],
            Err(error) => {
                if let Some(tab) = self.tab_mut(tab_id) {
                    tab.phase = Phase::Failed(error);
                }
                Vec::new()
            }
        }
    }

    /// The user's answer to the certificate question of an RDP tab.
    pub(super) fn rdp_certificate_decision(
        &mut self,
        tab_id: TabId,
        trust: KeyTrust,
    ) -> Vec<Effect> {
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        let (Phase::HostKey { .. }, Some(fingerprint), TabProfile::Rdp(profile)) =
            (&tab.phase, tab.pending_rdp_key.take(), &tab.profile)
        else {
            return Vec::new();
        };
        match trust {
            KeyTrust::Refused => {
                tab.phase = Phase::Failed(UiError::Cancelled);
                Vec::new()
            }
            // Held in memory for this run: the file is not written.
            KeyTrust::Once => {
                let server = (profile.host.clone(), profile.port, fingerprint);
                if !self.rdp_run_trust.contains(&server) {
                    self.rdp_run_trust.push(server);
                }
                self.reconnect_rdp(tab_id, None)
            }
            // Recorded by the next attempt, and only if the server presents exactly this key.
            KeyTrust::Always => self.reconnect_rdp(tab_id, Some(fingerprint)),
        }
    }

    /// Forgets the key recorded for the server of an RDP tab whose key changed, then
    /// connects again: the certificate question comes back.
    pub(super) fn forget_rdp_certificate(&mut self, tab_id: TabId) -> Vec<Effect> {
        let path = self.known_rdp_hosts();
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        // The RDP server's own key only: a gateway's changed SSH key is not this server's.
        let (Phase::Failed(UiError::HostKeyChanged { target: None, .. }), TabProfile::Rdp(profile)) =
            (&tab.phase, &tab.profile)
        else {
            return Vec::new();
        };
        if let Err(error) = KnownRdpHosts::new(path).forget(&profile.host, profile.port) {
            tab.phase = Phase::Failed(UiError::KnownHosts {
                detail: error.to_string(),
            });
            return Vec::new();
        }
        self.reconnect_rdp(tab_id, None)
    }

    /// Keyboard or mouse input for the remote desktop of a tab.
    pub(super) fn desktop_input(&mut self, tab_id: TabId, inputs: &[DesktopInput]) {
        // A dialog owns the input: what is meant for it must not reach the server.
        if self.dialog.is_some() {
            return;
        }
        // The desktop exists only while the session is connected.
        if let Some(pane) = self.tab(tab_id).and_then(|tab| tab.desktop.as_ref()) {
            pane.send(inputs);
        }
    }
}
