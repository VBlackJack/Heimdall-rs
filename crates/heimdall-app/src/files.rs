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

//! The Files tab: a local and a remote pane, and file transfers between them.
//!
//! State and decisions live here and in [`crate::App`]; the asynchronous work (listing a
//! folder, running a transfer) is in the functions at the end, which the UI layer runs for
//! the effects the application asks for.

use std::collections::{BTreeSet, VecDeque};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use heimdall_files::{
    ItemKind, LocalName, LocalNameError, Plan, Ready, Refusal, RemoteError, RemoteItem, RemotePath,
    RemoteSession, Root, Rules, display_bytes,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use crate::text::server_text;

/// Shortest interval between two progress events of one transfer.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Transfer events buffered before the transfer waits for the UI.
const TRANSFER_QUEUE_LENGTH: usize = 16;

/// Identifies a transfer for the life of the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransferId(u64);

impl TransferId {
    /// A new identifier, never used before.
    #[must_use]
    pub fn fresh() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    /// The number, for logs.
    #[must_use]
    pub fn value(self) -> u64 {
        self.0
    }
}

/// One of the two panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// This machine.
    Local,
    /// The server.
    Remote,
}

impl Side {
    /// The other pane.
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::Local => Self::Remote,
            Self::Remote => Self::Local,
        }
    }
}

/// What a key does in a Files tab; the pane it acts on is the one with the focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesKey {
    /// Select the entry above.
    Previous,
    /// Select the entry below.
    Next,
    /// Select the first entry.
    First,
    /// Select the last entry.
    Last,
    /// Open the selected folder, or send the selected file to the other side.
    Open,
    /// Show the parent folder.
    Parent,
    /// Show the folder shown before, as the C# Files tab's Backspace.
    Back,
    /// Give the focus to the other pane.
    SwitchPane,
    /// Give the focus to a pane.
    Focus(Side),
    /// Ask for a new name for the selected entry.
    Rename,
    /// Ask to delete the selected entry.
    Delete,
    /// List the folder again.
    Refresh,
    /// Copy the full path of the selected entry, as the C# Files tab's Ctrl+Shift+C.
    CopyPath,
    /// Cut the server's entries chosen, as the C# Ctrl+X.
    Cut,
    /// Copy the server's entries chosen, as the C# Ctrl+C.
    Copy,
    /// Paste what is held, or else the files copied in Explorer, as the C# Ctrl+V.
    Paste,
    /// Select every entry of the pane, Ctrl+A.
    SelectAll,
    /// Ask for a new folder's name, as the C# F7.
    NewFolder,
    /// Download the server's entries chosen, as the C# Ctrl+Shift+D.
    Download,
    /// Upload this computer's entries chosen, as the C# Ctrl+Shift+U.
    Upload,
    /// Type in the pane's path bar, as the C# Alt+D and F4: the window's to do.
    FocusPath,
}

/// What an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryKind {
    /// A folder.
    Directory,
    /// A regular file.
    File,
    /// A symbolic link, not followed.
    Link,
    /// A device, socket, pipe or unknown.
    Other,
}

/// An entry of the remote pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    /// The server's bytes.
    pub name: Vec<u8>,
    /// Text to show, made safe.
    pub label: String,
    /// Kind.
    pub kind: EntryKind,
    /// Size in bytes.
    pub size: Option<u64>,
    /// Modification time.
    pub modified: Option<SystemTime>,
    /// Permission bits.
    pub permissions: Option<u32>,
    /// The owner's user number.
    pub owner: Option<u32>,
    /// The group's number.
    pub group: Option<u32>,
}

impl RemoteEntry {
    /// The entry for a listed item.
    #[must_use]
    pub fn from_listing(item: RemoteItem) -> Self {
        Self {
            label: server_text(&display_bytes(&item.name)),
            name: item.name,
            kind: match item.kind {
                ItemKind::Directory => EntryKind::Directory,
                ItemKind::File => EntryKind::File,
                ItemKind::Link => EntryKind::Link,
                ItemKind::Other => EntryKind::Other,
            },
            size: item.size,
            modified: item.modified,
            permissions: item.permissions,
            owner: item.owner,
            group: item.group,
        }
    }
}

/// An entry of the local pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEntry {
    /// The name.
    pub name: OsString,
    /// Text to show.
    pub label: String,
    /// Kind.
    pub kind: EntryKind,
    /// Size in bytes.
    pub size: Option<u64>,
    /// Modification time.
    pub modified: Option<SystemTime>,
}

/// A column a pane's entries are sorted by, as the C# Files tab's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortColumn {
    /// The name, whatever its case.
    #[default]
    Name,
    /// The size.
    Size,
    /// The modification time.
    Modified,
    /// The permission bits.
    Permissions,
    /// The owner.
    Owner,
}

/// How a pane's entries are sorted: folders first always, then by a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sort {
    /// The column.
    pub column: SortColumn,
    /// Largest, latest or last first.
    pub descending: bool,
}

impl Sort {
    /// The sort a click on `column`'s header asks for: the other way on the column
    /// sorted by, else that column from the smallest, as the C# header does.
    #[must_use]
    pub fn clicked(self, column: SortColumn) -> Self {
        Self {
            column,
            descending: self.column == column && !self.descending,
        }
    }
}

/// What a pane's entries are sorted by.
pub trait Listed {
    /// Kind.
    fn kind(&self) -> EntryKind;
    /// Text shown.
    fn label(&self) -> &str;
    /// Size.
    fn size(&self) -> Option<u64>;
    /// Modification time.
    fn modified(&self) -> Option<SystemTime>;
    /// Permission bits.
    fn permissions(&self) -> Option<u32> {
        None
    }
    /// Owner.
    fn owner(&self) -> Option<u32> {
        None
    }
}

impl Listed for RemoteEntry {
    fn kind(&self) -> EntryKind {
        self.kind
    }
    fn label(&self) -> &str {
        &self.label
    }
    fn size(&self) -> Option<u64> {
        self.size
    }
    fn modified(&self) -> Option<SystemTime> {
        self.modified
    }
    fn permissions(&self) -> Option<u32> {
        self.permissions
    }
    fn owner(&self) -> Option<u32> {
        self.owner
    }
}

impl Listed for LocalEntry {
    fn kind(&self) -> EntryKind {
        self.kind
    }
    fn label(&self) -> &str {
        &self.label
    }
    fn size(&self) -> Option<u64> {
        self.size
    }
    fn modified(&self) -> Option<SystemTime> {
        self.modified
    }
}

/// Sorts `entries` by `sort`, folders first always; entries equal by the column keep the
/// order of their names.
pub fn sort_entries<E: Listed>(entries: &mut [E], sort: Sort) {
    entries.sort_by(|a, b| {
        let folders = (a.kind() != EntryKind::Directory).cmp(&(b.kind() != EntryKind::Directory));
        let by_column = match sort.column {
            SortColumn::Name => a.label().to_lowercase().cmp(&b.label().to_lowercase()),
            SortColumn::Size => a.size().cmp(&b.size()),
            SortColumn::Modified => a.modified().cmp(&b.modified()),
            SortColumn::Permissions => a.permissions().cmp(&b.permissions()),
            SortColumn::Owner => a.owner().cmp(&b.owner()),
        };
        let by_column = if sort.descending {
            by_column.reverse()
        } else {
            by_column
        };
        folders
            .then(by_column)
            .then_with(|| a.label().to_lowercase().cmp(&b.label().to_lowercase()))
    });
}

