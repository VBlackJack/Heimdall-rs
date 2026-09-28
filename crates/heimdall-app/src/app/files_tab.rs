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

//! What the application decides in a Files tab.

use std::path::PathBuf;

use heimdall_files::{RemotePath, RemoteSession};
use tokio_util::sync::CancellationToken;

use super::{App, Dialog, Effect, NameAction};
use crate::files::{
    Direction, EntryKind, FileOperation, FileProperties, FilesError, FilesKey, FilesPane, Side,
    SortColumn, Transfer, TransferEvent, TransferId, TransferRequest, TransferState, download_name,
    octal_mode, typed_name,
};
use crate::ids::TabId;

/// Something that happened in a Files tab.
#[derive(Clone)]
pub enum FilesMessage {
    /// A remote listing arrived.
    RemoteListed {
        /// Tab.
        tab: TabId,
        /// The folder, absolute, and its entries; or why not.
        result: Result<(RemotePath, Vec<crate::files::RemoteEntry>), FilesError>,
    },
    /// A local listing arrived.
    LocalListed {
        /// Tab.
        tab: TabId,
        /// The folder and its entries; or why not.
        result: Result<(PathBuf, Vec<crate::files::LocalEntry>), FilesError>,
    },
    /// An entry was selected.
    Select {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// Entry.
        index: usize,
    },
    /// An entry was opened: a folder is entered, a file is sent to the other pane.
    Open {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// Entry.
        index: usize,
    },
    /// Go to the parent folder.
    Up {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
    },
    /// The folder typed in a pane's path bar changed.
    PathEdited {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// What is typed.
        text: String,
    },
    /// Select an entry with the others, or no longer: Ctrl+click.
    Toggle {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// Entry.
        index: usize,
    },
    /// Select every entry from the one selected to this one: Shift+click.
    Range {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// Entry.
        index: usize,
    },
    /// Sort a pane by a column, or the other way when sorted by it already.
    SortBy {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// Column.
        column: SortColumn,
    },
    /// Bookmark the server's folder shown.
    Bookmark {
        /// Tab.
        tab: TabId,
    },
    /// Go to one of the server's folders bookmarked.
    OpenBookmark {
        /// Tab.
        tab: TabId,
        /// Which, in the order they were bookmarked.
        index: usize,
    },
    /// Copy the full path of a pane's selected entry, as the C# "Copy path".
    CopyPath {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
    },
    /// Go to the folder typed in a pane's path bar.
    GoTo {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
    },
    /// List the folder again.
    Refresh {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
    },
    /// Send the selected file to the other pane.
    Transfer {
        /// Tab.
        tab: TabId,
        /// Direction.
        direction: Direction,
    },
    /// A running transfer reported.
    TransferEvent {
        /// Tab.
        tab: TabId,
        /// Transfer.
        id: TransferId,
        /// Report.
        event: TransferEvent,
    },
    /// Stop a transfer.
    Cancel {
        /// Tab.
        tab: TabId,
        /// Transfer.
        id: TransferId,
    },
    /// Ask for the name of a new folder in a pane.
    AskNewFolder {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
    },
    /// Ask for a new name for the selected entry.
    AskRename {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
    },
    /// Ask for new permission bits for the selected entry of the server.
    AskPermissions {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
    },
    /// Show what the selected entry of the server is.
    ShowProperties {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
    },
    /// Ask to confirm deleting the selected entry.
    AskDelete {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
    },
    /// The name typed in the question changed.
    NameEdited(String),
    /// A key pressed while the tab is shown.
    Key {
        /// Tab.
        tab: TabId,
        /// What it does.
        key: FilesKey,
    },
    /// A file operation finished.
    OperationDone {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// How it went.
        result: Result<(), FilesError>,
    },
}

impl FilesMessage {
    /// The pane a user gesture acts on: the one that takes the focus.
    fn gesture(&self) -> Option<(TabId, Side)> {
        match *self {
            Self::Select { tab, side, .. }
            | Self::Open { tab, side, .. }
            | Self::Up { tab, side }
            | Self::Refresh { tab, side }
            | Self::GoTo { tab, side }
            | Self::SortBy { tab, side, .. }
            | Self::Toggle { tab, side, .. }
            | Self::Range { tab, side, .. }
            | Self::CopyPath { tab, side }
            | Self::AskNewFolder { tab, side }
            | Self::AskRename { tab, side }
            | Self::AskDelete { tab, side }
            | Self::AskPermissions { tab, side }
            | Self::ShowProperties { tab, side } => Some((tab, side)),
            _ => None,
        }
    }
}

