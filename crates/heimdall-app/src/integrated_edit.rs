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

//! The integrated editor of a Files tab, as the C# one: a server's text file shown in
//! place of the two lists, saved back only over the file it was opened from.
//!
//! The text itself lives with the window, which draws it; this side knows the file, how
//! it is stored, the server's file as last read or saved, and what the editor says.

use heimdall_files::{Fingerprint, RemotePath, RemoteSession};
use tokio_util::sync::CancellationToken;

use crate::files::FilesError;
use crate::ids::EditorId;
use crate::text_codec::{TextEncoding, Unencodable, decode, looks_binary};

/// Largest file the integrated editor opens. The C# takes 16 MiB, but iced lays out the
/// whole text when it takes it, on the window's thread: measured 2026-10-04 at about
/// 0.67 s per MiB, so the cap is where opening stays near half a second, as Julien set it.
/// A larger file opens in the external editor, which takes 16 MiB.
pub const INTEGRATED_EDIT_LIMIT: u64 = 512 * 1024;

/// A server's file open in a Files tab's integrated editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegratedEdit {
    /// The editor, so a late answer for an earlier one is told apart.
    pub id: EditorId,
    /// The server's file.
    pub remote: RemotePath,
    /// Its name, as shown.
    pub name: String,
    /// How it is stored and the server's file as last read or saved; `None` while it is
    /// read.
    pub opened: Option<(TextEncoding, Fingerprint)>,
    /// The text differs from what was last read or saved.
    pub dirty: bool,
    /// A save is under way.
    pub saving: bool,
    /// What the editor says, under its title.
    pub notice: Option<EditorNotice>,
}

/// What the integrated editor says of its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorNotice {
    /// The file is not UTF-8: opened as Latin-1, and saved as Latin-1.
    Latin1,
    /// The text was saved on the server.
    Saved,
    /// The server's file changed since it was opened: not written over, unless asked.
    ChangedOnServer,
    /// A character the file's encoding cannot store: nothing was saved.
    Unencodable(Unencodable),
    /// A save is still under way.
    SaveRunning,
    /// The session ended: the text is kept, saved once connected again.
    SessionEnded,
    /// The save failed.
    Failed(FilesError),
}

/// A server's file read for the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// Its text.
    pub text: String,
    /// How it is stored.
    pub encoding: TextEncoding,
    /// The server's file as read.
    pub fingerprint: Fingerprint,
}

/// Reads `remote` for the editor: [`INTEGRATED_EDIT_LIMIT`] at most, and text only.
///
/// # Errors
///
/// The server's refusal, a file too large, one that looks like a program, an image or an
/// archive, or one whose byte order mark its bytes do not follow.
pub async fn open(client: RemoteSession, remote: RemotePath) -> Result<Opened, FilesError> {
    let (bytes, fingerprint) = client
        .read_whole(&remote, INTEGRATED_EDIT_LIMIT, &CancellationToken::new())
        .await
        .map_err(|error| match FilesError::from(&error) {
            FilesError::FileTooLarge => FilesError::TooLargeForEditor,
            error => error,
        })?;
    if looks_binary(&bytes) {
        return Err(FilesError::LooksBinary);
    }
    let decoded = decode(&bytes).map_err(|_| FilesError::NotText)?;
    Ok(Opened {
        text: decoded.text,
        encoding: decoded.encoding,
        fingerprint,
    })
}

/// Saves `bytes` as `remote`, only over the file `expected` describes; over whatever is
/// there when `expected` is `None`, the user having said to. The server's file as saved.
///
/// # Errors
///
/// The server's file changed since, or the server's refusal.
pub async fn save(
    client: RemoteSession,
    remote: RemotePath,
    bytes: Vec<u8>,
    expected: Option<Fingerprint>,
) -> Result<Fingerprint, FilesError> {
    let expected = match expected {
        Some(expected) => expected,
        None => client
            .fingerprint(&remote)
            .await
            .map_err(|error| FilesError::from(&error))?,
    };
    client
        .replace_if(&remote, &bytes, &expected, &CancellationToken::new())
        .await
        .map_err(|error| FilesError::from(&error))
}