/// `mode`'s permission bits as the C# column shows them, `rwxr-xr-x`, the set-user,
/// set-group and sticky bits in place of the execute ones.
#[must_use]
pub fn symbolic_mode(mode: u32) -> String {
    let bit = |mask: u32, set: char| if mode & mask == 0 { '-' } else { set };
    let special = |execute: u32, flag: u32, lower: char, upper: char| match (
        mode & execute != 0,
        mode & flag != 0,
    ) {
        (true, true) => lower,
        (false, true) => upper,
        (true, false) => 'x',
        (false, false) => '-',
    };
    [
        bit(0o400, 'r'),
        bit(0o200, 'w'),
        special(0o100, 0o4000, 's', 'S'),
        bit(0o040, 'r'),
        bit(0o020, 'w'),
        special(0o010, 0o2000, 's', 'S'),
        bit(0o004, 'r'),
        bit(0o002, 'w'),
        special(0o001, 0o1000, 't', 'T'),
    ]
    .into_iter()
    .collect()
}

/// A pane: where it is, what it lists, what is selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane<P, E> {
    /// Folder shown.
    pub path: P,
    /// Every entry listed, sorted.
    pub listing: Vec<E>,
    /// The entries shown: those the filter and the hidden-files toggle keep, sorted.
    pub entries: Vec<E>,
    /// Text a name must hold to be shown, whatever its case, as the C# filter.
    pub filter: String,
    /// Names starting with a dot are shown, as by default in the C# tab.
    pub show_hidden: bool,
    /// Selected entry: the one a key or a single entry's action applies to.
    pub selected: Option<usize>,
    /// The entries selected with it, with Ctrl or Shift, as in the C# Files tab.
    pub marked: BTreeSet<usize>,
    /// A listing is on its way.
    pub loading: bool,
    /// Why the last listing failed.
    pub error: Option<FilesError>,
    /// A folder typed in its path bar, not gone to yet.
    pub typed: Option<String>,
    /// How its entries are sorted.
    pub sort: Sort,
    /// A link being entered: the folder it was opened from, and its name. Its listing tells
    /// whether it points at a folder; when it does not, the pane goes back.
    pub entering_link: Option<(P, String)>,
    /// The folders left, the most recent last, as the C# Files tab's history: Back goes to
    /// the last one. At most [`HISTORY_MAX`].
    pub history: Vec<P>,
    /// The folder first shown, Home's: the server's home folder, or the local one the tab
    /// opened in.
    pub home: Option<P>,
    /// How the listing on its way was asked for.
    navigation: Option<Navigation<P>>,
}

/// Folders a pane's history keeps; older ones are forgotten.
pub const HISTORY_MAX: usize = 100;

/// How a pane's listing was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Navigation<P> {
    /// To another folder, leaving this one: kept in the history once the other is shown.
    Away(P),
    /// Back to the last folder of the history: taken out of it once shown.
    Back,
}

impl<P: Clone + PartialEq, E> Pane<P, E> {
    /// The folder shown is about to be left for another, by `path` or by a listing: kept in
    /// the history once the other is shown.
    pub(crate) fn leave(&mut self) {
        self.navigation = Some(Navigation::Away(self.path.clone()));
    }

    /// Goes back to the last folder of the history, taken out of it once shown; `false`
    /// when there is none.
    pub(crate) fn back(&mut self) -> bool {
        let Some(previous) = self.history.last().cloned() else {
            return false;
        };
        self.path = previous;
        self.navigation = Some(Navigation::Back);
        true
    }

    /// Goes to Home; `false` when no folder was shown yet.
    pub(crate) fn go_home(&mut self) -> bool {
        let Some(home) = self.home.clone() else {
            return false;
        };
        self.leave();
        self.path = home;
        true
    }

    /// `path` was listed: the history follows how it was asked for, and the first folder
    /// shown is Home.
    pub(crate) fn arrived(&mut self, path: &P) {
        match self.navigation.take() {
            Some(Navigation::Away(left)) if left != *path => {
                self.history.push(left);
                if self.history.len() > HISTORY_MAX {
                    self.history.remove(0);
                }
            }
            Some(Navigation::Back) => {
                self.history.pop();
            }
            _ => {}
        }
        if self.home.is_none() {
            self.home = Some(path.clone());
        }
    }

    /// The listing on its way failed: the history stays as it was.
    pub(crate) fn not_arrived(&mut self) {
        self.navigation = None;
    }
}

impl<P, E> Pane<P, E> {
    fn new(path: P) -> Self {
        Self {
            path,
            listing: Vec::new(),
            entries: Vec::new(),
            filter: String::new(),
            show_hidden: true,
            selected: None,
            marked: BTreeSet::new(),
            loading: true,
            error: None,
            typed: None,
            sort: Sort::default(),
            entering_link: None,
            history: Vec::new(),
            home: None,
            navigation: None,
        }
    }

    /// Whether Back has a folder to go to.
    #[must_use]
    pub fn can_go_back(&self) -> bool {
        !self.history.is_empty()
    }

    /// Selects `index` alone, or nothing.
    pub fn select_only(&mut self, index: Option<usize>) {
        self.selected = index;
        self.marked.clear();
    }

    /// The entries selected, the one selected and those with it, in their order.
    #[must_use]
    pub fn chosen(&self) -> Vec<usize> {
        let mut chosen: BTreeSet<usize> = self.marked.clone();
        chosen.extend(self.selected);
        chosen
            .into_iter()
            .filter(|index| *index < self.entries.len())
            .collect()
    }

    /// Ctrl+A: every entry chosen, the one selected staying where it was, or the first.
    pub fn select_all(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let anchor = self.selected.unwrap_or(0);
        self.selected = Some(anchor);
        self.marked = (0..self.entries.len()).filter(|at| *at != anchor).collect();
    }

    /// Ctrl+click on `index`: selected with the others, or no longer.
    pub fn toggle(&mut self, index: usize) {
        if self.selected == Some(index) {
            self.selected = self.marked.pop_first();
        } else if !self.marked.remove(&index) {
            self.marked.extend(self.selected);
            self.selected = Some(index);
        }
    }

    /// Shift+click on `index`: every entry from the one selected to it, the one selected
    /// staying where the range started.
    pub fn extend_to(&mut self, index: usize) {
        let Some(anchor) = self.selected else {
            self.select_only(Some(index));
            return;
        };
        self.marked = (anchor.min(index)..=anchor.max(index))
            .filter(|at| *at != anchor)
            .collect();
    }
}

impl<P, E: Listed + PartialEq + Clone> Pane<P, E> {
    /// Shows `entries`, a new listing, sorted and filtered as the pane is.
    pub fn show(&mut self, entries: Vec<E>) {
        self.listing = entries;
        sort_entries(&mut self.listing, self.sort);
        self.select_only(None);
        self.refresh();
    }

    /// Sorts the entries by `sort`; the entries selected stay selected.
    pub fn sort_by(&mut self, sort: Sort) {
        self.sort = sort;
        sort_entries(&mut self.listing, sort);
        self.refresh();
    }

    /// Shows only the entries whose name holds `text`, whatever its case.
    pub fn filter_by(&mut self, text: String) {
        self.filter = text;
        self.refresh();
    }

    /// Shows the names starting with a dot, or no longer.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.refresh();
    }

    /// Whether `entry` is shown.
    fn shows(&self, entry: &E) -> bool {
        let wanted = self.filter.trim().to_lowercase();
        (self.show_hidden || !entry.label().starts_with('.'))
            && entry.label().to_lowercase().contains(&wanted)
    }

    /// The entries shown again from the listing; those selected and still shown stay
    /// selected.
    fn refresh(&mut self) {
        let entry = |index: &usize| self.entries.get(*index).cloned();
        let chosen = self.selected.as_ref().and_then(entry);
        let marked: Vec<E> = self.marked.iter().filter_map(entry).collect();
        let shown: Vec<E> = self
            .listing
            .iter()
            .filter(|entry| self.shows(entry))
            .cloned()
            .collect();
        self.entries = shown;
        let place = |wanted: &E| self.entries.iter().position(|e| e == wanted);
        self.selected = chosen.as_ref().and_then(place);
        self.marked = marked.iter().filter_map(place).collect();
        if self.selected.is_none() {
            // The one selected hidden: another selected takes its place.
            self.selected = self.marked.pop_first();
        }
    }
}

