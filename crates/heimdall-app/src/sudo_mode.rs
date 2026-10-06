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

//! A Files tab's sudo mode over its own SSH connection, by the scripts of
//! [`heimdall_files::privileged_mode`]: the server's folder listed as root, an entry deleted
//! as root once confirmed, permissions given as root when the server refuses them to the
//! account. The password is the tab's, as "Edit with sudo" keeps it.

use std::time::Duration;

use heimdall_files::privileged::Sudo;
use heimdall_files::privileged_mode::{
    CHANGED, NOT_A_FOLDER, chmod_script, deletable_as_root, list_output, list_script, remove_script,
};
use heimdall_files::server_copy::random_token;
use heimdall_files::{ItemKind, Refusal, RemoteError, RemotePath, RemoteSession, Special};
use heimdall_ssh::{CommandEnd, ConnectError, Connection};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroize as _;

use crate::files::{EntryKind, FilesError, RemoteEntry, sort_remote};
use crate::sudo_edit::{SHELL, SudoPassword, connection_error, sudo_error};

/// The time one listing, delete or change as root has, as the C# allows a sudo command: a
/// large folder takes long to delete.
const ROOT_TIME: Duration = Duration::from_mins(10);

/// The most of a listing's output kept: a folder whose records and names go past it is too
/// large to list as root.
const LISTING_KEPT: usize = 32 * 1024 * 1024;

/// What runs as root in a tab: its SSH connection, and the password sudo took for it.
#[derive(Debug, Clone)]
pub struct SudoAccess {
    /// The SSH connection under the tab's SFTP session.
    pub shell: Connection,
    /// The password sudo took for the tab; none when sudo asks for none.
    pub password: Option<SudoPassword>,
}

impl SudoAccess {
    /// The password's bytes, for the script.
    fn password(&self) -> Option<&[u8]> {
        self.password.as_ref().map(SudoPassword::bytes)
    }
}

/// The server's folder `folder` listed as root, with `password` when sudo asks for one:
/// the folder as the server names it once entered, and its entries, each with its inode.
///
/// # Errors
///
/// [`FilesError::SudoPasswordNeeded`] when sudo asks for a password and none was given, the
/// other sudo errors of [`FilesError`], [`FilesError::Server`] for a folder that cannot be
/// entered, [`FilesError::TooLarge`] past what is kept, or the connection's.
pub async fn sudo_list(
    shell: &Connection,
    folder: &RemotePath,
    password: Option<&[u8]>,
    sudo: Sudo<'_>,
) -> Result<(RemotePath, Vec<RemoteEntry>), FilesError> {
    let token = random_token().ok_or(FilesError::SudoFailed)?;
    let mut list = list_script(folder.as_bytes(), password, token, sudo)
        .map_err(|_| FilesError::InvalidName)?;
    let ended = shell
        .run_command_keeping(
            SHELL,
            &list.script,
            LISTING_KEPT,
            ROOT_TIME,
            CancellationToken::new(),
        )
        .await;
    list.script.zeroize();
    let ended = ended.map_err(|error| connection_error(&error))?;
    match ended.status {
        Some(0) => {}
        Some(NOT_A_FOLDER) => {
            return Err(FilesError::Server {
                refusal: Refusal::NoSuchFile,
                message: String::new(),
            });
        }
        _ => return Err(sudo_error(&ended)),
    }
    if ended.stdout.len() >= LISTING_KEPT {
        return Err(FilesError::TooLarge);
    }
    let listing = list_output(&ended.stdout, &list.done).ok_or(FilesError::SudoFailed)?;
    let mut entries: Vec<RemoteEntry> = listing
        .entries
        .into_iter()
        .map(RemoteEntry::from_sudo_listing)
        .collect();
    sort_remote(&mut entries);
    Ok((RemotePath::from_bytes(listing.folder), entries))
}

