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

//! The working folder a shell reports (OSC 7), kept on its tab, and the SFTP pane beside
//! an SSH shell going there, as the C# `FollowSftpToCurrentDirectory`.
//!
//! The C# listens to OSC 7 alone: it never makes the shell report, and here nothing is
//! ever typed into the shell either. A report names a path and a host; the host is left
//! aside, as the C# leaves it, so a shell gone on to another server through a nested
//! `ssh` sends the pane to that path on its own server, where it may not exist.
//!
//! Of the panes split with the shell, the first SFTP pane whose "cwd" toggle is on goes
//! there, as the C# picks it; an FTP pane never does. It goes there as a path typed in its
//! bar would: the folder left kept in its history, and a folder that cannot be listed said
//! in the pane, which keeps showing the one it showed, as the C# says the listing failure.
//! While the pane lists a folder already, the report is let go, as the C# load gate lets it
//! go; here the same folder reported again at the next prompt is followed then, and not
//! listed again otherwise. While the pane asks for a name or a confirmation, the report is
//! let go too, so that a new folder is never made elsewhere than where it was asked for.

use heimdall_files::RemotePath;

use super::{App, Effect, TabProfile};
use crate::driver::Purpose;
use crate::ids::TabId;

impl App {
    /// Shell `shell` reported `directory` as its working folder: kept on its tab, and gone
    /// to by the SFTP pane following it, when the shell is an SSH one.
    pub(super) fn working_directory_reported(
        &mut self,
        shell: TabId,
        directory: String,
    ) -> Vec<Effect> {
        let Some(tab) = self.tab_mut(shell) else {
            return Vec::new();
        };
        let ssh_shell = tab.purpose == Purpose::Shell && matches!(tab.profile, TabProfile::Ssh(_));
        tab.working_directory = Some(directory);
        if !ssh_shell {
            return Vec::new();
        }
        let Some(pane) = self.following_pane(shell) else {
            return Vec::new();
        };
        self.follow_shell(pane, shell)
    }

    /// The first pane split with `shell` that follows it: an SFTP pane, its toggle on.
    fn following_pane(&self, shell: TabId) -> Option<TabId> {
        let host = self.host_of(shell)?;
        self.panes_of(host).into_iter().find(|pane| {
            *pane != shell
                && self
                    .tab(*pane)
                    .and_then(|tab| tab.files.as_deref())
                    .and_then(|files| files.follow.as_ref())
                    .is_some_and(|follow| follow.on)
        })
    }

    /// SFTP pane `pane` goes to the folder `shell` last reported, unless it went there
    /// already, is not connected, lists a folder already or asks a question about itself.
    fn follow_shell(&mut self, pane: TabId, shell: TabId) -> Vec<Effect> {
        let Some(directory) = self
            .tab(shell)
            .and_then(|tab| tab.working_directory.clone())
        else {
            return Vec::new();
        };
        let asking = self
            .pending_operation
            .as_ref()
            .is_some_and(|pending| pending.tab() == pane);
        let Some(files) = self.files_mut(pane) else {
            return Vec::new();
        };
        let Some(client) = files.client.clone() else {
            return Vec::new();
        };
        if asking || files.remote.loading {
            return Vec::new();
        }
        let Some(follow) = files.follow.as_mut() else {
            return Vec::new();
        };
        if follow.followed.as_deref() == Some(directory.as_str()) {
            return Vec::new();
        }
        let path = RemotePath::from(directory.as_str());
        follow.followed = Some(directory);
        files.remote.leave();
        files.remote.loading = true;
        files.remote.discard_listing = false;
        vec![super::files_sudo::remote_listing(files, pane, client, path)]
    }

    /// The "cwd" toggle of SFTP pane `tab`, as the C# one: this pane alone follows its
    /// shell, or no longer; the setting stays as it is. Turned on, the pane goes nowhere
    /// until the shell reports again, as the C# waits for the next report.
    pub(super) fn toggle_follow(&mut self, tab: TabId) -> Vec<Effect> {
        if let Some(follow) = self.files_mut(tab).and_then(|files| files.follow.as_mut()) {
            follow.on = !follow.on;
            follow.followed = None;
        }
        Vec::new()
    }
}