/// The remote pane.
pub type RemotePane = Pane<RemotePath, RemoteEntry>;

/// The local pane.
pub type LocalPane = Pane<PathBuf, LocalEntry>;

/// Which way a transfer goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Server to this machine.
    Download,
    /// This machine to the server.
    Upload,
}

/// Where a transfer stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferState {
    /// Waiting for the transfer before it in this tab to end: one runs at a time, as the
    /// C# transfer queue.
    Queued,
    /// Its turn came: what it would write is checked against what is there, and asked
    /// about, before it runs.
    Preparing,
    /// Running.
    Running,
    /// Complete.
    Done,
    /// A folder transfer finished with entries left out (links, unusable names, failures).
    Incomplete {
        /// Entries left out.
        skipped: usize,
    },
    /// Stopped by the user; a download can be resumed by starting it again.
    Cancelled,
    /// Failed.
    Failed(FilesError),
}

impl TransferState {
    /// Whether it has ended, whatever the outcome.
    #[must_use]
    pub fn ended(&self) -> bool {
        !matches!(self, Self::Queued | Self::Preparing | Self::Running)
    }

    /// Whether Retry runs it again: it failed or was stopped, as the C# `CanRetry`.
    #[must_use]
    pub fn retryable(&self) -> bool {
        matches!(self, Self::Cancelled | Self::Failed(_))
    }
}

/// How often a transfer's speed is measured at most, as the C# `TransferProgressTracker`.
const RATE_SAMPLE: Duration = Duration::from_millis(250);

/// How much a new measure counts against the speed so far, as the C#.
const RATE_SMOOTHING: f64 = 0.3;

/// How fast a transfer goes, measured as the C# `TransferProgressTracker` measures it: at
/// most every quarter second, each measure smoothed into the speed so far.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rate {
    /// The bytes done at the last measure, and when.
    last: Option<(u64, Instant)>,
    bytes_per_second: f64,
}

impl Rate {
    /// `bytes` were done at `now`.
    pub fn sample(&mut self, bytes: u64, now: Instant) {
        let Some((before, then)) = self.last else {
            self.last = Some((bytes, now));
            return;
        };
        let elapsed = now.saturating_duration_since(then);
        if elapsed < RATE_SAMPLE {
            return;
        }
        #[expect(clippy::cast_precision_loss, reason = "a speed shown to three digits")]
        let measured = bytes.saturating_sub(before) as f64 / elapsed.as_secs_f64();
        self.bytes_per_second = if self.bytes_per_second <= 0.0 {
            measured
        } else {
            RATE_SMOOTHING * measured + (1.0 - RATE_SMOOTHING) * self.bytes_per_second
        };
        self.last = Some((bytes, now));
    }

    /// Bytes a second, once measured.
    #[must_use]
    pub fn bytes_per_second(&self) -> Option<f64> {
        (self.bytes_per_second > 0.0).then_some(self.bytes_per_second)
    }

    /// The time left to go from `done` to `total` at this speed; `None` until measured, or
    /// when the size is not known.
    #[must_use]
    pub fn remaining(&self, done: u64, total: Option<u64>) -> Option<Duration> {
        let speed = self.bytes_per_second()?;
        #[expect(clippy::cast_precision_loss, reason = "a time shown to the second")]
        let left = total?.saturating_sub(done) as f64;
        Duration::try_from_secs_f64(left / speed).ok()
    }
}

/// A transfer shown in the tab.
#[derive(Debug, Clone)]
pub struct Transfer {
    /// Identifier.
    pub id: TransferId,
    /// Direction.
    pub direction: Direction,
    /// The file's name, made safe.
    pub label: String,
    /// Bytes done.
    pub bytes: u64,
    /// Size, when known.
    pub total: Option<u64>,
    /// How fast it goes.
    pub rate: Rate,
    /// State.
    pub state: TransferState,
    /// The entry picked, planned again by Retry; none for what could not be picked at all.
    pub picked: Option<PlannedRoot>,
    pub(crate) cancel: CancellationToken,
}

/// Why a Files operation failed; shown in the user's language by the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesError {
    /// The server refused, with its own message (made safe).
    Server {
        /// What was refused.
        refusal: Refusal,
        /// The server's message, made safe; may be empty.
        message: String,
    },
    /// The session with the server is over.
    SessionClosed,
    /// The transfer stopped short, for no reason it could tell.
    Interrupted,
    /// The file looks like a program, an image or an archive: not opened as text.
    LooksBinary,
    /// The file is larger than the integrated editor opens: the external editor takes it.
    TooLargeForEditor,
    /// The file's byte order mark names an encoding its bytes do not follow.
    NotText,
    /// The local file system failed.
    Local {
        /// Operating system message.
        detail: String,
    },
    /// The server's name cannot be a local file name.
    UnsafeName {
        /// The name, made safe.
        name: String,
        /// Why.
        reason: LocalNameError,
    },
    /// Not a regular file or a folder: a link, a device.
    NotAFile,
    /// A symbolic link: its permissions are not changed, the server would change those of
    /// what it points to.
    IsLink,
    /// The file an upload would replace is not a regular file; it was left as it is.
    DestinationNotAFile,
    /// The file an upload would replace was left as it is.
    ReplaceNotSafe,
    /// A folder holds more entries than a transfer or a delete walks.
    TooLarge,
    /// The name typed cannot be used.
    InvalidName,
    /// An entry of that name exists already.
    Exists,
    /// The permissions typed are not an octal mode, 755 or 4755.
    InvalidPermissions,
    /// The server did not carry out a server-side copy: it runs no POSIX shell with GNU
    /// tools for this account, or the copy failed there. Nothing is copied any other way.
    CopyRefused,
    /// The file changed on the server since it was opened: it was left as it is.
    ChangedOnServer,
    /// The file is larger than what is edited: it is downloaded instead.
    FileTooLarge,
    /// The folder a file is edited in could not be kept the user's own: the file was not
    /// opened, as the C# refuses it.
    WorkingFolderUnprotected,
    /// The external editor could not be started.
    EditorFailed {
        /// What the system said, or the path set.
        detail: String,
    },
    /// The external editor set is a shell, a script host or an interpreter: it would run
    /// the file, not show it.
    EditorRunsFiles,
    /// sudo asks for a password and none was given.
    SudoPasswordNeeded,
    /// sudo refused the password.
    SudoPasswordRejected,
    /// sudo on the server wants a terminal (`requiretty`), which is not given to it.
    SudoNeedsTerminal,
    /// The `sudo` found on the server is not the system's: nothing ran as root.
    SudoUntrusted,
    /// The server lacks a GNU coreutils tool the privileged script needs.
    SudoToolingMissing,
    /// sudo did not do it, for a reason it gave in the log.
    SudoFailed,
    /// A folder pasted into itself or one of its own folders.
    PasteIntoItself {
        /// The folder's name, made safe.
        name: String,
    },
}