/// Deletes the server's `path` as root, only while it is still of `kind` and `inode` as
/// listed when the delete was confirmed: a folder with everything in it on its own file
/// system, anything else alone, never what a link points to. Refused for what
/// [`deletable_as_root`] refuses, `home` the account's home folder.
///
/// # Errors
///
/// [`FilesError::SudoProtected`], [`FilesError::ChangedSinceConfirmed`] with the entry left
/// as it is, the sudo errors of [`FilesError`], or the connection's.
pub async fn sudo_remove(
    access: &SudoAccess,
    path: &RemotePath,
    (kind, inode): (EntryKind, Option<u64>),
    home: Option<&RemotePath>,
    sudo: Sudo<'_>,
) -> Result<(), FilesError> {
    let target =
        deletable_as_root(path.as_bytes(), home.map(RemotePath::as_bytes)).map_err(|refused| {
            log::warn!("delete as root refused: {refused:?}");
            FilesError::SudoProtected
        })?;
    let kind = match kind {
        EntryKind::Directory => ItemKind::Directory,
        EntryKind::File => ItemKind::File,
        EntryKind::Link => ItemKind::Link,
        EntryKind::Other(Special::Unknown) => return Err(FilesError::NotAFile),
        EntryKind::Other(special) => ItemKind::Other(special),
    };
    // Not listed as root: nothing to check it against.
    let inode = inode.ok_or(FilesError::ChangedSinceConfirmed)?;
    let token = random_token().ok_or(FilesError::SudoFailed)?;
    let mut remove = remove_script(&target, (kind, inode), access.password(), token, sudo)
        .map_err(|_| FilesError::InvalidName)?;
    let ended = access
        .shell
        .run_command(SHELL, &remove.script, ROOT_TIME, CancellationToken::new())
        .await;
    remove.script.zeroize();
    finished(ended, &remove.done, FilesError::ChangedSinceConfirmed)
}

/// Gives the server's `path` the permission bits `mode` as root, as the C# chmod fallback;
/// never through a link.
///
/// # Errors
///
/// [`FilesError::IsLink`], the sudo errors of [`FilesError`], or the connection's.
pub async fn sudo_chmod(
    access: &SudoAccess,
    path: &RemotePath,
    mode: u32,
    sudo: Sudo<'_>,
) -> Result<(), FilesError> {
    let token = random_token().ok_or(FilesError::SudoFailed)?;
    let mut change = chmod_script(path.as_bytes(), mode, access.password(), token, sudo)
        .map_err(|_| FilesError::InvalidName)?;
    let ended = access
        .shell
        .run_command(SHELL, &change.script, ROOT_TIME, CancellationToken::new())
        .await;
    change.script.zeroize();
    match finished(ended, &change.done, FilesError::ChangedSinceConfirmed) {
        Err(FilesError::NotAFile) => Err(FilesError::IsLink),
        result => result,
    }
}

/// Gives the server's `path` the permission bits `mode` over the session; refused there for
/// permissions while the sudo mode is on (`sudo` given), as root instead, as the C# chmod
/// fallback.
///
/// # Errors
///
/// The session's, when it refuses for another reason or the sudo mode is off; else as
/// [`sudo_chmod`].
pub async fn chmod_or_sudo(
    client: &RemoteSession,
    sudo_access: Option<&SudoAccess>,
    path: &RemotePath,
    mode: u32,
    sudo: Sudo<'_>,
) -> Result<(), FilesError> {
    let refused = match client.set_permissions(path, mode).await {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    let denied = matches!(
        refused,
        RemoteError::Refused {
            refusal: Refusal::PermissionDenied,
            ..
        }
    );
    match sudo_access {
        Some(access) if denied => {
            log::info!("permissions refused to the account: given as root");
            sudo_chmod(access, path, mode, sudo).await
        }
        _ => Err(FilesError::from(&refused)),
    }
}

/// How a script run as root ended: done when it said so; `changed` for [`CHANGED`].
fn finished(
    ended: Result<CommandEnd, ConnectError>,
    done: &[u8],
    changed: FilesError,
) -> Result<(), FilesError> {
    let ended = ended.map_err(|error| connection_error(&error))?;
    match ended.status {
        Some(0) if ended.stdout == done => Ok(()),
        // Ended well without saying so: something else answered; taken for a refusal.
        Some(0) => Err(FilesError::SudoFailed),
        Some(CHANGED) => Err(changed),
        _ => Err(sudo_error(&ended)),
    }
}