impl std::fmt::Debug for FilesMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RemoteListed { tab, result } => write!(
                f,
                "RemoteListed({}, {})",
                tab.value(),
                result.as_ref().map_or(0, |(_, entries)| entries.len())
            ),
            Self::LocalListed { tab, result } => write!(
                f,
                "LocalListed({}, {})",
                tab.value(),
                result.as_ref().map_or(0, |(_, entries)| entries.len())
            ),
            Self::Select { tab, side, index } => {
                write!(f, "Select({}, {side:?}, {index})", tab.value())
            }
            Self::Open { tab, side, index } => {
                write!(f, "Open({}, {side:?}, {index})", tab.value())
            }
            Self::Up { tab, side } => write!(f, "Up({}, {side:?})", tab.value()),
            Self::Refresh { tab, side } => write!(f, "Refresh({}, {side:?})", tab.value()),
            Self::PathEdited { tab, side, .. } => {
                write!(f, "PathEdited({}, {side:?}, ..)", tab.value())
            }
            Self::GoTo { tab, side } => write!(f, "GoTo({}, {side:?})", tab.value()),
            Self::CopyPath { tab, side } => write!(f, "CopyPath({}, {side:?})", tab.value()),
            Self::Bookmark { tab } => write!(f, "Bookmark({})", tab.value()),
            Self::OpenBookmark { tab, index } => {
                write!(f, "OpenBookmark({}, {index})", tab.value())
            }
            Self::Toggle { tab, side, index } => {
                write!(f, "Toggle({}, {side:?}, {index})", tab.value())
            }
            Self::Range { tab, side, index } => {
                write!(f, "Range({}, {side:?}, {index})", tab.value())
            }
            Self::SortBy { tab, side, column } => {
                write!(f, "SortBy({}, {side:?}, {column:?})", tab.value())
            }
            Self::Transfer { tab, direction } => {
                write!(f, "Transfer({}, {direction:?})", tab.value())
            }
            Self::TransferEvent { tab, id, event } => {
                write!(
                    f,
                    "TransferEvent({}, {}, {event:?})",
                    tab.value(),
                    id.value()
                )
            }
            Self::Cancel { tab, id } => write!(f, "Cancel({}, {})", tab.value(), id.value()),
            Self::Key { tab, key } => write!(f, "Key({}, {key:?})", tab.value()),
            Self::AskNewFolder { tab, side } => {
                write!(f, "AskNewFolder({}, {side:?})", tab.value())
            }
            Self::AskRename { tab, side } => write!(f, "AskRename({}, {side:?})", tab.value()),
            Self::AskDelete { tab, side } => write!(f, "AskDelete({}, {side:?})", tab.value()),
            Self::AskPermissions { tab, side } => {
                write!(f, "AskPermissions({}, {side:?})", tab.value())
            }
            Self::ShowProperties { tab, side } => {
                write!(f, "ShowProperties({}, {side:?})", tab.value())
            }
            Self::NameEdited(_) => f.write_str("NameEdited(..)"),
            Self::OperationDone { tab, side, result } => write!(
                f,
                "OperationDone({}, {side:?}, {})",
                tab.value(),
                result.is_ok()
            ),
        }
    }
}

/// An operation waiting for the user's name or confirmation.
#[derive(Debug, Clone)]
pub(super) struct PendingOperation {
    tab: TabId,
    side: Side,
    kind: PendingKind,
}

#[derive(Debug, Clone)]
enum PendingKind {
    /// A folder to create in the pane's folder.
    NewFolder,
    /// The entry at this path to rename in the same folder.
    Rename { remote: RemotePath, local: PathBuf },
    /// The entries at these paths to delete.
    Delete { targets: Vec<(RemotePath, PathBuf)> },
    /// The entries of the server at these paths to give new permission bits.
    Permissions { remotes: Vec<RemotePath> },
}

/// A transfer waiting for the user to confirm it replaces an existing file.
#[derive(Debug, Clone)]
pub(super) struct PendingTransfer {
    tab: TabId,
    request: TransferRequest,
    label: String,
    total: Option<u64>,
}

