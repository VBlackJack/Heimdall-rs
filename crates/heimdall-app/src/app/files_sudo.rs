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

//! A Files tab's sudo mode, as the C# "sudo" toggle: turned on and off by the user only,
//! over SSH only; while on, the server's folders are listed as root, the password sudo asks
//! for asked once for the tab and forgotten when the mode is turned off.

use heimdall_files::{RemotePath, RemoteSession};

use super::files_edit::{SudoAction, is_password_error};
use super::{App, Effect, Notice};
use crate::files::{FilesError, FilesPane, RemoteEntry, Side};
use crate::ids::TabId;

/// The listing of the server's folder `path` in `tab`: as root while its sudo mode is on,
/// with the password sudo took for the tab.
pub(super) fn remote_listing(
    files: &FilesPane,
    tab: TabId,
    client: RemoteSession,
    path: RemotePath,
) -> Effect {
    match files.shell.clone().filter(|_| files.sudo_mode) {
        Some(shell) => Effect::SudoListRemote {
            tab,
            shell,
            path,
            password: files.sudo_password.clone(),
        },
        None => Effect::ListRemote { tab, client, path },
    }
}

impl App {
    /// The "sudo" toggle: the server pane's sudo mode turned on, its folder listed again as
    /// root; or off. Never offered without an SSH connection: an FTP tab has none.
    pub(super) fn toggle_sudo(&mut self, tab: TabId) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        if files.shell.is_none() || files.client.is_none() {
            return Vec::new();
        }
        if files.sudo_mode {
            return self.sudo_mode_off(tab);
        }
        files.sudo_mode = true;
        self.tell(Notice::FilesSudoMode(true));
        self.list(tab, Side::Remote)
    }

    /// The sudo mode turned off: the password sudo took for the tab forgotten, its bytes
    /// wiped, and the folder listed again as the account.
    pub(super) fn sudo_mode_off(&mut self, tab: TabId) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab).filter(|files| files.sudo_mode) else {
            return Vec::new();
        };
        files.sudo_mode = false;
        files.sudo_password = None;
        self.tell(Notice::FilesSudoMode(false));
        self.list(tab, Side::Remote)
    }

    /// A folder listed as root arrived: shown as any listing; sudo wanting a password asks
    /// for it, once for the tab. Arriving after the sudo mode was turned off, it is not
    /// shown: the listing as the account is on its way.
    pub(super) fn sudo_listed(
        &mut self,
        tab: TabId,
        path: RemotePath,
        result: Result<(RemotePath, Vec<RemoteEntry>), FilesError>,
    ) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        if !files.sudo_mode {
            return Vec::new();
        }
        match result {
            Err(error) if is_password_error(&error) => {
                // Given up with Escape: nothing is asked.
                if std::mem::take(&mut files.remote.discard_listing) {
                    return Vec::new();
                }
                files.remote.loading = false;
                let name = crate::text::server_text(&path.display());
                self.sudo_refused(tab, name, SudoAction::List(path), error);
                Vec::new()
            }
            result => self.remote_listed(tab, result),
        }
    }

    /// Lists `path` as root again, now the password is kept.
    pub(super) fn list_as_root(&mut self, tab: TabId, path: RemotePath) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab).filter(|files| files.sudo_mode) else {
            return Vec::new();
        };
        let Some(client) = files.client.clone() else {
            return Vec::new();
        };
        files.remote.loading = true;
        files.remote.discard_listing = false;
        vec![remote_listing(files, tab, client, path)]
    }
}
