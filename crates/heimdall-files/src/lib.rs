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

//! What the Files tab asks of a server, whatever the protocol.
//!
//! A [`RemoteSession`] lists, creates, renames, deletes and transfers; its answers and its
//! failures say nothing of the protocol under it. SFTP is the one protocol today; another
//! (FTP, FTPS) is one more variant and one more arm per method, and nothing above changes.
//!
//! An enum rather than a trait object: the methods are async, the set of protocols is closed
//! and small, and a session must stay `Clone` and `Debug` to travel in the application's
//! effects.
//!
//! Every method keeps the guarantees the Files tab relies on, which each protocol must meet
//! its own way: a rename never replaces, a delete never follows a link, a download never
//! reads anything but a regular file.

pub mod conflict;
mod ftp;
pub mod ftps_trust;
mod plan;

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use heimdall_sftp::protocol::{Attributes, StatusCode};
use heimdall_sftp::transfer::{self, TransferConfig, TransferError};
use heimdall_sftp::tree::{self, TreeError};
use heimdall_sftp::{DirEntry, SftpClient, SftpError};
use tokio_util::sync::CancellationToken;

pub use ftp::{FtpClient, FtpConnectError, FtpSecurity, FtpTarget};
pub use heimdall_sftp::RemotePath;
pub use heimdall_sftp::local_name::{LocalName, LocalNameError, Rules};
pub use heimdall_sftp::path::display_bytes;
pub use plan::{Plan, Ready, Root, Step};

/// The permission bits of a mode, set-user, set-group and sticky included.
const PERMISSION_BITS: u32 = 0o7777;

/// An open session with a server's file system.
#[derive(Debug, Clone)]
pub enum RemoteSession {
    /// SFTP over an SSH connection.
    Sftp(SftpClient),
    /// FTP, plain or explicit FTPS.
    Ftp(FtpClient),
}

/// What an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// A folder.
    Directory,
    /// A regular file.
    File,
    /// A symbolic link, not followed.
    Link,
    /// A device, socket, pipe or unknown.
    Other,
}

/// An entry of a remote folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteItem {
    /// The server's bytes.
    pub name: Vec<u8>,
    /// Kind.
    pub kind: ItemKind,
    /// Size in bytes, when the server says.
    pub size: Option<u64>,
    /// Modification time, when the server says.
    pub modified: Option<SystemTime>,
    /// Permission bits, when the server says.
    pub permissions: Option<u32>,
    /// The owner's user number, when the server says.
    pub owner: Option<u32>,
    /// The group's number, when the server says.
    pub group: Option<u32>,
}

/// What a server refused, in terms the user can be told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Nothing at that path.
    NoSuchFile,
    /// Not allowed.
    PermissionDenied,
    /// The server does not do that.
    Unsupported,
    /// Anything else.
    Failure,
}

/// Why a remote operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteError {
    /// The server refused.
    Refused {
        /// What was refused.
        refusal: Refusal,
        /// The server's own words, untrusted and possibly empty.
        message: Vec<u8>,
    },
    /// The session is over.
    SessionClosed,
    /// The local file system failed.
    Local {
        /// Operating system message.
        detail: String,
    },
    /// Not a regular file: a link, a device.
    NotAFile,
    /// A folder holds more entries than a transfer or a delete walks.
    TooLarge,
    /// Stopped by the caller; a download can be resumed by starting it again.
    Cancelled,
    /// A local file is at a download's name and replacing it was not agreed; it was left as
    /// it is.
    LocalExists,
    /// The entry is a symbolic link: changing its permissions would change what it points
    /// to.
    IsLink,
}

/// How a folder transfer ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderReport {
    /// Entries left out: links, unusable names, failures.
    pub skipped: usize,
}

impl RemoteSession {
    /// The absolute form of `path`.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] from the server.
    pub async fn canonical(&self, path: &RemotePath) -> Result<RemotePath, RemoteError> {
        match self {
            Self::Sftp(client) => client.realpath(path).await.map_err(|e| sftp_error(&e)),
            Self::Ftp(client) => client.canonical(path).await,
        }
    }

    /// The entries of folder `path`, in the server's order.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] from the server.
    pub async fn list(&self, path: &RemotePath) -> Result<Vec<RemoteItem>, RemoteError> {
        match self {
            Self::Sftp(client) => Ok(client
                .read_dir(path)
                .await
                .map_err(|e| sftp_error(&e))?
                .into_iter()
                .map(sftp_item)
                .collect()),
            Self::Ftp(client) => client.list(path).await,
        }
    }