#[cfg(unix)]
fn name_bytes(name: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt as _;
    name.as_bytes().to_vec()
}

#[cfg(not(unix))]
fn name_bytes(name: &std::ffi::OsStr) -> Vec<u8> {
    name.to_string_lossy().into_owned().into_bytes()
}

impl App {
    fn files_mut(&mut self, tab: TabId) -> Option<&mut FilesPane> {
        self.tab_mut(tab)?.files.as_deref_mut()
    }

    /// Effects that list both panes, once the session is open.
    pub(super) fn files_ready(&mut self, tab: TabId) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let mut effects = Vec::new();
        if let Some(client) = files.client.clone() {
            effects.push(Effect::ListRemote {
                tab,
                client,
                path: files.remote.path.clone(),
            });
        }
        effects.push(Effect::ListLocal {
            tab,
            path: files.local.path.clone(),
        });
        effects
    }

    fn list(&mut self, tab: TabId, side: Side) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        match side {
            Side::Remote => {
                let Some(client) = files.client.clone() else {
                    return Vec::new();
                };
                files.remote.loading = true;
                vec![Effect::ListRemote {
                    tab,
                    client,
                    path: files.remote.path.clone(),
                }]
            }
            Side::Local => {
                files.local.loading = true;
                vec![Effect::ListLocal {
                    tab,
                    path: files.local.path.clone(),
                }]
            }
        }
    }

    pub(super) fn files(&mut self, message: FilesMessage) -> Vec<Effect> {
        if let Some((tab, side)) = message.gesture()
            && let Some(files) = self.files_mut(tab)
        {
            files.focus = side;
        }
        match message {
            FilesMessage::RemoteListed { tab, result } => {
                if let Some(files) = self.files_mut(tab) {
                    let pane = &mut files.remote;
                    pane.loading = false;
                    match result {
                        Ok((path, entries)) => {
                            pane.path = path;
                            pane.show(entries);
                            pane.error = None;
                        }
                        Err(error) => pane.error = Some(error),
                    }
                }
                Vec::new()
            }
            FilesMessage::LocalListed { tab, result } => {
                if let Some(files) = self.files_mut(tab) {
                    let pane = &mut files.local;
                    pane.loading = false;
                    match result {
                        Ok((path, entries)) => {
                            pane.path = path;
                            pane.show(entries);
                            pane.error = None;
                        }
                        Err(error) => pane.error = Some(error),
                    }
                }
                Vec::new()
            }
            FilesMessage::Select { tab, side, index } => self.select(tab, side, index),
            FilesMessage::AskNewFolder { tab, side } => self.ask(tab, side, NameAction::NewFolder),
            FilesMessage::AskRename { tab, side } => self.ask(tab, side, NameAction::Rename),
            FilesMessage::AskDelete { tab, side } => self.ask_delete(tab, side),
            FilesMessage::NameEdited(value) => {
                if let Some(Dialog::AskName { value: typed, .. }) = self.dialog.as_mut() {
                    *typed = value;
                }
                Vec::new()
            }
            FilesMessage::OperationDone { tab, side, result } => {
                if let (Some(files), Err(error)) = (self.files_mut(tab), &result) {
                    match side {
                        Side::Remote => files.remote.error = Some(error.clone()),
                        Side::Local => files.local.error = Some(error.clone()),
                    }
                }
                self.list(tab, side)
            }
            FilesMessage::Open { tab, side, index } => self.open_entry(tab, side, index),
            FilesMessage::Up { tab, side } => {
                let Some(files) = self.files_mut(tab) else {
                    return Vec::new();
                };
                match side {
                    Side::Remote => files.remote.path = files.remote.path.parent(),
                    Side::Local => {
                        if let Some(parent) = files.local.path.parent() {
                            files.local.path = parent.to_owned();
                        }
                    }
                }
                self.list(tab, side)
            }
            FilesMessage::Refresh { tab, side } => self.list(tab, side),
            message @ (FilesMessage::PathEdited { .. }
            | FilesMessage::GoTo { .. }
            | FilesMessage::SortBy { .. }
            | FilesMessage::CopyPath { .. }
            | FilesMessage::AskPermissions { .. }
            | FilesMessage::ShowProperties { .. }
            | FilesMessage::Toggle { .. }
            | FilesMessage::Range { .. }
            | FilesMessage::Bookmark { .. }
            | FilesMessage::OpenBookmark { .. }) => self.pane_message(message),
            FilesMessage::Key { tab, key } => self.files_key(tab, key),
            FilesMessage::Transfer { tab, direction } => self.start_transfer(tab, direction),
            FilesMessage::TransferEvent { tab, id, event } => self.transfer_event(tab, id, event),
            FilesMessage::Cancel { tab, id } => {
                if let Some(files) = self.files_mut(tab)
                    && let Some(transfer) = files.transfers.iter().find(|t| t.id == id)
                {
                    transfer.cancel.cancel();
                }
                Vec::new()
            }
        }
    }

    /// A change to how a pane shows its folder: the path typed, gone to, the sort.
    fn pane_message(&mut self, message: FilesMessage) -> Vec<Effect> {
        match message {
            FilesMessage::PathEdited { tab, side, text } => {
                if let Some(files) = self.files_mut(tab) {
                    match side {
                        Side::Remote => files.remote.typed = Some(text),
                        Side::Local => files.local.typed = Some(text),
                    }
                }
                Vec::new()
            }
            FilesMessage::GoTo { tab, side } => self.go_to(tab, side),
            FilesMessage::CopyPath { tab, side } => self.copy_path(tab, side),
            FilesMessage::Bookmark { tab } => {
                self.bookmark(tab);
                Vec::new()
            }
            FilesMessage::OpenBookmark { tab, index } => self.open_bookmark(tab, index),
            FilesMessage::Toggle { tab, side, index }
            | FilesMessage::Range { tab, side, index } => {
                let toggle = matches!(message, FilesMessage::Toggle { .. });
                if let Some(files) = self.files_mut(tab) {
                    match (side, toggle) {
                        (Side::Remote, true) => files.remote.toggle(index),
                        (Side::Local, true) => files.local.toggle(index),
                        (Side::Remote, false) => files.remote.extend_to(index),
                        (Side::Local, false) => files.local.extend_to(index),
                    }
                }
                Vec::new()
            }
            FilesMessage::AskPermissions { tab, side } => {
                self.ask(tab, side, NameAction::Permissions)
            }
            FilesMessage::ShowProperties { tab, side } => {
                self.show_properties(tab, side);
                Vec::new()
            }
            FilesMessage::SortBy { tab, side, column } => {
                if let Some(files) = self.files_mut(tab) {
                    match side {
                        Side::Remote => files.remote.sort_by(files.remote.sort.clicked(column)),
                        Side::Local => files.local.sort_by(files.local.sort.clicked(column)),
                    }
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Bookmarks the server's folder shown, once, and says so.
    fn bookmark(&mut self, tab: TabId) {
        let Some(files) = self.files_mut(tab) else {
            return;
        };
        let path = files.remote.path.clone();
        if files.bookmarks.contains(&path) {
            return;
        }
        let shown = crate::text::server_text(&path.display());
        files.bookmarks.push(path);
        self.tell(super::Notice::Bookmarked(shown));
    }

    /// Lists the server's folder bookmarked at `index`; the folder shown stays until the
    /// listing comes back.
    fn open_bookmark(&mut self, tab: TabId, index: usize) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let (Some(client), Some(path)) =
            (files.client.clone(), files.bookmarks.get(index).cloned())
        else {
            return Vec::new();
        };
        files.focus = Side::Remote;
        files.remote.loading = true;
        vec![Effect::ListRemote { tab, client, path }]
    }

    /// Copies the full path of `side`'s selected entry, and says so.
    fn copy_path(&mut self, tab: TabId, side: Side) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let path = match side {
            Side::Remote => files
                .remote
                .selected
                .and_then(|index| files.remote.entries.get(index))
                .map(|entry| files.remote.path.join(&entry.name).display()),
            Side::Local => files
                .local
                .selected
                .and_then(|index| files.local.entries.get(index))
                .map(|entry| files.local.path.join(&entry.name).display().to_string()),
        };
        let Some(path) = path else {
            return Vec::new();
        };
        self.tell(super::Notice::Copied(path.clone()));
        vec![Effect::WriteClipboard(path)]
    }

    /// Lists the folder typed in `side`'s path bar, from the folder shown when relative; the
    /// folder shown stays until the listing comes back.
    fn go_to(&mut self, tab: TabId, side: Side) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        match side {
            Side::Remote => {
                let Some(typed) = files.remote.typed.take() else {
                    return Vec::new();
                };
                let typed = typed.trim();
                let Some(client) = files.client.clone().filter(|_| !typed.is_empty()) else {
                    return Vec::new();
                };
                let typed = RemotePath::from(typed);
                let path = if typed.is_absolute() {
                    typed
                } else {
                    files.remote.path.join(typed.as_bytes())
                };
                files.remote.loading = true;
                vec![Effect::ListRemote { tab, client, path }]
            }
            Side::Local => {
                let Some(typed) = files.local.typed.take() else {
                    return Vec::new();
                };
                let typed = typed.trim();
                if typed.is_empty() {
                    return Vec::new();
                }
                // Joining an absolute path gives that path.
                let path = files.local.path.join(typed);
                files.local.loading = true;
                vec![Effect::ListLocal { tab, path }]
            }
        }
    }

    fn select(&mut self, tab: TabId, side: Side, index: usize) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        // A second click on a selected folder opens it.
        let (selected, is_folder) = match side {
            Side::Remote => (
                files.remote.selected,
                files
                    .remote
                    .entries
                    .get(index)
                    .map(|entry| entry.kind == EntryKind::Directory),
            ),
            Side::Local => (
                files.local.selected,
                files
                    .local
                    .entries
                    .get(index)
                    .map(|entry| entry.kind == EntryKind::Directory),
            ),
        };
        match is_folder {
            Some(true) if selected == Some(index) => self.open_entry(tab, side, index),
            Some(_) => {
                match side {
                    Side::Remote => files.remote.select_only(Some(index)),
                    Side::Local => files.local.select_only(Some(index)),
                }
                Vec::new()
            }
            None => Vec::new(),
        }
    }

    fn files_key(&mut self, tab: TabId, key: FilesKey) -> Vec<Effect> {
        // A question on screen takes the keys; its own field answers them.
        if self.dialog.is_some() {
            return Vec::new();
        }
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let side = files.focus;
        let (selected, count) = files.focused();
        let last = count.checked_sub(1);
        let target = match key {
            FilesKey::Previous => selected.map_or(last, |index| Some(index.saturating_sub(1))),
            FilesKey::Next => match selected {
                Some(index) => last.map(|last| (index + 1).min(last)),
                None => last.map(|_| 0),
            },
            FilesKey::First => last.map(|_| 0),
            FilesKey::Last => last,
            FilesKey::SwitchPane => {
                files.focus = side.other();
                return Vec::new();
            }
            FilesKey::Focus(chosen) => {
                files.focus = chosen;
                return Vec::new();
            }
            FilesKey::Open => {
                return selected.map_or_else(Vec::new, |index| self.open_entry(tab, side, index));
            }
            FilesKey::Parent => return self.files(FilesMessage::Up { tab, side }),
            FilesKey::Rename => return self.ask(tab, side, NameAction::Rename),
            FilesKey::Delete => return self.ask_delete(tab, side),
            FilesKey::Refresh => return self.list(tab, side),
        };
        match side {
            Side::Remote => files.remote.select_only(target),
            Side::Local => files.local.select_only(target),
        }
        Vec::new()
    }

    fn open_entry(&mut self, tab: TabId, side: Side, index: usize) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        match side {
            Side::Remote => {
                let Some(entry) = files.remote.entries.get(index) else {
                    return Vec::new();
                };
                if entry.kind == EntryKind::Directory {
                    files.remote.path = files.remote.path.join(&entry.name);
                    return self.list(tab, side);
                }
                files.remote.select_only(Some(index));
                self.start_transfer(tab, Direction::Download)
            }
            Side::Local => {
                let Some(entry) = files.local.entries.get(index) else {
                    return Vec::new();
                };
                if entry.kind == EntryKind::Directory {
                    files.local.path = files.local.path.join(&entry.name);
                    return self.list(tab, side);
                }
                files.local.select_only(Some(index));
                self.start_transfer(tab, Direction::Upload)
            }
        }
    }

    fn start_transfer(&mut self, tab: TabId, direction: Direction) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let Some(client) = files.client.clone() else {
            return Vec::new();
        };
        let chosen = match direction {
            Direction::Download => files.remote.chosen(),
            Direction::Upload => files.local.chosen(),
        };
        let prepared: Vec<_> = chosen
            .into_iter()
            .filter_map(|index| prepare(files, client.clone(), direction, index))
            .collect();
        let mut effects = Vec::new();
        for (request, label, total, exists) in prepared {
            if exists {
                // Asked one after the other, once nothing else is asked.
                self.pending_transfers.push_back(PendingTransfer {
                    tab,
                    request,
                    label,
                    total,
                });
            } else {
                effects.extend(self.launch(tab, request, label, total));
            }
        }
        self.ask_next_overwrite();
        effects
    }

    /// Asks about the next transfer waiting to replace a file.
    fn ask_next_overwrite(&mut self) {
        if let Some(next) = self.pending_transfers.front() {
            self.dialog = Some(Dialog::ConfirmOverwrite {
                tab: next.tab,
                direction: next.request.direction,
                name: next.label.clone(),
            });
        }
    }

    fn launch(
        &mut self,
        tab: TabId,
        request: TransferRequest,
        label: String,
        total: Option<u64>,
    ) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let id = TransferId::fresh();
        files.transfers.push(Transfer {
            id,
            direction: request.direction,
            label,
            bytes: 0,
            total,
            state: TransferState::Running,
            cancel: request.cancel.clone(),
        });
        vec![Effect::Transfer {
            tab,
            id,
            request: Box::new(request),
        }]
    }

    /// The user confirmed replacing an existing file.
    pub(super) fn confirm_overwrite(&mut self) -> Vec<Effect> {
        let Some(pending) = self.pending_transfers.pop_front() else {
            return Vec::new();
        };
        let mut request = pending.request;
        request.replace = true;
        let effects = self.launch(pending.tab, request, pending.label, pending.total);
        self.ask_next_overwrite();
        effects
    }

    fn transfer_event(&mut self, tab: TabId, id: TransferId, event: TransferEvent) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let Some(transfer) = files.transfers.iter_mut().find(|t| t.id == id) else {
            return Vec::new();
        };
        match event {
            TransferEvent::Progress(bytes) => {
                transfer.bytes = bytes;
                Vec::new()
            }
            TransferEvent::Finished(state) => {
                let done = state == TransferState::Done;
                let direction = transfer.direction;
                transfer.state = state;
                if !done {
                    return Vec::new();
                }
                // The destination pane now holds the file.
                self.list(
                    tab,
                    match direction {
                        Direction::Download => Side::Local,
                        Direction::Upload => Side::Remote,
                    },
                )
            }
        }
    }
}

