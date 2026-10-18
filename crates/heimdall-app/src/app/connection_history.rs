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

//! The connection history the C# Heimdall keeps (`ConnectionHistory.RecordConnect` when a
//! session is ready, `RecordDisconnect` when its pane closes): a line for each session
//! opened or closed, in the log folder the window gives. Without a folder, as in a test,
//! nothing is written.

use std::path::PathBuf;

use heimdall_core::connection_history::{self, Action};
use heimdall_core::profile::ProfileId;

use super::{App, Phase, ProfileKind, Tab, TabProfile};
use crate::ids::TabId;

impl TabProfile {
    /// The saved profile it connects to; `None` for a local shell or a tool, which have none.
    #[must_use]
    pub fn profile_id(&self) -> Option<&ProfileId> {
        match self {
            Self::Ssh(profile) => Some(&profile.id),
            Self::Rdp(profile) => Some(&profile.id),
            Self::Telnet(profile) => Some(&profile.id),
            Self::Vnc(profile) => Some(&profile.id),
            Self::Ftp(profile) => Some(&profile.id),
            Self::WinRm(profile) => Some(&profile.id),
            Self::Citrix(profile) => Some(&profile.id),
            Self::Local(_) | Self::Tool(_) => None,
        }
    }
}

/// The protocol of a history line, as the C# `ConnectionTypeCatalog` names it.
fn connection_type(kind: ProfileKind) -> &'static str {
    match kind {
        ProfileKind::Ssh => "SSH",
        ProfileKind::Sftp => "SFTP",
        ProfileKind::Rdp => "RDP",
        ProfileKind::Telnet => "Telnet",
        ProfileKind::Vnc => "VNC",
        ProfileKind::Ftp => "FTP",
        ProfileKind::Local => "Local",
        ProfileKind::WinRm => "WINRM",
        ProfileKind::Citrix => "Citrix",
        ProfileKind::Web => "WEB",
    }
}

impl App {
    /// The folder the connection history is written in; `None` for none, which is how a
    /// test runs.
    pub fn set_connection_history(&mut self, folder: Option<PathBuf>) {
        self.connection_history = folder;
    }

    /// Records `action` for the session `tab` holds. Nothing for a tab with no saved profile
    /// (a local shell, a tool).
    pub(super) fn record_connection(&self, tab: &Tab, action: Action) {
        let (Some(folder), Some(id)) = (&self.connection_history, tab.profile.profile_id()) else {
            return;
        };
        connection_history::record(
            folder,
            action,
            id.as_str(),
            tab.profile.name(),
            connection_type(tab.profile.kind()),
        );
    }

    /// Records the session of `tab_id` as opened when `was_connected` was false and it now
    /// is: once for each time a session comes up.
    pub(super) fn note_connected(&self, tab_id: TabId, was_connected: bool) {
        if was_connected {
            return;
        }
        if let Some(tab) = self.tab(tab_id).filter(|tab| tab.phase == Phase::Connected) {
            self.record_connection(tab, Action::Connected);
        }
    }
}
