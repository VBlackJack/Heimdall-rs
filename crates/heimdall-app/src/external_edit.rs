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

//! A server's file edited with an external editor, as the C# SFTP view's "Edit with
//! external editor": copied to a folder of the user's own, opened in the editor, and sent
//! back each time the editor saves it.
//!
//! Safer than the C#, whose last save wins:
//! - a save is sent only while the server's file is still the one opened, or last sent: one
//!   changed meanwhile is left as it is and the save kept, said on the pane;
//! - a save counts once it holds still across two looks, then is read whole and hashed: an
//!   editor saving by rename, or writing in several goes, is never sent half written, and a
//!   save that changes nothing is not sent;
//! - what is recorded as sent is what was sent, so a save landing during an upload is sent
//!   at the next look.
//!
//! The server's file is replaced by a rename: it gets a new owner, its ACLs, extended
//! attributes and hard links are lost, and a folder the user cannot write refuses it.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use heimdall_files::server_copy::random_token;
use heimdall_files::{Fingerprint, RemotePath, RemoteSession};
use tokio_util::sync::CancellationToken;

use crate::files::{FilesError, download_name};

/// How often edited files are looked at: a save is sent within two looks.
pub const EDIT_LOOK: std::time::Duration = std::time::Duration::from_secs(2);

/// Largest file edited, as the C#: a larger one is downloaded instead.
pub const EDIT_SIZE_LIMIT: u64 = 16 * 1024 * 1024;

/// What runs the editor: a program, and what goes before the file's path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    /// The program.
    pub program: PathBuf,
    /// Its arguments before the file.
    pub arguments: Vec<OsString>,
}

/// Why the editor chosen is not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorRefused {
    /// Nothing at that path.
    NotFound(String),
    /// A shell, a script host or an interpreter: it would run the file, not show it, as the
    /// C# refuses them.
    Runs(String),
    /// Not a program Windows runs by itself: a `.bat` or `.cmd` goes through `cmd.exe`.
    NotAProgram(String),
}

/// Programs that run what they are given rather than show it.
const RUNNERS: &[&str] = &[
    "bash",
    "bitsadmin",
    "certutil",
    "cmd",
    "command",
    "conhost",
    "csh",
    "cscript",
    "dash",
    "env",
    "fish",
    "ksh",
    "mshta",
    "msiexec",
    "osascript",
    "powershell",
    "powershell_ise",
    "pwsh",
    "regsvr32",
    "rundll32",
    "sh",
    "sudo",
    "tcsh",
    "wscript",
    "wsl",
    "zsh",
];

/// Interpreters, whatever their version: `python3.12`, `node18`.
const INTERPRETERS: &[&str] = &["node", "perl", "php", "py", "python", "ruby"];

/// The editor `setting` names, or the system's own text editor when it is empty.
///
/// # Errors
///
/// [`EditorRefused`] for a path that is not there, a shell, a script host or an
/// interpreter, and on Windows anything but an `.exe`. A setting the user wrote: this keeps
/// a slip from running a file, it is not a security boundary.
pub fn editor(setting: &str) -> Result<Editor, EditorRefused> {
    let setting = setting.trim().trim_matches('"');
    if setting.is_empty() {
        return Ok(system_editor());
    }
    let named = expanded(setting);
    let program =
        std::fs::canonicalize(&named).map_err(|_| EditorRefused::NotFound(named.clone()))?;
    // The name as written and the one it leads to: a link named "editor" to sh is sh.
    for path in [Path::new(&named), program.as_path()] {
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let runs = RUNNERS.contains(&stem.as_str())
            || INTERPRETERS.iter().any(|name| {
                stem.strip_prefix(name)
                    .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit() || c == '.'))
            });
        if runs {
            return Err(EditorRefused::Runs(named.clone()));
        }
    }
    if cfg!(windows)
        && !program
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err(EditorRefused::NotAProgram(named));
    }
    Ok(Editor {
        program,
        arguments: Vec::new(),
    })
}

