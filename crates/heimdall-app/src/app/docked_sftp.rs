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

//! The SFTP pane an SSH shell gets beside it once connected, as the C# `AutoOpenSftpAsync`.
//!
//! The C# opens it after every SSH session made ready, its first connection or a reconnect,
//! for a server of the inventory with an account: a session typed in Quick Connect or
//! opened with "Connect as" has none, and gets no pane. The pane is a connection of its own
//! to the same server, through the same gateways, with the profile's credentials; it is not
//! counted against the session limit. Its failure is said in the status bar and the shell
//! is left alone.
//!
//! Here the pane is a Files tab of the profile docked in the shell's tab, side by side, as
//! the second pane at an even share, the keyboard staying where it was. It docks at once and
//! connects in its pane, asking there what it must; one that fails before it connects is
//! closed, as the C# never shows it. A shell already split has no room for it: a reconnect
//! keeps the pane it had rather than docking another.

use heimdall_core::profile::SshProfile;

use super::split::{Axis, MAX_PANES, Placement, SplitMessage};
use super::{App, Effect, Notice, TabProfile};
use crate::driver::Purpose;
use crate::error::UiError;
use crate::ids::TabId;

impl App {
    /// Docks the SFTP pane beside SSH shell `shell`, just connected, when the settings ask
    /// for it and the shell has room: what connects the pane.
    pub(super) fn dock_sftp(&mut self, shell: TabId) -> Vec<Effect> {
        // Panes closed or opened again before they connected are no longer waited for.
        let tabs: Vec<TabId> = self.tabs.iter().map(|tab| tab.id).collect();
        self.docking_sftp.retain(|docking| tabs.contains(docking));
        if !self.settings.sftp_browser.auto_opens() {
            return Vec::new();
        }
        let Some(profile) = self.sftp_companion(shell) else {
            return Vec::new();
        };
        // Split already, a reconnect among them, or docked in another tab: no room.
        if self.is_docked(shell) || self.panes_of(shell).len() >= MAX_PANES {
            return Vec::new();
        }
        let active = self.active;
        // Not counted against the session limit, as the C# pane is no session tab.
        let (pane, mut effects) = self.open_ssh_tab(profile, Purpose::Files);
        if let Some(files) = self.tab_mut(pane).and_then(|tab| tab.files.as_deref_mut()) {
            // Half a tab wide: the server's files alone, as the C# pane shows them.
            files.show_local(false);
        }
        effects.extend(self.split_message(SplitMessage::Merge {
            host: shell,
            tab: pane,
            axis: Axis::SideBySide,
            placement: Placement::Second,
        }));
        // The keyboard stays on the shell, and on the tab shown when another one is.
        if let Some(layout) = self.tab_mut(shell).and_then(|tab| tab.layout.as_mut()) {
            layout.focus = shell;
        }
        self.active = active;
        self.docking_sftp.push(pane);
        effects
    }

    /// The saved profile the SFTP pane beside `shell` opens: the shell's own, read again
    /// from the store as the C# reads its inventory, when it has an account; none for a tab
    /// that is not an SSH shell, or whose session is saved nowhere.
    fn sftp_companion(&self, shell: TabId) -> Option<SshProfile> {
        let tab = self
            .tab(shell)
            .filter(|tab| tab.purpose == Purpose::Shell)?;
        let TabProfile::Ssh(opened) = &tab.profile else {
            return None;
        };
        self.profiles()
            .iter()
            .find(|profile| profile.id == opened.id)
            .filter(|profile| {
                profile
                    .username
                    .as_deref()
                    .is_some_and(|user| !user.trim().is_empty())
            })
            .cloned()
    }

    /// The SFTP pane `tab`, docked by itself, failed before it connected: closed, and the
    /// failure said, as the C# says it; whether it was one. Taken out of its split by then,
    /// it stays, showing its failure as any Files tab.
    pub(super) fn docked_sftp_failed(&mut self, tab: TabId, error: UiError) -> bool {
        let Some(at) = self.docking_sftp.iter().position(|docking| *docking == tab) else {
            return false;
        };
        self.docking_sftp.remove(at);
        if !self.is_docked(tab) {
            return false;
        }
        self.close_pane(tab);
        self.tell(Notice::SftpAutoOpenFailed(error));
        true
    }
}