impl From<&RemoteError> for FilesError {
    fn from(error: &RemoteError) -> Self {
        match error {
            RemoteError::Refused { refusal, message } => Self::Server {
                refusal: *refusal,
                message: server_text(&String::from_utf8_lossy(message)),
            },
            RemoteError::Local { detail } => Self::Local {
                detail: detail.clone(),
            },
            RemoteError::NotAFile => Self::NotAFile,
            RemoteError::IsLink => Self::IsLink,
            RemoteError::LocalExists => Self::Exists,
            RemoteError::DestinationNotAFile => Self::DestinationNotAFile,
            RemoteError::ReplaceNotSafe => Self::ReplaceNotSafe,
            RemoteError::Changed => Self::ChangedOnServer,
            RemoteError::FileTooLarge => Self::FileTooLarge,
            RemoteError::TooLarge => Self::TooLarge,
            // A cancel is a state of the transfer, not a failure: callers handle it first.
            RemoteError::SessionClosed | RemoteError::Cancelled => Self::SessionClosed,
        }
    }
}

/// The Files tab's state.
#[derive(Debug)]
pub struct FilesPane {
    /// The session with the server, once open.
    pub client: Option<RemoteSession>,
    /// The SSH connection under an SFTP session, to run commands on the server; none for
    /// FTP.
    pub shell: Option<heimdall_ssh::Connection>,
    /// Server side.
    pub remote: RemotePane,
    /// Local side.
    pub local: LocalPane,
    /// Transfers, oldest first.
    pub transfers: Vec<Transfer>,
    /// What waits its turn, in order.
    pub(crate) queue: VecDeque<Waiting>,
    /// The pane keys act on: the last one clicked or chosen.
    pub focus: Side,
    /// The server's folders bookmarked in this tab, in the order they were, as the C#
    /// Files tab keeps them: for the session.
    pub bookmarks: Vec<RemotePath>,
    /// What stops the server-side copy running, while one runs: one at a time.
    pub copying: Option<CancellationToken>,
    /// The server's files being edited with an external editor; kept while the tab is
    /// open, connected or not.
    pub edits: Vec<crate::external_edit::EditSession>,
    /// A look at the files being edited runs: one at a time.
    pub checking_edits: bool,
    /// The password sudo took for this tab, kept until the tab's session ends or sudo
    /// refuses it, as the user chose; never shown, wiped when dropped.
    pub sudo_password: Option<crate::sudo_edit::SudoPassword>,
    /// The server's file open in the integrated editor, shown in place of the lists: one
    /// at a time, as the C#.
    pub editor: Option<crate::integrated_edit::IntegratedEdit>,
}

impl FilesPane {
    /// A pane starting locally in `local`, remotely in the server's starting folder.
    #[must_use]
    pub fn new(local: PathBuf) -> Self {
        Self {
            client: None,
            shell: None,
            remote: Pane::new(RemotePath::from(".")),
            local: Pane::new(local),
            transfers: Vec::new(),
            queue: VecDeque::new(),
            focus: Side::Local,
            bookmarks: Vec::new(),
            copying: None,
            edits: Vec::new(),
            checking_edits: false,
            sudo_password: None,
            editor: None,
        }
    }

    /// The selected entry of the focused pane, and how many entries it holds.
    #[must_use]
    pub fn focused(&self) -> (Option<usize>, usize) {
        match self.focus {
            Side::Remote => (self.remote.selected, self.remote.entries.len()),
            Side::Local => (self.local.selected, self.local.entries.len()),
        }
    }

    /// Cancels every running transfer and drops the session.
    pub(crate) fn stop(&mut self) {
        self.stop_transfers();
        if let Some(copying) = self.copying.take() {
            copying.cancel();
        }
        self.client = None;
        self.shell = None;
        self.sudo_password = None;
    }

    /// Transfers still running, waiting their turn or prepared again.
    #[must_use]
    pub fn running(&self) -> usize {
        self.transfers
            .iter()
            .filter(|transfer| !transfer.state.ended())
            .count()
    }

    /// Stops every transfer: those not ended are said cancelled, so Retry can run them.
    fn stop_transfers(&mut self) {
        self.queue.clear();
        for transfer in &mut self.transfers {
            transfer.cancel.cancel();
            if !transfer.state.ended() {
                transfer.state = TransferState::Cancelled;
            }
        }
    }

    /// The transfers listed, every one stopped, for the pane of the connection opened again
    /// in this one's place: Retry runs them there.
    pub(crate) fn hand_over_transfers(&mut self) -> Vec<Transfer> {
        self.stop_transfers();
        std::mem::take(&mut self.transfers)
    }

    /// Stops what waits, the session being over: what is planned stops, what waits is said
    /// cancelled, to be retried; what runs ends by itself.
    pub(crate) fn cancel_waiting(&mut self) {
        self.queue.clear();
        for transfer in &mut self.transfers {
            if matches!(
                transfer.state,
                TransferState::Queued | TransferState::Preparing
            ) {
                transfer.cancel.cancel();
                transfer.state = TransferState::Cancelled;
            }
        }
    }

    /// Stops transfer `id`: a waiting one never starts, one being planned stops with the
    /// entries planned with it, as the C# job, and a running one stops where it stands.
    pub(crate) fn cancel_transfer(&mut self, id: TransferId) {
        let Some(transfer) = self.transfers.iter_mut().find(|t| t.id == id) else {
            return;
        };
        match transfer.state {
            TransferState::Queued => {
                transfer.state = TransferState::Cancelled;
                self.queue
                    .retain(|waiting| !matches!(waiting, Waiting::Run(queued, _) if *queued == id));
            }
            TransferState::Preparing => {
                transfer.cancel.cancel();
                // One plan at a time: every entry being planned is this one's.
                for transfer in &mut self.transfers {
                    if transfer.state == TransferState::Preparing {
                        transfer.state = TransferState::Cancelled;
                    }
                }
            }
            _ => transfer.cancel.cancel(),
        }
    }

    /// Whether `id` is listed and being planned.
    pub(crate) fn preparing(&self, id: TransferId) -> bool {
        self.transfers
            .iter()
            .any(|transfer| transfer.id == id && transfer.state == TransferState::Preparing)
    }

    /// What comes next, once nothing runs or is planned: a transfer to run, said running, or
    /// entries to plan, said being planned. What was cancelled while waiting is left out.
    pub(crate) fn next_waiting(&mut self) -> Option<Waiting> {
        if self.transfers.iter().any(|transfer| {
            matches!(
                transfer.state,
                TransferState::Running | TransferState::Preparing
            )
        }) {
            return None;
        }
        while let Some(waiting) = self.queue.pop_front() {
            match waiting {
                Waiting::Run(id, request) => {
                    if let Some(transfer) = self
                        .transfers
                        .iter_mut()
                        .find(|t| t.id == id && t.state == TransferState::Queued)
                    {
                        transfer.state = TransferState::Running;
                        return Some(Waiting::Run(id, request));
                    }
                }
                Waiting::Plan(mut request) => {
                    let mut roots = Vec::new();
                    let mut rows = Vec::new();
                    for (mut picked, id) in request.roots.drain(..).zip(request.rows.drain(..)) {
                        let Some(transfer) = self
                            .transfers
                            .iter_mut()
                            .find(|t| t.id == id && t.state == TransferState::Queued)
                        else {
                            continue;
                        };
                        if request.direction == Direction::Upload {
                            picked.read_again();
                            transfer.total = picked.total;
                        }
                        transfer.state = TransferState::Preparing;
                        transfer.cancel = request.cancel.clone();
                        roots.push(picked);
                        rows.push(id);
                    }
                    if !rows.is_empty() {
                        request.roots = roots;
                        request.rows = rows;
                        return Some(Waiting::Plan(request));
                    }
                }
            }
        }
        None
    }
}

/// What waits its turn in a Files tab, one thing at a time, as the C# transfer queue.
#[derive(Debug)]
pub enum Waiting {
    /// Entries picked, planned and asked about once their turn comes, as the C# job: what
    /// is in the way is what is there then.
    Plan(Box<PlanRequest>),
    /// One entry planned and answered, to run.
    Run(TransferId, Box<TransferRequest>),
}