/// The system's own text editor: Notepad, as the C# default; on macOS the text editor
/// `open -t` names; elsewhere what `xdg-open` picks for the file.
fn system_editor() -> Editor {
    if cfg!(windows) {
        Editor {
            program: system_program(NOTEPAD_PROGRAM),
            arguments: Vec::new(),
        }
    } else if cfg!(target_os = "macos") {
        Editor {
            program: PathBuf::from("/usr/bin/open"),
            arguments: vec![OsString::from("-t")],
        }
    } else {
        Editor {
            program: PathBuf::from("xdg-open"),
            arguments: Vec::new(),
        }
    }
}

/// `setting` with its `%NAME%` variables replaced, as the C# default `%windir%` is written;
/// on Windows only.
fn expanded(setting: &str) -> String {
    if !cfg!(windows) {
        return setting.to_owned();
    }
    let mut out = String::new();
    let mut rest = setting;
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else { break };
        out.push_str(&rest[..start]);
        let name = &after[..end];
        match std::env::var(name) {
            Ok(value) if !name.is_empty() => out.push_str(&value),
            _ => out.push_str(&rest[start..start + end + 2]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Starts `editor` on `file`, without a shell between them.
///
/// # Errors
///
/// What the system said when the program could not start.
pub fn launch(editor: &Editor, file: &Path) -> io::Result<()> {
    std::process::Command::new(&editor.program)
        .args(&editor.arguments)
        .arg(file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
}

/// How long an edit's folder is kept after its last change, as the C# sweeper.
pub const EDIT_FOLDER_KEPT: std::time::Duration = std::time::Duration::from_hours(24);

/// Removes the folders under `base` whose newest file changed more than
/// [`EDIT_FOLDER_KEPT`] ago, but those in `keep`: edits left behind by a tab closed, or a
/// run that ended. A folder in use by another run is kept as long as its file changes.
pub fn sweep(base: &Path, keep: &[PathBuf]) {
    let Ok(folders) = std::fs::read_dir(base) else {
        return;
    };
    let now = SystemTime::now();
    for folder in folders.flatten() {
        let path = folder.path();
        if keep.contains(&path) || !folder.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let newest = std::fs::read_dir(&path)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|file| file.metadata().and_then(|found| found.modified()).ok())
            .chain(folder.metadata().and_then(|found| found.modified()).ok())
            .max();
        let old = newest.is_some_and(|newest| {
            now.duration_since(newest)
                .is_ok_and(|age| age > EDIT_FOLDER_KEPT)
        });
        if old {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

/// A new folder of the user's own under `base` for one edit: `base` made, or found, the
/// user's only (unix: mode 0700, never a link; Windows: never a reparse point, and under
/// the user's profile), the folder created anew, never taken over.
///
/// # Errors
///
/// [`io::ErrorKind::PermissionDenied`] when `base` cannot be kept the user's only; what the
/// file system said otherwise.
pub fn edit_folder(base: &Path) -> io::Result<PathBuf> {
    std::fs::create_dir_all(base)?;
    private_base(base)?;
    let token = random_token().ok_or_else(|| io::Error::other("no random source"))?;
    let name = token.iter().fold(String::new(), |mut name, byte| {
        let _ = write!(name, "{byte:02x}");
        name
    });
    let folder = base.join(name);
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt as _;
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        builder
    };
    #[cfg(not(unix))]
    let builder = std::fs::DirBuilder::new();
    builder.create(&folder)?;
    Ok(folder)
}

#[cfg(unix)]
pub(crate) fn private_base(base: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let found = std::fs::symlink_metadata(base)?;
    if !found.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "not a folder",
        ));
    }
    if found.permissions().mode() & 0o077 != 0 {
        // Only its owner can take it back: anyone else's is refused here.
        std::fs::set_permissions(base, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "not the user's own"))?;
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn private_base(base: &Path) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt as _;
    /// `FILE_ATTRIBUTE_REPARSE_POINT`: a link or a junction.
    const REPARSE_POINT: u32 = 0x400;
    let found = std::fs::symlink_metadata(base)?;
    if !found.file_type().is_dir() || found.file_attributes() & REPARSE_POINT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "not a plain folder",
        ));
    }
    Ok(())
}

