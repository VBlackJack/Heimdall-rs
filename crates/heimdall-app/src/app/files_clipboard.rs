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

//! Cut, Copy, Paste and Duplicate in a Files tab, as the C# SFTP view's: the entries cut or
//! copied are held for every Files tab, and pasted into the folder a tab shows: cut ones on
//! the same server; copied ones on the same server, or on another one, as the C# pastes
//! across servers, while the tab they were copied in is still connected.
//!
//! - Cut entries are moved by a rename that never replaces what is there. What was moved
//!   leaves the clipboard; what could not stays, to be pasted again, as the C# keeps it.
//!   Pasting where an entry already is changes nothing and counts as done.
//! - Copied entries are copied on the server itself, under their own name or their first
//!   free copy name, and stay on the clipboard to be pasted again. Duplicate copies the
//!   chosen entries into their own folder, the clipboard left as it is. Only an SFTP tab,
//!   which can run the copy on its SSH connection, copies on its server; one copy runs at a
//!   time. An FTP tab's entries are copied too, as the C# copies whatever the protocol, to
//!   be pasted on another server: FTP has no copy on the server, which the C# refuses too.

use heimdall_core::profile::ProfileId;
use heimdall_files::RemotePath;

use super::{App, Effect, Notice, TabProfile};
use crate::files::{CopySource, EntryKind, FilesError, Side};
use crate::ids::TabId;

/// Entries cut or copied in a Files tab, waiting to be pasted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesClipboard {
    /// Each entry, by its full path on the server, and whether it is a folder.
    pub entries: Vec<CopySource>,
    /// Moved or copied when pasted.
    pub mode: ClipMode,
    /// The server they are on: protocol, account, host, port and gateway.
    endpoint: String,
    /// The tab they were cut or copied in, whose connection reads them for another server.
    source: TabId,
}

/// What a paste does with the entries held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipMode {
    /// Moves them.
    Cut,
    /// Copies them on the server.
    Copy,
}

impl App {
    /// The server `tab_id`'s Files tab is on, as the clipboard keys it: never a match when
    /// unknown.
    pub(super) fn files_endpoint(&self, tab_id: TabId) -> Option<String> {
        let tab = self.tab(tab_id)?;
        tab.files.as_ref()?;
        // The gateway is part of the server: one address behind two gateways can be two
        // machines, and a paste must never rename on the wrong one.
        let (protocol, host, port, user, gateway) = match &tab.profile {
            TabProfile::Ssh(profile) => (
                "sftp",
                &profile.host,
                profile.port,
                &profile.username,
                profile.gateway.as_ref().map(ProfileId::as_str),
            ),
            TabProfile::Ftp(profile) => {
                ("ftp", &profile.host, profile.port, &profile.username, None)
            }
            _ => return None,
        };
        Some(format!(
            "{protocol}://{}@{}:{port} via {}",
            user.as_deref().unwrap_or_default(),
            host.to_ascii_lowercase(),
            gateway.unwrap_or_default()
        ))
    }

    /// The remote entries chosen in `tab_id`, with whether each is a folder; links and
    /// special files only when `any_kind`, as only a move takes them.
    fn chosen_sources(&mut self, tab_id: TabId, any_kind: bool) -> Vec<CopySource> {
        let Some(files) = self.files_mut(tab_id) else {
            return Vec::new();
        };
        files
            .remote
            .chosen()
            .into_iter()
            .filter_map(|index| files.remote.entries.get(index))
            .filter(|entry| {
                any_kind || matches!(entry.kind, EntryKind::File | EntryKind::Directory)
            })
            .map(|entry| CopySource {
                path: files.remote.path.join(&entry.name),
                folder: entry.kind == EntryKind::Directory,
            })
            .collect()
    }

    /// "Cut" or "Copy": the remote entries chosen in `tab_id`, held to be pasted.
    pub(super) fn hold_entries(&mut self, tab_id: TabId, mode: ClipMode) -> Vec<Effect> {
        // The local file browser copies files of this computer.
        if mode == ClipMode::Copy && self.is_local_browser_files(tab_id) {
            return self.copy_local(tab_id);
        }
        if mode == ClipMode::Copy && !self.can_hold_copy(tab_id) {
            return Vec::new();
        }
        let Some(endpoint) = self.files_endpoint(tab_id) else {
            return Vec::new();
        };
        let entries = self.chosen_sources(tab_id, mode == ClipMode::Cut);
        if entries.is_empty() {
            return Vec::new();
        }
        let count = entries.len();
        // Files of this computer copied before give way, as on a clipboard.
        self.local_copied.clear();
        self.files_clipboard = Some(FilesClipboard {
            entries,
            mode,
            endpoint,
            source: tab_id,
        });
        self.tell(match mode {
            ClipMode::Cut => Notice::FilesCut(count),
            ClipMode::Copy => Notice::FilesCopied(count),
        });
        Vec::new()
    }