    /// Creates folder `path`.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] from the server.
    pub async fn make_folder(&self, path: &RemotePath) -> Result<(), RemoteError> {
        match self {
            Self::Sftp(client) => client
                .mkdir(path, Attributes::default())
                .await
                .map_err(|e| sftp_error(&e)),
            Self::Ftp(client) => client.make_folder(path).await,
        }
    }

    /// Renames `from` to `to`; an existing `to` makes it fail, never replaced.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] from the server.
    pub async fn rename(&self, from: &RemotePath, to: &RemotePath) -> Result<(), RemoteError> {
        match self {
            // SFTP version 3 rename refuses an existing target.
            Self::Sftp(client) => client
                .rename(from, to, false)
                .await
                .map_err(|e| sftp_error(&e)),
            Self::Ftp(client) => client.rename(from, to).await,
        }
    }

    /// Gives `path` the permission bits `mode`, set-user, set-group and sticky included.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] from the server; [`RemoteError::IsLink`] for a symbolic link, left as
    /// it is.
    pub async fn set_permissions(&self, path: &RemotePath, mode: u32) -> Result<(), RemoteError> {
        match self {
            // SETSTAT follows a link: what it points to would change instead.
            Self::Sftp(client)
                if client
                    .lstat(path)
                    .await
                    .map_err(|e| sftp_error(&e))?
                    .is_symlink() =>
            {
                Err(RemoteError::IsLink)
            }
            Self::Sftp(client) => client
                .setstat(
                    path,
                    Attributes {
                        permissions: Some(mode),
                        ..Attributes::default()
                    },
                )
                .await
                .map_err(|e| sftp_error(&e)),
            Self::Ftp(client) => client.set_permissions(path, mode).await,
        }
    }

    /// Deletes `path`, a folder with everything in it, never following a link: a link is
    /// deleted, not what it points to.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] from the server; [`RemoteError::TooLarge`] for a tree too large to
    /// walk.
    pub async fn remove(&self, path: &RemotePath) -> Result<(), RemoteError> {
        match self {
            Self::Sftp(client) => tree::remove_tree(client, path, &CancellationToken::new())
                .await
                .map(|_| ())
                .map_err(|e| tree_error(&e)),
            Self::Ftp(client) => client.remove(path).await,
        }
    }

    /// Downloads regular file `remote` to `local`, resuming an earlier attempt and replacing
    /// a local file there; returns its size.
    ///
    /// # Errors
    ///
    /// [`RemoteError`]; [`RemoteError::Cancelled`] when `cancel` fired.
    pub async fn download(
        &self,
        remote: &RemotePath,
        local: &Path,
        cancel: &CancellationToken,
        progress: impl FnMut(u64) + Send,
    ) -> Result<u64, RemoteError> {
        self.download_with(remote, local, true, cancel, progress)
            .await
    }

    /// As [`Self::download`], but a local file at `local` is replaced only with `replace`:
    /// without it, a file found there when the download ends is left as it is.
    ///
    /// # Errors
    ///
    /// [`RemoteError`]; [`RemoteError::LocalExists`] for a local file left as it is, the
    /// download kept to be resumed.
    pub async fn download_with(
        &self,
        remote: &RemotePath,
        local: &Path,
        replace: bool,
        cancel: &CancellationToken,
        progress: impl FnMut(u64) + Send,
    ) -> Result<u64, RemoteError> {
        match self {
            Self::Sftp(client) => transfer::download(
                client,
                remote,
                local,
                &TransferConfig {
                    replace_local: replace,
                    ..TransferConfig::default()
                },
                cancel,
                progress,
            )
            .await
            .map(|report| report.bytes)
            .map_err(|e| transfer_error(&e)),
            Self::Ftp(client) => {
                client
                    .download_with(remote, local, replace, cancel, progress)
                    .await
            }
        }
    }

    /// Uploads `local` to `remote`, replacing an existing file only when `replace`; returns
    /// the bytes sent.
    ///
    /// # Errors
    ///
    /// [`RemoteError`]; [`RemoteError::Cancelled`] when `cancel` fired.
    pub async fn upload(
        &self,
        local: &Path,
        remote: &RemotePath,
        replace: bool,
        cancel: &CancellationToken,
        progress: impl FnMut(u64) + Send,
    ) -> Result<u64, RemoteError> {
        match self {
            Self::Sftp(client) => transfer::upload(
                client,
                local,
                remote,
                replace,
                &TransferConfig::default(),
                cancel,
                progress,
            )
            .await
            .map(|report| report.bytes)
            .map_err(|e| transfer_error(&e)),
            Self::Ftp(client) => {
                client
                    .upload(local, remote, replace, cancel, progress)
                    .await
            }
        }
    }
}

