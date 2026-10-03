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

use std::path::{Path, PathBuf};

use heimdall_files::conflict::{Choice, Kind};
use heimdall_files::{Plan, RemotePath, Root};
use tokio_util::sync::CancellationToken;

use super::{App, ConflictRow, Dialog, Effect, NameAction};
use crate::files::{
    Direction, EntryKind, FileOperation, FileProperties, FilesError, FilesKey, FilesPane,
    PlanRequest, PlannedRoot, Side, SortColumn, Transfer, TransferEvent, TransferId,
    TransferRequest, TransferState, download_name, octal_mode, typed_name,
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
    /// The filter of a pane changed.
    Filter {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// What is typed.
        text: String,
    },
    /// Show a pane's hidden entries, or no longer.
    ToggleHidden {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
    },
    /// A file or folder of this computer dropped on the tab: sent to the server's folder
    /// shown, as the C# tab does with what Explorer drops on it.
    Dropped {
        /// Tab.
        tab: TabId,
        /// What was dropped.
        path: PathBuf,
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
    /// Hold the server's selected entries to be pasted, as the C# "Cut".
    Cut {
        /// Tab.
        tab: TabId,
    },
    /// Hold the server's selected entries to be copied when pasted, as the C# "Copy".
    Copy {
        /// Tab.
        tab: TabId,
    },
    /// Move or copy the entries held into the server's folder shown, as the C# "Paste".
    Paste {
        /// Tab.
        tab: TabId,
    },
    /// Open the server's selected file in the external editor, as the C# "Edit with
    /// external editor".
    EditExternal {
        /// Tab.
        tab: TabId,
    },
    /// The file is open in the editor, or why not.
    EditStarted {
        /// Tab.
        tab: TabId,
        /// The file being edited; or why not.
        result: Result<Box<crate::external_edit::EditSession>, FilesError>,
    },
    /// The editor was started again on a file being edited, or why not.
    EditorLaunched {
        /// Tab.
        tab: TabId,
        /// Whether it started.
        result: Result<(), FilesError>,
    },
    /// Time to look at the files being edited.
    EditTick,
    /// A look at the files being edited ended.
    EditsChecked {
        /// Tab.
        tab: TabId,
        /// Each file, by its local copy, and what was found.
        results: Vec<(PathBuf, crate::external_edit::EditCheck)>,
    },
    /// Copy the server's selected entries into their own folder, as the C# "Duplicate".
    Duplicate {
        /// Tab.
        tab: TabId,
    },
    /// Open a shell on the server in the selected folder, else the folder shown, as the C#
    /// "Open in terminal".
    OpenInTerminal {
        /// Tab.
        tab: TabId,
    },
    /// The copies of a paste or a duplicate ended.
    Copied {
        /// Tab.
        tab: TabId,
        /// Each entry, by its path, and the copy made of it, or why not.
        results: Vec<(RemotePath, Result<RemotePath, FilesError>)>,
        /// A duplicate, rather than a paste.
        duplicate: bool,
    },
    /// The moves of a paste ended.
    Moved {
        /// Tab.
        tab: TabId,
        /// Each entry moved, by its path before, and how it went.
        results: Vec<(RemotePath, Result<(), FilesError>)>,
    },
    /// Go to the folder typed in a pane's path bar.
    GoTo {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
    },
    /// Go back to the folder a pane showed before, as the C# Files tab's Back.
    Back {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
    },
    /// Go to the folder a pane first showed, as the C# Files tab's Home.
    Home {
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
    /// A transfer's plan arrived: every entry it would write, checked against the
    /// destination.
    Planned {
        /// Tab.
        tab: TabId,
        /// What was planned.
        request: Box<PlanRequest>,
        /// The plan; or why not.
        result: Result<Box<Plan>, FilesError>,
    },
    /// The answer picked for one destination in the way.
    ConflictChosen {
        /// Row of the question.
        row: usize,
        /// Answer.
        choice: Choice,
    },
    /// The same answer for every destination in the way that allows it.
    ConflictAll(Choice),
}

impl FilesMessage {
    /// The pane a user gesture acts on: the one that takes the focus.
    fn gesture(&self) -> Option<(TabId, Side)> {
        match *self {
            Self::Select { tab, side, .. }
            | Self::Open { tab, side, .. }
            | Self::Up { tab, side }
            | Self::Back { tab, side }
            | Self::Home { tab, side }
            | Self::Refresh { tab, side }
            | Self::GoTo { tab, side }
            | Self::SortBy { tab, side, .. }
            | Self::Toggle { tab, side, .. }
            | Self::Range { tab, side, .. }
            | Self::Filter { tab, side, .. }
            | Self::ToggleHidden { tab, side }
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
    #[expect(clippy::too_many_lines, reason = "one arm per message")]
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
            Self::Back { tab, side } => write!(f, "Back({}, {side:?})", tab.value()),
            Self::Home { tab, side } => write!(f, "Home({}, {side:?})", tab.value()),
            Self::Refresh { tab, side } => write!(f, "Refresh({}, {side:?})", tab.value()),
            Self::PathEdited { tab, side, .. } => {
                write!(f, "PathEdited({}, {side:?}, ..)", tab.value())
            }
            Self::GoTo { tab, side } => write!(f, "GoTo({}, {side:?})", tab.value()),
            Self::CopyPath { tab, side } => write!(f, "CopyPath({}, {side:?})", tab.value()),
            Self::Cut { tab } => write!(f, "Cut({})", tab.value()),
            Self::Paste { tab } => write!(f, "Paste({})", tab.value()),
            Self::Copy { tab } => write!(f, "Copy({})", tab.value()),
            Self::Duplicate { tab } => write!(f, "Duplicate({})", tab.value()),
            Self::EditExternal { tab } => write!(f, "EditExternal({})", tab.value()),
            Self::EditStarted { tab, result } => {
                write!(f, "EditStarted({}, {})", tab.value(), result.is_ok())
            }
            Self::EditorLaunched { tab, result } => {
                write!(f, "EditorLaunched({}, {})", tab.value(), result.is_ok())
            }
            Self::EditTick => f.write_str("EditTick"),
            Self::EditsChecked { tab, results } => {
                write!(f, "EditsChecked({}, {})", tab.value(), results.len())
            }
            Self::OpenInTerminal { tab } => write!(f, "OpenInTerminal({})", tab.value()),
            Self::Copied { tab, results, .. } => {
                write!(f, "Copied({}, {})", tab.value(), results.len())
            }
            Self::Moved { tab, results } => write!(f, "Moved({}, {})", tab.value(), results.len()),
            Self::Bookmark { tab } => write!(f, "Bookmark({})", tab.value()),
            Self::Dropped { tab, .. } => write!(f, "Dropped({}, ..)", tab.value()),
            Self::Filter { tab, side, .. } => write!(f, "Filter({}, {side:?}, ..)", tab.value()),
            Self::ToggleHidden { tab, side } => {
                write!(f, "ToggleHidden({}, {side:?})", tab.value())
            }
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
            Self::Planned { tab, result, .. } => {
                write!(f, "Planned({}, {})", tab.value(), result.is_ok())
            }
            Self::ConflictChosen { row, choice } => write!(f, "ConflictChosen({row}, {choice:?})"),
            Self::ConflictAll(choice) => write!(f, "ConflictAll({choice:?})"),
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

/// A planned transfer waiting for the user's answers to the destinations in its way.
#[derive(Debug, Clone)]
pub(super) struct PendingPlan {
    tab: TabId,
    request: PlanRequest,
    plan: Plan,
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
    pub(super) fn files_mut(&mut self, tab: TabId) -> Option<&mut FilesPane> {
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

    pub(super) fn list(&mut self, tab: TabId, side: Side) -> Vec<Effect> {
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

    /// A remote listing arrived. A link being entered that cannot be listed points at no
    /// folder: the pane goes back where it was opened from, as the C# Files tab stays, and
    /// says so.
    fn remote_listed(
        &mut self,
        tab: TabId,
        result: Result<(RemotePath, Vec<crate::files::RemoteEntry>), FilesError>,
    ) -> Vec<Effect> {
        let mut not_a_folder = None;
        if let Some(files) = self.files_mut(tab) {
            let pane = &mut files.remote;
            pane.loading = false;
            match (result, pane.entering_link.take()) {
                (Ok((path, entries)), _) => {
                    pane.arrived(&path);
                    pane.path = path;
                    pane.show(entries);
                    pane.error = None;
                }
                (Err(_), Some((from, name))) => {
                    pane.not_arrived();
                    pane.path = from;
                    not_a_folder = Some(name);
                }
                (Err(error), None) => {
                    pane.not_arrived();
                    pane.error = Some(error);
                }
            }
        }
        if let Some(name) = not_a_folder {
            self.tell(super::Notice::LinkNotAFolder(name));
        }
        Vec::new()
    }

    /// A local listing arrived.
    fn local_listed(
        &mut self,
        tab: TabId,
        result: Result<(PathBuf, Vec<crate::files::LocalEntry>), FilesError>,
    ) -> Vec<Effect> {
        if let Some(files) = self.files_mut(tab) {
            let pane = &mut files.local;
            pane.loading = false;
            match result {
                Ok((path, entries)) => {
                    pane.arrived(&path);
                    pane.path = path;
                    pane.show(entries);
                    pane.error = None;
                }
                Err(error) => {
                    pane.not_arrived();
                    pane.error = Some(error);
                }
            }
        }
        Vec::new()
    }

    pub(super) fn files(&mut self, message: FilesMessage) -> Vec<Effect> {
        if let Some((tab, side)) = message.gesture()
            && let Some(files) = self.files_mut(tab)
        {
            files.focus = side;
        }
        match message {
            FilesMessage::RemoteListed { tab, result } => self.remote_listed(tab, result),
            FilesMessage::LocalListed { tab, result } => self.local_listed(tab, result),
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
                    Side::Remote => {
                        files.remote.leave();
                        files.remote.path = files.remote.path.parent();
                    }
                    Side::Local => {
                        if let Some(parent) = files.local.path.parent() {
                            let parent = parent.to_owned();
                            files.local.leave();
                            files.local.path = parent;
                        }
                    }
                }
                self.list(tab, side)
            }
            FilesMessage::Back { tab, side } => self.go_back(tab, side),
            FilesMessage::Home { tab, side } => self.go_home(tab, side),
            FilesMessage::Refresh { tab, side } => self.list(tab, side),
            FilesMessage::Cut { tab } => self.hold_entries(tab, super::ClipMode::Cut),
            FilesMessage::Copy { tab } => self.hold_entries(tab, super::ClipMode::Copy),
            FilesMessage::Paste { tab } => self.paste_held(tab),
            FilesMessage::Duplicate { tab } => self.duplicate(tab),
            message @ (FilesMessage::EditExternal { .. }
            | FilesMessage::EditStarted { .. }
            | FilesMessage::EditorLaunched { .. }
            | FilesMessage::EditTick
            | FilesMessage::EditsChecked { .. }) => self.edit_message(message),
            FilesMessage::OpenInTerminal { tab } => self.open_in_terminal(tab),
            FilesMessage::Moved { tab, results } => self.moved_held(tab, results),
            FilesMessage::Copied {
                tab,
                results,
                duplicate,
            } => self.copied(tab, results, duplicate),
            message @ (FilesMessage::PathEdited { .. }
            | FilesMessage::GoTo { .. }
            | FilesMessage::SortBy { .. }
            | FilesMessage::CopyPath { .. }
            | FilesMessage::AskPermissions { .. }
            | FilesMessage::ShowProperties { .. }
            | FilesMessage::Toggle { .. }
            | FilesMessage::Range { .. }
            | FilesMessage::Bookmark { .. }
            | FilesMessage::OpenBookmark { .. }
            | FilesMessage::Filter { .. }
            | FilesMessage::ToggleHidden { .. }
            | FilesMessage::Dropped { .. }) => self.pane_message(message),
            FilesMessage::Key { tab, key } => self.files_key(tab, key),
            FilesMessage::Transfer { tab, direction } => self.start_transfer(tab, direction),
            FilesMessage::TransferEvent { tab, id, event } => self.transfer_event(tab, id, event),
            message @ (FilesMessage::Planned { .. }
            | FilesMessage::ConflictChosen { .. }
            | FilesMessage::ConflictAll(_)) => self.plan_message(message),
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
            FilesMessage::Dropped { tab, path } => self.upload_dropped(tab, &path),
            FilesMessage::Filter { tab, side, text } => {
                if let Some(files) = self.files_mut(tab) {
                    match side {
                        Side::Remote => files.remote.filter_by(text),
                        Side::Local => files.local.filter_by(text),
                    }
                }
                Vec::new()
            }
            FilesMessage::ToggleHidden { tab, side } => {
                if let Some(files) = self.files_mut(tab) {
                    match side {
                        Side::Remote => files.remote.toggle_hidden(),
                        Side::Local => files.local.toggle_hidden(),
                    }
                }
                Vec::new()
            }
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

    /// Sends `path`, dropped on the tab, to the server's folder shown; asked first when it
    /// would replace a name listed there.
    fn upload_dropped(&mut self, tab: TabId, path: &Path) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let (Some(client), Some(name)) = (files.client.clone(), path.file_name()) else {
            return Vec::new();
        };
        let folder = path.is_dir();
        if !folder && !path.is_file() {
            let label = name.to_string_lossy().into_owned();
            files
                .transfers
                .push(failed(Direction::Upload, label, FilesError::NotAFile));
            return Vec::new();
        }
        let size = (!folder)
            .then(|| path.metadata().map(|meta| meta.len()).ok())
            .flatten();
        let root = PlannedRoot {
            root: Root {
                remote: files.remote.path.join(&name_bytes(name)),
                local: path.to_owned(),
                kind: if folder { Kind::Folder } else { Kind::File },
            },
            label: name.to_string_lossy().into_owned(),
            total: size,
        };
        vec![Effect::PlanTransfer {
            tab,
            request: Box::new(PlanRequest {
                client,
                direction: Direction::Upload,
                roots: vec![root],
            }),
        }]
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
        files.remote.leave();
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
        self.tell(super::Notice::PathCopied(path.clone()));
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
                files.remote.leave();
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
                files.local.leave();
                files.local.loading = true;
                vec![Effect::ListLocal { tab, path }]
            }
        }
    }

    /// Back to the folder `side` showed before, as the C# Files tab's Back.
    fn go_back(&mut self, tab: TabId, side: Side) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let moved = match side {
            Side::Remote => files.remote.back(),
            Side::Local => files.local.back(),
        };
        if moved {
            self.list(tab, side)
        } else {
            Vec::new()
        }
    }

    /// To the folder `side` first showed, as the C# Files tab's Home.
    fn go_home(&mut self, tab: TabId, side: Side) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let moved = match side {
            Side::Remote => files.remote.go_home(),
            Side::Local => files.local.go_home(),
        };
        if moved {
            self.list(tab, side)
        } else {
            Vec::new()
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
            FilesKey::Back => return self.go_back(tab, side),
            FilesKey::Rename => return self.ask(tab, side, NameAction::Rename),
            FilesKey::Delete => return self.ask_delete(tab, side),
            FilesKey::Refresh => return self.list(tab, side),
            FilesKey::CopyPath => return self.copy_path(tab, side),
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
                    let path = files.remote.path.join(&entry.name);
                    files.remote.leave();
                    files.remote.path = path;
                    return self.list(tab, side);
                }
                // The listing does not say what a link points at; listing it tells. A link is
                // never downloaded from here, as in the C# Files tab.
                if entry.kind == EntryKind::Link {
                    let from = files.remote.path.clone();
                    let path = from.join(&entry.name);
                    files.remote.entering_link = Some((from, entry.label.clone()));
                    files.remote.leave();
                    files.remote.path = path;
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
                    let path = files.local.path.join(&entry.name);
                    files.local.leave();
                    files.local.path = path;
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
        let roots: Vec<_> = chosen
            .into_iter()
            .filter_map(|index| prepare(files, direction, index))
            .collect();
        if roots.is_empty() {
            return Vec::new();
        }
        // Planned whole first: nothing is written before every conflict is answered.
        vec![Effect::PlanTransfer {
            tab,
            request: Box::new(PlanRequest {
                client,
                direction,
                roots,
            }),
        }]
    }

    /// A transfer's plan, or an answer about what is in its way.
    fn plan_message(&mut self, message: FilesMessage) -> Vec<Effect> {
        match message {
            FilesMessage::Planned {
                tab,
                request,
                result,
            } => self.planned(tab, *request, result),
            FilesMessage::ConflictChosen { row, choice } => {
                self.choose_conflict(Some(row), choice);
                Vec::new()
            }
            FilesMessage::ConflictAll(choice) => {
                self.choose_conflict(None, choice);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// A transfer's plan arrived: launched when nothing is in the way, asked about
    /// otherwise.
    fn planned(
        &mut self,
        tab: TabId,
        request: PlanRequest,
        result: Result<Box<Plan>, FilesError>,
    ) -> Vec<Effect> {
        let plan = match result {
            Ok(plan) => *plan,
            Err(error) => {
                if let Some(files) = self.files_mut(tab) {
                    for root in request.roots {
                        files
                            .transfers
                            .push(failed(request.direction, root.label, error.clone()));
                    }
                }
                return Vec::new();
            }
        };
        if !plan.has_conflicts() {
            return self.launch_plan(tab, &request, &plan, &[]);
        }
        // Asked one after the other, once nothing else is asked.
        self.pending_plans
            .push_back(PendingPlan { tab, request, plan });
        self.ask_next_conflicts();
        Vec::new()
    }

    /// Asks about the next planned transfer with destinations in its way, unless something
    /// else is being asked.
    pub(super) fn ask_next_conflicts(&mut self) {
        if self.dialog.is_some() {
            return;
        }
        if let Some(next) = self.pending_plans.front() {
            self.dialog = Some(Dialog::FileConflicts {
                tab: next.tab,
                rows: next
                    .plan
                    .conflicts()
                    .filter_map(|(index, step, checked)| {
                        let allowed = checked.allowed?;
                        Some(ConflictRow {
                            index,
                            target: crate::text::server_text(&target_text(&step.target)),
                            folder: step.kind == Kind::Folder,
                            allowed,
                            choice: allowed.default_choice()?,
                        })
                    })
                    .collect(),
            });
        }
    }

    /// Picks `choice` for row `only`, or for every row without it, where it is allowed.
    fn choose_conflict(&mut self, only: Option<usize>, choice: Choice) {
        if let Some(Dialog::FileConflicts { rows, .. }) = self.dialog.as_mut() {
            for (index, row) in rows.iter_mut().enumerate() {
                if only.is_none_or(|only| only == index) && row.allowed.allows(choice) {
                    row.choice = choice;
                }
            }
        }
    }

    /// The user answered every destination in the way of the transfer asked about.
    pub(super) fn confirm_conflicts(&mut self, rows: &[ConflictRow]) -> Vec<Effect> {
        let Some(pending) = self.pending_plans.pop_front() else {
            return Vec::new();
        };
        let answers: Vec<_> = rows.iter().map(|row| (row.index, row.choice)).collect();
        let effects = self.launch_plan(pending.tab, &pending.request, &pending.plan, &answers);
        self.ask_next_conflicts();
        effects
    }

    /// The question about the transfer shown was cancelled: that transfer goes nowhere.
    pub(super) fn cancel_conflicts(&mut self) {
        self.pending_plans.pop_front();
    }

    /// Starts one transfer per picked entry, as answered; an entry skipped whole is not
    /// started.
    fn launch_plan(
        &mut self,
        tab: TabId,
        request: &PlanRequest,
        plan: &Plan,
        answers: &[(usize, Choice)],
    ) -> Vec<Effect> {
        let Ok(ready) = plan.resolve(answers) else {
            // Rows are built from the plan's own conflicts, each with an allowed answer.
            return Vec::new();
        };
        let mut effects = Vec::new();
        for (index, (picked, steps)) in request.roots.iter().zip(ready).enumerate() {
            let transfer = match picked.root.kind {
                Kind::File => {
                    let Some(step) = steps.into_iter().next() else {
                        continue;
                    };
                    TransferRequest {
                        client: request.client.clone(),
                        direction: request.direction,
                        remote: step.remote,
                        local: step.local,
                        replace: step.replace,
                        folder: false,
                        steps: Vec::new(),
                        left_out: 0,
                        cancel: CancellationToken::new(),
                    }
                }
                Kind::Folder => {
                    if steps.is_empty() {
                        continue;
                    }
                    TransferRequest {
                        client: request.client.clone(),
                        direction: request.direction,
                        remote: picked.root.remote.clone(),
                        local: picked.root.local.clone(),
                        replace: false,
                        folder: true,
                        steps,
                        left_out: plan.left_out(index),
                        cancel: CancellationToken::new(),
                    }
                }
            };
            effects.extend(self.launch(tab, transfer, picked.label.clone(), picked.total));
        }
        effects
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

/// The selected entry at `index` as a transfer's picked entry. A refusal is recorded as a
/// failed transfer.
fn prepare(files: &mut FilesPane, direction: Direction, index: usize) -> Option<PlannedRoot> {
    let kind = |entry_kind| {
        if entry_kind == EntryKind::Directory {
            Kind::Folder
        } else {
            Kind::File
        }
    };
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
            PlannedRoot {
                root: Root {
                    remote: files.remote.path.join(&entry.name),
                    local: files.local.path.join(&name.name),
                    kind: kind(entry.kind),
                },
                label,
                // A folder's own size is not what its transfer moves.
                total: entry.size.filter(|_| entry.kind != EntryKind::Directory),
            }
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
            PlannedRoot {
                root: Root {
                    remote: files.remote.path.join(&name_bytes(&entry.name)),
                    local: files.local.path.join(&entry.name),
                    kind: kind(entry.kind),
                },
                label,
                // A folder's own size is not what its transfer moves.
                total: entry.size.filter(|_| entry.kind != EntryKind::Directory),
            }
        }
    })
}

/// A destination's names below the destination folder, joined as a path.
fn target_text(target: &[Vec<u8>]) -> String {
    target
        .iter()
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect::<Vec<_>>()
        .join("/")
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
