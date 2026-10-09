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

use heimdall_core::profile::{DesktopSizing, ProfileId, RdpMode, RdpProfile, SshGateway};
use heimdall_rdp::{AcceptedCertificate, CertificateHash, Fingerprint, KnownRdpHosts};
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

/// The kind of server a certificate question is about: each has its file of pins and its
/// way to connect again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CertifiedServer {
    Rdp,
    Ftps,
    Vnc,
}

impl CertifiedServer {
    /// The kind of `profile`'s server.
    fn of(profile: &TabProfile) -> Self {
        match profile {
            TabProfile::Ftp(_) => Self::Ftps,
            TabProfile::Vnc(_) => Self::Vnc,
            _ => Self::Rdp,
        }
    }

    /// As the log names it.
    fn name(self) -> &'static str {
        match self {
            Self::Rdp => "RDP",
            Self::Ftps => "FTPS",
            Self::Vnc => "VNC",
        }
    }
}

/// Applies an event only an RDP attempt sends.
pub(super) fn apply(tab: &mut Tab, event: ConnectionEvent) {
    match event {
        ConnectionEvent::UnknownRdpCertificate {
            host,
            port,
            fingerprint,
            certificate,
            ..
        } => {
            tab.retry = None;
            tab.prompts.clear();
            tab.pending_rdp_key = Some((fingerprint, certificate));
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

/// The user's answer to a certificate question, for the attempt built on it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Decided {
    /// The certificate's key: another key presented is the changed-key alarm.
    key: Fingerprint,
    /// The hash of the whole certificate asked about, as a server is pinned: what the answer
    /// trusts, this very certificate and no other on the same key.
    whole: CertificateHash,
    /// Trusted for good, to be recorded; else for this run only.
    record: bool,
}

impl Decided {
    /// The whole certificate accepted to be recorded, and the one trusted once: one of
    /// them.
    fn whole(self) -> (Option<CertificateHash>, Option<CertificateHash>) {
        if self.record {
            (Some(self.whole), None)
        } else {
            (None, Some(self.whole))
        }
    }
}

impl App {
    /// The whole certificates the user trusted for `host`:`port` for this run only, by
    /// their hash, as a server is pinned, an RDP, FTPS or VNC one; the host told apart
    /// regardless of case, as the pin files tell it.
    pub(super) fn whole_certificates_trusted_for_run(
        &self,
        host: &str,
        port: u16,
    ) -> Vec<CertificateHash> {
        self.rdp_run_trust
            .iter()
            .filter(|(known, known_port, ..)| {
                known.eq_ignore_ascii_case(host) && *known_port == port
            })
            .map(|(_, _, whole)| *whole)
            .collect()
    }

    /// Forgets the certificates trusted for `host`:`port` for this run, the host told
    /// apart regardless of case: else a certificate trusted once and no longer valid would
    /// be refused until the application restarts.
    pub(super) fn forget_run_trust(&mut self, host: &str, port: u16) {
        self.rdp_run_trust.retain(|(known, known_port, ..)| {
            !(known.eq_ignore_ascii_case(host) && *known_port == port)
        });
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
            Option<AcceptedCertificate>,
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
            trusted_for_run: self.whole_certificates_trusted_for_run(&profile.host, profile.port),
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
            credential_guard: self
                .settings
                .require_credential_guard
                .then(|| std::sync::Arc::clone(&self.credential_guard.detector)),
        })
    }

    /// Opens an RDP tab for a saved profile.
    pub(super) fn open_rdp(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(profile) = self.rdp_profiles().iter().find(|p| &p.id == id).cloned() else {
            return Vec::new();
        };
        self.open_rdp_profile(profile)
    }

    /// Opens saved RDP profile `id` in `mode` this once, as the C# "Connect with"
    /// (`ServerListViewModel.cs:835-854`, `RdpHandler.cs:662-675`): the profile is not
    /// changed, and everything else is as a plain connection, the Windows Hello gate passed
    /// already. Remote Desktop Connection goes through an SSH gateway as a plain connection
    /// does, and an RD Gateway, which the built-in client does not go through, still opens
    /// there. The tab opened keeps `mode` for its Reconnect, as the C# tab keeps it.
    pub(super) fn open_rdp_with(&mut self, id: &ProfileId, mode: RdpMode) -> Vec<Effect> {
        let Some(mut profile) = self.rdp_profiles().iter().find(|p| &p.id == id).cloned() else {
            return Vec::new();
        };
        profile.extras.external = mode.is_external();
        let before = self.tabs.len();
        let effects = self.open_rdp_profile(profile);
        if self.tabs.len() > before
            && let Some(tab) = self.tabs.last_mut()
        {
            tab.rdp_mode_override = Some(mode);
        }
        effects
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

    /// Connects `tab` again, with `accepted` as the certificate the user just agreed to.
    pub(super) fn reconnect_rdp(
        &mut self,
        tab_id: TabId,
        accepted: Option<AcceptedCertificate>,
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
        // An RDP server's certificate, an FTPS one's or a VNC one's: the same question, the
        // same pins.
        let (Phase::HostKey { .. }, Some((fingerprint, whole)), Some((host, port))) = (
            &tab.phase,
            tab.pending_rdp_key.take(),
            tab.profile
                .endpoint()
                .map(|(host, port)| (host.to_owned(), port)),
        ) else {
            return Vec::new();
        };
        let server = CertifiedServer::of(&tab.profile);
        tab.certificate_context = None;
        // As the C# `[RdpCertPrompt]` line: the server, the certificate, the answer.
        log::info!(
            "certificate question for the {} server {}: {whole}, key {fingerprint}, answered {}",
            server.name(),
            heimdall_core::profile::display_address(&host, port),
            match trust {
                KeyTrust::Refused => "refused",
                KeyTrust::Once => "trusted for this run",
                KeyTrust::Always => "trusted, recorded at the next connection",
            }
        );
        match trust {
            // Said as the C# says it of an RDP server: the user stopped it, at the certificate.
            KeyTrust::Refused => {
                tab.phase = Phase::Failed(if server == CertifiedServer::Rdp {
                    UiError::CertificateRefused
                } else {
                    UiError::Cancelled
                });
                Vec::new()
            }
            // Held in memory for this run: the file is not written. The next attempt takes it
            // as it is, later ones check it as a pin.
            KeyTrust::Once => {
                let trusted = (host, port, whole);
                if !self.rdp_run_trust.contains(&trusted) {
                    self.rdp_run_trust.push(trusted);
                }
                let decided = Decided {
                    key: fingerprint,
                    whole,
                    record: false,
                };
                self.reconnect_certified(tab_id, server, Some(decided))
            }
            // Recorded by the next attempt, and only if the server presents exactly this
            // certificate.
            KeyTrust::Always => {
                let decided = Decided {
                    key: fingerprint,
                    whole,
                    record: true,
                };
                self.reconnect_certified(tab_id, server, Some(decided))
            }
        }
    }

    /// Connects the tab of a `server` asking about its certificate again, with what the user
    /// just `decided`, if anything. An RDP server takes a certificate trusted once from those
    /// trusted for this run, and records the certificate accepted; an FTPS or VNC one takes
    /// the very certificate decided on.
    fn reconnect_certified(
        &mut self,
        tab_id: TabId,
        server: CertifiedServer,
        decided: Option<Decided>,
    ) -> Vec<Effect> {
        let whole = decided.map_or((None, None), Decided::whole);
        match server {
            CertifiedServer::Rdp => self.reconnect_rdp(
                tab_id,
                decided
                    .filter(|decided| decided.record)
                    .map(|decided| AcceptedCertificate {
                        key: decided.key,
                        certificate: decided.whole,
                    }),
            ),
            CertifiedServer::Ftps => self.reconnect_ftp(tab_id, whole),
            CertifiedServer::Vnc => self.reconnect_vnc(tab_id, whole),
        }
    }

    /// Forgets the certificates recorded for the server of an RDP, FTPS or VNC tab whose key
    /// changed, or whose trusted certificate is no longer valid, and those trusted for it
    /// for this run, then connects again: the certificate question comes back.
    pub(super) fn forget_rdp_certificate(&mut self, tab_id: TabId) -> Vec<Effect> {
        let (rdp_file, ftps_file, vnc_file) = (
            self.known_rdp_hosts(),
            self.known_ftps_hosts(),
            self.known_vnc_hosts(),
        );
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        // The server's own key only: a gateway's changed SSH key is not this server's.
        if !matches!(
            tab.phase,
            Phase::Failed(
                UiError::HostKeyChanged { target: None, .. }
                    | UiError::PinnedCertificateInvalid { .. }
            )
        ) {
            return Vec::new();
        }
        let (path, host, port) = match &tab.profile {
            TabProfile::Rdp(profile) => (rdp_file, profile.host.clone(), profile.port),
            TabProfile::Ftp(profile) => (ftps_file, profile.host.clone(), profile.port),
            TabProfile::Vnc(profile) => (vnc_file, profile.host.clone(), profile.port),
            _ => return Vec::new(),
        };
        if let Err(error) = KnownRdpHosts::new(path).forget(&host, port) {
            tab.phase = Phase::Failed(UiError::KnownHosts {
                detail: error.to_string(),
            });
            return Vec::new();
        }
        let server = CertifiedServer::of(&tab.profile);
        self.forget_run_trust(&host, port);
        self.reconnect_certified(tab_id, server, None)
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

/// How often the RDP desktops settling after connecting are looked at, as the C#
/// countdown's tick: the wait ends within this of its time, and the session bar counts down.
pub const STABILIZATION_TICK: Duration = Duration::from_secs(1);

impl App {
    /// How often the desktops settling after connecting are looked at; `None` while none is,
    /// so nothing ticks for nothing.
    #[must_use]
    pub fn stabilization_interval(&self) -> Option<Duration> {
        self.tabs
            .iter()
            .any(|tab| {
                tab.desktop
                    .as_ref()
                    .is_some_and(|pane| pane.stabilizing_until().is_some())
            })
            .then_some(STABILIZATION_TICK)
    }

    /// A tick at `now`: the desktops whose wait is over follow their tab again.
    pub(super) fn stabilization_tick(&mut self, now: std::time::Instant) {
        for tab in &mut self.tabs {
            if let Some(pane) = tab.desktop.as_deref_mut() {
                pane.settle(now);
            }
        }
    }

    /// The Resolution menu's "Skip stabilization": `tab`'s desktop follows its tab now, said
    /// as the C# says it.
    pub(super) fn skip_stabilization(&mut self, tab: TabId) {
        let skipped = self
            .tab_mut(tab)
            .and_then(|found| found.desktop.as_deref_mut())
            .is_some_and(DesktopPane::end_stabilization);
        if skipped {
            log::info!("RDP desktop settling skipped by the user");
            self.tell(super::Notice::StabilizationSkipped);
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
        let global_delay = self.settings.rdp_resize_enable_delay_ms;
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        let ready = matches!(event, ConnectionEvent::RdpReady { .. });
        apply(tab, event);
        // Each connection, a reconnection included, settles before following the tab, as
        // the C# one: the wait the profile sets, else the settings'.
        if ready {
            let own = match &tab.profile {
                TabProfile::Rdp(profile) => profile.options.resize_enable_delay_ms,
                _ => None,
            };
            if let Some(pane) = tab.desktop.as_deref_mut() {
                let delay = heimdall_core::settings::rdp_resize_enable_delay(own, global_delay);
                pane.stabilize(delay, std::time::Instant::now());
                if pane.stabilizing_until().is_some() {
                    log::info!(
                        "RDP desktop settling for {} ms before following its tab",
                        delay.as_millis()
                    );
                }
            }
        }
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
            TabProfile::Vnc(_) => (self.known_vnc_hosts(), None),
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
