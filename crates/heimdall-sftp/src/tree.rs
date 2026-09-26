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

//! Folders: transferred whole, or deleted, without ever following a link.
//!
//! Walks use what `LSTAT` reports (a directory listing's attributes are `LSTAT`'s): a
//! symbolic link is never entered, so a link to `/` or to the user's home cannot turn a
//! folder download into a copy of the whole server, or a delete into the loss of what the
//! link points to. Links and special files are listed as skipped. One file failing does not
//! stop the rest; the report says what was left out and why. A walk stops past
//! [`MAX_DEPTH`] levels or [`MAX_ENTRIES`] entries.

use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

use crate::client::{SftpClient, SftpError};
use crate::local_name::{FolderNames, LocalName, LocalNameError, Rules};
use crate::path::{RemotePath, display_bytes};
use crate::protocol::{Attributes, PLAIN_PERMISSIONS};
use crate::transfer::{TransferConfig, TransferError, download, upload};

/// Deepest folder level a walk enters.
pub const MAX_DEPTH: usize = 64;

/// Most entries a walk visits.
pub const MAX_ENTRIES: usize = 200_000;

/// Mode of a remote folder created by an upload when the local one has none (Windows).
#[cfg(not(unix))]
const DEFAULT_FOLDER_MODE: u32 = 0o755;

/// Why an entry was left out of a folder transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    /// A symbolic link: never followed.
    Link,
    /// A device, socket or pipe.
    Special,
    /// The name cannot be used on this computer.
    UnsafeName(LocalNameError),
    /// Another entry of the folder takes the same local name (letter case, escaping).
    Collision,
    /// Deeper than [`MAX_DEPTH`].
    TooDeep,
    /// The transfer of this file failed; the text says why.
    Failed(String),
}

/// What a folder transfer did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeReport {
    /// Files transferred.
    pub files: u64,
    /// Folders created.
    pub folders: u64,
    /// Bytes transferred.
    pub bytes: u64,
    /// Entries left out: their path below the root, shown safely, and why.
    pub skipped: Vec<(String, Skip)>,
}