impl App {
    /// The selected entry of a pane: its label, both path forms, and whether it is a folder.
    fn selected(&mut self, tab: TabId, side: Side) -> Option<(String, RemotePath, PathBuf, bool)> {
        let files = self.files_mut(tab)?;
        match side {
            Side::Remote => {
                let entry = files.remote.entries.get(files.remote.selected?)?;
                Some((
                    entry.label.clone(),
                    files.remote.path.join(&entry.name),
                    PathBuf::new(),
                    entry.kind == EntryKind::Directory,
                ))
            }
            Side::Local => {
                let entry = files.local.entries.get(files.local.selected?)?;
                Some((
                    entry.label.clone(),
                    RemotePath::default(),
                    files.local.path.join(&entry.name),
                    entry.kind == EntryKind::Directory,
                ))
            }
        }
    }

    /// Shows what the selected entry of the server is.
    fn show_properties(&mut self, tab: TabId, side: Side) {
        let Some(files) = self.files_mut(tab).filter(|_| side == Side::Remote) else {
            return;
        };
        let pane = &files.remote;
        let Some(entry) = pane.selected.and_then(|index| pane.entries.get(index)) else {
            return;
        };
        let properties = FileProperties {
            name: entry.label.clone(),
            kind: entry.kind,
            size: entry.size,
            modified: entry.modified,
            permissions: entry.permissions,
            owner: entry.owner,
            group: entry.group,
            path: crate::text::server_text(&pane.path.join(&entry.name).display()),
        };
        self.dialog = Some(Dialog::FileProperties(Box::new(properties)));
    }

