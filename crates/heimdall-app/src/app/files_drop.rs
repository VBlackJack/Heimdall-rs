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

//! Entries dragged from the file list of one tab and dropped on the file list of another,
//! both shown side by side, as the C# lists take a drag from one another.
//!
//! The C# decides by what the drag carries (`EmbeddedSftpView.xaml.cs:1971-2008`,
//! `2090-2147`; `LocalFileBrowserView.xaml.cs:613-685`):
//!
//! - Files of this computer, dragged from a local list, carry their paths, as Explorer's
//!   do: a server's list uploads them, into its folder entry dropped on or the folder it
//!   shows, while it is connected. Here they go through the upload a "Paste" of files of
//!   this computer, or a drop from Explorer, goes through: one plan, one question for
//!   whatever is in the way, queued on the server's tab.
//! - A server's entries, dragged from its list, carry that list: a local list downloads
//!   them, into its folder entry dropped on or the folder it shows. The download is the
//!   source list's, as the C# `payload.Source.DownloadFilesAsync`: queued, planned and
//!   answered on the server's tab, as its own "Download"; the local list is listed again
//!   once each entry is down.
//! - A server's entries dropped on another server's list, and files of this computer
//!   dropped on another local list, are refused: nothing is lit and nothing happens, as the
//!   C# answers `DragDropEffects.None`. Copying between servers is "Copy" then "Paste".
//!
//! A drop always copies: nothing leaves the list it was dragged from.

use std::path::{Path, PathBuf};

use heimdall_files::RemotePath;

use super::{App, Effect};
use crate::files::{Direction, EntryKind, FilesPane, Side};
use crate::ids::TabId;

impl App {
    /// Whether the entries chosen in pane `from` of tab `from_tab` can be dropped on pane
    /// `onto` of another tab, `tab`: files of this computer on a connected server's list, or
    /// a connected server's entries on a local list.
    #[must_use]
    pub fn takes_drop_across(
        &self,
        (from_tab, from): (TabId, Side),
        (tab, onto): (TabId, Side),
    ) -> bool {
        if from_tab == tab {
            return false;
        }
        let pane = |id| self.tab(id).and_then(|found| found.files.as_deref());
        let (Some(source), Some(target)) = (pane(from_tab), pane(tab)) else {
            return false;
        };
        match (from, onto) {
            (Side::Local, Side::Remote) => !target.local_only && target.client.is_some(),
            (Side::Remote, Side::Local) => !source.local_only && source.client.is_some(),
            _ => false,
        }
    }

    /// The entries chosen in pane `from` of tab `from_tab` dropped on pane `onto` of `tab`,
    /// into its folder entry at `into`, or the folder it shows: uploaded or downloaded as
    /// the module says; refused, nothing.
    pub(super) fn drop_across(
        &mut self,
        (from_tab, from): (TabId, Side),
        (tab, onto): (TabId, Side),
        into: Option<usize>,
    ) -> Vec<Effect> {
        if !self.takes_drop_across((from_tab, from), (tab, onto)) {
            return Vec::new();
        }
        // Beside the entries, into the folder shown: not while its listing is on its way. A
        // folder entry dropped on is its own destination.
        if !self.drops_on_folder(tab, onto, into) && !self.takes_into_shown(tab, onto) {
            return Vec::new();
        }
        match from {
            Side::Local => {
                let paths = self.chosen_local_paths(from_tab);
                let Some(folder) = self.remote_drop_folder(tab, into) else {
                    return Vec::new();
                };
                self.upload_paths_into(tab, &paths, &folder)
            }
            Side::Remote => {
                let Some(folder) = self.local_drop_folder(tab, into) else {
                    return Vec::new();
                };
                self.download_into(from_tab, folder)
            }
        }
    }

    /// The full paths of the entries chosen in `tab`'s local pane, in the folder they were
    /// listed from.
    fn chosen_local_paths(&self, tab: TabId) -> Vec<PathBuf> {
        let Some(local) = self
            .tab(tab)
            .and_then(|found| found.files.as_deref())
            .map(|files| &files.local)
        else {
            return Vec::new();
        };
        local
            .chosen()
            .into_iter()
            .filter_map(|index| local.entries.get(index))
            .map(|entry| local.entries_folder().join(&entry.name))
            .collect()
    }

    /// Whether a drop on pane `side` of `tab` at `into` is on one of its folder entries,
    /// rather than on the folder it shows.
    fn drops_on_folder(&self, tab: TabId, side: Side, into: Option<usize>) -> bool {
        let Some(files) = self.tab(tab).and_then(|found| found.files.as_deref()) else {
            return false;
        };
        let kind = into.and_then(|index| match side {
            Side::Remote => files.remote.entries.get(index).map(|entry| entry.kind),
            Side::Local => files.local.entries.get(index).map(|entry| entry.kind),
        });
        kind == Some(EntryKind::Directory)
    }

    /// The server's folder a drop on `tab` goes into: its folder entry at `into`, or the
    /// folder it shows.
    fn remote_drop_folder(&self, tab: TabId, into: Option<usize>) -> Option<RemotePath> {
        let remote = &self.tab(tab)?.files.as_deref()?.remote;
        Some(
            match into
                .and_then(|index| remote.entries.get(index))
                .filter(|entry| entry.kind == EntryKind::Directory)
            {
                Some(entry) => remote.entries_folder().join(&entry.name),
                None => remote.path.clone(),
            },
        )
    }

    /// This computer's folder a drop on `tab` goes into: its folder entry at `into`, or the
    /// folder it shows.
    fn local_drop_folder(&self, tab: TabId, into: Option<usize>) -> Option<PathBuf> {
        let local = &self.tab(tab)?.files.as_deref()?.local;
        Some(
            match into
                .and_then(|index| local.entries.get(index))
                .filter(|entry| entry.kind == EntryKind::Directory)
            {
                Some(entry) => local.entries_folder().join(&entry.name),
                None => local.path.clone(),
            },
        )
    }

    /// The entries chosen in `tab`'s server pane downloaded into `folder`, queued on `tab`
    /// as its "Download" queues them.
    fn download_into(&mut self, tab: TabId, folder: PathBuf) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let Some(client) = files.client.clone() else {
            return Vec::new();
        };
        let destination = (folder, RemotePath::default());
        let roots: Vec<_> = files
            .remote
            .chosen()
            .into_iter()
            .filter_map(|index| {
                super::files_tab::prepare(files, Direction::Download, index, &destination)
            })
            .collect();
        self.queue_transfer(tab, client, Direction::Download, roots)
    }

    /// The local panes of the tabs other than `tab` that show `folder`, listed again: an
    /// entry was downloaded there from `tab`.
    pub(super) fn list_local_elsewhere(&mut self, tab: TabId, folder: &Path) -> Vec<Effect> {
        let showing: Vec<TabId> = self
            .tabs
            .iter()
            .filter(|other| other.id != tab)
            .filter(|other| {
                other
                    .files
                    .as_deref()
                    .is_some_and(|files: &FilesPane| files.local.path == folder)
            })
            .map(|other| other.id)
            .collect();
        showing
            .into_iter()
            .flat_map(|other| self.list(other, Side::Local))
            .collect()
    }
}
