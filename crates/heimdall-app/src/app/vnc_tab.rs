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

//! VNC tabs: a remote desktop like an RDP one, drawn from a VNC session.

use std::path::PathBuf;

use heimdall_core::profile::{ProfileId, VncProfile};
use heimdall_rdp::CertificateHash;
use tokio_util::sync::CancellationToken;

use super::{App, Effect, Phase, Tab, TabProfile};
use crate::desktop::DesktopPane;
use crate::driver::Purpose;
use crate::event::ConnectionEvent;
use crate::ids::{AttemptId, TabId};
use crate::vnc_driver::VncRequest;

/// File of the VNC servers whose certificate the user trusts, beside the FTPS one.
const KNOWN_VNC_HOSTS_FILE_NAME: &str = "known_vnc_hosts";

impl App {
    /// File of the VNC servers whose certificate the user trusts.
    pub(super) fn known_vnc_hosts(&self) -> PathBuf {
        self.config
            .known_hosts
            .with_file_name(KNOWN_VNC_HOSTS_FILE_NAME)
    }

    /// What connecting to `profile` needs, with the key the user just `accepted` or
    /// `trusted_once`, after the certificate question.
    fn vnc_request(
        &self,
        profile: &VncProfile,
        (accepted, trusted_once): (Option<CertificateHash>, Option<CertificateHash>),
        cancel: CancellationToken,
    ) -> VncRequest {
        VncRequest {
            profile: profile.clone(),
            known_hosts: self.known_vnc_hosts(),
            accepted,
            trusted_once,
            trusted_for_run: self.whole_certificates_trusted_for_run(&profile.host, profile.port),
            cancel,
        }
    }

    /// Opens a VNC tab for a saved profile.
    pub(super) fn open_vnc(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(profile) = self.vnc_profiles().iter().find(|p| &p.id == id).cloned() else {
            return Vec::new();
        };
        self.open_vnc_profile(profile)
    }

    /// Opens a VNC tab for `profile`.
    pub(super) fn open_vnc_profile(&mut self, profile: VncProfile) -> Vec<Effect> {
        if self.session_limit_reached() {
            return Vec::new();
        }
        let tab_id = TabId::fresh();
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = self.vnc_request(&profile, (None, None), cancel.clone());
        let mut tab = Tab::new(
            self.terminal_palette(),
            tab_id,
            TabProfile::Vnc(profile),
            Purpose::Vnc,
            self.viewport,
            attempt,
            cancel,
        );
        tab.files = None;
        self.tabs.push(tab);
        self.active = Some(tab_id);
        vec![Effect::ConnectVnc {
            tab: tab_id,
            attempt,
            request: Box::new(request),
        }]
    }

    /// Connects a VNC tab again, with the key the user just accepted, or trusted once, if
    /// any.
    pub(super) fn reconnect_vnc(
        &mut self,
        tab_id: TabId,
        decided: (Option<CertificateHash>, Option<CertificateHash>),
    ) -> Vec<Effect> {
        let Some(TabProfile::Vnc(profile)) = self.tab(tab_id).map(|tab| tab.profile.clone()) else {
            return Vec::new();
        };
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = self.vnc_request(&profile, decided, cancel.clone());
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        tab.attempt = attempt;
        tab.cancel = cancel;
        tab.phase = Phase::Connecting;
        tab.desktop = None;
        vec![Effect::ConnectVnc {
            tab: tab_id,
            attempt,
            request: Box::new(request),
        }]
    }
}

/// Applies the event that opens a VNC desktop, or names it anew.
pub(super) fn apply(tab: &mut Tab, event: ConnectionEvent) {
    if let ConnectionEvent::DesktopRenamed(name) = &event {
        if let Some(pane) = tab.desktop.as_deref_mut() {
            let name = crate::text::server_text(name.trim());
            pane.desktop_name = (!name.is_empty()).then_some(name);
        }
        return;
    }
    if let ConnectionEvent::VncReady {
        name,
        framebuffer,
        input,
        cursor,
        tls,
    } = event
    {
        let view_only = matches!(&tab.profile, TabProfile::Vnc(profile) if profile.view_only);
        tab.phase = Phase::Connected;
        let mut pane = DesktopPane::vnc(framebuffer, (input, cursor), view_only);
        // Shown on its bar, as the C# session title: the server's words, made safe.
        let name = crate::text::server_text(name.trim());
        pane.desktop_name = (!name.is_empty()).then_some(name);
        pane.tls = tls;
        tab.desktop = Some(Box::new(pane));
    }
}
