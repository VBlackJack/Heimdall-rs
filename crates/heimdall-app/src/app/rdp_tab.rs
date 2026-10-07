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

use super::{App, CertificateContext, Effect, KeyTrust, Phase, Tab, TabProfile};
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
            ..
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
            pane.aspect = tab.desktop_aspect;
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
        (accepted, chosen, at): (
            Option<Fingerprint>,
            Option<DesktopSizing>,
            Option<(u16, u16)>,
        ),
        cancel: CancellationToken,
    ) -> Result<RdpRequest, UiError> {
        // Straight to the server, it could not be reached, or not the one meant.
        if let Some(gateway) = profile.extras.rd_gateway() {
            return Err(UiError::NeedsRdGateway(gateway.to_owned()));
        }
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
            desktop: match at.map_or_else(
                || chosen.unwrap_or_else(|| profile.options.sizing()),
                |(width, height)| DesktopSizing::Fixed { width, height },
            ) {
                DesktopSizing::Fixed { width, height } => (width, height),
                // Replaced by the tab's size as soon as it is shown.
                DesktopSizing::FollowsTab | DesktopSizing::TabSizeOnce => DEFAULT_DESKTOP,
            },
            desktop_scale: heimdall_rdp::desktop_scale_factor(self.display_scale),
            logon_timeout: match self.settings.rdp_connect_timeout {
                0 => None,
                seconds => Some(std::time::Duration::from_secs(u64::from(seconds))),
            },
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

    /// Opens an RDP tab for `profile`; in Remote Desktop Connection instead when it is set
    /// to, or names an RD Gateway.
    pub(super) fn open_rdp_profile(&mut self, profile: RdpProfile) -> Vec<Effect> {
        // With the application's options when the profile follows them, as they are now.
        let profile = profile.effective(&self.settings.rdp_defaults);
        // Remote Desktop Connection's window, not a tab: no session counted.
        if let Some(effects) = self.open_rdp_external(&profile) {
            return effects;
        }
        if self.session_limit_reached() {
            return Vec::new();
        }
        let tab_id = TabId::fresh();
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = self.rdp_request(&profile, (None, None, None), cancel.clone());
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
        // A size the server could not take live is asked as the connection opens.
        let at = tab
            .resize_fallback
            .as_mut()
            .and_then(|fallback| fallback.pending.take());
        match self.rdp_request(&profile, (accepted, chosen, at), cancel) {
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
        tab.certificate_context = None;
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

    /// Forgets the keys recorded for the server of an RDP or FTPS tab whose key changed,
    /// then connects again: the certificate question comes back.
    pub(super) fn forget_rdp_certificate(&mut self, tab_id: TabId) -> Vec<Effect> {
        let (rdp_file, ftps_file) = (self.known_rdp_hosts(), self.known_ftps_hosts());
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        // The server's own key only: a gateway's changed SSH key is not this server's.
        if !matches!(
            tab.phase,
            Phase::Failed(UiError::HostKeyChanged { target: None, .. })
        ) {
            return Vec::new();
        }
        let (path, host, port, ftp) = match &tab.profile {
            TabProfile::Rdp(profile) => (rdp_file, &profile.host, profile.port, false),
            TabProfile::Ftp(profile) => (ftps_file, &profile.host, profile.port, true),
            _ => return Vec::new(),
        };
        if let Err(error) = KnownRdpHosts::new(path).forget(host, port) {
            tab.phase = Phase::Failed(UiError::KnownHosts {
                detail: error.to_string(),
            });
            return Vec::new();
        }
        if ftp {
            self.reconnect_ftp(tab_id, None)
        } else {
            self.reconnect_rdp(tab_id, None)
        }
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

/// A connection made again for a size the server could not take live, as the C# reconnects
/// when a resize throws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResizeFallback {
    /// The size connected again for.
    pub(super) size: (u16, u16),
    /// That size, until the new connection asks it.
    pub(super) pending: Option<(u16, u16)>,
    /// Said once the new connection is ready: said before, it would go with the state.
    pub(super) announce: bool,
}

impl App {
    /// A certificate question, a desktop ready or a desktop drawn again, for `tab_id`.
    /// The files copied were not offered to the server: said, with why.
    pub(super) fn clipboard_files_event(
        &mut self,
        tab_id: TabId,
        event: ConnectionEvent,
    ) -> Vec<Effect> {
        let pane = self
            .tab_mut(tab_id)
            .and_then(|tab| tab.desktop.as_deref_mut());
        let notice = match (event, pane) {
            (ConnectionEvent::RdpFilesRefused(refusal), _) => Some(match refusal {
                heimdall_rdp::CopyRefusal::TooManyEntries => super::Notice::RdpFilesTooMany,
                heimdall_rdp::CopyRefusal::TooLarge => super::Notice::RdpFilesTooLarge,
            }),
            (ConnectionEvent::RdpRemoteFiles(available), Some(pane)) => {
                pane.set_remote_files(available);
                None
            }
            (ConnectionEvent::RdpSaveProgress { saved, total }, Some(pane)) => {
                pane.save_progress(saved, total);
                None
            }
            (ConnectionEvent::RdpSaveEnded(end), pane) => {
                if let Some(pane) = pane {
                    pane.save_ended();
                }
                Some(super::Notice::RdpFilesSaveEnded(end))
            }
            _ => None,
        };
        let Some(notice) = notice else {
            return Vec::new();
        };
        let ended = matches!(notice, super::Notice::RdpFilesSaveEnded(_));
        self.tell(notice);
        if ended {
            self.offer_again(tab_id)
        } else {
            Vec::new()
        }
    }

    /// This side's clipboard, held while the server's files were saved, offered again to
    /// the desktop shown: what was copied meanwhile reaches it.
    fn offer_again(&self, tab_id: TabId) -> Vec<Effect> {
        if self.active != Some(tab_id) {
            return Vec::new();
        }
        self.tab(tab_id)
            .map(super::clipboard_offer)
            .unwrap_or_default()
    }

    /// The user saves the RDP server's copied files: a folder asked for, then given to the
    /// session; or stops.
    pub(super) fn save_message(&mut self, message: super::Message) -> Vec<Effect> {
        let tab = match &message {
            super::Message::SaveRemoteFiles(tab)
            | super::Message::CancelSave(tab)
            | super::Message::SaveFolderPicked { tab, .. } => *tab,
            _ => return Vec::new(),
        };
        let Some(pane) = self
            .tab_mut(tab)
            .and_then(|found| found.desktop.as_deref_mut())
        else {
            return Vec::new();
        };
        match message {
            super::Message::SaveRemoteFiles(_) if pane.ask_save() => {
                vec![Effect::PickSaveFolder { tab }]
            }
            super::Message::SaveFolderPicked { folder, .. } => {
                pane.save_into(folder);
                if pane.save_state().is_none() {
                    return self.offer_again(tab);
                }
                Vec::new()
            }
            super::Message::CancelSave(_) => {
                pane.cancel_save();
                if pane.save_state().is_none() {
                    return self.offer_again(tab);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn rdp_event(&mut self, tab_id: TabId, event: ConnectionEvent) -> Vec<Effect> {
        let context = match &event {
            ConnectionEvent::UnknownRdpCertificate {
                host,
                port,
                subject,
                details,
                ..
            } => self
                .certificate_context(tab_id, host, *port)
                .map(|context| CertificateContext {
                    subject: subject.clone(),
                    details: details.as_deref().cloned(),
                    ..context
                }),
            _ => None,
        };
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        let ready = matches!(event, ConnectionEvent::RdpReady { .. });
        apply(tab, event);
        if context.is_some() {
            tab.certificate_context = context;
        }
        // A new desktop is offered this side's clipboard at once.
        let effects = super::clipboard_offer(tab);
        // Back at the size the server could not take live: said now it holds.
        let announce = ready
            && tab
                .resize_fallback
                .as_mut()
                .is_some_and(|fallback| std::mem::take(&mut fallback.announce));
        if announce {
            self.tell(super::Notice::ResolutionReconnected);
        }
        effects
    }

    /// What the certificate question of `tab_id` says beside the fingerprint, as the C# one:
    /// how many other certificates its profile trusts at `host:port`, and the gateways it
    /// goes through. Read once, when the question comes, not each time it is drawn.
    fn certificate_context(
        &self,
        tab_id: TabId,
        host: &str,
        port: u16,
    ) -> Option<CertificateContext> {
        let tab = self.tab(tab_id)?;
        let (file, gateway) = match &tab.profile {
            TabProfile::Rdp(profile) => (self.known_rdp_hosts(), profile.gateway.as_ref()),
            TabProfile::Ftp(_) => (self.known_ftps_hosts(), None),
            _ => return None,
        };
        let host = host.to_ascii_lowercase();
        // A file that cannot be read trusts nothing here: the question is asked all the same.
        let others = KnownRdpHosts::new(file).entries().map_or(0, |entries| {
            entries
                .iter()
                .filter(|entry| entry.host == host && entry.port == port)
                .count()
        });
        let route = self
            .store
            .route(gateway)
            .map(|route| route.into_iter().map(|gateway| gateway.name).collect())
            .unwrap_or_default();
        Some(CertificateContext {
            others,
            route,
            subject: None,
            details: None,
        })
    }

    /// The server of `tab_id` could not take `size` live: the session connects again at
    /// that size, once per size, and says so once back, as the C# by default (it asks first
    /// only when a hidden setting says so). A server that cannot take it even then keeps
    /// its own.
    pub(super) fn resize_refused(&mut self, tab_id: TabId, size: (u16, u16)) -> Vec<Effect> {
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        if tab.phase != Phase::Connected
            || tab
                .resize_fallback
                .is_some_and(|fallback| fallback.size == size)
        {
            return Vec::new();
        }
        tab.resize_fallback = Some(ResizeFallback {
            size,
            pending: Some(size),
            announce: true,
        });
        // The live session ends: its own end is not this attempt's any more.
        tab.cancel.cancel();
        self.reconnect_rdp(tab_id, None)
    }
}
