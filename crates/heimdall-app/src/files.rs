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

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use heimdall_sftp::local_name::{LocalName, LocalNameError, Rules};
use heimdall_sftp::path::display_bytes;
use heimdall_sftp::protocol::{Attributes, StatusCode};
use heimdall_sftp::transfer::{TransferConfig, TransferError, download, upload};
use heimdall_sftp::{DirEntry, RemotePath, SftpClient, SftpError};
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
    /// Modification time, seconds since the epoch.
    pub modified: Option<u32>,
    /// POSIX mode bits.
    pub permissions: Option<u32>,
}

impl RemoteEntry {
    /// The entry for a listed name.
    #[must_use]
    pub fn from_listing(entry: DirEntry) -> Self {
        let kind = kind_of(&entry.attributes);
        Self {
            label: server_text(&display_bytes(&entry.name)),
            name: entry.name,
            kind,
            size: entry.attributes.size,
            modified: entry.attributes.times.map(|(_, modified)| modified),
            permissions: entry.attributes.permissions,
        }
    }
}

fn kind_of(attributes: &Attributes) -> EntryKind {
    if attributes.is_directory() {
        EntryKind::Directory
    } else if attributes.is_regular_file() {
        EntryKind::File
    } else if attributes.is_symlink() {
        EntryKind::Link
    } else {
        EntryKind::Other
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

/// A pane: where it is, what it lists, what is selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane<P, E> {
    /// Folder shown.
    pub path: P,
    /// Entries, folders first, then by name.
    pub entries: Vec<E>,
    /// Selected entry.
    pub selected: Option<usize>,
    /// A listing is on its way.
    pub loading: bool,
    /// Why the last listing failed.
    pub error: Option<FilesError>,
}

impl<P, E> Pane<P, E> {
    fn new(path: P) -> Self {
        Self {
            path,
            entries: Vec::new(),
            selected: None,
            loading: true,
            error: None,
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
    /// Running.
    Running,
    /// Complete.
    Done,
    /// Stopped by the user; a download can be resumed by starting it again.
    Cancelled,
    /// Failed.
    Failed(FilesError),
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
    /// State.
    pub state: TransferState,
    pub(crate) cancel: CancellationToken,
}

/// Why a Files operation failed; shown in the user's language by the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesError {
    /// The server refused, with its status and message (made safe).
    Server {
        /// Status code.
        code: StatusCode,
        /// The server's message, made safe.
        message: String,
    },
    /// The SFTP session is over.
    SessionClosed,
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
    /// Not a regular file: folders are not transferred yet.
    NotAFile,
}

impl From<&SftpError> for FilesError {
    fn from(error: &SftpError) -> Self {
        match error {
            SftpError::Status { code, message } => Self::Server {
                code: *code,
                message: server_text(&String::from_utf8_lossy(message)),
            },
            _ => Self::SessionClosed,
        }
    }
}

impl From<&TransferError> for FilesError {
    fn from(error: &TransferError) -> Self {
        match error {
            TransferError::Sftp(error) => error.into(),
            TransferError::Local { source, .. } => Self::Local {
                detail: source.to_string(),
            },
            TransferError::NotARegularFile => Self::NotAFile,
            // A cancel is a state of the transfer, not a failure: callers handle it first.
            TransferError::Cancelled { .. } => Self::SessionClosed,
        }
    }
}

/// The Files tab's state.
#[derive(Debug)]
pub struct FilesPane {
    /// The SFTP session, once open.
    pub client: Option<SftpClient>,
    /// Server side.
    pub remote: RemotePane,
    /// Local side.
    pub local: LocalPane,
    /// Transfers, oldest first.
    pub transfers: Vec<Transfer>,
}

impl FilesPane {
    /// A pane starting locally in `local`, remotely in the server's starting folder.
    #[must_use]
    pub fn new(local: PathBuf) -> Self {
        Self {
            client: None,
            remote: Pane::new(RemotePath::from(".")),
            local: Pane::new(local),
            transfers: Vec::new(),
        }
    }

    /// Cancels every running transfer and drops the session.
    pub(crate) fn stop(&mut self) {
        for transfer in &self.transfers {
            transfer.cancel.cancel();
        }
        self.client = None;
    }

    /// Transfers still running.
    #[must_use]
    pub fn running(&self) -> usize {
        self.transfers
            .iter()
            .filter(|transfer| transfer.state == TransferState::Running)
            .count()
    }
}

/// Folders first, then by name, ignoring case.
pub fn sort_remote(entries: &mut [RemoteEntry]) {
    entries.sort_by(|a, b| {
        (a.kind != EntryKind::Directory, a.label.to_lowercase())
            .cmp(&(b.kind != EntryKind::Directory, b.label.to_lowercase()))
    });
}

/// Folders first, then by name, ignoring case.
pub fn sort_local(entries: &mut [LocalEntry]) {
    entries.sort_by(|a, b| {
        (a.kind != EntryKind::Directory, a.label.to_lowercase())
            .cmp(&(b.kind != EntryKind::Directory, b.label.to_lowercase()))
    });
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
    client: SftpClient,
    path: RemotePath,
) -> Result<(RemotePath, Vec<RemoteEntry>), FilesError> {
    let absolute = client
        .realpath(&path)
        .await
        .map_err(|e| FilesError::from(&e))?;
    let listed = client
        .read_dir(&absolute)
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
    pub client: SftpClient,
    /// Direction.
    pub direction: Direction,
    /// Remote file.
    pub remote: RemotePath,
    /// Local file.
    pub local: PathBuf,
    /// Replace an existing target (uploads; a download always replaces its target, the
    /// user having confirmed).
    pub replace: bool,
    /// Stops it.
    pub cancel: CancellationToken,
}

/// Runs a transfer on the current tokio runtime; its events arrive on the stream, which
/// ends after [`TransferEvent::Finished`]. Progress comes at most ten times a second.
#[must_use]
pub fn transfer_events(request: TransferRequest) -> ReceiverStream<TransferEvent> {
    let (events, receiver) = mpsc::channel(TRANSFER_QUEUE_LENGTH);
    tokio::spawn(async move {
        let mut last: Option<Instant> = None;
        let progress = |bytes: u64| {
            if last.is_none_or(|sent| sent.elapsed() >= PROGRESS_INTERVAL) {
                last = Some(Instant::now());
                let _ = events.try_send(TransferEvent::Progress(bytes));
            }
        };
        let config = TransferConfig::default();
        let result = match request.direction {
            Direction::Download => download(
                &request.client,
                &request.remote,
                &request.local,
                &config,
                &request.cancel,
                progress,
            )
            .await
            .map(|report| report.bytes),
            Direction::Upload => upload(
                &request.client,
                &request.local,
                &request.remote,
                request.replace,
                &config,
                &request.cancel,
                progress,
            )
            .await
            .map(|report| report.bytes),
        };
        let state = match result {
            Ok(bytes) => {
                let _ = events.send(TransferEvent::Progress(bytes)).await;
                TransferState::Done
            }
            Err(TransferError::Cancelled { .. }) => TransferState::Cancelled,
            Err(TransferError::NotARegularFile) => TransferState::Failed(FilesError::NotAFile),
            Err(error) => TransferState::Failed(FilesError::from(&error)),
        };
        let _ = events.send(TransferEvent::Finished(state)).await;
    });
    ReceiverStream::new(receiver)
}

#[cfg(test)]
mod tests {
    use heimdall_sftp::DirEntry;
    use heimdall_sftp::protocol::Attributes;

    use super::{EntryKind, RemoteEntry, sort_remote};

    fn entry(name: &[u8], mode: u32) -> RemoteEntry {
        RemoteEntry::from_listing(DirEntry {
            name: name.to_vec(),
            attributes: Attributes {
                permissions: Some(mode),
                ..Attributes::default()
            },
        })
    }

    #[test]
    fn folders_come_first_then_names_ignoring_case() {
        let mut entries = vec![
            entry(b"b.txt", 0o100_644),
            entry(b"Zeta", 0o040_755),
            entry(b"a.txt", 0o100_644),
            entry(b"alpha", 0o040_755),
            entry(b"link", 0o120_777),
        ];
        sort_remote(&mut entries);
        let labels: Vec<&str> = entries.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["alpha", "Zeta", "a.txt", "b.txt", "link"]);
        assert_eq!(entries[4].kind, EntryKind::Link);
    }

    #[test]
    fn a_hostile_name_is_shown_escaped_and_safe() {
        let shown = entry(b"caf\xE9\x1b[2J\xE2\x80\xAE.txt", 0o100_644);
        assert!(!shown.label.contains('\u{1b}'));
        assert!(!shown.label.contains('\u{202e}'));
        assert!(shown.label.contains("\\xE9"), "{}", shown.label);
        assert_eq!(
            shown.name, b"caf\xE9\x1b[2J\xE2\x80\xAE.txt",
            "the exact bytes are kept"
        );
    }
}
