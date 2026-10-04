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

use heimdall_core::profile::{ProfileId, VncProfile};
use tokio_util::sync::CancellationToken;

use super::{App, Effect, Phase, Tab, TabProfile};
use crate::desktop::DesktopPane;
use crate::driver::Purpose;
use crate::event::ConnectionEvent;
use crate::ids::{AttemptId, TabId};
use crate::vnc_driver::VncRequest;

impl App {
    /// Opens a VNC tab for a saved profile.
    pub(super) fn open_vnc(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(profile) = self.vnc_profiles().iter().find(|p| &p.id == id).cloned() else {
            return Vec::new();
        };
        self.open_vnc_profile(profile)
    }

    /// Opens a VNC tab for `profile`.
    pub(super) fn open_vnc_profile(&mut self, profile: VncProfile) -> Vec<Effect> {
        let tab_id = TabId::fresh();
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = VncRequest {
            profile: profile.clone(),
            cancel: cancel.clone(),
        };
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
}

/// Applies the event that opens a VNC desktop.
pub(super) fn apply(tab: &mut Tab, event: ConnectionEvent) {
    if let ConnectionEvent::VncReady {
        name,
        framebuffer,
        input,
    } = event
    {
        let view_only = matches!(&tab.profile, TabProfile::Vnc(profile) if profile.view_only);
        tab.phase = Phase::Connected;
        let mut pane = DesktopPane::vnc(framebuffer, input, view_only);
        // Shown on its bar, as the C# session title: the server's words, made safe.
        let name = crate::text::server_text(name.trim());
        pane.desktop_name = (!name.is_empty()).then_some(name);
        tab.desktop = Some(Box::new(pane));
    }
}
