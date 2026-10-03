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

//! Cut and Paste in a Files tab, as the C# SFTP view's: the entries cut are held for every
//! Files tab, and pasted, on the same server, into the folder a tab shows, each moved by a
//! rename that never replaces what is there.
//!
//! What was moved leaves the clipboard; what could not stays, to be pasted again, as the C#
//! keeps it. Pasting where an entry already is changes nothing and counts as done.

use heimdall_files::RemotePath;

use super::{App, Effect, Notice, TabProfile};
use crate::files::FilesError;
use crate::ids::TabId;

/// Entries cut in a Files tab, waiting to be pasted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesClipboard {
    /// Each entry, by its full path on the server.
    pub entries: Vec<RemotePath>,
    /// The server they are on: protocol, account, host and port.
    endpoint: String,
}

impl App {
    /// The server `tab_id`'s Files tab is on, as the clipboard keys it: never a match when
    /// unknown.
    fn files_endpoint(&self, tab_id: TabId) -> Option<String> {
        let tab = self.tab(tab_id)?;
        tab.files.as_ref()?;
        let (protocol, host, port, user) = match &tab.profile {
            TabProfile::Ssh(profile) => ("sftp", &profile.host, profile.port, &profile.username),
            TabProfile::Ftp(profile) => ("ftp", &profile.host, profile.port, &profile.username),
            _ => return None,
        };
        Some(format!(
            "{protocol}://{}@{}:{port}",
            user.as_deref().unwrap_or_default(),
            host.to_ascii_lowercase()
        ))
    }

    /// "Cut": the remote entries chosen in `tab_id`, held to be pasted.
    pub(super) fn cut_entries(&mut self, tab_id: TabId) -> Vec<Effect> {
        let Some(endpoint) = self.files_endpoint(tab_id) else {
            return Vec::new();
        };
        let Some(files) = self.files_mut(tab_id) else {
            return Vec::new();
        };
        let entries: Vec<RemotePath> = files
            .remote
            .chosen()
            .into_iter()
            .filter_map(|index| files.remote.entries.get(index))
            .map(|entry| files.remote.path.join(&entry.name))
            .collect();
        if entries.is_empty() {
            return Vec::new();
        }
        let count = entries.len();
        self.files_clipboard = Some(FilesClipboard { entries, endpoint });
        self.tell(Notice::FilesCut(count));
        Vec::new()
    }

    /// Whether `tab_id` can paste what is cut: something is, on its own server.
    #[must_use]
    pub fn can_paste(&self, tab_id: TabId) -> bool {
        match (&self.files_clipboard, self.files_endpoint(tab_id)) {
            (Some(clipboard), Some(endpoint)) => {
                !clipboard.entries.is_empty() && clipboard.endpoint == endpoint
            }
            _ => false,
        }
    }

    /// "Paste": the entries cut moved into the folder `tab_id` shows, one after another.
    pub(super) fn paste_cut(&mut self, tab_id: TabId) -> Vec<Effect> {
        if !self.can_paste(tab_id) {
            return Vec::new();
        }
        let Some(entries) = self
            .files_clipboard
            .as_ref()
            .map(|clip| clip.entries.clone())
        else {
            return Vec::new();
        };
        let Some(files) = self.files_mut(tab_id) else {
            return Vec::new();
        };
        let Some(client) = files.client.clone() else {
            return Vec::new();
        };
        let folder = files.remote.path.clone();
        let moves: Vec<(RemotePath, RemotePath)> = entries
            .into_iter()
            .filter_map(|from| {
                let to = folder.join(from.file_name()?);
                // Already there: nothing to move, and done.
                (to != from).then_some((from, to))
            })
            .collect();
        if moves.is_empty() {
            self.files_clipboard = None;
            self.tell(Notice::FilesPasted);
            return Vec::new();
        }
        vec![Effect::MoveRemote {
            tab: tab_id,
            client,
            moves,
        }]
    }

    /// The moves of a paste ended: what moved leaves the clipboard, the first failure is
    /// said, and the folder is listed again.
    pub(super) fn moved_cut(
        &mut self,
        tab_id: TabId,
        results: Vec<(RemotePath, Result<(), FilesError>)>,
    ) -> Vec<Effect> {
        let mut failure = None;
        for (from, result) in results {
            match result {
                Ok(()) => {
                    if let Some(clipboard) = self.files_clipboard.as_mut() {
                        clipboard.entries.retain(|entry| *entry != from);
                    }
                }
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        if self
            .files_clipboard
            .as_ref()
            .is_some_and(|clipboard| clipboard.entries.is_empty())
        {
            self.files_clipboard = None;
        }
        match failure {
            None => self.tell(Notice::FilesPasted),
            Some(error) => {
                if let Some(files) = self.files_mut(tab_id) {
                    files.remote.error = Some(error);
                }
            }
        }
        self.list(tab_id, crate::files::Side::Remote)
    }
}
