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

//! Reconnecting a tab, as the C# Heimdall's Reconnect does: a failed or ended session opens
//! again in its place, from its profile as it is now saved, so a profile fixed after a
//! failure is the one used. And the way past a changed SSH key: the recorded key is
//! forgotten, then the new one is asked about as on a first connection.

use heimdall_core::profile::ProfileId;
use heimdall_ssh::KnownHosts;

use super::tree::ProfileKind;
use super::{App, Effect, Message, Phase, Tab, TabProfile};
use crate::driver::Purpose;
use crate::error::UiError;
use crate::ids::TabId;
use crate::local_driver::LocalShell;

/// How a tab opens again.
#[derive(Debug, Clone)]
pub(super) enum Reopen {
    /// From its saved profile, as it is when reopened.
    Profile(ProfileId),
    /// The same local shell again: one started from the sidebar, with no profile.
    Shell(LocalShell),
}

impl Reopen {
    /// How a tab showing `profile` opens again, before its opener says more.
    pub(super) fn of(profile: &TabProfile) -> Self {
        match profile {
            TabProfile::Ssh(profile) => Self::Profile(profile.id.clone()),
            TabProfile::Rdp(profile) => Self::Profile(profile.id.clone()),
            TabProfile::Telnet(profile) => Self::Profile(profile.id.clone()),
            TabProfile::Vnc(profile) => Self::Profile(profile.id.clone()),
            TabProfile::Local(shell) => Self::Shell(shell.clone()),
        }
    }
}

impl App {
    /// Whether `tab` offers Reconnect: its session failed or ended, and what it ran can run
    /// again, its profile still saved.
    #[must_use]
    pub fn can_reconnect(&self, tab: &Tab) -> bool {
        if !matches!(tab.phase, Phase::Failed(_) | Phase::Closed { .. }) {
            return false;
        }
        match &tab.reopen {
            Reopen::Profile(id) => self.profile_summary(id).is_some(),
            Reopen::Shell(_) => true,
        }
    }

    /// Opens the session of `tab_id` again, in its place; the old attempt is stopped. When
    /// nothing opens (a local command waiting for approval), the tab stays as it was.
    pub(super) fn reconnect_tab(&mut self, tab_id: TabId) -> Vec<Effect> {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return Vec::new();
        };
        if !self.can_reconnect(&self.tabs[index]) {
            return Vec::new();
        }
        let (reopen, purpose) = (self.tabs[index].reopen.clone(), self.tabs[index].purpose);
        let before = self.tabs.len();
        let effects = match reopen {
            Reopen::Profile(id) => self.open_saved(&id, purpose),
            Reopen::Shell(shell) => self.open_local(shell),
        };
        if self.tabs.len() > before
            && let Some(reopened) = self.tabs.pop()
        {
            let mut old = std::mem::replace(&mut self.tabs[index], reopened);
            old.stop();
            self.active = Some(self.tabs[index].id);
        }
        effects
    }

    /// Opens saved profile `id` with its own protocol; an SSH profile for `purpose`, a shell
    /// or its files.
    pub(super) fn open_saved(&mut self, id: &ProfileId, purpose: Purpose) -> Vec<Effect> {
        let Some(profile) = self.profile_summary(id) else {
            return Vec::new();
        };
        let message = match profile.kind {
            ProfileKind::Ssh if purpose == Purpose::Files => Message::OpenFiles(profile.id),
            ProfileKind::Ssh => Message::OpenProfile(profile.id),
            ProfileKind::Rdp => Message::OpenRdp(profile.id),
            ProfileKind::Telnet => Message::OpenTelnet(profile.id),
            ProfileKind::Vnc => Message::OpenVnc(profile.id),
            ProfileKind::Local => Message::OpenLocalProfile(profile.id),
            ProfileKind::WinRm => Message::OpenWinRm(profile.id),
        };
        self.update(message)
    }

    /// Records how the tab just opened (the last one) opens again.
    pub(super) fn reopened_by(&mut self, reopen: Reopen) {
        if let Some(tab) = self.tabs.last_mut() {
            tab.reopen = reopen;
        }
    }

    /// The way past a changed key, as the C# Heimdall's "Accept new key": the key recorded
    /// for the server that presented another is forgotten, and the tab connects again, which
    /// asks about the new key with its fingerprint before trusting it. For an RDP server's
    /// own certificate, its record is forgotten the same way.
    pub(super) fn forget_server(&mut self, tab_id: TabId) -> Vec<Effect> {
        let Some(tab) = self.tab(tab_id) else {
            return Vec::new();
        };
        let Phase::Failed(UiError::HostKeyChanged { target, .. }) = &tab.phase else {
            return Vec::new();
        };
        let Some(server) = target.clone() else {
            return self.forget_rdp_certificate(tab_id);
        };
        // An SSH key: the tab's own server's, or a gateway's on the way, in the one file.
        let forgotten = KnownHosts::new(&self.config.known_hosts).forget(&server.host, server.port);
        if let Err(error) = forgotten {
            if let Some(tab) = self.tab_mut(tab_id) {
                tab.phase = Phase::Failed(UiError::from(&error));
            }
            return Vec::new();
        }
        match self.tab(tab_id).map(|tab| &tab.profile) {
            // An RDP server reached through a gateway: its connection starts again.
            Some(TabProfile::Rdp(_)) => self.reconnect_rdp(tab_id, None),
            _ => self.reconnect_tab(tab_id),
        }
    }
}
