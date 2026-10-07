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

//! The working folder a shell reports (OSC 7, or `ConEmu`'s OSC 9;9), kept on its tab, and
//! the pane beside the shell going there: the SFTP pane beside an SSH shell, as the C#
//! `FollowSftpToCurrentDirectory`, and the file browser docked beside a local shell.
//!
//! The C# listens to OSC 7 alone: it never makes the shell report, and here nothing is
//! ever typed into the shell either. A report names a path and a host; the host is left
//! aside, as the C# leaves it, so a shell gone on to another server through a nested
//! `ssh` sends the pane to that path on its own server, where it may not exist.
//!
//! Of the panes split with an SSH shell, the first SFTP pane whose "cwd" toggle is on goes
//! there, as the C# picks it; an FTP pane never does, nor a local browser. It follows OSC
//! 7 alone, a POSIX path: an OSC 9;9 report names a Windows path, in the convention of a
//! Windows shell rather than of the server's files, and moves no SFTP pane. Of the panes
//! split with a local shell, the first local browser whose toggle is on goes there, never a
//! server's pane. The C# local browser makes the shell follow it instead, typing `cd` into
//! the shell; this is the other way round, and only when the shell reports. A local shell
//! reports a path as a file URL writes it, or a Windows path in an OSC 9;9 report, as
//! Windows Terminal's shell integration and the usual Windows prompts send it, which
//! [`heimdall_term::local_folder`] reads as a folder of this computer: on Windows
//! `/C:/Users/x` and `C:\Users\x` are `C:\Users\x`, and a Linux folder a WSL shell reports
//! names none, the report then left alone.
//!
//! The pane goes there as a path typed in its bar would: the folder left kept in its
//! history, and a folder that cannot be listed said in the pane, which keeps showing the
//! one it showed, as the C# says the listing failure. While the pane lists a folder
//! already, the report is let go, as the C# load gate lets it go; here the same folder
//! reported again at the next prompt is followed then, and not listed again otherwise.
//! While the pane asks for a name or a confirmation, the report is let go too, so that a
//! new folder is never made elsewhere than where it was asked for.

use heimdall_files::RemotePath;

use super::{App, Effect, TabProfile};
use crate::driver::Purpose;
use crate::files::{FilesPane, Side};
use crate::ids::TabId;

/// What a POSIX path, the only kind an SFTP pane follows, starts with: an OSC 7 report's
/// path always does, an OSC 9;9 report's Windows path never.
const POSIX_ROOT: char = '/';

impl App {
    /// Shell `shell` reported `directory` as its working folder: kept on its tab, and gone
    /// to by the pane following it: the SFTP pane of an SSH shell, when it is a POSIX path
    /// (OSC 7), the local browser of a local shell.
    pub(super) fn working_directory_reported(
        &mut self,
        shell: TabId,
        directory: String,
    ) -> Vec<Effect> {
        let Some(tab) = self.tab_mut(shell) else {
            return Vec::new();
        };
        let posix = directory.starts_with(POSIX_ROOT);
        let followed = match tab.profile {
            TabProfile::Ssh(_) if posix => Some(Side::Remote),
            TabProfile::Local(_) => Some(Side::Local),
            _ => None,
        }
        .filter(|_| tab.purpose == Purpose::Shell);
        tab.working_directory = Some(directory);
        let Some(side) = followed else {
            return Vec::new();
        };
        let Some(pane) = self.following_pane(shell, side) else {
            return Vec::new();
        };
        match side {
            Side::Remote => self.follow_shell(pane, shell),
            Side::Local => self.follow_local_shell(pane, shell),
        }
    }

    /// The first pane split with `shell` that follows it, its toggle on: an SFTP pane when
    /// `side` is the server's, a local browser when it is this computer's.
    fn following_pane(&self, shell: TabId, side: Side) -> Option<TabId> {
        let host = self.host_of(shell)?;
        let local = side == Side::Local;
        self.panes_of(host).into_iter().find(|pane| {
            *pane != shell
                && self
                    .tab(*pane)
                    .and_then(|tab| tab.files.as_deref())
                    .filter(|files| files.local_only == local)
                    .and_then(|files| files.follow.as_ref())
                    .is_some_and(|follow| follow.on)
        })
    }

    /// SFTP pane `pane` goes to the folder `shell` last reported, unless it went there
    /// already, is not connected, lists a folder already or asks a question about itself.
    fn follow_shell(&mut self, pane: TabId, shell: TabId) -> Vec<Effect> {
        let Some((files, directory)) = self.follow_target(pane, shell, Side::Remote) else {
            return Vec::new();
        };
        let Some(client) = files.client.clone() else {
            return Vec::new();
        };
        let path = RemotePath::from(directory.as_str());
        files.remote.leave();
        files.remote.loading = true;
        files.remote.discard_listing = false;
        vec![super::files_sudo::remote_listing(files, pane, client, path)]
    }

    /// Local browser `pane` goes to the folder `shell` last reported, unless it names no
    /// folder of this computer, the browser went there already, lists a folder already or
    /// asks a question about itself.
    fn follow_local_shell(&mut self, pane: TabId, shell: TabId) -> Vec<Effect> {
        let Some(folder) = self
            .tab(shell)
            .and_then(|tab| tab.working_directory.as_deref())
            .and_then(heimdall_term::local_folder)
        else {
            return Vec::new();
        };
        let Some((files, _)) = self.follow_target(pane, shell, Side::Local) else {
            return Vec::new();
        };
        files.local.leave();
        files.local.loading = true;
        files.local.discard_listing = false;
        vec![Effect::ListLocal {
            tab: pane,
            path: folder,
        }]
    }

    /// Pane `pane`, and the folder `shell` last reported, when the pane is to go there on
    /// `side`: it went elsewhere last, is connected when it is a server's, lists no folder
    /// and asks no question about itself. The folder is then the one it followed.
    fn follow_target(
        &mut self,
        pane: TabId,
        shell: TabId,
        side: Side,
    ) -> Option<(&mut FilesPane, String)> {
        let directory = self.tab(shell)?.working_directory.clone()?;
        let asking = self
            .pending_operation
            .as_ref()
            .is_some_and(|pending| pending.tab() == pane);
        let files = self.files_mut(pane)?;
        let busy = match side {
            Side::Remote => files.client.is_none() || files.remote.loading,
            Side::Local => files.local.loading,
        };
        if asking || busy {
            return None;
        }
        let follow = files.follow.as_mut()?;
        if follow.followed.as_deref() == Some(directory.as_str()) {
            return None;
        }
        follow.followed = Some(directory.clone());
        Some((files, directory))
    }

    /// The "cwd" toggle of pane `tab`, an SFTP pane as the C# one, or a local browser: this
    /// pane alone follows its shell, or no longer; the setting stays as it is. Turned on,
    /// the pane goes nowhere until the shell reports again, as the C# waits for the next
    /// report.
    pub(super) fn toggle_follow(&mut self, tab: TabId) -> Vec<Effect> {
        if let Some(follow) = self.files_mut(tab).and_then(|files| files.follow.as_mut()) {
            follow.on = !follow.on;
            follow.followed = None;
        }
        Vec::new()
    }
}