/// A file being edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditSession {
    /// The server's file.
    pub remote: RemotePath,
    /// Its name, made safe, to say.
    pub name: String,
    /// The local copy the editor has.
    pub local: PathBuf,
    /// The hash of what the server has from us: the content opened, then each save sent.
    pub sent: [u8; 32],
    /// What the server said of its file once it had that content.
    pub fingerprint: Fingerprint,
    /// The local copy's modification time last dealt with.
    pub seen: Option<SystemTime>,
    /// A newer modification time, seen once: dealt with when seen again unchanged.
    pub candidate: Option<SystemTime>,
    /// Why the last save was not sent, until one is.
    pub refused: Option<FilesError>,
    /// Saved with sudo, as the user asked: every save goes through it, only over the content
    /// the server has from us (its SHA-256, [`EditSession::sent`]).
    pub privileged: bool,
}

/// SHA-256 of `data`.
#[must_use]
pub fn hash(data: &[u8]) -> [u8; 32] {
    let digest = ring::digest::digest(&ring::digest::SHA256, data);
    let mut out = [0; 32];
    out.copy_from_slice(digest.as_ref());
    out
}

/// Copies the server's `remote` file into a new folder of the user's own under `base`,
/// readable by the user only, and starts `editor` on it.
///
/// # Errors
///
/// [`FilesError::FileTooLarge`], [`FilesError::WorkingFolderUnprotected`],
/// [`FilesError::EditorFailed`], or the server's.
pub async fn start_edit(
    client: RemoteSession,
    remote: RemotePath,
    editor: Editor,
    (base, keep): (PathBuf, Vec<PathBuf>),
    cancel: CancellationToken,
) -> Result<EditSession, FilesError> {
    let file_name = remote.file_name().ok_or(FilesError::NotAFile)?.to_vec();
    let local_name = download_name(&file_name)?;
    let (data, fingerprint) = client
        .read_whole(&remote, EDIT_SIZE_LIMIT, &cancel)
        .await
        .map_err(|error| FilesError::from(&error))?;
    let sent = hash(&data);
    let (local, seen) = open_copy(local_name, data, editor, (base, keep)).await?;
    Ok(EditSession {
        name: crate::text::server_text(&heimdall_files::display_bytes(&file_name)),
        remote,
        local,
        sent,
        fingerprint,
        seen,
        candidate: None,
        refused: None,
        privileged: false,
    })
}

/// Writes `data` as `name` in a new folder of the user's own under `base`, readable by the
/// user only, the old folders but `keep` swept, and starts `editor` on it: the copy, and
/// its modification time as written.
pub(crate) async fn open_copy(
    name: heimdall_files::LocalName,
    data: Vec<u8>,
    editor: Editor,
    (base, keep): (PathBuf, Vec<PathBuf>),
) -> Result<(PathBuf, Option<SystemTime>), FilesError> {
    tokio::task::spawn_blocking(
        move || -> Result<(PathBuf, Option<SystemTime>), FilesError> {
            let folder = edit_folder(&base).map_err(|_| FilesError::WorkingFolderUnprotected)?;
            sweep(&base, &keep);
            let local = folder.join(&name.name);
            write_new(&local, &data).map_err(|error| FilesError::Local {
                detail: error.to_string(),
            })?;
            let modified = std::fs::metadata(&local)
                .and_then(|found| found.modified())
                .ok();
            launch(&editor, &local).map_err(|error| FilesError::EditorFailed {
                detail: error.to_string(),
            })?;
            Ok((local, modified))
        },
    )
    .await
    .map_err(|error| FilesError::Local {
        detail: error.to_string(),
    })?
}

