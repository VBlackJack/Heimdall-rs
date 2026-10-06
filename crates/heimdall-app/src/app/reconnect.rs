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
    /// The same session saved nowhere, as "Connect as..." opened it, for its purpose.
    Transient(Box<TabProfile>, Purpose),
}

impl Reopen {
    /// How a tab showing `profile` opens again, before its opener says more.
    pub(super) fn of(profile: &TabProfile) -> Self {
        match profile {
            TabProfile::Ssh(profile) => Self::Profile(profile.id.clone()),
            TabProfile::Rdp(profile) => Self::Profile(profile.id.clone()),
            TabProfile::Telnet(profile) => Self::Profile(profile.id.clone()),
            TabProfile::Vnc(profile) => Self::Profile(profile.id.clone()),
            TabProfile::Ftp(profile) => Self::Profile(profile.id.clone()),
            TabProfile::Local(shell) => Self::Shell(shell.clone()),
            TabProfile::WinRm(profile) => Self::Profile(profile.id.clone()),
        }
    }
}

impl Tab {
    /// The saved profile it opened, when it opened one: not a local shell started from the
    /// sidebar, nor a session "Connect as..." opened.
    pub(super) fn saved_profile(&self) -> Option<&ProfileId> {
        match &self.reopen {
            Reopen::Profile(id) => Some(id),
            Reopen::Shell(_) | Reopen::Transient(..) => None,
        }
    }
}

impl App {
    /// Whether `tab` offers Reconnect on its card: its session failed or ended, and what it
    /// ran can run again, its profile still saved.
    #[must_use]
    pub fn can_reconnect(&self, tab: &Tab) -> bool {
        matches!(tab.phase, Phase::Failed(_) | Phase::Closed { .. }) && self.can_reopen(tab)
    }

    /// Whether the tab's menu offers Reconnect: as the C# Heimdall's, a live session too,
    /// once it is past connecting.
    #[must_use]
    pub fn can_restart(&self, tab: &Tab) -> bool {
        matches!(
            tab.phase,
            Phase::Connected | Phase::Failed(_) | Phase::Closed { .. }
        ) && self.can_reopen(tab)
    }

    /// Opens the session of `tab_id` again, in its place, under the name the user gave it;
    /// the old session is stopped. When nothing opens (a local command waiting for
    /// approval), the tab stays as it was.
    pub(super) fn reconnect_tab(&mut self, tab_id: TabId) -> Vec<Effect> {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return Vec::new();
        };
        if !self.can_restart(&self.tabs[index]) {
            return Vec::new();
        }
        let (reopen, purpose) = (self.tabs[index].reopen.clone(), self.tabs[index].purpose);
        let before = self.tabs.len();
        self.replacing = true;
        let effects = self.open_again(reopen, purpose);
        self.replacing = false;
        if self.tabs.len() > before
            && let Some(reopened) = self.tabs.pop()
        {
            let mut old = std::mem::replace(&mut self.tabs[index], reopened);
            let reopened = self.tabs[index].id;
            self.tabs[index].custom_title = old.custom_title.take();
            // Its place on the strip, pinned, and in a split: the host's split goes with it,
            // a docked pane stays docked, as the C# `ReconnectPaneAsync`.
            self.tabs[index].pinned = old.pinned;
            self.tabs[index].layout = old.layout.take();
            self.repoint_pane(tab_id, reopened);
            // A health panel shown stays shown, asked again once connected.
            self.tabs[index].health.shown = old.health.shown;
            self.tabs[index].tunnels_panel = old.tunnels_panel;
            // Files open in an external editor go on being watched: their saves are sent
            // once the new connection is up.
            if let (Some(files), Some(before)) = (
                self.tabs[index].files.as_deref_mut(),
                old.files.as_deref_mut(),
            ) {
                files.edits = std::mem::take(&mut before.edits);
                // The server's files alone, when they were.
                files.show_local(!before.local_hidden);
                // The transfers listed stay, those cut short stopped: Retry runs them on
                // the new connection.
                files.transfers = before.hand_over_transfers();
                // As the text in the integrated editor: saved once connected again.
                files.editor = before.editor.take();
            }
            old.stop();
            self.active = Some(reopened);
            self.sync_focus();
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
            // An SFTP profile opens its files, as the C# one.
            ProfileKind::Ssh if purpose == Purpose::Files => Message::OpenFiles(profile.id),
            ProfileKind::Sftp => Message::OpenFiles(profile.id),
            ProfileKind::Ssh => Message::OpenProfile(profile.id),
            ProfileKind::Rdp => Message::OpenRdp(profile.id),
            ProfileKind::Telnet => Message::OpenTelnet(profile.id),
            ProfileKind::Vnc => Message::OpenVnc(profile.id),
            ProfileKind::Ftp => Message::OpenFtp(profile.id),
            ProfileKind::Local => Message::OpenLocalProfile(profile.id),
            ProfileKind::WinRm => Message::OpenWinRm(profile.id),
            ProfileKind::Citrix => Message::OpenCitrix(profile.id),
        };
        self.update(message)
    }

    /// Opens what `reopen` names again, in a new tab, the last, which opens again the same
    /// way; a saved profile for `purpose`.
    pub(super) fn open_again(&mut self, reopen: Reopen, purpose: Purpose) -> Vec<Effect> {
        match reopen {
            Reopen::Profile(id) => self.open_saved(&id, purpose),
            Reopen::Shell(shell) => self.open_local(shell),
            Reopen::Transient(profile, purpose) => {
                let effects = self.open_transient(TabProfile::clone(&profile), purpose);
                self.reopened_by(Reopen::Transient(profile, purpose));
                effects
            }
        }
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
