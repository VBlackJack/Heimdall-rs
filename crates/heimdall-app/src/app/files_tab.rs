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

use heimdall_files::conflict::{Choice, Kind};
use heimdall_files::{Plan, RemotePath, Root};
use tokio_util::sync::CancellationToken;

use super::{App, ConflictRow, Dialog, Effect, NameAction};
use heimdall_files::RemoteSession;

use crate::files::{
    Batch, BatchKind, Direction, EntryKind, FileOperation, FileProperties, FilesError, FilesKey,
    FilesPane, PlanRequest, PlannedRoot, Side, SortColumn, Transfer, TransferEvent, TransferId,
    TransferRequest, TransferState, Waiting, download_name, octal_mode, typed_name,
};
use crate::ids::TabId;
use crate::sudo_mode::SudoAccess;

/// The entries a delete as root names in its question; past them, how many more go.
pub const SUDO_DELETE_NAMED: usize = 10;

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
    /// What is typed in a pane's path bar given up: it shows the folder shown again.
    PathCancelled {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
    },
    /// Go `levels` folders up at once, as a click on a folder of the C# breadcrumb; none
    /// lists the folder shown again.
    Ascend {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// Folders up.
        levels: usize,
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
    /// Show this computer's pane beside the server's, or hide it.
    ToggleLocal {
        /// Tab.
        tab: TabId,
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
    /// Take one of the server's folders off the bookmarks, as the C# "Remove a bookmark".
    RemoveBookmark {
        /// Tab.
        tab: TabId,
        /// Which, in the order they were bookmarked.
        index: usize,
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
    /// Pick files of this computer to upload into the server's folder shown, as the C#
    /// "Upload here...".
    UploadHere {
        /// Tab.
        tab: TabId,
    },
    /// Upload the files copied in Explorer into the server's folder shown, as the C# "Paste
    /// from Explorer".
    PasteFromExplorer {
        /// Tab.
        tab: TabId,
    },
    /// The files copied in Explorer, read; none when the clipboard holds none.
    ExplorerFilesRead {
        /// Tab.
        tab: TabId,
        /// The files.
        paths: Vec<PathBuf>,
    },
    /// The files picked to upload; none when the picker was closed.
    UploadPicked {
        /// Tab.
        tab: TabId,
        /// The files.
        paths: Vec<PathBuf>,
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
    /// Open the server's selected file in the integrated editor, as the C# "Edit".
    EditIntegrated {
        /// Tab.
        tab: TabId,
    },
    /// The file of the integrated editor was read: how it is stored and the server's file
    /// as read, or why it is not opened. Its text stays with the window.
    EditorOpened {
        /// Tab.
        tab: TabId,
        /// The editor.
        id: crate::ids::EditorId,
        /// How it is stored and the server's file as read, or why not.
        result: Result<(crate::text_codec::TextEncoding, heimdall_files::Fingerprint), FilesError>,
    },
    /// The integrated editor's text now differs from what was last read or saved, or no
    /// longer does.
    EditorChanged {
        /// Tab.
        tab: TabId,
        /// The editor.
        id: crate::ids::EditorId,
        /// It differs.
        dirty: bool,
    },
    /// Save the integrated editor's text.
    EditorSave {
        /// Tab.
        tab: TabId,
        /// The editor.
        id: crate::ids::EditorId,
        /// The text.
        text: String,
        /// Write over the server's file even though it changed, as the user said.
        overwrite: bool,
    },
    /// The integrated editor's text was saved, or why not.
    EditorSaved {
        /// Tab.
        tab: TabId,
        /// The editor.
        id: crate::ids::EditorId,
        /// The server's file as saved, or why not.
        result: Result<heimdall_files::Fingerprint, FilesError>,
        /// The text still differs from what was saved: edited during the save.
        dirty: bool,
    },
    /// Close the integrated editor; asked first when its text is not saved.
    EditorClose {
        /// Tab.
        tab: TabId,
        /// The editor.
        id: crate::ids::EditorId,
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
    /// Send a refused save over the server's file as it is now.
    EditSendAnyway {
        /// Tab.
        tab: TabId,
        /// The edit, by its local copy.
        local: PathBuf,
    },
    /// The save sent anyway, or why not.
    EditSentAnyway {
        /// Tab.
        tab: TabId,
        /// The edit, by its local copy.
        local: PathBuf,
        /// What happened.
        check: crate::external_edit::EditCheck,
    },
    /// Open the server's selected file with sudo, as "Edit with sudo".
    EditWithSudo {
        /// Tab.
        tab: TabId,
    },
    /// Send an edit's refused save with sudo, as "Save with sudo".
    EditSaveWithSudo {
        /// Tab.
        tab: TabId,
        /// The edit, by its local copy.
        local: PathBuf,
    },
    /// The file opened with sudo, or why not.
    SudoOpened {
        /// Tab.
        tab: TabId,
        /// The server's file.
        remote: RemotePath,
        /// The file being edited; or why not.
        result: Result<Box<crate::external_edit::EditSession>, FilesError>,
    },
    /// The save sent with sudo, or why not.
    SudoSaved {
        /// Tab.
        tab: TabId,
        /// The edit, by its local copy.
        local: PathBuf,
        /// What happened.
        check: crate::external_edit::EditCheck,
    },
    /// The password typed for sudo, as the question asked.
    SudoPasswordGiven {
        /// Tab.
        tab: TabId,
        /// The password.
        password: crate::sudo_edit::SudoPassword,
    },
    /// Turn the server pane's sudo mode on or off, as the C# "sudo" toggle; over SSH only.
    ToggleSudo {
        /// Tab.
        tab: TabId,
    },
    /// Turn the SFTP pane's following of its SSH shell's working folder on or off, as the
    /// C# "cwd" toggle; over SFTP only.
    ToggleFollow {
        /// Tab.
        tab: TabId,
    },
    /// A remote folder listed as root arrived.
    SudoListed {
        /// Tab.
        tab: TabId,
        /// The folder asked for.
        path: RemotePath,
        /// The folder, absolute, and its entries; or why not.
        result: Result<(RemotePath, Vec<crate::files::RemoteEntry>), FilesError>,
    },
    /// Show the folder of an edit's local copy.
    EditOpenFolder {
        /// Tab.
        tab: TabId,
        /// The edit, by its local copy.
        local: PathBuf,
    },
    /// Stop watching an edit: its next saves are not sent.
    EditStop {
        /// Tab.
        tab: TabId,
        /// The edit, by its local copy.
        local: PathBuf,
    },
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
    /// Open the local folder selected, else the folder shown, in the system's file manager,
    /// as the C# local file browser's "Open in Explorer".
    OpenInExplorer {
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
    /// Run a failed or cancelled transfer again, planned anew.
    Retry {
        /// Tab.
        tab: TabId,
        /// Transfer.
        id: TransferId,
    },
    /// Take the ended transfers off the list.
    ClearFinished {
        /// Tab.
        tab: TabId,
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
    /// The entries chosen in `from` dragged onto `onto`, into its folder entry at `into`, or
    /// the folder it shows: sent to the other side, or moved into a folder of their own.
    DropEntries {
        /// Tab.
        tab: TabId,
        /// The pane dragged from.
        from: Side,
        /// The pane dropped on.
        onto: Side,
        /// The folder entry dropped on; `None` for the folder the pane shows.
        into: Option<usize>,
    },
    /// The entry of a delete or a change of permissions being worked on ended.
    BatchStepDone {
        /// Tab.
        tab: TabId,
        /// How it went.
        result: Result<(), FilesError>,
    },
    /// Stop the delete or the change of permissions running, after the entry being worked
    /// on.
    StopBatch {
        /// Tab.
        tab: TabId,
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
    #[must_use]
    pub fn gesture(&self) -> Option<(TabId, Side)> {
        match *self {
            Self::Select { tab, side, .. }
            | Self::Open { tab, side, .. }
            | Self::Up { tab, side }
            | Self::Ascend { tab, side, .. }
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
            Self::PathCancelled { tab, side } => {
                write!(f, "PathCancelled({}, {side:?})", tab.value())
            }
            Self::Ascend { tab, side, levels } => {
                write!(f, "Ascend({}, {side:?}, {levels})", tab.value())
            }
            Self::CopyPath { tab, side } => write!(f, "CopyPath({}, {side:?})", tab.value()),
            Self::Cut { tab } => write!(f, "Cut({})", tab.value()),
            Self::UploadHere { tab } => write!(f, "UploadHere({})", tab.value()),
            Self::PasteFromExplorer { tab } => write!(f, "PasteFromExplorer({})", tab.value()),
            Self::ExplorerFilesRead { tab, paths } => {
                write!(f, "ExplorerFilesRead({}, {})", tab.value(), paths.len())
            }
            Self::UploadPicked { tab, paths } => {
                write!(f, "UploadPicked({}, {})", tab.value(), paths.len())
            }
            Self::Paste { tab } => write!(f, "Paste({})", tab.value()),
            Self::Copy { tab } => write!(f, "Copy({})", tab.value()),
            Self::Duplicate { tab } => write!(f, "Duplicate({})", tab.value()),
            Self::EditExternal { tab } => write!(f, "EditExternal({})", tab.value()),
            Self::EditIntegrated { tab } => write!(f, "EditIntegrated({})", tab.value()),
            Self::EditorOpened { tab, result, .. } => {
                write!(f, "EditorOpened({}, {})", tab.value(), result.is_ok())
            }
            Self::EditorChanged { tab, dirty, .. } => {
                write!(f, "EditorChanged({}, {dirty})", tab.value())
            }
            Self::EditorSave { tab, overwrite, .. } => {
                write!(f, "EditorSave({}, {overwrite})", tab.value())
            }
            Self::EditorSaved { tab, result, .. } => {
                write!(f, "EditorSaved({}, {})", tab.value(), result.is_ok())
            }
            Self::EditorClose { tab, .. } => write!(f, "EditorClose({})", tab.value()),
            Self::EditStarted { tab, result } => {
                write!(f, "EditStarted({}, {})", tab.value(), result.is_ok())
            }
            Self::EditorLaunched { tab, result } => {
                write!(f, "EditorLaunched({}, {})", tab.value(), result.is_ok())
            }
            Self::EditTick => f.write_str("EditTick"),
            Self::EditSendAnyway { tab, .. } => write!(f, "EditSendAnyway({})", tab.value()),
            Self::EditSentAnyway { tab, .. } => write!(f, "EditSentAnyway({})", tab.value()),
            Self::EditOpenFolder { tab, .. } => write!(f, "EditOpenFolder({})", tab.value()),
            Self::EditWithSudo { tab } => write!(f, "EditWithSudo({})", tab.value()),
            Self::EditSaveWithSudo { tab, .. } => write!(f, "EditSaveWithSudo({})", tab.value()),
            Self::SudoOpened { tab, result, .. } => {
                write!(f, "SudoOpened({}, {})", tab.value(), result.is_ok())
            }
            Self::SudoSaved { tab, .. } => write!(f, "SudoSaved({})", tab.value()),
            Self::SudoPasswordGiven { tab, .. } => {
                write!(f, "SudoPasswordGiven({}, ..)", tab.value())
            }
            Self::ToggleSudo { tab } => write!(f, "ToggleSudo({})", tab.value()),
            Self::ToggleFollow { tab } => write!(f, "ToggleFollow({})", tab.value()),
            Self::SudoListed { tab, result, .. } => write!(
                f,
                "SudoListed({}, {})",
                tab.value(),
                result.as_ref().map_or(0, |(_, entries)| entries.len())
            ),
            Self::EditStop { tab, .. } => write!(f, "EditStop({})", tab.value()),
            Self::EditsChecked { tab, results } => {
                write!(f, "EditsChecked({}, {})", tab.value(), results.len())
            }
            Self::OpenInTerminal { tab } => write!(f, "OpenInTerminal({})", tab.value()),
            Self::OpenInExplorer { tab } => write!(f, "OpenInExplorer({})", tab.value()),
            Self::Copied { tab, results, .. } => {
                write!(f, "Copied({}, {})", tab.value(), results.len())
            }
            Self::Moved { tab, results } => write!(f, "Moved({}, {})", tab.value(), results.len()),
            Self::Bookmark { tab } => write!(f, "Bookmark({})", tab.value()),
            Self::Dropped { tab, .. } => write!(f, "Dropped({}, ..)", tab.value()),
            Self::Filter { tab, side, .. } => write!(f, "Filter({}, {side:?}, ..)", tab.value()),
            Self::ToggleLocal { tab } => write!(f, "ToggleLocal({})", tab.value()),
            Self::ToggleHidden { tab, side } => {
                write!(f, "ToggleHidden({}, {side:?})", tab.value())
            }
            Self::RemoveBookmark { tab, index } => {
                write!(f, "RemoveBookmark({}, {index})", tab.value())
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
            Self::Retry { tab, id } => write!(f, "Retry({}, {})", tab.value(), id.value()),
            Self::ClearFinished { tab } => write!(f, "ClearFinished({})", tab.value()),
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
            Self::DropEntries {
                tab,
                from,
                onto,
                into,
            } => write!(
                f,
                "DropEntries({}, {from:?} onto {onto:?} {into:?})",
                tab.value()
            ),
            Self::BatchStepDone { tab, result } => {
                write!(f, "BatchStepDone({}, {})", tab.value(), result.is_ok())
            }
            Self::StopBatch { tab } => write!(f, "StopBatch({})", tab.value()),
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

impl PendingOperation {
    /// The Files tab the operation is asked about.
    pub(super) fn tab(&self) -> TabId {
        self.tab
    }
}

#[derive(Debug, Clone)]
enum PendingKind {
    /// A folder to create in the pane's folder.
    NewFolder,
    /// The entry at this path to rename in the same folder.
    Rename { remote: RemotePath, local: PathBuf },
    /// The entries at these paths to delete, each with its name.
    Delete {
        targets: Vec<(String, RemotePath, PathBuf)>,
    },
    /// The entries of the server at these paths to give new permission bits, each with its
    /// name.
    Permissions { remotes: Vec<(String, RemotePath)> },
    /// The entries of the server to delete as root, each with its name.
    SudoDelete {
        targets: Vec<(String, FileOperation)>,
    },
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

    /// Effects that list both panes, once the session is open; the server's bookmarks
    /// kept from before brought back.
    pub(super) fn files_ready(&mut self, tab: TabId) -> Vec<Effect> {
        let kept: Vec<RemotePath> = self
            .files_endpoint(tab)
            .map(|server| {
                self.files_state
                    .bookmarks(&server)
                    .iter()
                    .map(|path| RemotePath::from(path.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        for path in kept {
            if !files.bookmarks.contains(&path) {
                files.bookmarks.push(path);
            }
        }
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
                files.remote.discard_listing = false;
                let path = files.remote.path.clone();
                vec![super::files_sudo::remote_listing(files, tab, client, path)]
            }
            Side::Local => {
                files.local.loading = true;
                files.local.discard_listing = false;
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
    pub(super) fn remote_listed(
        &mut self,
        tab: TabId,
        result: Result<(RemotePath, Vec<crate::files::RemoteEntry>), FilesError>,
    ) -> Vec<Effect> {
        let mut not_a_folder = None;
        if let Some(files) = self.files_mut(tab) {
            let pane = &mut files.remote;
            // Given up with Escape: the folder shown stays.
            if std::mem::take(&mut pane.discard_listing) {
                return Vec::new();
            }
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
        if result.is_err() && self.local_browser_start_failed(tab) {
            return self.list(tab, Side::Local);
        }
        if let Some(files) = self.files_mut(tab) {
            let pane = &mut files.local;
            if std::mem::take(&mut pane.discard_listing) {
                return Vec::new();
            }
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
        // The local file browser keeps the keys on the one pane it has.
        if let Some((tab, side)) = message.gesture()
            && let Some(files) = self.files_mut(tab).filter(|files| !files.local_only)
        {
            files.focus = side;
        }
        match message {
            FilesMessage::RemoteListed { tab, result } => self.remote_listed(tab, result),
            FilesMessage::SudoListed { tab, path, result } => self.sudo_listed(tab, path, result),
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
            FilesMessage::BatchStepDone { tab, result } => self.batch_step_done(tab, result),
            FilesMessage::StopBatch { tab } => self.stop_batch(tab),
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
            FilesMessage::Up { tab, side } => self.ascend(tab, side, 1),
            FilesMessage::Ascend { tab, side, levels } => self.ascend(tab, side, levels),
            FilesMessage::Back { tab, side } => self.go_back(tab, side),
            FilesMessage::Home { tab, side } => self.go_home(tab, side),
            FilesMessage::Refresh { tab, side } => self.list(tab, side),
            FilesMessage::Cut { tab } => self.hold_entries(tab, super::ClipMode::Cut),
            FilesMessage::UploadHere { tab } => self.upload_here(tab),
            FilesMessage::PasteFromExplorer { tab } => self.paste_from_explorer(tab),
            FilesMessage::Copy { tab } => self.hold_entries(tab, super::ClipMode::Copy),
            FilesMessage::Paste { tab } => self.paste_held(tab),
            FilesMessage::Duplicate { tab } => self.duplicate(tab),
            FilesMessage::OpenInTerminal { tab } => self.open_in_terminal(tab),
            FilesMessage::OpenInExplorer { tab } => self.open_in_explorer(tab),
            FilesMessage::Moved { tab, results } => self.moved_held(tab, results),
            FilesMessage::DropEntries {
                tab,
                from,
                onto,
                into,
            } => self.drop_entries(tab, from, onto, into),
            FilesMessage::Copied {
                tab,
                results,
                duplicate,
            } => self.copied(tab, results, duplicate),
            message @ (FilesMessage::PathEdited { .. }
            | FilesMessage::PathCancelled { .. }
            | FilesMessage::GoTo { .. }
            | FilesMessage::SortBy { .. }
            | FilesMessage::CopyPath { .. }
            | FilesMessage::AskPermissions { .. }
            | FilesMessage::ShowProperties { .. }
            | FilesMessage::Toggle { .. }
            | FilesMessage::Range { .. }
            | FilesMessage::Bookmark { .. }
            | FilesMessage::OpenBookmark { .. }
            | FilesMessage::RemoveBookmark { .. }
            | FilesMessage::Filter { .. }
            | FilesMessage::ToggleHidden { .. }
            | FilesMessage::ToggleLocal { .. }
            | FilesMessage::ToggleSudo { .. }
            | FilesMessage::ToggleFollow { .. }
            | FilesMessage::Dropped { .. }
            | FilesMessage::UploadPicked { .. }
            | FilesMessage::ExplorerFilesRead { .. }) => self.pane_message(message),
            FilesMessage::Key { tab, key } => self.files_key(tab, key),
            FilesMessage::Transfer { tab, direction } => self.start_transfer(tab, direction),
            FilesMessage::TransferEvent { tab, id, event } => self.transfer_event(tab, id, event),
            message @ (FilesMessage::Planned { .. }
            | FilesMessage::ConflictChosen { .. }
            | FilesMessage::ConflictAll(_)) => self.plan_message(message),
            FilesMessage::Cancel { tab, id } => {
                if let Some(files) = self.files_mut(tab) {
                    files.cancel_transfer(id);
                }
                // What it held up runs.
                self.next_turn(tab)
            }
            FilesMessage::Retry { tab, id } => self.retry_transfer(tab, id),
            FilesMessage::ClearFinished { tab } => {
                if let Some(files) = self.files_mut(tab) {
                    files.transfers.retain(|transfer| !transfer.state.ended());
                }
                Vec::new()
            }
            // What is left is about a file edited with the external editor.
            message => self.edit_message(message),
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
            FilesMessage::PathCancelled { tab, side } => {
                if let Some(files) = self.files_mut(tab) {
                    match side {
                        Side::Remote => files.remote.typed = None,
                        Side::Local => files.local.typed = None,
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
            FilesMessage::RemoveBookmark { tab, index } => {
                self.remove_bookmark(tab, index);
                Vec::new()
            }
            FilesMessage::Dropped { tab, path } => self.upload_paths(tab, &[path]),
            FilesMessage::ExplorerFilesRead { paths, .. } if paths.is_empty() => {
                self.tell(super::Notice::ExplorerHoldsNoFiles);
                Vec::new()
            }
            FilesMessage::UploadPicked { tab, paths }
            | FilesMessage::ExplorerFilesRead { tab, paths } => self.upload_paths(tab, &paths),
            FilesMessage::Filter { tab, side, text } => {
                if let Some(files) = self.files_mut(tab) {
                    match side {
                        Side::Remote => files.remote.filter_by(text),
                        Side::Local => files.local.filter_by(text),
                    }
                }
                Vec::new()
            }
            FilesMessage::ToggleLocal { tab } => {
                if let Some(files) = self.files_mut(tab) {
                    files.show_local(files.local_hidden);
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
            // Only SFTP changes a server's permissions, as the C#: never asked over FTP.
            FilesMessage::AskPermissions {
                tab,
                side: Side::Remote,
            } if !self.files_over_sftp(tab) => Vec::new(),
            FilesMessage::AskPermissions { tab, side } => {
                self.ask(tab, side, NameAction::Permissions)
            }
            FilesMessage::ToggleSudo { tab } => self.toggle_sudo(tab),
            FilesMessage::ToggleFollow { tab } => self.toggle_follow(tab),
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
    /// "Paste from Explorer": the files copied in Explorer read, while connected.
    fn paste_from_explorer(&self, tab: TabId) -> Vec<Effect> {
        let connected = self
            .tab(tab)
            .and_then(|found| found.files.as_deref())
            .is_some_and(|files| files.client.is_some());
        if connected {
            vec![Effect::ReadExplorerFiles { tab }]
        } else {
            Vec::new()
        }
    }

    /// "Upload here...": the files to upload asked of the user, while connected.
    fn upload_here(&self, tab: TabId) -> Vec<Effect> {
        let connected = self
            .tab(tab)
            .and_then(|found| found.files.as_deref())
            .is_some_and(|files| files.client.is_some());
        if connected {
            vec![Effect::PickUploads { tab }]
        } else {
            Vec::new()
        }
    }

    /// Uploads `paths`, files and folders of this computer, into the server's folder shown,
    /// in one plan: one question for whatever is in the way. What is neither a file nor a
    /// folder is said failed, the others go.
    fn upload_paths(&mut self, tab: TabId, paths: &[PathBuf]) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let Some(client) = files.client.clone() else {
            return Vec::new();
        };
        let mut roots = Vec::new();
        for path in paths {
            let Some(name) = path.file_name() else {
                continue;
            };
            let folder = path.is_dir();
            if !folder && !path.is_file() {
                let label = name.to_string_lossy().into_owned();
                files
                    .transfers
                    .push(failed(Direction::Upload, label, FilesError::NotAFile));
                continue;
            }
            let metadata = path.metadata().ok();
            let size = (!folder)
                .then(|| metadata.as_ref().map(std::fs::Metadata::len))
                .flatten();
            roots.push(PlannedRoot {
                root: Root {
                    remote: files.remote.path.join(&name_bytes(name)),
                    local: path.clone(),
                    kind: if folder { Kind::Folder } else { Kind::File },
                    stamp: heimdall_files::Stamp {
                        size,
                        modified: metadata.and_then(|metadata| metadata.modified().ok()),
                    },
                },
                label: name.to_string_lossy().into_owned(),
                total: size,
            });
        }
        self.queue_transfer(tab, client, Direction::Upload, roots)
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
        self.save_bookmarks(tab);
        self.tell(super::Notice::Bookmarked(shown));
    }

    /// Takes the server's folder bookmarked at `index` off, and says so.
    fn remove_bookmark(&mut self, tab: TabId, index: usize) {
        let Some(files) = self.files_mut(tab) else {
            return;
        };
        if index >= files.bookmarks.len() {
            return;
        }
        let path = files.bookmarks.remove(index);
        self.save_bookmarks(tab);
        self.tell(super::Notice::BookmarkRemoved(crate::text::server_text(
            &path.display(),
        )));
    }

    /// Keeps the bookmarks of `tab`'s server for the next run, as the C# keeps them by
    /// server: shared by every tab on it. A folder whose name is not text stays for the run.
    fn save_bookmarks(&mut self, tab: TabId) {
        let Some(server) = self.files_endpoint(tab) else {
            return;
        };
        let Some(files) = self.tab(tab).and_then(|found| found.files.as_deref()) else {
            return;
        };
        let kept: Vec<String> = files
            .bookmarks
            .iter()
            .filter_map(|path| std::str::from_utf8(path.as_bytes()).ok().map(str::to_owned))
            .collect();
        if let Err(error) = self.files_state.set_bookmarks(&server, kept) {
            log::warn!("the bookmarks were not saved: {error}");
        }
    }

    /// The folder of this computer a Files tab opens on: where the last download went, as
    /// the C# remembers it, while it is still there; the home folder otherwise.
    pub(super) fn files_start(&self) -> PathBuf {
        self.files_state.last_download_folder().map_or_else(
            || self.config.files_start.clone(),
            std::path::Path::to_owned,
        )
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
        files.remote.discard_listing = false;
        vec![super::files_sudo::remote_listing(files, tab, client, path)]
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
    /// `levels` folders up from the one `side` shows, as one move Back undoes; at the root,
    /// no further. The folder reached is listed.
    fn ascend(&mut self, tab: TabId, side: Side, levels: usize) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        if levels > 0 {
            match side {
                Side::Remote => {
                    let target =
                        (0..levels).fold(files.remote.path.clone(), |path, _| path.parent());
                    files.remote.leave();
                    files.remote.path = target;
                }
                Side::Local => {
                    let target = files
                        .local
                        .path
                        .ancestors()
                        .nth(levels)
                        .map(std::path::Path::to_owned);
                    if let Some(target) = target {
                        files.local.leave();
                        files.local.path = target;
                    }
                }
            }
        }
        self.list(tab, side)
    }

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
                files.remote.discard_listing = false;
                vec![super::files_sudo::remote_listing(files, tab, client, path)]
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
                files.local.discard_listing = false;
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
        let holding = self.files_clipboard.is_some();
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        // The lists are hidden behind the integrated editor: their keys do nothing.
        if files.editor.is_some() {
            return Vec::new();
        }
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
            // This computer's pane hidden, the server's keeps the keys.
            FilesKey::SwitchPane | FilesKey::Focus(_) if files.local_hidden || files.local_only => {
                return Vec::new();
            }
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
            // The server's entries only, as the C# clipboard holds them.
            FilesKey::Cut | FilesKey::Copy if side == Side::Local => return Vec::new(),
            FilesKey::Cut => return self.hold_entries(tab, super::ClipMode::Cut),
            FilesKey::Copy => return self.hold_entries(tab, super::ClipMode::Copy),
            // Nothing held: the files copied in Explorer, as the C# Ctrl+V falls back.
            FilesKey::Paste if !holding => {
                return self.paste_from_explorer(tab);
            }
            FilesKey::Paste => return self.paste_held(tab),
            FilesKey::SelectAll => {
                match side {
                    Side::Remote => files.remote.select_all(),
                    Side::Local => files.local.select_all(),
                }
                return Vec::new();
            }
            FilesKey::ExtendPrevious | FilesKey::ExtendNext => {
                let up = key == FilesKey::ExtendPrevious;
                match side {
                    Side::Remote => files.remote.extend_by_one(up),
                    Side::Local => files.local.extend_by_one(up),
                }
                return Vec::new();
            }
            // The entry at the cursor in or out of the selection, as a Ctrl+click on it.
            FilesKey::ToggleMark => {
                let cursor = match side {
                    Side::Remote => files.remote.cursor(),
                    Side::Local => files.local.cursor(),
                };
                if let Some(index) = cursor.or(selected) {
                    match side {
                        Side::Remote => files.remote.toggle(index),
                        Side::Local => files.local.toggle(index),
                    }
                }
                return Vec::new();
            }
            FilesKey::NewFolder => return self.files(FilesMessage::AskNewFolder { tab, side }),
            FilesKey::Download => return self.start_transfer(tab, Direction::Download),
            FilesKey::Upload => return self.start_transfer(tab, Direction::Upload),
            // The window gives the path bar the keyboard.
            FilesKey::FocusPath | FilesKey::Lower => return Vec::new(),
            // Both panes: Escape gives up whatever is still on its way.
            FilesKey::CancelLoad => {
                let remote = files.remote.cancel_listing();
                let local = files.local.cancel_listing();
                if remote || local {
                    self.tell(super::Notice::ListingCancelled);
                }
                return Vec::new();
            }
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
                // A text file is opened in the integrated editor, as the C# opens it.
                if self.opens_in_editor(tab, index) {
                    return self.edit_integrated(tab, true);
                }
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
                // The local file browser has nowhere to send it: the file is opened.
                if files.local_only {
                    return self.open_local_file(tab, index);
                }
                files.local.select_only(Some(index));
                self.start_transfer(tab, Direction::Upload)
            }
        }
    }

    pub(super) fn start_transfer(&mut self, tab: TabId, direction: Direction) -> Vec<Effect> {
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
        // Where downloads go, kept for the next tab to open on.
        if direction == Direction::Download && !chosen.is_empty() {
            let folder = files.local.path.clone();
            if let Err(error) = self.files_state.set_last_download_folder(&folder) {
                log::warn!("the download folder was not kept: {error}");
            }
        }
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let roots: Vec<_> = chosen
            .into_iter()
            .filter_map(|index| {
                let into = (files.local.path.clone(), files.remote.path.clone());
                prepare(files, direction, index, &into)
            })
            .collect();
        self.queue_transfer(tab, client, direction, roots)
    }

    /// Lists the entries picked as waiting, and queues them as one, as the C# job: planned
    /// whole once their turn comes, nothing written before every conflict is answered.
    fn queue_transfer(
        &mut self,
        tab: TabId,
        client: RemoteSession,
        direction: Direction,
        roots: Vec<PlannedRoot>,
    ) -> Vec<Effect> {
        if roots.is_empty() {
            return Vec::new();
        }
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let cancel = CancellationToken::new();
        let rows = roots
            .iter()
            .map(|picked| {
                let id = TransferId::fresh();
                files.transfers.push(Transfer {
                    id,
                    direction,
                    label: picked.label.clone(),
                    bytes: 0,
                    total: picked.total,
                    rate: crate::files::Rate::default(),
                    state: TransferState::Queued,
                    picked: Some(picked.clone()),
                    cancel: cancel.clone(),
                });
                id
            })
            .collect();
        files.queue.push_back(Waiting::Plan(Box::new(PlanRequest {
            client,
            direction,
            roots,
            rows,
            cancel,
        })));
        self.next_turn(tab)
    }

    /// Starts what comes next in `tab`, once nothing runs or is planned there.
    fn next_turn(&mut self, tab: TabId) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        match files.next_waiting() {
            Some(Waiting::Run(id, request)) => vec![Effect::Transfer { tab, id, request }],
            Some(Waiting::Plan(request)) => vec![Effect::PlanTransfer { tab, request }],
            None => Vec::new(),
        }
    }

    /// Whether some entry of `request` is still being planned: none when the user stopped
    /// it meanwhile, or the tab is gone.
    fn plan_is_live(&self, tab: TabId, request: &PlanRequest) -> bool {
        self.tab(tab)
            .and_then(|found| found.files.as_deref())
            .is_some_and(|files| request.rows.iter().any(|id| files.preparing(*id)))
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
        // Stopped while planned: it is left as it is, and what follows runs.
        if !self.plan_is_live(tab, &request) {
            return self.next_turn(tab);
        }
        let plan = match result {
            Ok(plan) => *plan,
            Err(error) => {
                if let Some(files) = self.files_mut(tab) {
                    for transfer in &mut files.transfers {
                        if request.rows.contains(&transfer.id)
                            && transfer.state == TransferState::Preparing
                        {
                            transfer.state = TransferState::Failed(error.clone());
                        }
                    }
                }
                return self.next_turn(tab);
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
        // What was stopped meanwhile is not asked about.
        while self
            .pending_plans
            .front()
            .is_some_and(|next| !self.plan_is_live(next.tab, &next.request))
        {
            self.pending_plans.pop_front();
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
                            incoming: step.stamp,
                            existing: next.plan.existing(index),
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
                // "Replace if newer" is a file's answer: a folder keeps its own.
                let fits = choice != Choice::ReplaceIfNewer || !row.folder;
                if only.is_none_or(|only| only == index) && row.allowed.allows(choice) && fits {
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

    /// The question about the transfer shown was cancelled: that transfer goes nowhere,
    /// listed cancelled to be retried, and what waits behind it runs.
    pub(super) fn cancel_conflicts(&mut self) -> Vec<Effect> {
        let Some(pending) = self.pending_plans.pop_front() else {
            return Vec::new();
        };
        if let Some(files) = self.files_mut(pending.tab) {
            for transfer in &mut files.transfers {
                if pending.request.rows.contains(&transfer.id)
                    && transfer.state == TransferState::Preparing
                {
                    transfer.state = TransferState::Cancelled;
                }
            }
        }
        self.next_turn(pending.tab)
    }

    /// Queues one transfer per picked entry, as answered, ahead of what waits, and starts
    /// the first; an entry skipped whole is taken off the list.
    fn launch_plan(
        &mut self,
        tab: TabId,
        request: &PlanRequest,
        plan: &Plan,
        answers: &[(usize, Choice)],
    ) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let Ok(ready) = plan.resolve(answers) else {
            // Rows are built from the plan's own conflicts, each with an allowed answer; were
            // one refused, the batch stops, to be retried, rather than hold the turn.
            for transfer in &mut files.transfers {
                if request.rows.contains(&transfer.id) && transfer.state == TransferState::Preparing
                {
                    transfer.state = TransferState::Cancelled;
                }
            }
            return self.next_turn(tab);
        };
        let mut runs = Vec::new();
        let mut skipped = Vec::new();
        for (index, ((picked, steps), id)) in request
            .roots
            .iter()
            .zip(ready)
            .zip(&request.rows)
            .enumerate()
        {
            // Stopped while asked about.
            if !files.preparing(*id) {
                continue;
            }
            let transfer = match picked.root.kind {
                Kind::File => {
                    let Some(step) = steps.into_iter().next() else {
                        skipped.push(*id);
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
                        skipped.push(*id);
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
            if let Some(listed) = files.transfers.iter_mut().find(|t| t.id == *id) {
                listed.state = TransferState::Queued;
                listed.cancel = transfer.cancel.clone();
            }
            runs.push(Waiting::Run(*id, Box::new(transfer)));
        }
        files.transfers.retain(|t| !skipped.contains(&t.id));
        // Its turn goes on: its transfers run before what was queued after it.
        for run in runs.into_iter().rev() {
            files.queue.push_front(run);
        }
        self.next_turn(tab)
    }

    /// Runs failed or cancelled transfer `id` again, as the C# Retry: queued anew, its entry
    /// planned again once its turn comes, so what is in the way is asked about again.
    /// While the session lives only.
    fn retry_transfer(&mut self, tab: TabId, id: TransferId) -> Vec<Effect> {
        if !self.tab(tab).is_some_and(super::Tab::is_live) {
            return Vec::new();
        }
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let Some(client) = files.client.clone() else {
            return Vec::new();
        };
        let Some(transfer) = files
            .transfers
            .iter_mut()
            .find(|t| t.id == id && t.state.retryable())
        else {
            return Vec::new();
        };
        let Some(picked) = transfer.picked.clone() else {
            return Vec::new();
        };
        // A new identity: a report still coming from the run stopped is not this one's.
        let again = TransferId::fresh();
        let cancel = CancellationToken::new();
        transfer.id = again;
        transfer.state = TransferState::Queued;
        transfer.bytes = 0;
        transfer.rate = crate::files::Rate::default();
        transfer.cancel = cancel.clone();
        let direction = transfer.direction;
        files.queue.push_back(Waiting::Plan(Box::new(PlanRequest {
            client,
            direction,
            roots: vec![picked],
            rows: vec![again],
            cancel,
        })));
        self.next_turn(tab)
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
                transfer.rate.sample(bytes, std::time::Instant::now());
                Vec::new()
            }
            TransferEvent::Finished(state) => {
                let done = state == TransferState::Done;
                let direction = transfer.direction;
                transfer.state = state;
                // What comes next runs.
                let mut effects = self.next_turn(tab);
                if done {
                    // The destination pane now holds the file.
                    effects.extend(self.list(
                        tab,
                        match direction {
                            Direction::Download => Side::Local,
                            Direction::Upload => Side::Remote,
                        },
                    ));
                }
                effects
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
                    .map(|entry| (entry.label.clone(), pane.path.join(&entry.name)))
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
        // One run at a time in a tab: the one going on is stopped first.
        if self
            .tab(tab)
            .and_then(|tab| tab.files.as_deref())
            .is_some_and(|files| files.batch.is_some())
        {
            return Vec::new();
        }
        if side == Side::Remote && self.files_mut(tab).is_some_and(|files| files.sudo_mode) {
            return self.ask_sudo_delete(tab);
        }
        let chosen = self.chosen(tab, side);
        let Some((name, ..)) = chosen.first().cloned() else {
            return Vec::new();
        };
        let folder = chosen.iter().any(|(.., folder)| *folder);
        let count = chosen.len();
        let targets = chosen
            .into_iter()
            .map(|(name, remote, local, _)| (name, remote, local))
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

    /// Asks to delete the server's chosen entries as root, the sudo mode being on: a danger
    /// question naming them. When one of them is never deleted as root, the pane says so
    /// and nothing is asked.
    fn ask_sudo_delete(&mut self, tab: TabId) -> Vec<Effect> {
        use heimdall_files::privileged_mode::deletable_as_root;

        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let Some(shell) = files.shell.clone() else {
            return Vec::new();
        };
        let access = SudoAccess {
            shell,
            password: files.sudo_password.clone(),
        };
        let pane = &files.remote;
        let home = pane.home.clone();
        let targets: Option<Vec<(String, FileOperation)>> = pane
            .chosen()
            .into_iter()
            .filter_map(|index| pane.entries.get(index))
            .map(|entry| {
                let path = pane.path.join(&entry.name);
                deletable_as_root(path.as_bytes(), home.as_ref().map(RemotePath::as_bytes)).ok()?;
                let removal = FileOperation::RemoteSudoRemove {
                    access: access.clone(),
                    path,
                    kind: entry.kind,
                    inode: entry.inode,
                    home: home.clone(),
                };
                Some((entry.label.clone(), removal))
            })
            .collect();
        let Some(targets) = targets else {
            files.remote.error = Some(FilesError::SudoProtected);
            return Vec::new();
        };
        if targets.is_empty() {
            return Vec::new();
        }
        let names = targets
            .iter()
            .take(SUDO_DELETE_NAMED)
            .map(|(name, _)| name.clone())
            .collect();
        let more = targets.len().saturating_sub(SUDO_DELETE_NAMED);
        self.pending_operation = Some(PendingOperation {
            tab,
            side: Side::Remote,
            kind: PendingKind::SudoDelete { targets },
        });
        self.dialog = Some(Dialog::ConfirmSudoDelete { tab, names, more });
        Vec::new()
    }

    /// The user confirmed a name or a delete.
    pub(super) fn confirm_operation(&mut self, typed: Option<&str>) -> Vec<Effect> {
        let Some(pending) = self.pending_operation.take() else {
            return Vec::new();
        };
        let (tab, side) = (pending.tab, pending.side);
        let Some(client) = self.files_mut(tab).map(|files| files.client.clone()) else {
            return Vec::new();
        };
        let kind = match pending.kind {
            PendingKind::Permissions { remotes } => {
                return self.confirm_permissions(tab, remotes, typed);
            }
            PendingKind::Delete { targets } => return self.confirm_delete(tab, side, targets),
            PendingKind::SudoDelete { targets } => {
                return self.start_batch(tab, Side::Remote, BatchKind::Delete, targets);
            }
            kind => kind,
        };
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
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
            Some(match (side, kind, name) {
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

impl App {
    /// The permission bits `typed` given to `remotes`, one after another; typed wrong, the
    /// pane says so and nothing changes.
    fn confirm_permissions(
        &mut self,
        tab: TabId,
        remotes: Vec<(String, RemotePath)>,
        typed: Option<&str>,
    ) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let mode = match octal_mode(typed.unwrap_or_default()) {
            Ok(mode) => mode,
            Err(error) => {
                files.remote.error = Some(error);
                return Vec::new();
            }
        };
        let Some(client) = files.client.clone() else {
            return Vec::new();
        };
        // The sudo mode on: given as root where the server refuses them to the account.
        let sudo = files
            .shell
            .clone()
            .filter(|_| files.sudo_mode)
            .map(|shell| SudoAccess {
                shell,
                password: files.sudo_password.clone(),
            });
        let entries = remotes
            .into_iter()
            .map(|(name, path)| {
                let change = FileOperation::RemoteSetPermissions {
                    client: client.clone(),
                    path,
                    mode,
                    sudo: sudo.clone(),
                };
                (name, change)
            })
            .collect();
        self.start_batch(tab, Side::Remote, BatchKind::Permissions, entries)
    }

    /// `targets` of `side` deleted, one after another.
    fn confirm_delete(
        &mut self,
        tab: TabId,
        side: Side,
        targets: Vec<(String, RemotePath, PathBuf)>,
    ) -> Vec<Effect> {
        let client = self.files_mut(tab).and_then(|files| files.client.clone());
        let entries = targets
            .into_iter()
            .filter_map(|(name, remote, local)| {
                let removal = match side {
                    Side::Remote => FileOperation::RemoteRemove {
                        client: client.clone()?,
                        path: remote,
                    },
                    Side::Local => FileOperation::LocalRemove { path: local },
                };
                Some((name, removal))
            })
            .collect();
        self.start_batch(tab, side, BatchKind::Delete, entries)
    }

    /// Starts `entries` one after another in `side` of `tab`, as the C# deletes them, the
    /// pane saying which one it is at.
    fn start_batch(
        &mut self,
        tab: TabId,
        side: Side,
        kind: BatchKind,
        entries: Vec<(String, FileOperation)>,
    ) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab).filter(|files| files.batch.is_none()) else {
            return Vec::new();
        };
        let Some((batch, first)) = Batch::start(side, kind, entries) else {
            return Vec::new();
        };
        files.batch = Some(batch);
        vec![Effect::FileBatchStep {
            tab,
            side,
            operation: Box::new(first),
        }]
    }

    /// The entry of `tab`'s run being worked on ended: the next one starts; or, the run
    /// over, its pane is listed again and what did not go is said.
    fn batch_step_done(&mut self, tab: TabId, result: Result<(), FilesError>) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let Some(batch) = files.batch.as_mut() else {
            return Vec::new();
        };
        let side = batch.side;
        if let Some(next) = batch.step(result) {
            return vec![Effect::FileBatchStep {
                tab,
                side,
                operation: Box::new(next),
            }];
        }
        let outcome = files.batch.take().and_then(|batch| batch.outcome());
        if let Some(outcome) = outcome {
            self.tell(super::Notice::FilesBatch(outcome));
        }
        self.list(tab, side)
    }

    /// Stops `tab`'s run after the entry being worked on.
    fn stop_batch(&mut self, tab: TabId) -> Vec<Effect> {
        if let Some(batch) = self.files_mut(tab).and_then(|files| files.batch.as_mut()) {
            batch.stopping = true;
        }
        Vec::new()
    }
}

/// The selected entry at `index` as a transfer's picked entry. A refusal is recorded as a
/// failed transfer.
fn prepare(
    files: &mut FilesPane,
    direction: Direction,
    index: usize,
    (local_dir, remote_dir): &(PathBuf, RemotePath),
) -> Option<PlannedRoot> {
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
                    local: local_dir.join(&name.name),
                    kind: kind(entry.kind),
                    stamp: heimdall_files::Stamp {
                        size: entry.size,
                        modified: entry.modified,
                    },
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
                    remote: remote_dir.join(&name_bytes(&entry.name)),
                    local: files.local.path.join(&entry.name),
                    kind: kind(entry.kind),
                    stamp: heimdall_files::Stamp {
                        size: entry.size,
                        modified: entry.modified,
                    },
                },
                label,
                // A folder's own size is not what its transfer moves.
                total: entry.size.filter(|_| entry.kind != EntryKind::Directory),
            }
        }
    })
}

impl App {
    /// The entries chosen in `from` dropped on `onto`, as the C# Files tab takes a drop: on
    /// the other pane, sent there, into the folder entry dropped on or the folder it shows;
    /// on a folder entry of their own pane, moved into it. Dropped where they are, nothing.
    fn drop_entries(
        &mut self,
        tab: TabId,
        from: Side,
        onto: Side,
        into: Option<usize>,
    ) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let chosen = match from {
            Side::Remote => files.remote.chosen(),
            Side::Local => files.local.chosen(),
        };
        // Only a folder takes a drop: on a file, the drop is on the folder shown.
        let into = into.filter(|index| match onto {
            Side::Remote => files
                .remote
                .entries
                .get(*index)
                .is_some_and(|entry| entry.kind == EntryKind::Directory),
            Side::Local => files
                .local
                .entries
                .get(*index)
                .is_some_and(|entry| entry.kind == EntryKind::Directory),
        });
        if chosen.is_empty() {
            return Vec::new();
        }
        if from == onto {
            let Some(folder) = into.filter(|index| !chosen.contains(index)) else {
                return Vec::new();
            };
            return self.move_into(tab, from, &chosen, folder);
        }
        let destination = (
            match (onto, into) {
                (Side::Local, Some(index)) => {
                    files.local.path.join(&files.local.entries[index].name)
                }
                _ => files.local.path.clone(),
            },
            match (onto, into) {
                (Side::Remote, Some(index)) => {
                    files.remote.path.join(&files.remote.entries[index].name)
                }
                _ => files.remote.path.clone(),
            },
        );
        let Some(client) = files.client.clone() else {
            return Vec::new();
        };
        let direction = match from {
            Side::Remote => Direction::Download,
            Side::Local => Direction::Upload,
        };
        let roots: Vec<_> = chosen
            .into_iter()
            .filter_map(|index| prepare(files, direction, index, &destination))
            .collect();
        self.queue_transfer(tab, client, direction, roots)
    }

    /// The entries `chosen` of `side` moved into its folder entry `folder`: renamed on the
    /// server, moved on this computer.
    fn move_into(
        &mut self,
        tab: TabId,
        side: Side,
        chosen: &[usize],
        folder: usize,
    ) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        match side {
            Side::Remote => {
                let Some(client) = files.client.clone() else {
                    return Vec::new();
                };
                let pane = &files.remote;
                let target = pane.path.join(&pane.entries[folder].name);
                let moves = chosen
                    .iter()
                    .filter_map(|index| pane.entries.get(*index))
                    .map(|entry| (pane.path.join(&entry.name), target.join(&entry.name)))
                    .collect();
                vec![Effect::MoveRemote { tab, client, moves }]
            }
            Side::Local => {
                let pane = &files.local;
                let target = pane.path.join(&pane.entries[folder].name);
                chosen
                    .iter()
                    .filter_map(|index| pane.entries.get(*index))
                    .map(|entry| Effect::FileOperation {
                        tab,
                        side,
                        operation: Box::new(FileOperation::LocalRename {
                            from: pane.path.join(&entry.name),
                            to: target.join(&entry.name),
                        }),
                    })
                    .collect()
            }
        }
    }
}

/// A destination's names below the destination folder, joined as a path.
fn target_text(target: &[Vec<u8>]) -> String {
    target
        .iter()
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// A transfer refused before anything was planned: nothing for Retry to plan again.
fn failed(direction: Direction, label: String, error: FilesError) -> Transfer {
    Transfer {
        id: TransferId::fresh(),
        direction,
        label,
        bytes: 0,
        total: None,
        rate: crate::files::Rate::default(),
        state: TransferState::Failed(error),
        picked: None,
        cancel: CancellationToken::new(),
    }
}
