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

//! A server's file read and replaced with sudo, over the Files tab's own SSH connection, by
//! the scripts of [`heimdall_files::privileged`]: what "Open with sudo" and "Save with sudo"
//! run once the user asked for them.

use std::time::Duration;

use heimdall_files::RemotePath;
use heimdall_files::privileged::{
    AUTHENTICATION, CHANGED, METADATA, NOT_A_FILE, PASSWORD_NEEDED, Sudo, TOO_LARGE, TOOLING,
    UNTRUSTED_SUDO, read_output, read_script, replace_script,
};
use heimdall_files::server_copy::random_token;
use heimdall_ssh::{CommandEnd, ConnectError, Connection};
use tokio_util::sync::CancellationToken;

use crate::external_edit::{EDIT_SIZE_LIMIT, EditCheck, EditSession, Editor, hash, open_copy};
use crate::files::{FilesError, download_name};

/// The password sudo asked for, kept for a tab while it is open: never shown, never
/// logged, its bytes wiped when dropped.
#[derive(Clone)]
pub struct SudoPassword(zeroize::Zeroizing<Vec<u8>>);

impl SudoPassword {
    /// The password typed.
    #[must_use]
    pub fn new(typed: &str) -> Self {
        Self(zeroize::Zeroizing::new(typed.as_bytes().to_vec()))
    }

    /// Its bytes, for the script.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for SudoPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SudoPassword(..)")
    }
}

/// What a sudo script is given to: a POSIX shell reading it on its input.
const SHELL: &str = "sh -s";

/// The time a sudo script has, as the C# allows: ten minutes, and one more second for every
/// 32 KiB it carries.
fn time_for(bytes: usize) -> Duration {
    const BYTES_A_SECOND: usize = 32 * 1024;
    Duration::from_mins(10)
        + Duration::from_secs(u64::try_from(bytes / BYTES_A_SECOND).unwrap_or(u64::MAX))
}

/// The server's `remote` file read whole with sudo, [`EDIT_SIZE_LIMIT`] bytes at most, with
/// `password` when sudo asks for one; and the SHA-256 of what was read, which a later
/// [`sudo_replace`] must find still there.
///
/// # Errors
///
/// [`FilesError::SudoPasswordNeeded`] when sudo asks for a password and none was given, the
/// other sudo errors of [`FilesError`], [`FilesError::FileTooLarge`],
/// [`FilesError::NotAFile`], or the connection's.
pub async fn sudo_read(
    shell: &Connection,
    remote: &RemotePath,
    password: Option<&[u8]>,
    sudo: Sudo<'_>,
) -> Result<(Vec<u8>, [u8; 32]), FilesError> {
    let token = random_token().ok_or(FilesError::CopyRefused)?;
    let read = read_script(remote.as_bytes(), EDIT_SIZE_LIMIT, password, token, sudo)
        .map_err(|_| FilesError::InvalidName)?;
    let limit = usize::try_from(EDIT_SIZE_LIMIT).unwrap_or(usize::MAX);
    // Base64 is four bytes for three, with a line end every 76; and the line saying done.
    let kept = limit / 3 * 4 + limit / 57 + 1024;
    let ended = shell
        .run_command_keeping(
            SHELL,
            &read.script,
            kept,
            time_for(limit),
            CancellationToken::new(),
        )
        .await
        .map_err(|error| connection_error(&error))?;
    if ended.status != Some(0) {
        return Err(sudo_error(&ended));
    }
    let content = read_output(&ended.stdout, &read.done).ok_or(FilesError::CopyRefused)?;
    let sha256 = hash(&content);
    Ok((content, sha256))
}

/// Replaces the server's `remote` file with `content` through sudo, only while its content
/// still has the SHA-256 `expected`, with `password` when sudo asks for one.
///
/// # Errors
///
/// [`FilesError::ChangedOnServer`] with the file left as it is, the sudo errors of
/// [`FilesError`], [`FilesError::NotAFile`], or the connection's.
pub async fn sudo_replace(
    shell: &Connection,
    remote: &RemotePath,
    content: &[u8],
    expected: &[u8; 32],
    password: Option<&[u8]>,
    sudo: Sudo<'_>,
) -> Result<(), FilesError> {
    let token = random_token().ok_or(FilesError::CopyRefused)?;
    let replace = replace_script(remote.as_bytes(), content, expected, password, token, sudo)
        .map_err(|_| FilesError::InvalidName)?;
    let ended = shell
        .run_command(
            SHELL,
            &replace.script,
            time_for(content.len()),
            CancellationToken::new(),
        )
        .await
        .map_err(|error| connection_error(&error))?;
    if ended.status == Some(0) && ended.stdout == replace.done {
        Ok(())
    } else if ended.status == Some(0) {
        // Ended well without saying so: something else answered; taken for a refusal.
        Err(FilesError::CopyRefused)
    } else {
        Err(sudo_error(&ended))
    }
}

