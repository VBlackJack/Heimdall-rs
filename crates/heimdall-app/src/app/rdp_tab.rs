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
use std::time::Duration;

use heimdall_core::profile::{DesktopSizing, ProfileId, RdpProfile, SshGateway};
use heimdall_rdp::{Fingerprint, KnownRdpHosts};
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
            // The size chosen for the session, its reconnections included; else its
            // profile's.
            let sizing = match &tab.profile {
                TabProfile::Rdp(profile) => tab
                    .desktop_sizing
                    .unwrap_or_else(|| profile.options.sizing()),
                _ => DesktopSizing::FollowsTab,
            };
            let mut pane = DesktopPane::rdp(framebuffer, input, (size, sizing), clipboard);
            // Started again on each connection, a reconnection included, as the C# one.
            pane.anti_idle = matches!(&tab.profile, TabProfile::Rdp(profile) if profile.anti_idle);
            tab.desktop = Some(Box::new(pane));
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
    /// The certificate keys the user trusted for `host`:`port` for this run only, an RDP
    /// server's or an FTPS one's.
    pub(super) fn certificates_trusted_for_run(&self, host: &str, port: u16) -> Vec<Fingerprint> {
        self.rdp_run_trust
            .iter()
            .filter(|(known, known_port, _)| known == host && *known_port == port)
            .map(|(_, _, key)| *key)
            .collect()
    }

    pub(super) fn known_rdp_hosts(&self) -> PathBuf {
        self.config
            .known_hosts
            .with_file_name(KNOWN_RDP_HOSTS_FILE_NAME)
    }

    /// What connecting to `profile` needs, its SSH gateways included; an error when they
    /// cannot be followed.
    fn rdp_request(
        &self,
        profile: &RdpProfile,
        (accepted, chosen): (Option<Fingerprint>, Option<DesktopSizing>),
        cancel: CancellationToken,
    ) -> Result<RdpRequest, UiError> {
        let route = self
            .store
            .route(profile.gateway.as_ref())
            .map_err(UiError::Route)?;
        let ssh = self.ssh_options();
        Ok(RdpRequest {
            profile: profile.clone(),
            known_hosts: self.known_rdp_hosts(),
            accepted,
            trusted_for_run: self.certificates_trusted_for_run(&profile.host, profile.port),
            desktop: match chosen.unwrap_or_else(|| profile.options.sizing()) {
                DesktopSizing::Fixed { width, height } => (width, height),
                // Replaced by the tab's size as soon as it is shown.
                DesktopSizing::FollowsTab | DesktopSizing::TabSizeOnce => DEFAULT_DESKTOP,
            },
            desktop_scale: heimdall_rdp::desktop_scale_factor(self.display_scale),
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
        // With the application's options when the profile follows them, as they are now.
        let profile = profile.effective(&self.settings.rdp_defaults);
        let tab_id = TabId::fresh();
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = self.rdp_request(&profile, (None, None), cancel.clone());
        let mut tab = Tab::new(
            self.terminal_palette(),
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
        let defaults = self.settings.rdp_defaults;
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        let TabProfile::Rdp(profile) = tab.profile.clone() else {
            return Vec::new();
        };
        let profile = profile.effective(&defaults);
        // The tab draws with the options the session gets.
        tab.profile = TabProfile::Rdp(profile.clone());
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        tab.attempt = attempt;
        tab.cancel = cancel.clone();
        tab.phase = Phase::Connecting;
        tab.desktop = None;
        let chosen = tab.desktop_sizing;
        match self.rdp_request(&profile, (accepted, chosen), cancel) {
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
        // An RDP server's certificate, or an FTPS one's: the same question, the same pins.
        let (Phase::HostKey { .. }, Some(fingerprint), Some((host, port))) = (
            &tab.phase,
            tab.pending_rdp_key.take(),
            tab.profile
                .endpoint()
                .map(|(host, port)| (host.to_owned(), port)),
        ) else {
            return Vec::new();
        };
        let ftp = matches!(tab.profile, TabProfile::Ftp(_));
        match trust {
            // Said as the C# says it of an RDP server: the user stopped it, at the certificate.
            KeyTrust::Refused => {
                tab.phase = Phase::Failed(if ftp {
                    UiError::Cancelled
                } else {
                    UiError::CertificateRefused
                });
                Vec::new()
            }
            // Held in memory for this run: the file is not written.
            KeyTrust::Once => {
                let server = (host, port, fingerprint);
                if !self.rdp_run_trust.contains(&server) {
                    self.rdp_run_trust.push(server);
                }
                if ftp {
                    self.reconnect_ftp(tab_id, None)
                } else {
                    self.reconnect_rdp(tab_id, None)
                }
            }
            // Recorded by the next attempt, and only if the server presents exactly this key.
            KeyTrust::Always if ftp => self.reconnect_ftp(tab_id, Some(fingerprint)),
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

    /// How often the sessions asking for anti-idle keys get one; `None` when none does or
    /// the settings turn them off.
    #[must_use]
    pub fn anti_idle_interval(&self) -> Option<Duration> {
        let seconds = self.settings.anti_idle_interval;
        (seconds > 0 && self.tabs.iter().any(anti_idle))
            .then(|| Duration::from_secs(u64::from(seconds)))
    }

    /// Whether `tab`'s session gets anti-idle keys: its badge shows while it does.
    #[must_use]
    pub fn anti_idle_on(&self, tab: TabId) -> bool {
        self.settings.anti_idle_interval > 0 && self.tab(tab).is_some_and(anti_idle)
    }

    /// An anti-idle tick: Shift to each session asking for it. Sent while a dialog is open
    /// too: it is no input of the user's, and the session is kept all the same.
    pub(super) fn anti_idle_tick(&self) {
        if self.settings.anti_idle_interval == 0 {
            return;
        }
        let inputs = crate::desktop::anti_idle_inputs();
        for pane in self.tabs.iter().filter(|tab| anti_idle(tab)) {
            if let Some(pane) = pane.desktop.as_ref() {
                pane.send(&inputs);
            }
        }
    }

    /// Stops the anti-idle keys of `tab` for this session, as the C# badge's click does.
    pub(super) fn stop_anti_idle(&mut self, tab: TabId) {
        if let Some(pane) = self.tab_mut(tab).and_then(|found| found.desktop.as_mut()) {
            pane.anti_idle = false;
        }
    }
}

/// Whether `tab`'s connected session asks for anti-idle keys.
fn anti_idle(tab: &Tab) -> bool {
    tab.desktop.as_ref().is_some_and(|pane| pane.anti_idle)
}