    /// The entries selected in `side`, each as `selected` gives the one selected.
    fn chosen(&mut self, tab: TabId, side: Side) -> Vec<(String, RemotePath, PathBuf, bool)> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        match side {
            Side::Remote => files
                .remote
                .chosen()
                .into_iter()
                .filter_map(|index| files.remote.entries.get(index))
                .map(|entry| {
                    (
                        entry.label.clone(),
                        files.remote.path.join(&entry.name),
                        PathBuf::new(),
                        entry.kind == EntryKind::Directory,
                    )
                })
                .collect(),
            Side::Local => files
                .local
                .chosen()
                .into_iter()
                .filter_map(|index| files.local.entries.get(index))
                .map(|entry| {
                    (
                        entry.label.clone(),
                        RemotePath::default(),
                        files.local.path.join(&entry.name),
                        entry.kind == EntryKind::Directory,
                    )
                })
                .collect(),
        }
    }

    fn ask(&mut self, tab: TabId, side: Side, action: NameAction) -> Vec<Effect> {
        let (kind, value) = match action {
            NameAction::NewFolder => (PendingKind::NewFolder, String::new()),
            // The server's permissions only, as in the C# Files tab; the ones it has, to
            // change from.
            NameAction::Permissions => {
                let Some(files) = self.files_mut(tab).filter(|_| side == Side::Remote) else {
                    return Vec::new();
                };
                let pane = &files.remote;
                let Some(entry) = pane.selected.and_then(|index| pane.entries.get(index)) else {
                    return Vec::new();
                };
                let value = entry
                    .permissions
                    .map(|mode| format!("{mode:o}"))
                    .unwrap_or_default();
                let remotes = pane
                    .chosen()
                    .into_iter()
                    .filter_map(|index| pane.entries.get(index))
                    .map(|entry| pane.path.join(&entry.name))
                    .collect();
                (PendingKind::Permissions { remotes }, value)
            }
            NameAction::Rename => {
                // One entry at a time, as in the C# tab.
                if self.chosen(tab, side).len() != 1 {
                    return Vec::new();
                }
                let Some((label, remote, local, _)) = self.selected(tab, side) else {
                    return Vec::new();
                };
                (PendingKind::Rename { remote, local }, label)
            }
        };
        self.pending_operation = Some(PendingOperation { tab, side, kind });
        self.dialog = Some(Dialog::AskName {
            tab,
            side,
            action,
            value,
        });
        Vec::new()
    }

    fn ask_delete(&mut self, tab: TabId, side: Side) -> Vec<Effect> {
        let chosen = self.chosen(tab, side);
        let Some((name, ..)) = chosen.first().cloned() else {
            return Vec::new();
        };
        let folder = chosen.iter().any(|(.., folder)| *folder);
        let count = chosen.len();
        let targets = chosen
            .into_iter()
            .map(|(_, remote, local, _)| (remote, local))
            .collect();
        self.pending_operation = Some(PendingOperation {
            tab,
            side,
            kind: PendingKind::Delete { targets },
        });
        self.dialog = Some(Dialog::ConfirmDelete {
            tab,
            side,
            name,
            folder,
            count,
        });
        Vec::new()
    }

    /// The user confirmed a name or a delete.
    pub(super) fn confirm_operation(&mut self, typed: Option<&str>) -> Vec<Effect> {
        let Some(pending) = self.pending_operation.take() else {
            return Vec::new();
        };
        let (tab, side) = (pending.tab, pending.side);
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let client = files.client.clone();
        let operation = |operation| Effect::FileOperation {
            tab,
            side,
            operation: Box::new(operation),
        };
        if let PendingKind::Permissions { remotes } = pending.kind {
            let mode = match octal_mode(typed.unwrap_or_default()) {
                Ok(mode) => mode,
                Err(error) => {
                    files.remote.error = Some(error);
                    return Vec::new();
                }
            };
            let Some(client) = client else {
                return Vec::new();
            };
            return remotes
                .into_iter()
                .map(|path| {
                    operation(FileOperation::RemoteSetPermissions {
                        client: client.clone(),
                        path,
                        mode,
                    })
                })
                .collect();
        }
        if let PendingKind::Delete { targets } = pending.kind {
            return targets
                .into_iter()
                .filter_map(|(remote, local)| match side {
                    Side::Remote => Some(FileOperation::RemoteRemove {
                        client: client.clone()?,
                        path: remote,
                    }),
                    Side::Local => Some(FileOperation::LocalRemove { path: local }),
                })
                .map(operation)
                .collect();
        }
        let name = match typed {
            Some(typed) => match typed_name(side, typed) {
                Ok(name) => Some(name),
                Err(error) => {
                    match side {
                        Side::Remote => files.remote.error = Some(error),
                        Side::Local => files.local.error = Some(error),
                    }
                    return Vec::new();
                }
            },
            None => None,
        };
        let remote_folder = files.remote.path.clone();
        let local_folder = files.local.path.clone();
        let operation = (|| {
            Some(match (side, pending.kind, name) {
                (Side::Remote, PendingKind::NewFolder, Some(name)) => {
                    FileOperation::RemoteMakeFolder {
                        client: client?,
                        path: remote_folder.join(name.name.to_string_lossy().as_bytes()),
                    }
                }
                (Side::Remote, PendingKind::Rename { remote, .. }, Some(name)) => {
                    FileOperation::RemoteRename {
                        client: client?,
                        to: remote.parent().join(name.name.to_string_lossy().as_bytes()),
                        from: remote,
                    }
                }
                (Side::Local, PendingKind::NewFolder, Some(name)) => {
                    FileOperation::LocalMakeFolder {
                        path: local_folder.join(&name.name),
                    }
                }
                (Side::Local, PendingKind::Rename { local, .. }, Some(name)) => {
                    FileOperation::LocalRename {
                        to: local.with_file_name(&name.name),
                        from: local,
                    }
                }
                _ => return None,
            })
        })();
        operation
            .into_iter()
            .map(|done| Effect::FileOperation {
                tab,
                side,
                operation: Box::new(done),
            })
            .collect()
    }
}