/// Folders first, then by name, ignoring case.
pub fn sort_remote(entries: &mut [RemoteEntry]) {
    sort_entries(entries, Sort::default());
}

/// Folders first, then by name, ignoring case.
pub fn sort_local(entries: &mut [LocalEntry]) {
    sort_entries(entries, Sort::default());
}

/// The local name a download of `remote_name` gets.
///
/// # Errors
///
/// [`FilesError::UnsafeName`] when the name cannot be used here.
pub fn download_name(remote_name: &[u8]) -> Result<LocalName, FilesError> {
    LocalName::from_remote(remote_name, Rules::native()).map_err(|reason| FilesError::UnsafeName {
        name: server_text(&display_bytes(remote_name)),
        reason,
    })
}

// ---- the work the UI runs for the application's effects -------------------------------

/// Lists remote folder `path`, first resolved to its absolute form.
///
/// # Errors
///
/// [`FilesError`] from the server.
pub async fn list_remote(
    client: RemoteSession,
    path: RemotePath,
) -> Result<(RemotePath, Vec<RemoteEntry>), FilesError> {
    let absolute = client
        .canonical(&path)
        .await
        .map_err(|e| FilesError::from(&e))?;
    let listed = client
        .list(&absolute)
        .await
        .map_err(|e| FilesError::from(&e))?;
    let mut entries: Vec<RemoteEntry> = listed.into_iter().map(RemoteEntry::from_listing).collect();
    sort_remote(&mut entries);
    Ok((absolute, entries))
}

fn local_kind(file_type: std::fs::FileType) -> EntryKind {
    if file_type.is_symlink() {
        EntryKind::Link
    } else if file_type.is_dir() {
        EntryKind::Directory
    } else if file_type.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

fn list_local_now(path: &Path) -> Result<Vec<LocalEntry>, FilesError> {
    let local = |error: std::io::Error| FilesError::Local {
        detail: error.to_string(),
    };
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path).map_err(local)? {
        let entry = entry.map_err(local)?;
        // Links are shown as links, never followed.
        let Ok(metadata) = entry.path().symlink_metadata() else {
            continue;
        };
        let name = entry.file_name();
        entries.push(LocalEntry {
            label: name.to_string_lossy().into_owned(),
            name,
            kind: local_kind(metadata.file_type()),
            size: metadata.is_file().then_some(metadata.len()),
            modified: metadata.modified().ok(),
        });
    }
    sort_local(&mut entries);
    Ok(entries)
}

/// Lists local folder `path`.
///
/// # Errors
///
/// [`FilesError::Local`].
pub async fn list_local(path: PathBuf) -> Result<(PathBuf, Vec<LocalEntry>), FilesError> {
    tokio::task::spawn_blocking(move || list_local_now(&path).map(|entries| (path, entries)))
        .await
        .unwrap_or_else(|error| {
            Err(FilesError::Local {
                detail: error.to_string(),
            })
        })
}

/// What a running transfer reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferEvent {
    /// Bytes done so far.
    Progress(u64),
    /// Finished: `Ok` when complete, [`TransferState::Cancelled`] or a failure otherwise.
    Finished(TransferState),
}

/// A transfer to run.
#[derive(Debug, Clone)]
pub struct TransferRequest {
    /// Session.
    pub client: RemoteSession,
    /// Direction.
    pub direction: Direction,
    /// Remote file.
    pub remote: RemotePath,
    /// Local file.
    pub local: PathBuf,
    /// Replace an existing target, the user having confirmed; without it a target found at
    /// the end of a file transfer is left as it is.
    pub replace: bool,
    /// A folder with everything in it, rather than one file.
    pub folder: bool,
    /// A folder's files and folders, in order, as planned and answered.
    pub steps: Vec<Ready>,
    /// Entries the folder's plan left out: links, special files, unusable names.
    pub left_out: usize,
    /// Stops it.
    pub cancel: CancellationToken,
}

/// The entries a transfer is planned for, before anything is written.
#[derive(Debug, Clone)]
pub struct PlanRequest {
    /// Session.
    pub client: RemoteSession,
    /// Direction.
    pub direction: Direction,
    /// The entries picked, both ends, with what the transfers list shows of each.
    pub roots: Vec<PlannedRoot>,
    /// The transfers listing the entries, in the same order.
    pub rows: Vec<TransferId>,
    /// Stops the plan.
    pub cancel: CancellationToken,
}

/// One entry picked for a transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedRoot {
    /// Both ends.
    pub root: Root,
    /// Its name, made safe.
    pub label: String,
    /// Its size when known: a file's, never a folder's.
    pub total: Option<u64>,
}

impl PlannedRoot {
    /// What is uploaded read as it is now, its turn come: its size and time.
    fn read_again(&mut self) {
        let Ok(metadata) = self.root.local.metadata() else {
            return;
        };
        self.root.stamp.modified = metadata.modified().ok();
        if self.root.kind == heimdall_files::conflict::Kind::File {
            self.root.stamp.size = Some(metadata.len());
            self.total = Some(metadata.len());
        }
    }
}

/// Plans `request` whole: every file and folder it would write, checked against what the
/// destination holds.
///
/// # Errors
///
/// [`FilesError`] when the session ends or the walk is too large.
pub async fn plan_transfer(request: PlanRequest) -> Result<Plan, FilesError> {
    let roots: Vec<Root> = request
        .roots
        .into_iter()
        .map(|planned| planned.root)
        .collect();
    let plan = match request.direction {
        Direction::Download => request.client.plan_download(&roots, &request.cancel).await,
        Direction::Upload => request.client.plan_upload(&roots, &request.cancel).await,
    };
    plan.map_err(|error| FilesError::from(&error))
}

/// Runs a transfer on the current tokio runtime; its events arrive on the stream, which
/// ends after [`TransferEvent::Finished`]. Progress comes at most ten times a second.
#[must_use]
pub fn transfer_events(request: TransferRequest) -> ReceiverStream<TransferEvent> {
    let (events, receiver) = mpsc::channel(TRANSFER_QUEUE_LENGTH);
    let reports = events.clone();
    let work = tokio::spawn(async move {
        let events = reports;
        let mut last: Option<Instant> = None;
        let progress = |bytes: u64| {
            if last.is_none_or(|sent| sent.elapsed() >= PROGRESS_INTERVAL) {
                last = Some(Instant::now());
                let _ = events.try_send(TransferEvent::Progress(bytes));
            }
        };
        if request.folder {
            match run_folder(&request, progress).await {
                Ok(0) => TransferState::Done,
                Ok(skipped) => TransferState::Incomplete { skipped },
                Err(error) => failed(&error),
            }
        } else {
            let result = match request.direction {
                Direction::Download => {
                    request
                        .client
                        .download_with(
                            &request.remote,
                            &request.local,
                            request.replace,
                            &request.cancel,
                            progress,
                        )
                        .await
                }
                Direction::Upload => {
                    request
                        .client
                        .upload(
                            &request.local,
                            &request.remote,
                            request.replace,
                            &request.cancel,
                            progress,
                        )
                        .await
                }
            };
            match result {
                Ok(bytes) => {
                    let _ = events.send(TransferEvent::Progress(bytes)).await;
                    TransferState::Done
                }
                Err(error) => failed(&error),
            }
        }
    });
    tokio::spawn(async move {
        // A transfer that stopped short still ends: the transfers waiting behind it run.
        let state = work
            .await
            .unwrap_or(TransferState::Failed(FilesError::Interrupted));
        let _ = events.send(TransferEvent::Finished(state)).await;
    });
    ReceiverStream::new(receiver)
}

/// How a transfer that did not complete ended: a cancel is a state, not a failure.
fn failed(error: &RemoteError) -> TransferState {
    match error {
        RemoteError::Cancelled => TransferState::Cancelled,
        other => TransferState::Failed(FilesError::from(other)),
    }
}