/// What a sudo script's end says, by its status, and for an authentication by what sudo
/// wrote on its error stream.
fn sudo_error(ended: &CommandEnd) -> FilesError {
    let said = String::from_utf8_lossy(&ended.stderr).to_lowercase();
    log::warn!(
        "sudo script ended with {:?}: {}",
        ended.status,
        crate::text::server_text(said.trim())
    );
    match ended.status {
        Some(NOT_A_FILE) => FilesError::NotAFile,
        Some(TOO_LARGE) => FilesError::FileTooLarge,
        Some(METADATA) => FilesError::ReplaceNotSafe,
        Some(TOOLING) => FilesError::SudoToolingMissing,
        Some(UNTRUSTED_SUDO) => FilesError::SudoUntrusted,
        Some(CHANGED) => FilesError::ChangedOnServer,
        Some(PASSWORD_NEEDED) => FilesError::SudoPasswordNeeded,
        Some(AUTHENTICATION) if said.contains("tty") || said.contains("terminal") => {
            FilesError::SudoNeedsTerminal
        }
        Some(AUTHENTICATION)
            if said.contains("incorrect password")
                || said.contains("try again")
                || said.contains("no password was provided") =>
        {
            FilesError::SudoPasswordRejected
        }
        _ => FilesError::SudoFailed,
    }
}

/// A failure of the connection itself.
fn connection_error(error: &ConnectError) -> FilesError {
    match error {
        ConnectError::CommandRefused | ConnectError::Timeout => FilesError::SudoFailed,
        _ => FilesError::SessionClosed,
    }
}

/// Opens the server's `remote` file through sudo, as "Edit with sudo": read with sudo,
/// copied into a folder of the user's own and opened in `editor`; a privileged edit, each
/// save sent with sudo.
///
/// # Errors
///
/// As [`sudo_read`], and as the copy of [`crate::external_edit::start_edit`].
pub async fn start_sudo_edit(
    shell: Connection,
    remote: RemotePath,
    editor: Editor,
    folders: (std::path::PathBuf, Vec<std::path::PathBuf>),
    password: Option<SudoPassword>,
) -> Result<EditSession, FilesError> {
    let file_name = remote.file_name().ok_or(FilesError::NotAFile)?.to_vec();
    let local_name = download_name(&file_name)?;
    let (data, sent) = sudo_read(
        &shell,
        &remote,
        password.as_ref().map(SudoPassword::bytes),
        Sudo::System,
    )
    .await?;
    let (local, seen) = open_copy(local_name, data, editor, folders).await?;
    Ok(EditSession {
        name: crate::text::server_text(&heimdall_files::display_bytes(&file_name)),
        remote,
        local,
        sent,
        // Not read: a privileged edit is checked by its content, with sudo.
        fingerprint: heimdall_files::Fingerprint {
            size: None,
            modified: None,
            permissions: None,
            uid_gid: None,
        },
        seen,
        candidate: None,
        refused: None,
        privileged: true,
    })
}

/// Sends `session`'s local copy through sudo, as "Save with sudo" asks the first time:
/// read now, then [`send_with_sudo`].
pub async fn save_with_sudo(
    shell: &Connection,
    session: &EditSession,
    password: Option<&SudoPassword>,
) -> EditCheck {
    let local = async {
        let modified = tokio::fs::metadata(&session.local).await?.modified()?;
        let data = tokio::fs::read(&session.local).await?;
        Ok::<_, std::io::Error>((modified, data))
    };
    match local.await {
        Ok((modified, data)) => send_with_sudo(shell, session, password, (modified, &data)).await,
        Err(error) => EditCheck::Failed(FilesError::Local {
            detail: error.to_string(),
        }),
    }
}

/// Sends `data`, `session`'s save of `modified`, through sudo, only over the content the
/// server has from us.
pub(crate) async fn send_with_sudo(
    shell: &Connection,
    session: &EditSession,
    password: Option<&SudoPassword>,
    (modified, data): (std::time::SystemTime, &[u8]),
) -> EditCheck {
    if data.len() as u64 > EDIT_SIZE_LIMIT {
        return EditCheck::Refused {
            modified,
            error: FilesError::FileTooLarge,
        };
    }
    match sudo_replace(
        shell,
        &session.remote,
        data,
        &session.sent,
        password.map(SudoPassword::bytes),
        Sudo::System,
    )
    .await
    {
        Ok(()) => EditCheck::Sent {
            modified,
            sent: hash(data),
            fingerprint: session.fingerprint,
        },
        Err(FilesError::SessionClosed) => EditCheck::Failed(FilesError::SessionClosed),
        Err(error) => EditCheck::Refused { modified, error },
    }
}
