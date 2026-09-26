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

use heimdall_sftp::RemotePath;
use tokio_util::sync::CancellationToken;

use super::{App, Dialog, Effect};
use crate::files::{
    Direction, EntryKind, FilesError, FilesPane, Side, Transfer, TransferEvent, TransferId,
    TransferRequest, TransferState, download_name,
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
        }
    }
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
        match message {
            FilesMessage::RemoteListed { tab, result } => {
                if let Some(files) = self.files_mut(tab) {
                    let pane = &mut files.remote;
                    pane.loading = false;
                    match result {
                        Ok((path, entries)) => {
                            pane.path = path;
                            pane.entries = entries;
                            pane.selected = None;
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
                            pane.entries = entries;
                            pane.selected = None;
                            pane.error = None;
                        }
                        Err(error) => pane.error = Some(error),
                    }
                }
                Vec::new()
            }
            FilesMessage::Select { tab, side, index } => {
                if let Some(files) = self.files_mut(tab) {
                    match side {
                        Side::Remote if index < files.remote.entries.len() => {
                            files.remote.selected = Some(index);
                        }
                        Side::Local if index < files.local.entries.len() => {
                            files.local.selected = Some(index);
                        }
                        _ => {}
                    }
                }
                Vec::new()
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
                files.remote.selected = Some(index);
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
                files.local.selected = Some(index);
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
        let (request, label, total, exists) = match direction {
            Direction::Download => {
                let Some(entry) = files
                    .remote
                    .selected
                    .and_then(|i| files.remote.entries.get(i))
                else {
                    return Vec::new();
                };
                let label = entry.label.clone();
                if !matches!(entry.kind, EntryKind::File | EntryKind::Link) {
                    files
                        .transfers
                        .push(failed(direction, label, FilesError::NotAFile));
                    return Vec::new();
                }
                let name = match download_name(&entry.name) {
                    Ok(name) => name,
                    Err(error) => {
                        files.transfers.push(failed(direction, label, error));
                        return Vec::new();
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
                        cancel: CancellationToken::new(),
                    },
                    label,
                    entry.size,
                    exists,
                )
            }
            Direction::Upload => {
                let Some(entry) = files
                    .local
                    .selected
                    .and_then(|i| files.local.entries.get(i))
                else {
                    return Vec::new();
                };
                let label = entry.label.clone();
                if entry.kind != EntryKind::File {
                    files
                        .transfers
                        .push(failed(direction, label, FilesError::NotAFile));
                    return Vec::new();
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
                        cancel: CancellationToken::new(),
                    },
                    label,
                    entry.size,
                    exists,
                )
            }
        };
        if exists {
            self.dialog = Some(Dialog::ConfirmOverwrite {
                tab,
                direction,
                name: label.clone(),
            });
            self.pending_transfer = Some(PendingTransfer {
                tab,
                request,
                label,
                total,
            });
            return Vec::new();
        }
        self.launch(tab, request, label, total)
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
        let Some(pending) = self.pending_transfer.take() else {
            return Vec::new();
        };
        let mut request = pending.request;
        request.replace = true;
        self.launch(pending.tab, request, pending.label, pending.total)
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