/// Runs a folder transfer's planned steps; returns how many entries it left out.
async fn run_folder(
    request: &TransferRequest,
    progress: impl FnMut(u64) + Send,
) -> Result<usize, RemoteError> {
    let download = request.direction == Direction::Download;
    let report = request
        .client
        .run(download, &request.steps, &request.cancel, progress)
        .await?;
    Ok(report.skipped + request.left_out)
}

/// A change to a folder's entries.
#[derive(Debug, Clone)]
pub enum FileOperation {
    /// Create a folder on the server.
    RemoteMakeFolder {
        /// Session.
        client: RemoteSession,
        /// Folder to create.
        path: RemotePath,
    },
    /// Rename on the server; an existing target makes it fail.
    RemoteRename {
        /// Session.
        client: RemoteSession,
        /// Current path.
        from: RemotePath,
        /// New path.
        to: RemotePath,
    },
    /// Delete on the server, a folder with everything in it, never following a link.
    RemoteRemove {
        /// Session.
        client: RemoteSession,
        /// What to delete.
        path: RemotePath,
    },
    /// Create a folder on this computer.
    LocalMakeFolder {
        /// Folder to create.
        path: PathBuf,
    },
    /// Rename on this computer; an existing target makes it fail.
    LocalRename {
        /// Current path.
        from: PathBuf,
        /// New path.
        to: PathBuf,
    },
    /// Delete on this computer, a folder with everything in it, never following a link.
    LocalRemove {
        /// What to delete.
        path: PathBuf,
    },
    /// Give an entry on the server new permission bits.
    RemoteSetPermissions {
        /// Session.
        client: RemoteSession,
        /// The entry.
        path: RemotePath,
        /// The bits.
        mode: u32,
    },
}

/// The permission bits typed as the C# dialog asks for them, octal: 755, or 4755 with the
/// set-user, set-group and sticky bits.
///
/// # Errors
///
/// [`FilesError::InvalidPermissions`] for anything but one to four octal digits.
pub fn octal_mode(typed: &str) -> Result<u32, FilesError> {
    let typed = typed.trim();
    if typed.is_empty() || typed.len() > 4 || !typed.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return Err(FilesError::InvalidPermissions);
    }
    u32::from_str_radix(typed, 8).map_err(|_| FilesError::InvalidPermissions)
}

/// What an entry of the server's pane is, as the C# Properties dialog shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProperties {
    /// Its name, made safe.
    pub name: String,
    /// Its kind.
    pub kind: EntryKind,
    /// Its size.
    pub size: Option<u64>,
    /// Its modification time.
    pub modified: Option<SystemTime>,
    /// Its permission bits.
    pub permissions: Option<u32>,
    /// Its owner's user number.
    pub owner: Option<u32>,
    /// Its group's number.
    pub group: Option<u32>,
    /// Its whole path, made safe.
    pub path: String,
}

fn local_failure(error: &std::io::Error) -> FilesError {
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        FilesError::Exists
    } else {
        FilesError::Local {
            detail: error.to_string(),
        }
    }
}

fn local_operation(operation: &FileOperation) -> Result<(), FilesError> {
    match operation {
        FileOperation::LocalMakeFolder { path } => {
            std::fs::create_dir(path).map_err(|e| local_failure(&e))
        }
        FileOperation::LocalRename { from, to } => {
            // Renaming never replaces: the check and the rename are two steps, the window
            // between them is the user's own.
            if to.symlink_metadata().is_ok() {
                return Err(FilesError::Exists);
            }
            std::fs::rename(from, to).map_err(|e| local_failure(&e))
        }
        FileOperation::LocalRemove { path } => {
            let metadata = path.symlink_metadata().map_err(|e| local_failure(&e))?;
            // remove_dir_all does not follow links (Rust 1.58 and later); a link itself is
            // removed as a file.
            if metadata.is_dir() {
                std::fs::remove_dir_all(path).map_err(|e| local_failure(&e))
            } else {
                std::fs::remove_file(path).map_err(|e| local_failure(&e))
            }
        }
        _ => Ok(()),
    }
}

/// Carries out a file operation.
///
/// # Errors
///
/// [`FilesError`] from the server or this computer.
pub async fn file_operation(operation: FileOperation) -> Result<(), FilesError> {
    match operation {
        FileOperation::RemoteMakeFolder { client, path } => client
            .make_folder(&path)
            .await
            .map_err(|e| FilesError::from(&e)),
        FileOperation::RemoteRename { client, from, to } => client
            .rename(&from, &to)
            .await
            .map_err(|e| FilesError::from(&e)),
        FileOperation::RemoteRemove { client, path } => {
            client.remove(&path).await.map_err(|e| FilesError::from(&e))
        }
        FileOperation::RemoteSetPermissions { client, path, mode } => client
            .set_permissions(&path, mode)
            .await
            .map_err(|e| FilesError::from(&e)),
        local => tokio::task::spawn_blocking(move || local_operation(&local))
            .await
            .unwrap_or_else(|error| {
                Err(FilesError::Local {
                    detail: error.to_string(),
                })
            }),
    }
}

/// Moves each entry of the server to its new path, one after another, by a rename that
/// never replaces what is there; one failing leaves the others to go on.
pub async fn move_remote(
    client: RemoteSession,
    moves: Vec<(RemotePath, RemotePath)>,
) -> Vec<(RemotePath, Result<(), FilesError>)> {
    let mut results = Vec::with_capacity(moves.len());
    for (from, to) in moves {
        let result = client
            .rename(&from, &to)
            .await
            .map_err(|e| FilesError::from(&e));
        results.push((from, result));
    }
    results
}

/// An entry of the server to copy on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopySource {
    /// Its full path.
    pub path: RemotePath,
    /// A folder, copied whole; otherwise a regular file.
    pub folder: bool,
}

/// Longest one server-side copy may run, as the C# allows.
pub const COPY_TIMEOUT: Duration = Duration::from_mins(10);

/// What a copy script is given to: a POSIX shell reading it on its input, whatever the
/// account's login shell.
const COPY_SHELL: &str = "sh -s";

/// Copies each of `sources` into `folder` on the server itself, one after another, under
/// its own name or, taken, its first free copy name ("a (copy).txt"); the first failure
/// stops the rest, as the C# does. Each result names the copy made.
pub async fn copy_remote(
    client: RemoteSession,
    shell: Option<heimdall_ssh::Connection>,
    sources: Vec<CopySource>,
    folder: RemotePath,
    cancel: CancellationToken,
) -> Vec<(RemotePath, Result<RemotePath, FilesError>)> {
    let mut results = Vec::with_capacity(sources.len());
    for source in sources {
        let result = copy_one(&client, shell.as_ref(), &source, &folder, &cancel).await;
        let failed = result.is_err();
        results.push((source.path, result));
        if failed {
            break;
        }
    }
    results
}