    /// Whether `tab_id` copies on its server: an SFTP tab with its SSH connection.
    #[must_use]
    pub fn can_copy(&self, tab_id: TabId) -> bool {
        self.tab(tab_id)
            .and_then(|tab| tab.files.as_deref())
            .is_some_and(|files| files.shell.is_some())
    }

    /// Whether "Copy" holds `tab_id`'s entries: a tab copying on its server, or a connected
    /// FTP tab, whose copies are pasted on another server.
    #[must_use]
    pub fn can_hold_copy(&self, tab_id: TabId) -> bool {
        self.can_copy(tab_id)
            || (self.files_over_ftp(tab_id) && self.files_client(tab_id).is_some())
    }

    /// Whether `tab_id` is a Files tab over FTP.
    fn files_over_ftp(&self, tab_id: TabId) -> bool {
        self.tab(tab_id)
            .is_some_and(|tab| tab.files.is_some() && matches!(tab.profile, TabProfile::Ftp(_)))
    }

    /// Whether `tab_id` is a Files tab over SFTP: permissions are changed there, and a link
    /// is never renamed, as the C# menu.
    #[must_use]
    pub fn files_over_sftp(&self, tab_id: TabId) -> bool {
        self.tab(tab_id)
            .is_some_and(|tab| tab.files.is_some() && matches!(tab.profile, TabProfile::Ssh(_)))
    }

    /// Whether `tab_id`'s Files tab is connected: its files can be edited.
    #[must_use]
    pub fn files_connected(&self, tab_id: TabId) -> bool {
        self.files_client(tab_id).is_some()
    }

    /// Whether `tab_id` can paste what is held: something is, on its own server; or, copied
    /// on another server, the tab it was copied in is still connected. Files of this
    /// computer copied in the local file browser are pasted as the module `local_menu`
    /// says.
    #[must_use]
    pub fn can_paste(&self, tab_id: TabId) -> bool {
        if let Some(local) = self.pastes_local_files(tab_id) {
            return local;
        }
        match (&self.files_clipboard, self.files_endpoint(tab_id)) {
            (Some(clipboard), Some(endpoint)) if !clipboard.entries.is_empty() => {
                if clipboard.endpoint == endpoint {
                    clipboard.mode == ClipMode::Cut || self.can_copy(tab_id)
                } else {
                    clipboard.mode == ClipMode::Copy
                        && self.files_client(clipboard.source).is_some()
                        && self.files_client(tab_id).is_some()
                }
            }
            _ => false,
        }
    }

    /// The connection of `tab_id`'s Files tab, while it is connected.
    fn files_client(&self, tab_id: TabId) -> Option<heimdall_files::RemoteSession> {
        self.tab(tab_id)?.files.as_deref()?.client.clone()
    }

    /// "Paste": the entries held moved or copied into the folder `tab_id` shows, one after
    /// another.
    pub(super) fn paste_held(&mut self, tab_id: TabId) -> Vec<Effect> {
        if !self.can_paste(tab_id) {
            return Vec::new();
        }
        if let Some(effects) = self.paste_local_files(tab_id) {
            return effects;
        }
        let Some(clipboard) = self.files_clipboard.clone() else {
            return Vec::new();
        };
        if self.files_endpoint(tab_id).as_ref() != Some(&clipboard.endpoint) {
            return self.copy_across(tab_id, clipboard.source, clipboard.entries);
        }
        match clipboard.mode {
            ClipMode::Cut => self.move_held(tab_id, clipboard.entries),
            ClipMode::Copy => self.copy_into_shown(tab_id, clipboard.entries, false),
        }
    }