// ---- SFTP ----------------------------------------------------------------------------------

fn sftp_item(entry: DirEntry) -> RemoteItem {
    let attributes = &entry.attributes;
    let kind = if attributes.is_directory() {
        ItemKind::Directory
    } else if attributes.is_regular_file() {
        ItemKind::File
    } else if attributes.is_symlink() {
        ItemKind::Link
    } else {
        ItemKind::Other
    };
    RemoteItem {
        kind,
        size: attributes.size,
        modified: attributes
            .times
            .map(|(_, modified)| UNIX_EPOCH + Duration::from_secs(u64::from(modified))),
        // The permission bits alone: the file type is the kind.
        permissions: attributes.permissions.map(|mode| mode & PERMISSION_BITS),
        owner: attributes.uid_gid.map(|(uid, _)| uid),
        group: attributes.uid_gid.map(|(_, gid)| gid),
        name: entry.name,
    }
}

fn sftp_error(error: &SftpError) -> RemoteError {
    match error {
        SftpError::Status { code, message } => RemoteError::Refused {
            refusal: match code {
                StatusCode::NoSuchFile => Refusal::NoSuchFile,
                StatusCode::PermissionDenied => Refusal::PermissionDenied,
                StatusCode::OpUnsupported => Refusal::Unsupported,
                _ => Refusal::Failure,
            },
            message: message.clone(),
        },
        _ => RemoteError::SessionClosed,
    }
}

fn transfer_error(error: &TransferError) -> RemoteError {
    match error {
        TransferError::Sftp(error) => sftp_error(error),
        TransferError::Local { source, .. } => RemoteError::Local {
            detail: source.to_string(),
        },
        TransferError::NotARegularFile => RemoteError::NotAFile,
        TransferError::Cancelled { .. } => RemoteError::Cancelled,
        TransferError::LocalExists => RemoteError::LocalExists,
    }
}

fn tree_error(error: &TreeError) -> RemoteError {
    match error {
        TreeError::TooLarge => RemoteError::TooLarge,
        TreeError::Transfer(error) => transfer_error(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(code: StatusCode) -> RemoteError {
        sftp_error(&SftpError::Status {
            code,
            message: b"why".to_vec(),
        })
    }

    #[test]
    fn every_sftp_status_keeps_the_refusal_the_user_is_told() {
        let refusal = |code| match status(code) {
            RemoteError::Refused { refusal, message } => {
                assert_eq!(message, b"why");
                refusal
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(refusal(StatusCode::NoSuchFile), Refusal::NoSuchFile);
        assert_eq!(
            refusal(StatusCode::PermissionDenied),
            Refusal::PermissionDenied
        );
        assert_eq!(refusal(StatusCode::OpUnsupported), Refusal::Unsupported);
        for other in [
            StatusCode::Failure,
            StatusCode::BadMessage,
            StatusCode::Eof,
            StatusCode::Other(99),
        ] {
            assert_eq!(refusal(other), Refusal::Failure, "{other:?}");
        }
    }

    #[test]
    fn a_cancel_is_a_cancel_and_a_non_file_is_named_so() {
        assert_eq!(
            transfer_error(&TransferError::Cancelled { kept: 3 }),
            RemoteError::Cancelled
        );
        assert_eq!(
            transfer_error(&TransferError::NotARegularFile),
            RemoteError::NotAFile
        );
        assert_eq!(tree_error(&TreeError::TooLarge), RemoteError::TooLarge);
    }

    #[test]
    fn a_listed_entry_keeps_its_bytes_kind_size_and_time() {
        let attributes = Attributes {
            size: Some(7),
            // S_IFDIR, rwxr-xr-x.
            permissions: Some(0o040_755),
            times: Some((0, 60)),
            ..Attributes::default()
        };
        let item = sftp_item(DirEntry {
            name: b"d\xff".to_vec(),
            attributes,
        });
        assert_eq!(item.name, b"d\xff");
        assert_eq!(item.kind, ItemKind::Directory);
        assert_eq!(item.size, Some(7));
        assert_eq!(item.modified, Some(UNIX_EPOCH + Duration::from_secs(60)));
        assert_eq!(item.permissions, Some(0o755), "without the file type");
        assert_eq!(item.owner, None, "not said");
        let owned = sftp_item(DirEntry {
            name: b"f".to_vec(),
            attributes: Attributes {
                uid_gid: Some((1000, 50)),
                permissions: Some(0o104_755),
                ..Attributes::default()
            },
        });
        assert_eq!(owned.owner, Some(1000));
        assert_eq!(owned.group, Some(50));
        assert_eq!(owned.permissions, Some(0o4755), "set-user kept");
    }
}