async fn copy_one(
    client: &RemoteSession,
    shell: Option<&heimdall_ssh::Connection>,
    source: &CopySource,
    folder: &RemotePath,
    cancel: &CancellationToken,
) -> Result<RemotePath, FilesError> {
    use heimdall_files::server_copy::{CopyKind, copy_script, is_same_or_inside, random_token};

    let shell = shell.ok_or(FilesError::CopyRefused)?;
    let name = source.path.file_name().ok_or(FilesError::NotAFile)?;
    if source.folder && is_same_or_inside(source.path.as_bytes(), folder.as_bytes()) {
        return Err(FilesError::PasteIntoItself {
            name: server_text(&display_bytes(name)),
        });
    }
    let listed = |items: Vec<RemoteItem>| -> Vec<(Vec<u8>, ItemKind)> {
        items
            .into_iter()
            .map(|item| (item.name, item.kind))
            .collect()
    };
    let taken = listed(
        client
            .list(folder)
            .await
            .map_err(|e| FilesError::from(&e))?,
    );
    let is_taken = |name: &[u8]| taken.iter().any(|(found, _)| found == name);
    let free = if is_taken(name) {
        heimdall_files::conflict::copy_names(name)
            .find(|candidate| !is_taken(candidate))
            .ok_or(FilesError::Exists)?
    } else {
        name.to_vec()
    };
    let destination = folder.join(&free);
    let (kind, expected) = if source.folder {
        (CopyKind::Folder, ItemKind::Directory)
    } else {
        (CopyKind::File, ItemKind::File)
    };
    let marker = random_token().ok_or(FilesError::CopyRefused)?;
    let script = copy_script(source.path.as_bytes(), destination.as_bytes(), kind, marker)
        .map_err(|_| FilesError::InvalidName)?;
    let ended = shell
        .run_command(COPY_SHELL, &script.script, COPY_TIMEOUT, cancel.clone())
        .await
        .map_err(|error| match error {
            heimdall_ssh::ConnectError::CommandRefused | heimdall_ssh::ConnectError::Timeout => {
                FilesError::CopyRefused
            }
            _ => FilesError::SessionClosed,
        })?;
    if ended.status != Some(0) || ended.stdout != script.done {
        log::warn!(
            "server-side copy refused: status {:?}, {}",
            ended.status,
            String::from_utf8_lossy(&ended.stderr).trim()
        );
        return Err(FilesError::CopyRefused);
    }
    // The script's word is checked against the server's listing: the copy is there, and is
    // what it should be.
    let listed = listed(
        client
            .list(folder)
            .await
            .map_err(|e| FilesError::from(&e))?,
    );
    if listed.contains(&(free, expected)) {
        Ok(destination)
    } else {
        Err(FilesError::CopyRefused)
    }
}