/// A transfer request for the selected entry: the request, its label, its size when known,
/// and whether the target exists. A refusal is recorded as a failed transfer.
fn prepare(
    files: &mut FilesPane,
    client: RemoteSession,
    direction: Direction,
    index: usize,
) -> Option<(TransferRequest, String, Option<u64>, bool)> {
    Some(match direction {
        Direction::Download => {
            let entry = files.remote.entries.get(index)?;
            let label = entry.label.clone();
            if !matches!(
                entry.kind,
                EntryKind::File | EntryKind::Link | EntryKind::Directory
            ) {
                files
                    .transfers
                    .push(failed(direction, label, FilesError::NotAFile));
                return None;
            }
            let name = match download_name(&entry.name) {
                Ok(name) => name,
                Err(error) => {
                    files.transfers.push(failed(direction, label, error));
                    return None;
                }
            };
            let local = files.local.path.join(&name.name);
            let exists = local.symlink_metadata().is_ok();
            let remote = files.remote.path.join(&entry.name);
            (
                TransferRequest {
                    client,
                    direction,
                    remote,
                    local,
                    replace: true,
                    folder: entry.kind == EntryKind::Directory,
                    cancel: CancellationToken::new(),
                },
                label,
                // A folder's own size is not what its transfer moves.
                entry.size.filter(|_| entry.kind != EntryKind::Directory),
                exists,
            )
        }
        Direction::Upload => {
            let entry = files.local.entries.get(index)?;
            let label = entry.label.clone();
            if !matches!(entry.kind, EntryKind::File | EntryKind::Directory) {
                files
                    .transfers
                    .push(failed(direction, label, FilesError::NotAFile));
                return None;
            }
            let name = name_bytes(&entry.name);
            let exists = files
                .remote
                .entries
                .iter()
                .any(|remote| remote.name == name);
            (
                TransferRequest {
                    client,
                    direction,
                    remote: files.remote.path.join(&name),
                    local: files.local.path.join(&entry.name),
                    replace: exists,
                    folder: entry.kind == EntryKind::Directory,
                    cancel: CancellationToken::new(),
                },
                label,
                // A folder's own size is not what its transfer moves.
                entry.size.filter(|_| entry.kind != EntryKind::Directory),
                exists,
            )
        }
    })
}

fn failed(direction: Direction, label: String, error: FilesError) -> Transfer {
    Transfer {
        id: TransferId::fresh(),
        direction,
        label,
        bytes: 0,
        total: None,
        state: TransferState::Failed(error),
        cancel: CancellationToken::new(),
    }
}