/// Writes `data` to a file that must not exist yet, readable by the user only.
pub(crate) fn write_new(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(data)?;
    file.sync_all()
}

/// What a look at an edited file found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditCheck {
    /// Nothing new, or a save still being written.
    Unchanged,
    /// A newer save, seen once: looked at again next time.
    Saving(SystemTime),
    /// Saved with nothing changed since what was sent.
    Same(SystemTime),
    /// Sent: the save's time, what was sent, and the server's file once replaced.
    Sent {
        /// The save's modification time.
        modified: SystemTime,
        /// The hash of what was sent.
        sent: [u8; 32],
        /// The server's file once replaced.
        fingerprint: Fingerprint,
    },
    /// Not sent, and not tried again until saved again: the save's time and why.
    Refused {
        /// The save's modification time.
        modified: SystemTime,
        /// Why.
        error: FilesError,
    },
    /// Not sent for now (the connection): tried again next time.
    Failed(FilesError),
}

/// Looks at `session`'s local copy and sends a save that holds still, only while the
/// server's file is still the one it replaces: through sudo for a privileged edit, over the
/// tab's SSH connection with the password kept for the tab, `sudo`.
pub async fn check_edit(
    client: &RemoteSession,
    session: &EditSession,
    sudo: Option<(
        &heimdall_ssh::Connection,
        Option<&crate::sudo_edit::SudoPassword>,
    )>,
) -> EditCheck {
    // Missing for a moment, or locked, while an editor saves by rename: not saved yet.
    let Ok(modified) = tokio::fs::metadata(&session.local)
        .await
        .and_then(|found| found.modified())
    else {
        return EditCheck::Unchanged;
    };
    if Some(modified) == session.seen {
        return EditCheck::Unchanged;
    }
    if Some(modified) != session.candidate {
        return EditCheck::Saving(modified);
    }
    let Ok(data) = tokio::fs::read(&session.local).await else {
        return EditCheck::Unchanged;
    };
    if data.len() as u64 > EDIT_SIZE_LIMIT {
        return EditCheck::Refused {
            modified,
            error: FilesError::FileTooLarge,
        };
    }
    let sent = hash(&data);
    if sent == session.sent {
        return EditCheck::Same(modified);
    }
    if session.privileged {
        return match sudo {
            Some((shell, password)) => {
                crate::sudo_edit::send_with_sudo(shell, session, password, (modified, &data)).await
            }
            None => EditCheck::Failed(FilesError::SessionClosed),
        };
    }
    match client
        .replace_if(
            &session.remote,
            &data,
            &session.fingerprint,
            &CancellationToken::new(),
        )
        .await
    {
        Ok(fingerprint) => EditCheck::Sent {
            modified,
            sent,
            fingerprint,
        },
        Err(error) => {
            let error = FilesError::from(&error);
            if error == FilesError::SessionClosed {
                EditCheck::Failed(error)
            } else {
                EditCheck::Refused { modified, error }
            }
        }
    }
}

/// Sends `session`'s local copy over the server's file as it is now, after the user saw
/// the refusal: still only while the server's file stays as it was just read.
pub async fn send_anyway(client: &RemoteSession, session: &EditSession) -> EditCheck {
    let local = async {
        let modified = tokio::fs::metadata(&session.local).await?.modified()?;
        let data = tokio::fs::read(&session.local).await?;
        Ok::<_, io::Error>((modified, data))
    };
    let (modified, data) = match local.await {
        Ok(local) => local,
        Err(error) => {
            return EditCheck::Failed(FilesError::Local {
                detail: error.to_string(),
            });
        }
    };
    let now = match client.fingerprint(&session.remote).await {
        Ok(now) => now,
        Err(error) => return EditCheck::Failed(FilesError::from(&error)),
    };
    let sent = hash(&data);
    match client
        .replace_if(&session.remote, &data, &now, &CancellationToken::new())
        .await
    {
        Ok(fingerprint) => EditCheck::Sent {
            modified,
            sent,
            fingerprint,
        },
        Err(error) => EditCheck::Refused {
            modified,
            error: FilesError::from(&error),
        },
    }
}