    fn move_held(&mut self, tab_id: TabId, entries: Vec<CopySource>) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab_id) else {
            return Vec::new();
        };
        let Some(client) = files.client.clone() else {
            return Vec::new();
        };
        let folder = files.remote.path.clone();
        let moves: Vec<(RemotePath, RemotePath)> = entries
            .into_iter()
            .filter_map(|entry| {
                let to = folder.join(entry.path.file_name()?);
                // Already there: nothing to move, and done.
                (to != entry.path).then_some((entry.path, to))
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

    /// The entries copied in `source`, on another server, copied into the folder `tab_id`
    /// shows, through this computer, unless a copy runs there already.
    fn copy_across(
        &mut self,
        tab_id: TabId,
        source: TabId,
        sources: Vec<CopySource>,
    ) -> Vec<Effect> {
        let Some(from) = self.files_client(source) else {
            return Vec::new();
        };
        let staging = self.edit_dir.clone().unwrap_or_else(std::env::temp_dir);
        let Some(files) = self.files_mut(tab_id) else {
            return Vec::new();
        };
        let Some(to) = files.client.clone() else {
            return Vec::new();
        };
        if files.copying.is_some() {
            return Vec::new();
        }
        let cancel = tokio_util::sync::CancellationToken::new();
        files.copying = Some(cancel.clone());
        vec![Effect::CopyAcross {
            tab: tab_id,
            from,
            to,
            sources,
            folder: files.remote.path.clone(),
            staging,
            cancel,
        }]
    }

    /// "Duplicate": the entries chosen in `tab_id` copied into their own folder.
    pub(super) fn duplicate(&mut self, tab_id: TabId) -> Vec<Effect> {
        if !self.can_copy(tab_id) {
            return Vec::new();
        }
        let sources = self.chosen_sources(tab_id, false);
        self.copy_into_shown(tab_id, sources, true)
    }

    /// Copies `sources` into the folder `tab_id` shows, unless a copy runs there already.
    fn copy_into_shown(
        &mut self,
        tab_id: TabId,
        sources: Vec<CopySource>,
        duplicate: bool,
    ) -> Vec<Effect> {
        if sources.is_empty() {
            return Vec::new();
        }
        let Some(files) = self.files_mut(tab_id) else {
            return Vec::new();
        };
        let (Some(client), Some(shell)) = (files.client.clone(), files.shell.clone()) else {
            return Vec::new();
        };
        if files.copying.is_some() {
            return Vec::new();
        }
        let cancel = tokio_util::sync::CancellationToken::new();
        files.copying = Some(cancel.clone());
        vec![Effect::CopyRemote {
            tab: tab_id,
            client,
            shell,
            sources,
            folder: files.remote.path.clone(),
            cancel,
            duplicate,
        }]
    }

    /// The moves of a paste ended: what moved leaves the clipboard, the first failure is
    /// said, and the folder is listed again.
    pub(super) fn moved_held(
        &mut self,
        tab_id: TabId,
        results: Vec<(RemotePath, Result<(), FilesError>)>,
    ) -> Vec<Effect> {
        let mut failure = None;
        for (from, result) in results {
            match result {
                Ok(()) => {
                    if let Some(clipboard) = self
                        .files_clipboard
                        .as_mut()
                        .filter(|clipboard| clipboard.mode == ClipMode::Cut)
                    {
                        clipboard.entries.retain(|entry| entry.path != from);
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
        self.ended_with(tab_id, failure, Notice::FilesPasted)
    }

    /// The copies of a paste or a duplicate ended: the first failure is said, and the
    /// folder is listed again. What was copied stays on the clipboard.
    pub(super) fn copied(
        &mut self,
        tab_id: TabId,
        results: Vec<(RemotePath, Result<RemotePath, FilesError>)>,
        duplicate: bool,
    ) -> Vec<Effect> {
        if let Some(files) = self.files_mut(tab_id) {
            files.copying = None;
        }
        let failure = results.into_iter().find_map(|(_, result)| result.err());
        let done = if duplicate {
            Notice::FilesDuplicated
        } else {
            Notice::FilesPasted
        };
        self.ended_with(tab_id, failure, done)
    }

    fn ended_with(
        &mut self,
        tab_id: TabId,
        failure: Option<FilesError>,
        done: Notice,
    ) -> Vec<Effect> {
        match failure {
            None => self.tell(done),
            Some(error) => {
                if let Some(files) = self.files_mut(tab_id) {
                    files.remote.error = Some(error);
                }
            }
        }
        self.list(tab_id, Side::Remote)
    }
}