/// A name typed for a new or renamed entry, checked for `side`.
///
/// # Errors
///
/// [`FilesError::InvalidName`] for an empty name, `.`, `..`, a separator or a control
/// character, and on this computer anything [`LocalName`] refuses.
pub fn typed_name(side: Side, typed: &str) -> Result<LocalName, FilesError> {
    let typed = typed.trim_matches(|c: char| c == '\n' || c == '\r');
    // Refused on both sides: a Unix file system takes a tab or an escape in a name, and a
    // name typed by hand never needs one.
    if typed.is_empty()
        || typed == "."
        || typed == ".."
        || typed.contains('/')
        || typed.chars().any(char::is_control)
    {
        return Err(FilesError::InvalidName);
    }
    match side {
        Side::Local => LocalName::from_remote(typed.as_bytes(), Rules::native())
            .map_err(|_| FilesError::InvalidName),
        Side::Remote => Ok(LocalName {
            name: OsString::from(typed),
            escaped: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn transfers_handed_over_to_a_new_connection_are_stopped_to_be_retried() {
        let mut pane = super::FilesPane::new(std::path::PathBuf::from("."));
        let transfer = |state| super::Transfer {
            id: super::TransferId::fresh(),
            direction: super::Direction::Download,
            label: "file".to_owned(),
            bytes: 0,
            total: None,
            rate: super::Rate::default(),
            state,
            picked: None,
            cancel: tokio_util::sync::CancellationToken::new(),
        };
        pane.transfers = vec![
            transfer(super::TransferState::Done),
            transfer(super::TransferState::Running),
            transfer(super::TransferState::Queued),
        ];
        let handed = pane.hand_over_transfers();
        assert!(pane.transfers.is_empty());
        assert_eq!(
            handed
                .iter()
                .map(|transfer| transfer.state.clone())
                .collect::<Vec<_>>(),
            [
                super::TransferState::Done,
                super::TransferState::Cancelled,
                super::TransferState::Cancelled
            ]
        );
        assert!(handed[1].cancel.is_cancelled(), "the run stops");
        assert_eq!(pane.running(), 0);
    }

    #[test]
    fn a_transfers_speed_is_measured_every_quarter_second_and_smoothed_as_the_csharp() {
        use std::time::{Duration, Instant};

        let start = Instant::now();
        let mut rate = super::Rate::default();
        rate.sample(0, start);
        assert_eq!(rate.bytes_per_second(), None, "one point is no speed");
        rate.sample(1_000, start + Duration::from_millis(100));
        assert_eq!(rate.bytes_per_second(), None, "too soon to measure");
        rate.sample(1_000_000, start + Duration::from_secs(1));
        assert!((rate.bytes_per_second().expect("measured") - 1_000_000.0).abs() < 1.0);
        // Half as fast for a second: 30% of the new measure, 70% of the speed so far.
        rate.sample(1_500_000, start + Duration::from_secs(2));
        assert!((rate.bytes_per_second().expect("measured") - 850_000.0).abs() < 1.0);
        let left = rate
            .remaining(1_500_000, Some(3_200_000))
            .expect("a time left");
        assert_eq!(left.as_secs(), 2);
        assert_eq!(rate.remaining(0, None), None, "no size, no time left");
    }

    use heimdall_files::{ItemKind, RemoteItem};

    use std::time::{Duration, UNIX_EPOCH};

    use super::{
        EntryKind, Pane, RemoteEntry, Sort, SortColumn, sort_entries, sort_remote, symbolic_mode,
    };

    /// A file listed with its size, time, mode and owner.
    fn file(name: &str, size: u64, at: u64, mode: u32, owner: u32) -> RemoteEntry {
        RemoteEntry::from_listing(RemoteItem {
            name: name.as_bytes().to_vec(),
            kind: ItemKind::File,
            size: Some(size),
            modified: Some(UNIX_EPOCH + Duration::from_secs(at)),
            permissions: Some(mode),
            owner: Some(owner),
            group: None,
        })
    }

    fn labels(entries: &[RemoteEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.label.as_str()).collect()
    }

    #[test]
    fn a_mode_is_shown_as_the_csharp_column_shows_it() {
        assert_eq!(symbolic_mode(0o755), "rwxr-xr-x");
        assert_eq!(symbolic_mode(0o640), "rw-r-----");
        assert_eq!(symbolic_mode(0), "---------");
        assert_eq!(symbolic_mode(0o4755), "rwsr-xr-x");
        assert_eq!(symbolic_mode(0o4644), "rwSr--r--");
        assert_eq!(symbolic_mode(0o2711), "rwx--s--x");
        assert_eq!(symbolic_mode(0o2600), "rw---S---");
        assert_eq!(symbolic_mode(0o1777), "rwxrwxrwt");
        assert_eq!(symbolic_mode(0o1776), "rwxrwxrwT");
    }

    #[test]
    fn a_header_clicked_sorts_by_it_then_the_other_way() {
        let name = Sort::default();
        assert_eq!(
            name.clicked(SortColumn::Name),
            Sort {
                column: SortColumn::Name,
                descending: true
            }
        );
        let size = name.clicked(SortColumn::Size);
        assert_eq!(
            size,
            Sort {
                column: SortColumn::Size,
                descending: false
            },
            "another column: from the smallest"
        );
        assert!(size.clicked(SortColumn::Size).descending);
        assert!(
            !size
                .clicked(SortColumn::Size)
                .clicked(SortColumn::Size)
                .descending
        );
    }

    #[test]
    fn each_column_sorts_after_the_folders_and_equal_ones_by_name() {
        let mut entries = vec![
            file("b", 30, 100, 0o644, 1000),
            file("a", 10, 300, 0o600, 0),
            entry(b"dir", ItemKind::Directory),
            file("C", 20, 200, 0o755, 33),
            file("d", 20, 200, 0o644, 1000),
        ];
        let by = |entries: &mut Vec<RemoteEntry>, column, descending| {
            sort_entries(entries, Sort { column, descending });
            labels(entries).join(" ")
        };
        assert_eq!(by(&mut entries, SortColumn::Name, false), "dir a b C d");
        assert_eq!(by(&mut entries, SortColumn::Name, true), "dir d C b a");
        assert_eq!(
            by(&mut entries, SortColumn::Size, false),
            "dir a C d b",
            "20 and 20 by name"
        );
        assert_eq!(by(&mut entries, SortColumn::Size, true), "dir b C d a");
        assert_eq!(by(&mut entries, SortColumn::Modified, false), "dir b C d a");
        assert_eq!(by(&mut entries, SortColumn::Modified, true), "dir a C d b");
        assert_eq!(
            by(&mut entries, SortColumn::Permissions, false),
            "dir a b d C"
        );
        assert_eq!(by(&mut entries, SortColumn::Owner, false), "dir a C b d");
        assert_eq!(
            by(&mut entries, SortColumn::Owner, true),
            "dir b d C a",
            "the folder first, the other way too"
        );
    }

    fn pane(names: &[&str]) -> Pane<(), RemoteEntry> {
        let mut pane = Pane::new(());
        pane.show(
            names
                .iter()
                .enumerate()
                .map(|(size, name)| file(name, size as u64, 0, 0o644, 0))
                .collect(),
        );
        pane
    }

    #[test]
    fn ctrl_adds_and_removes_and_shift_takes_a_range_from_the_one_selected() {
        let mut pane = pane(&["a", "b", "c", "d", "e"]);
        pane.toggle(1);
        assert_eq!(pane.chosen(), [1], "nothing before: selected alone");
        pane.toggle(3);
        assert_eq!((pane.selected, pane.chosen()), (Some(3), vec![1, 3]));
        pane.toggle(1);
        assert_eq!(pane.chosen(), [3], "unmarked");
        pane.toggle(0);
        pane.toggle(0);
        assert_eq!(
            (pane.selected, pane.chosen()),
            (Some(3), vec![3]),
            "the one selected unselected: another takes its place"
        );
        pane.toggle(3);
        assert_eq!((pane.selected, pane.chosen()), (None, vec![]));

        pane.select_only(Some(1));
        pane.extend_to(3);
        assert_eq!((pane.selected, pane.chosen()), (Some(1), vec![1, 2, 3]));
        pane.extend_to(0);
        assert_eq!(pane.chosen(), [0, 1], "the range from where it started");
        pane.extend_to(3);
        pane.toggle(1);
        assert_eq!(
            pane.chosen(),
            [2, 3],
            "where it started, unselected with Ctrl, leaves the rest"
        );
        pane.select_only(None);
        pane.extend_to(4);
        assert_eq!((pane.selected, pane.chosen()), (Some(4), vec![4]));
        pane.select_only(Some(2));
        assert_eq!(pane.chosen(), [2], "a plain selection alone");
    }

    #[test]
    fn the_filter_and_the_hidden_toggle_narrow_what_is_shown_not_what_is_listed() {
        let mut pane = pane(&[".profile", "Makefile", "main.rs", ".git"]);
        assert_eq!(
            labels(&pane.entries),
            [".git", ".profile", "main.rs", "Makefile"],
            "hidden names shown by default, as the C# tab"
        );
        pane.select_only(Some(2));
        pane.toggle(3);
        pane.filter_by(" MA ".to_owned());
        assert_eq!(
            labels(&pane.entries),
            ["main.rs", "Makefile"],
            "whatever the case"
        );
        assert_eq!(pane.chosen(), [0, 1], "still selected");
        pane.filter_by("main".to_owned());
        assert_eq!(labels(&pane.entries), ["main.rs"]);
        assert_eq!(
            (pane.selected, pane.chosen()),
            (Some(0), vec![0]),
            "the one selected no longer shown: the other takes its place"
        );
        pane.filter_by(String::new());
        pane.toggle_hidden();
        assert_eq!(labels(&pane.entries), ["main.rs", "Makefile"]);
        assert_eq!(pane.listing.len(), 4, "every entry still listed");
        pane.show(vec![
            file(".env", 1, 0, 0o600, 0),
            file("x", 1, 0, 0o644, 0),
        ]);
        assert_eq!(
            labels(&pane.entries),
            ["x"],
            "a new listing, narrowed the same"
        );
        pane.toggle_hidden();
        assert_eq!(labels(&pane.entries), [".env", "x"]);
    }

    #[test]
    fn marks_follow_their_entries_through_a_resort_and_a_listing_clears_them() {
        let mut pane = pane(&["a", "b", "c"]);
        pane.select_only(Some(0));
        pane.toggle(2);
        pane.sort_by(Sort {
            column: SortColumn::Name,
            descending: true,
        });
        assert_eq!(labels(&pane.entries), ["c", "b", "a"]);
        assert_eq!(
            (pane.selected, pane.chosen()),
            (Some(0), vec![0, 2]),
            "c, the one clicked, selected; a marked; both moved"
        );
        pane.toggle(1);
        assert_eq!(pane.marked.len(), 2);
        pane.show(vec![file("x", 1, 0, 0o644, 0)]);
        assert_eq!((pane.selected, pane.chosen()), (None, vec![]));
        pane.marked.insert(5);
        assert!(pane.chosen().is_empty(), "no mark beyond what is listed");
    }

    #[test]
    fn a_resort_keeps_the_entry_selected_and_a_new_listing_its_sort() {
        let mut pane: Pane<(), RemoteEntry> = Pane::new(());
        pane.show(vec![
            file("big", 90, 0, 0o644, 0),
            file("small", 1, 0, 0o644, 0),
        ]);
        assert_eq!(labels(&pane.entries), ["big", "small"]);
        pane.selected = Some(0);
        pane.sort_by(Sort {
            column: SortColumn::Size,
            descending: false,
        });
        assert_eq!(labels(&pane.entries), ["small", "big"]);
        assert_eq!(pane.selected, Some(1), "big, still");
        pane.show(vec![file("x", 50, 0, 0o644, 0), file("y", 5, 0, 0o644, 0)]);
        assert_eq!(labels(&pane.entries), ["y", "x"], "listed again, by size");
        assert_eq!(pane.selected, None);
    }

    fn entry(name: &[u8], kind: ItemKind) -> RemoteEntry {
        RemoteEntry::from_listing(RemoteItem {
            name: name.to_vec(),
            kind,
            size: None,
            modified: None,
            permissions: None,
            owner: None,
            group: None,
        })
    }

    #[test]
    fn folders_come_first_then_names_ignoring_case() {
        let mut entries = vec![
            entry(b"b.txt", ItemKind::File),
            entry(b"Zeta", ItemKind::Directory),
            entry(b"a.txt", ItemKind::File),
            entry(b"alpha", ItemKind::Directory),
            entry(b"link", ItemKind::Link),
        ];
        sort_remote(&mut entries);
        let labels: Vec<&str> = entries.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["alpha", "Zeta", "a.txt", "b.txt", "link"]);
        assert_eq!(entries[4].kind, EntryKind::Link);
    }

    #[test]
    fn a_hostile_name_is_shown_escaped_and_safe() {
        let shown = entry(b"caf\xE9\x1b[2J\xE2\x80\xAE.txt", ItemKind::File);
        assert!(!shown.label.contains('\u{1b}'));
        assert!(!shown.label.contains('\u{202e}'));
        assert!(shown.label.contains("\\xE9"), "{}", shown.label);
        assert_eq!(
            shown.name, b"caf\xE9\x1b[2J\xE2\x80\xAE.txt",
            "the exact bytes are kept"
        );
    }
}