/// Opens `folder` in the system's file manager.
///
/// # Errors
///
/// What the system said when it could not start.
pub fn open_folder(folder: &Path) -> io::Result<()> {
    let manager = if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "/usr/bin/open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(manager)
        .arg(folder)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
}

/// Opens local file `file` with the system's default program for it, as a double click
/// in the system's file manager would: on Windows the shell's handler through
/// `rundll32.exe url.dll,FileProtocolHandler`, as web addresses are opened; on macOS
/// `open`; elsewhere `xdg-open`. The path goes as one argument of its own, no shell reading
/// it; the file manager itself opens folders only.
///
/// # Errors
///
/// [`io::ErrorKind::InvalidInput`] for a path that is not absolute, which a handler could
/// take for a switch; what the system said when the handler could not start.
pub fn open_with_default(file: &Path) -> io::Result<()> {
    if !file.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not an absolute path",
        ));
    }
    let mut command = if cfg!(windows) {
        let mut command = std::process::Command::new(system_program(RUNDLL_PROGRAM));
        command.arg(FILE_HANDLER_ENTRY);
        command
    } else if cfg!(target_os = "macos") {
        std::process::Command::new(MAC_OPEN_PROGRAM)
    } else {
        std::process::Command::new(XDG_OPEN_PROGRAM)
    };
    command
        .arg(file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
}

/// The system's own text editor on Windows, as the C# default.
const NOTEPAD_PROGRAM: &str = "notepad.exe";
/// The Windows program that calls a library's entry point.
const RUNDLL_PROGRAM: &str = "rundll32.exe";
/// The shell's entry point opening a file or an address with its default program.
const FILE_HANDLER_ENTRY: &str = "url.dll,FileProtocolHandler";
/// What opens a file with its default program on macOS.
const MAC_OPEN_PROGRAM: &str = "/usr/bin/open";
/// What opens a file with its default program on other Unix desktops.
const XDG_OPEN_PROGRAM: &str = "xdg-open";
/// Windows' folder of its own programs, under its own folder.
const SYSTEM_FOLDER: &str = "system32";
/// Windows' own folder, when the environment does not say.
const DEFAULT_WINDOWS_FOLDER: &str = r"C:\Windows";

/// Windows program `name` in Windows' own folder of programs: never one of the same name
/// found first somewhere else.
fn system_program(name: &str) -> PathBuf {
    std::env::var_os("WINDIR")
        .or_else(|| std::env::var_os("SystemRoot"))
        .map_or_else(|| PathBuf::from(DEFAULT_WINDOWS_FOLDER), PathBuf::from)
        .join(SYSTEM_FOLDER)
        .join(name)
}

impl EditSession {
    /// Takes what a look found: what was dealt with, what was sent.
    pub fn apply(&mut self, check: &EditCheck) {
        match check {
            EditCheck::Unchanged | EditCheck::Failed(_) => {}
            EditCheck::Saving(modified) => self.candidate = Some(*modified),
            EditCheck::Same(modified) => {
                self.seen = Some(*modified);
                self.candidate = None;
                self.refused = None;
            }
            EditCheck::Refused { modified, error } => {
                self.seen = Some(*modified);
                self.candidate = None;
                self.refused = Some(error.clone());
            }
            EditCheck::Sent {
                modified,
                sent,
                fingerprint,
            } => {
                self.seen = Some(*modified);
                self.candidate = None;
                self.sent = *sent;
                self.fingerprint = *fingerprint;
                self.refused = None;
            }
        }
    }
}