/// Why a folder transfer or delete stopped as a whole.
#[derive(Debug, thiserror::Error)]
pub enum TreeError {
    /// The session, the root, or the local disk failed.
    #[error(transparent)]
    Transfer(#[from] TransferError),
    /// More than [`MAX_ENTRIES`] entries.
    #[error("more than {MAX_ENTRIES} entries")]
    TooLarge,
}

impl From<SftpError> for TreeError {
    fn from(error: SftpError) -> Self {
        Self::Transfer(TransferError::Sftp(error))
    }
}

/// Whether an error ends the whole walk rather than one file.
fn fatal(error: &TransferError) -> bool {
    matches!(
        error,
        TransferError::Cancelled { .. } | TransferError::Sftp(SftpError::Closed(_))
    )
}

fn shown(path: &[u8]) -> String {
    display_bytes(path)
}

/// Downloads remote folder `root` into local folder `target`, created if needed. `progress`
/// receives the bytes done so far across all files.
///
/// # Errors
///
/// [`TreeError`] when the session ends, the walk is cancelled, the root cannot be listed or
/// created, or the tree is too large; files that fail alone are in the report instead.
pub async fn download_tree(
    client: &SftpClient,
    root: &RemotePath,
    target: &Path,
    config: &TransferConfig,
    cancel: &CancellationToken,
    mut progress: impl FnMut(u64) + Send,
) -> Result<TreeReport, TreeError> {
    let mut report = TreeReport::default();
    let mut pending: Vec<(RemotePath, PathBuf, Vec<u8>, usize)> =
        vec![(root.clone(), target.to_owned(), Vec::new(), 0)];
    let mut visited = 0usize;
    while let Some((remote_dir, local_dir, below, depth)) = pending.pop() {
        if cancel.is_cancelled() {
            return Err(TransferError::Cancelled { kept: report.bytes }.into());
        }
        tokio::fs::create_dir_all(&local_dir)
            .await
            .map_err(|source| TransferError::Local {
                path: local_dir.clone(),
                source,
            })?;
        report.folders += 1;
        let mut names = FolderNames::new(Rules::native());
        for entry in client.read_dir(&remote_dir).await? {
            visited += 1;
            if visited > MAX_ENTRIES {
                return Err(TreeError::TooLarge);
            }
            let mut relative = below.clone();
            if !relative.is_empty() {
                relative.push(b'/');
            }
            relative.extend_from_slice(&entry.name);
            let label = shown(&relative);
            let local = match LocalName::from_remote(&entry.name, Rules::native()) {
                Ok(local) => local,
                Err(reason) => {
                    report.skipped.push((label, Skip::UnsafeName(reason)));
                    continue;
                }
            };
            if names.claim(&local).is_err() {
                report.skipped.push((label, Skip::Collision));
                continue;
            }
            let attributes = &entry.attributes;
            let remote = remote_dir.join(&entry.name);
            let local_path = local_dir.join(&local.name);
            if attributes.is_directory() {
                if depth + 1 > MAX_DEPTH {
                    report.skipped.push((label, Skip::TooDeep));
                } else {
                    pending.push((remote, local_path, relative, depth + 1));
                }
            } else if attributes.is_symlink() {
                report.skipped.push((label, Skip::Link));
            } else if attributes.is_regular_file() {
                let before = report.bytes;
                let result = download(client, &remote, &local_path, config, cancel, |done| {
                    progress(before + done);
                })
                .await;
                match result {
                    Ok(done) => {
                        report.files += 1;
                        report.bytes += done.bytes;
                    }
                    Err(error) if fatal(&error) => return Err(error.into()),
                    Err(error) => report
                        .skipped
                        .push((label, Skip::Failed(error.to_string()))),
                }
            } else {
                report.skipped.push((label, Skip::Special));
            }
        }
    }
    Ok(report)
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

#[cfg(unix)]
fn folder_mode(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & PLAIN_PERMISSIONS
}

#[cfg(not(unix))]
fn folder_mode(_metadata: &std::fs::Metadata) -> u32 {
    DEFAULT_FOLDER_MODE & PLAIN_PERMISSIONS
}

/// Creates remote folder `path`, accepting one that already exists as a folder.
async fn ensure_remote_folder(
    client: &SftpClient,
    path: &RemotePath,
    mode: u32,
) -> Result<bool, SftpError> {
    let created = client
        .mkdir(
            path,
            Attributes {
                permissions: Some(mode),
                ..Attributes::default()
            },
        )
        .await;
    match created {
        Ok(()) => Ok(true),
        Err(error) => match client.lstat(path).await {
            Ok(existing) if existing.is_directory() => Ok(false),
            _ => Err(error),
        },
    }
}

/// Uploads local folder `root` into remote folder `target`, created if needed; files
/// already there are replaced. Local links are not followed.
///
/// # Errors
///
/// As [`download_tree`].
pub async fn upload_tree(
    client: &SftpClient,
    root: &Path,
    target: &RemotePath,
    config: &TransferConfig,
    cancel: &CancellationToken,
    mut progress: impl FnMut(u64) + Send,
) -> Result<TreeReport, TreeError> {
    let mut report = TreeReport::default();
    let mut pending: Vec<(PathBuf, RemotePath, Vec<u8>, usize)> =
        vec![(root.to_owned(), target.clone(), Vec::new(), 0)];
    let mut visited = 0usize;
    while let Some((local_dir, remote_dir, below, depth)) = pending.pop() {
        if cancel.is_cancelled() {
            return Err(TransferError::Cancelled { kept: report.bytes }.into());
        }
        let local_error = |source| TransferError::Local {
            path: local_dir.clone(),
            source,
        };
        let metadata = tokio::fs::symlink_metadata(&local_dir)
            .await
            .map_err(local_error)?;
        if ensure_remote_folder(client, &remote_dir, folder_mode(&metadata)).await? {
            report.folders += 1;
        }
        let mut entries =
            tokio::fs::read_dir(&local_dir)
                .await
                .map_err(|source| TransferError::Local {
                    path: local_dir.clone(),
                    source,
                })?;
        while let Some(entry) =
            entries
                .next_entry()
                .await
                .map_err(|source| TransferError::Local {
                    path: local_dir.clone(),
                    source,
                })?
        {
            visited += 1;
            if visited > MAX_ENTRIES {
                return Err(TreeError::TooLarge);
            }
            let name = name_bytes(&entry.file_name());
            let mut relative = below.clone();
            if !relative.is_empty() {
                relative.push(b'/');
            }
            relative.extend_from_slice(&name);
            let label = shown(&relative);
            let path = entry.path();
            let Ok(kind) = tokio::fs::symlink_metadata(&path).await else {
                report
                    .skipped
                    .push((label, Skip::Failed("unreadable".to_owned())));
                continue;
            };
            let remote = remote_dir.join(&name);
            if kind.file_type().is_symlink() {
                report.skipped.push((label, Skip::Link));
            } else if kind.is_dir() {
                if depth + 1 > MAX_DEPTH {
                    report.skipped.push((label, Skip::TooDeep));
                } else {
                    pending.push((path, remote, relative, depth + 1));
                }
            } else if kind.is_file() {
                let before = report.bytes;
                let result = upload(client, &path, &remote, true, config, cancel, |done| {
                    progress(before + done);
                })
                .await;
                match result {
                    Ok(done) => {
                        report.files += 1;
                        report.bytes += done.bytes;
                    }
                    Err(error) if fatal(&error) => return Err(error.into()),
                    Err(error) => report
                        .skipped
                        .push((label, Skip::Failed(error.to_string()))),
                }
            } else {
                report.skipped.push((label, Skip::Special));
            }
        }
    }
    Ok(report)
}

/// Deletes `root` on the server: a file or a link alone, a folder with everything in it.
/// A link inside is removed as a link; what it points to is never entered. Returns the
/// number of entries removed.
///
/// # Errors
///
/// [`TreeError`]; entries removed before the error stay removed.
pub async fn remove_tree(
    client: &SftpClient,
    root: &RemotePath,
    cancel: &CancellationToken,
) -> Result<u64, TreeError> {
    let attributes = client.lstat(root).await?;
    if !attributes.is_directory() {
        client.remove(root).await?;
        return Ok(1);
    }
    let mut removed = 0u64;
    let mut visited = 0usize;
    // (folder, depth, emptied): a folder is listed first, removed once its entries are.
    let mut pending: Vec<(RemotePath, usize, bool)> = vec![(root.clone(), 0, false)];
    while let Some((folder, depth, emptied)) = pending.pop() {
        if cancel.is_cancelled() {
            return Err(TransferError::Cancelled { kept: removed }.into());
        }
        if emptied {
            client.rmdir(&folder).await?;
            removed += 1;
            continue;
        }
        if depth > MAX_DEPTH {
            return Err(TreeError::TooLarge);
        }
        pending.push((folder.clone(), depth, true));
        for entry in client.read_dir(&folder).await? {
            visited += 1;
            if visited > MAX_ENTRIES {
                return Err(TreeError::TooLarge);
            }
            let path = folder.join(&entry.name);
            // The listing's word is not enough to descend: a server that reports a link to
            // a folder as a folder would have the delete empty the link's target.
            if entry.attributes.is_directory() && client.lstat(&path).await?.is_directory() {
                pending.push((path, depth + 1, false));
            } else {
                client.remove(&path).await?;
                removed += 1;
            }
        }
    }
    Ok(removed)
}
