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

//! FTP and explicit FTPS for the Files tab, as the C# Heimdall's FTP profiles: one control
//! connection, passive data connections, binary transfers.
//!
//! FTP runs one command at a time on its control connection: the client holds it behind a
//! lock, and every operation takes it for its whole length, a folder walk included.
//!
//! The Files guarantees, the FTP way:
//! - a rename never replaces: the target is looked up first and an existing one refused.
//!   FTP has no atomic form of this; the window between the look and the rename is the
//!   same as the C# client's.
//! - a delete never follows a link: a link, which a listing shows as such, is deleted with
//!   `DELE`, never walked.
//! - a download reads only a regular file, into `<name>.heimdall-part`, renamed once
//!   complete. A later attempt resumes with `REST` when the remote file has the same size
//!   and date as when it started, recorded beside the part file; otherwise it starts over.
//! - an upload goes to a hidden temporary file beside the target, renamed onto it once
//!   complete: nobody sees a partial file under the real name.
//! - a file replaced (an upload asked to, an editor's save) is replaced recoverably, as the
//!   C# FTP commit: FTP has no rename that replaces on every server, so the old file is
//!   moved aside to a hidden backup name, the new one renamed onto its name, and the backup
//!   deleted only then. A failed rename puts the old file back; should that fail too, the
//!   old file is kept under its backup name, never deleted. Between the two renames, a few
//!   milliseconds, the name is free. The new file takes the server's permissions for a new
//!   file: a listing does not always say the old one's, and guessing them could open it up.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use heimdall_sftp::RemotePath;
use heimdall_sftp::transfer::Fingerprint;
use suppaftp::list::{File as Listed, ListParser};
use suppaftp::tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream};
use suppaftp::types::{FileType, Mode};
use suppaftp::{FtpError, Status};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::{ItemKind, Refusal, RemoteError, RemoteItem};

/// Suffix of a file being downloaded, as for SFTP.
const PART_SUFFIX: &str = ".heimdall-part";

/// Suffix of the record beside a part file that says what it was downloaded from.
const RESUME_SUFFIX: &str = ".heimdall-part.ftp-resume";

/// First line of a resume record, with its format version.
const RESUME_HEADER: &str = "heimdall-ftp-resume 1";

/// Prefix of an upload's temporary name: hidden, beside the target.
const UPLOAD_PREFIX: &str = ".heimdall-upload-";

/// Prefix of the name a replaced file is moved aside to until its successor is in place:
/// hidden, beside it, with a random part.
const BACKUP_PREFIX: &str = ".heimdall-backup-";

/// Random bytes in a backup's name: two replaces of one file never share it.
const BACKUP_TOKEN_BYTES: usize = 6;

/// The file type bits of a fingerprint's mode, as POSIX: a folder, a regular file, a link.
const TYPE_DIRECTORY: u32 = 0o040_000;
const TYPE_FILE: u32 = 0o100_000;
const TYPE_LINK: u32 = 0o120_000;

/// Bytes moved at a time, and between two progress reports.
const CHUNK: usize = 64 * 1024;

/// Entries a walk reads at most, as the SFTP one.
const MAX_ENTRIES: usize = heimdall_sftp::tree::MAX_ENTRIES;

/// The feature a server lists when it has the machine listing (`MLSD`).
const MLSD_FEATURE: &str = "MLST";

/// How a connection is secured.
#[derive(Clone)]
pub enum FtpSecurity {
    /// None: plain FTP, everything in clear, as a C# profile without "Enable SSL/TLS".
    Plain,
    /// Explicit FTPS (`AUTH TLS`) with `connector`, checking the certificate for `domain`.
    Explicit {
        /// What checks the server's certificate.
        connector: tokio_rustls::TlsConnector,
        /// The name the certificate must hold.
        domain: String,
    },
}

impl std::fmt::Debug for FtpSecurity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plain => f.write_str("Plain"),
            Self::Explicit { domain, .. } => {
                f.debug_struct("Explicit").field("domain", domain).finish()
            }
        }
    }
}

/// Where an FTP session goes and as whom.
#[derive(Debug, Clone)]
pub struct FtpTarget {
    /// Host name or address.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Account; `None` logs in as `anonymous`, as the C# `FluentFTP` does with no name.
    pub username: Option<String>,
    /// Password.
    pub password: String,
    /// Passive data connections, as the C# default; active otherwise.
    pub passive: bool,
    /// Plain or explicit FTPS.
    pub security: FtpSecurity,
    /// Longest wait for the server to answer the connection.
    pub timeout: Duration,
}

/// The account FTP logs in as with no name.
const ANONYMOUS: &str = "anonymous";

/// Why an FTP session could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum FtpConnectError {
    /// The network connection failed.
    #[error("network: {0}")]
    Network(#[source] std::io::Error),
    /// The server did not answer in time.
    #[error("the server did not answer in time")]
    Timeout,
    /// The server refused the account or its password.
    #[error("the server refused the login")]
    LoginRefused,
    /// Explicit FTPS failed: the server refused `AUTH TLS`, or its certificate.
    #[error("TLS: {0}")]
    Tls(String),
    /// Anything else the server said or did.
    #[error("ftp: {0}")]
    Protocol(String),
}

/// An open FTP session.
#[derive(Clone)]
pub struct FtpClient {
    control: Arc<Mutex<AsyncRustlsFtpStream>>,
    /// Whether the server has the machine listing, learnt from `FEAT` at login.
    machine_listing: bool,
}

impl std::fmt::Debug for FtpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FtpClient")
            .field("machine_listing", &self.machine_listing)
            .finish_non_exhaustive()
    }
}

impl FtpClient {
    /// Connects to `target`, secures the connection when asked, logs in and sets binary
    /// transfers.
    ///
    /// # Errors
    ///
    /// [`FtpConnectError`].
    pub async fn connect(target: &FtpTarget) -> Result<Self, FtpConnectError> {
        // The control connection raced over every address the name gives, as every
        // connection is (`heimdall_net`); the server's greeting read within the same time.
        let opening = async {
            let socket = heimdall_net::connect(&target.host, target.port)
                .await
                .map_err(FtpConnectError::Network)?;
            AsyncRustlsFtpStream::connect_with_stream(socket)
                .await
                .map_err(|error| match error {
                    FtpError::ConnectionError(source) => FtpConnectError::Network(source),
                    other => FtpConnectError::Protocol(other.to_string()),
                })
        };
        let mut stream = tokio::time::timeout(target.timeout, opening)
            .await
            .map_err(|_| FtpConnectError::Timeout)??;
        if let FtpSecurity::Explicit { connector, domain } = &target.security {
            stream = stream
                .into_secure(AsyncRustlsConnector::from(connector.clone()), domain)
                .await
                .map_err(|error| FtpConnectError::Tls(error.to_string()))?;
        }
        let user = target.username.as_deref().unwrap_or(ANONYMOUS);
        stream
            .login(user, &target.password)
            .await
            .map_err(|error| match error {
                FtpError::UnexpectedResponse(response)
                    if response.status == Status::NotLoggedIn =>
                {
                    FtpConnectError::LoginRefused
                }
                other => FtpConnectError::Protocol(other.to_string()),
            })?;
        stream.set_mode(if target.passive {
            Mode::Passive
        } else {
            Mode::Active
        });
        stream
            .transfer_type(FileType::Binary)
            .await
            .map_err(|error| FtpConnectError::Protocol(error.to_string()))?;
        // A server without FEAT is listed with LIST.
        let machine_listing = stream
            .feat()
            .await
            .is_ok_and(|features| features.contains_key(MLSD_FEATURE));
        Ok(Self {
            control: Arc::new(Mutex::new(stream)),
            machine_listing,
        })
    }

    /// The absolute form of `path`: the server's current folder for a relative one.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] from the server.
    pub async fn canonical(&self, path: &RemotePath) -> Result<RemotePath, RemoteError> {
        let mut control = self.control.lock().await;
        if path.is_absolute() {
            return Ok(path.clone());
        }
        let current = control.pwd().await.map_err(|e| ftp_error(&e))?;
        let base = RemotePath::from(current.as_str());
        Ok(match text(path).as_str() {
            "" | "." => base,
            _ => base.join(path.as_bytes()),
        })
    }

    /// The entries of folder `path`, `.` and `..` left out.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] from the server.
    pub async fn list(&self, path: &RemotePath) -> Result<Vec<RemoteItem>, RemoteError> {
        let mut control = self.control.lock().await;
        self.list_locked(&mut control, path).await
    }

    async fn list_locked(
        &self,
        control: &mut AsyncRustlsFtpStream,
        path: &RemotePath,
    ) -> Result<Vec<RemoteItem>, RemoteError> {
        let folder = text(path);
        let lines = if self.machine_listing {
            control.mlsd(Some(&folder)).await
        } else {
            control.list(Some(&folder)).await
        }
        .map_err(|e| ftp_error(&e))?;
        Ok(items(&lines, self.machine_listing))
    }

    /// The entry at `path`, looked up in its folder's listing; `None` when there is none.
    async fn entry_locked(
        &self,
        control: &mut AsyncRustlsFtpStream,
        path: &RemotePath,
    ) -> Result<Option<RemoteItem>, RemoteError> {
        let Some(name) = path.file_name() else {
            return Ok(None);
        };
        let name = name.to_vec();
        Ok(self
            .list_locked(control, &path.parent())
            .await?
            .into_iter()
            .find(|entry| entry.name == name))
    }

    /// Creates folder `path`.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] from the server.
    pub async fn make_folder(&self, path: &RemotePath) -> Result<(), RemoteError> {
        self.control
            .lock()
            .await
            .mkdir(text(path))
            .await
            .map_err(|e| ftp_error(&e))
    }

    /// Renames `from` to `to`; an existing `to` is refused, never replaced.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] from the server; [`Refusal::Failure`] when `to` exists.
    pub async fn rename(&self, from: &RemotePath, to: &RemotePath) -> Result<(), RemoteError> {
        let mut control = self.control.lock().await;
        if self.entry_locked(&mut control, to).await?.is_some() {
            return Err(exists());
        }
        control
            .rename(text(from), text(to))
            .await
            .map_err(|e| ftp_error(&e))
    }

    /// Gives `path` the permission bits `mode`, with `SITE CHMOD` as most servers take it.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] from the server; [`Refusal::Unsupported`] from one without it;
    /// [`RemoteError::IsLink`] for a link the listing names so, left as it is.
    pub async fn set_permissions(&self, path: &RemotePath, mode: u32) -> Result<(), RemoteError> {
        let mut control = self.control.lock().await;
        // The server's chmod follows a link: what it points to would change instead.
        if self
            .entry_locked(&mut control, path)
            .await?
            .is_some_and(|entry| entry.kind == ItemKind::Link)
        {
            return Err(RemoteError::IsLink);
        }
        control
            .site(format!("CHMOD {mode:o} {}", text(path)))
            .await
            .map(|_| ())
            .map_err(|e| ftp_error(&e))
    }

    /// Deletes `path`, a folder with everything in it, never following a link.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] from the server; [`RemoteError::TooLarge`] for a tree too large.
    pub async fn remove(&self, path: &RemotePath) -> Result<(), RemoteError> {
        let mut control = self.control.lock().await;
        let kind = self
            .entry_locked(&mut control, path)
            .await?
            .map_or(ItemKind::File, |entry| entry.kind);
        if !walks(kind) {
            return control.rm(text(path)).await.map_err(|e| ftp_error(&e));
        }
        // Depth first: a folder is removed once everything in it is.
        let mut visited = 0usize;
        let mut pending = vec![(path.clone(), false)];
        while let Some((folder, emptied)) = pending.pop() {
            if emptied {
                control
                    .rmdir(text(&folder))
                    .await
                    .map_err(|e| ftp_error(&e))?;
                continue;
            }
            pending.push((folder.clone(), true));
            for entry in self.list_locked(&mut control, &folder).await? {
                visited += 1;
                if visited > MAX_ENTRIES {
                    return Err(RemoteError::TooLarge);
                }
                let child = folder.join(&entry.name);
                if walks(entry.kind) {
                    pending.push((child, false));
                } else {
                    // A link is deleted itself, never what it points to.
                    control.rm(text(&child)).await.map_err(|e| ftp_error(&e))?;
                }
            }
        }
        Ok(())
    }

    /// Downloads regular file `remote` to `local`, resuming an earlier attempt when the
    /// remote file is unchanged; returns its size.
    ///
    /// # Errors
    ///
    /// [`RemoteError`]; [`RemoteError::NotAFile`] for anything but a regular file;
    /// [`RemoteError::Cancelled`] when `cancel` fired, the part file kept for a resume.
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

    /// As [`Self::download`], but a local file at `local` is replaced only with `replace`.
    ///
    /// # Errors
    ///
    /// As [`Self::download`]; [`RemoteError::LocalExists`] for a local file left as it is,
    /// the part file kept for a resume.
    pub async fn download_with(
        &self,
        remote: &RemotePath,
        local: &Path,
        replace: bool,
        cancel: &CancellationToken,
        progress: impl FnMut(u64) + Send,
    ) -> Result<u64, RemoteError> {
        let mut control = self.control.lock().await;
        download_locked(self, &mut control, remote, local, replace, cancel, progress).await
    }

    /// Uploads `local` to `remote` through a hidden temporary file renamed onto it; an
    /// existing `remote` is replaced only with `replace`. Returns the bytes sent.
    ///
    /// # Errors
    ///
    /// [`RemoteError`]; [`RemoteError::Cancelled`] when `cancel` fired. The temporary file
    /// is removed whatever stopped the upload.
    pub async fn upload(
        &self,
        local: &Path,
        remote: &RemotePath,
        replace: bool,
        cancel: &CancellationToken,
        progress: impl FnMut(u64) + Send,
    ) -> Result<u64, RemoteError> {
        let mut control = self.control.lock().await;
        upload_locked(self, &mut control, local, remote, replace, cancel, progress).await
    }

    /// What the server says of `path` itself, a link not followed: its kind, size, time,
    /// permissions and owner, the size and time of a file asked exactly (`SIZE`, `MDTM`)
    /// when the server answers.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] from the server; [`Refusal::NoSuchFile`] when nothing is there.
    pub async fn fingerprint(&self, path: &RemotePath) -> Result<Fingerprint, RemoteError> {
        let mut control = self.control.lock().await;
        self.fingerprint_locked(&mut control, path)
            .await
            .map(|(_, fingerprint)| fingerprint)
    }

    /// The entry at `path` and its fingerprint, with the control connection held.
    async fn fingerprint_locked(
        &self,
        control: &mut AsyncRustlsFtpStream,
        path: &RemotePath,
    ) -> Result<(RemoteItem, Fingerprint), RemoteError> {
        let entry = self
            .entry_locked(control, path)
            .await?
            .ok_or_else(|| refused(Refusal::NoSuchFile))?;
        let (mut size, mut modified) = (None, None);
        if entry.kind == ItemKind::File {
            let name = text(path);
            size = control
                .size(&name)
                .await
                .ok()
                .and_then(|size| u64::try_from(size).ok());
            modified = control
                .mdtm(&name)
                .await
                .ok()
                .and_then(|date| u32::try_from(date.and_utc().timestamp()).ok());
        }
        let fingerprint = Fingerprint {
            size: size.or(entry.size),
            modified: modified.or_else(|| {
                entry
                    .modified
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .and_then(|since| u32::try_from(since.as_secs()).ok())
            }),
            // The kind is part of it, as in an SFTP mode: a file become a folder is a change.
            permissions: Some(type_bits(entry.kind) | entry.permissions.unwrap_or_default()),
            uid_gid: entry.owner.zip(entry.group),
        };
        Ok((entry, fingerprint))
    }

    /// The regular file `path` read whole, `cap` bytes at most, with the fingerprint of that
    /// very content: described before and after the read, a change between the two refused.
    ///
    /// # Errors
    ///
    /// [`RemoteError::NotAFile`], [`RemoteError::FileTooLarge`] (said by its size, or found
    /// while reading), [`RemoteError::Changed`], [`RemoteError::Cancelled`], or the server's.
    pub async fn read_whole(
        &self,
        path: &RemotePath,
        cap: u64,
        cancel: &CancellationToken,
    ) -> Result<(Vec<u8>, Fingerprint), RemoteError> {
        let mut control = self.control.lock().await;
        let (entry, before) = self.fingerprint_locked(&mut control, path).await?;
        if entry.kind != ItemKind::File {
            return Err(RemoteError::NotAFile);
        }
        if before.size.is_some_and(|size| size > cap) {
            return Err(RemoteError::FileTooLarge);
        }
        let mut stream = control
            .retr_as_stream(text(path))
            .await
            .map_err(|e| ftp_error(&e))?;
        let mut data = Vec::new();
        // One byte past the cap tells a file too large without reading it all.
        let mut limited = (&mut stream).take(cap.saturating_add(1));
        let copied = copy(&mut limited, &mut data, 0, cancel, &mut |_| {}).await;
        // The reply of a read stopped early is a refusal: the file is too large regardless.
        let finished = stream.finish().await;
        copied?;
        if data.len() as u64 > cap {
            return Err(RemoteError::FileTooLarge);
        }
        finished.map_err(|e| ftp_error(&e))?;
        let (_, after) = self.fingerprint_locked(&mut control, path).await?;
        if after != before || after.size.is_some_and(|size| size != data.len() as u64) {
            return Err(RemoteError::Changed);
        }
        Ok((data, after))
    }

    /// Replaces the regular file `path` with `data`, only while it still has the fingerprint
    /// `expected`: checked first, and again once the data is on the server, just before the
    /// commit; returns its fingerprint once replaced. The data goes to a hidden temporary
    /// file beside it, then takes its place recoverably, as the module says.
    ///
    /// # Errors
    ///
    /// [`RemoteError::Changed`] with the file left as it is,
    /// [`RemoteError::DestinationNotAFile`], [`RemoteError::Cancelled`], or the server's. The
    /// temporary file is removed whatever stopped the replace.
    pub async fn replace_if(
        &self,
        path: &RemotePath,
        data: &[u8],
        expected: &Fingerprint,
        cancel: &CancellationToken,
    ) -> Result<Fingerprint, RemoteError> {
        let mut control = self.control.lock().await;
        let temporary = beside(path, UPLOAD_PREFIX.as_bytes());
        let replaced = async {
            self.unchanged_locked(&mut control, path, expected).await?;
            let mut source = data;
            let mut stream = control
                .put_with_stream(text(&temporary))
                .await
                .map_err(|e| ftp_error(&e))?;
            let copied = copy(&mut source, &mut stream, 0, cancel, &mut |_| {}).await;
            let finished = stream.finish().await;
            copied?;
            finished.map_err(|e| ftp_error(&e))?;
            // Checked again once the data is there: the window left is the commit's.
            self.unchanged_locked(&mut control, path, expected).await?;
            commit_replace(&mut control, &temporary, path).await
        }
        .await;
        if let Err(error) = replaced {
            let _ = control.rm(text(&temporary)).await;
            return Err(error);
        }
        self.fingerprint_locked(&mut control, path)
            .await
            .map(|(_, fingerprint)| fingerprint)
    }

    /// Whether `path` is still the regular file `expected` describes.
    async fn unchanged_locked(
        &self,
        control: &mut AsyncRustlsFtpStream,
        path: &RemotePath,
        expected: &Fingerprint,
    ) -> Result<(), RemoteError> {
        let (entry, now) = self.fingerprint_locked(control, path).await?;
        if entry.kind != ItemKind::File {
            return Err(RemoteError::DestinationNotAFile);
        }
        if now != *expected {
            return Err(RemoteError::Changed);
        }
        Ok(())
    }
}

/// Puts the complete file `temporary` in place of the existing file `target`, recoverably:
/// `target` moved aside to a hidden backup name, `temporary` renamed onto its name, the
/// backup deleted only then. A failed rename puts `target` back; should that fail too, the
/// old file stays under its backup name. `temporary` is left for the caller to remove.
async fn commit_replace(
    control: &mut AsyncRustlsFtpStream,
    temporary: &RemotePath,
    target: &RemotePath,
) -> Result<(), RemoteError> {
    let token = crate::server_copy::random_token().ok_or(RemoteError::ReplaceNotSafe)?;
    let mut prefix = BACKUP_PREFIX.as_bytes().to_vec();
    for byte in &token[..BACKUP_TOKEN_BYTES] {
        prefix.extend_from_slice(format!("{byte:02x}").as_bytes());
    }
    prefix.push(b'-');
    let backup = beside(target, &prefix);
    control
        .rename(text(target), text(&backup))
        .await
        .map_err(|e| ftp_error(&e))?;
    if let Err(error) = control.rename(text(temporary), text(target)).await {
        // The old file back under its name; failing that, kept under the backup name.
        let _ = control.rename(text(&backup), text(target)).await;
        return Err(ftp_error(&error));
    }
    // A backup left behind, the delete refused, is a stale copy, never the file.
    let _ = control.rm(text(&backup)).await;
    Ok(())
}

/// The hidden name beside `path`: its own name after `prefix`.
fn beside(path: &RemotePath, prefix: &[u8]) -> RemotePath {
    let mut name = prefix.to_vec();
    name.extend_from_slice(path.file_name().unwrap_or_default());
    path.parent().join(&name)
}

/// The POSIX file type bits a fingerprint gives an entry of `kind`.
fn type_bits(kind: ItemKind) -> u32 {
    match kind {
        ItemKind::Directory => TYPE_DIRECTORY,
        ItemKind::File => TYPE_FILE,
        ItemKind::Link => TYPE_LINK,
        ItemKind::Other(_) => 0,
    }
}

/// Downloads with the control connection held.
async fn download_locked(
    client: &FtpClient,
    control: &mut AsyncRustlsFtpStream,
    remote: &RemotePath,
    local: &Path,
    replace: bool,
    cancel: &CancellationToken,
    mut progress: impl FnMut(u64) + Send,
) -> Result<u64, RemoteError> {
    let entry = client
        .entry_locked(control, remote)
        .await?
        .ok_or_else(|| refused(Refusal::NoSuchFile))?;
    if entry.kind != ItemKind::File {
        return Err(RemoteError::NotAFile);
    }
    let path = text(remote);
    let size = control
        .size(&path)
        .await
        .ok()
        .and_then(|size| u64::try_from(size).ok())
        .or(entry.size);
    let modified = control
        .mdtm(&path)
        .await
        .ok()
        .map(|date| date.and_utc().timestamp());
    let part = with_suffix(local, PART_SUFFIX);
    let record = with_suffix(local, RESUME_SUFFIX);
    let signature = format!("{RESUME_HEADER}\n{size:?}\n{modified:?}\n");
    let resumable = size.is_some()
        && modified.is_some()
        && tokio::fs::read_to_string(&record).await.ok().as_deref() == Some(signature.as_str());
    let kept = if resumable {
        tokio::fs::metadata(&part)
            .await
            .map_or(0, |meta| meta.len())
    } else {
        0
    };
    tokio::fs::write(&record, &signature)
        .await
        .map_err(|e| local_error(&e))?;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(kept > 0)
        .truncate(kept == 0)
        .open(&part)
        .await
        .map_err(|e| local_error(&e))?;
    if kept > 0 {
        control
            .resume_transfer(usize::try_from(kept).unwrap_or(usize::MAX))
            .await
            .map_err(|e| ftp_error(&e))?;
    }
    let mut data = control
        .retr_as_stream(&path)
        .await
        .map_err(|e| ftp_error(&e))?;
    let copied = copy(&mut data, &mut file, kept, cancel, &mut progress).await;
    let finished = data.finish().await;
    let total = copied?;
    finished.map_err(|e| ftp_error(&e))?;
    file.flush().await.map_err(|e| local_error(&e))?;
    drop(file);
    match heimdall_sftp::transfer::commit_local(&part, local, replace).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(RemoteError::LocalExists);
        }
        Err(error) => return Err(local_error(&error)),
    }
    let _ = tokio::fs::remove_file(&record).await;
    Ok(total)
}

/// Uploads with the control connection held.
async fn upload_locked(
    client: &FtpClient,
    control: &mut AsyncRustlsFtpStream,
    local: &Path,
    remote: &RemotePath,
    replace: bool,
    cancel: &CancellationToken,
    mut progress: impl FnMut(u64) + Send,
) -> Result<u64, RemoteError> {
    let existing = client.entry_locked(control, remote).await?;
    if existing.is_some() && !replace {
        return Err(exists());
    }
    // Only a regular file is replaced: a folder or a link is left as it is, as over SFTP.
    if existing
        .as_ref()
        .is_some_and(|entry| entry.kind != ItemKind::File)
    {
        return Err(RemoteError::DestinationNotAFile);
    }
    let temporary = beside(remote, UPLOAD_PREFIX.as_bytes());
    let mut source = tokio::fs::File::open(local)
        .await
        .map_err(|e| local_error(&e))?;
    let sent = async {
        let mut data = control
            .put_with_stream(text(&temporary))
            .await
            .map_err(|e| ftp_error(&e))?;
        let copied = copy(&mut source, &mut data, 0, cancel, &mut progress).await;
        let finished = data.finish().await;
        let total = copied?;
        finished.map_err(|e| ftp_error(&e))?;
        if existing.is_some() {
            // FTP has no atomic replace: the old file is moved aside until the new one lands.
            commit_replace(control, &temporary, remote).await?;
            return Ok(total);
        }
        if !replace && client.entry_locked(control, remote).await?.is_some() {
            // As the C#: the name is checked again just before the move, since a rename can
            // replace what someone else put there while the data was sent. A file arriving
            // between this check and the move still can: FTP has no exclusive rename.
            return Err(exists());
        }
        control
            .rename(text(&temporary), text(remote))
            .await
            .map_err(|e| ftp_error(&e))?;
        Ok(total)
    }
    .await;
    if sent.is_err() {
        let _ = control.rm(text(&temporary)).await;
    }
    sent
}

/// Copies `from` into `to`, `already` bytes being there, reporting the running total.
async fn copy(
    from: &mut (impl AsyncRead + Unpin),
    to: &mut (impl AsyncWrite + Unpin),
    already: u64,
    cancel: &CancellationToken,
    progress: &mut (impl FnMut(u64) + Send),
) -> Result<u64, RemoteError> {
    let mut buffer = vec![0; CHUNK];
    let mut total = already;
    loop {
        let read = tokio::select! {
            // A stop is seen before the next bytes, even when both are ready.
            biased;
            () = cancel.cancelled() => return Err(RemoteError::Cancelled),
            read = from.read(&mut buffer) => read.map_err(|e| transfer_io(&e))?,
        };
        if read == 0 {
            to.flush().await.map_err(|e| transfer_io(&e))?;
            return Ok(total);
        }
        to.write_all(&buffer[..read])
            .await
            .map_err(|e| transfer_io(&e))?;
        total += read as u64;
        progress(total);
    }
}

/// `path` as FTP commands take it: its bytes, read as UTF-8 as servers now do.
fn text(path: &RemotePath) -> String {
    String::from_utf8_lossy(path.as_bytes()).into_owned()
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// The entries a listing's `lines` describe: machine lines (`MLSD`) or human ones (`LIST`,
/// UNIX or DOS format); `.` and `..`, and lines that do not read, left out.
fn items(lines: &[String], machine: bool) -> Vec<RemoteItem> {
    let mut unread = 0usize;
    let listed: Vec<RemoteItem> = lines
        .iter()
        .filter_map(|line| {
            let read = entry(line, machine);
            if read.is_none() && !summary(line) {
                unread += 1;
            }
            read
        })
        .filter(|item| !matches!(item.name.as_slice(), b"." | b".."))
        .collect();
    if unread > 0 {
        // Server data: the count only, never the lines themselves.
        log::debug!("FTP listing: {unread} line(s) not read, left out");
    }
    listed
}

/// The entry one listing line describes, if it reads.
fn entry(line: &str, machine: bool) -> Option<RemoteItem> {
    let (listed, owner, tolerated) = if machine {
        let listed = ListParser::parse_mlsd(line).ok()?;
        (listed, machine_owner(line), None)
    } else if let Ok(listed) = ListParser::parse_posix(line) {
        let owner = Owner {
            name: unix_owner(line),
            ..Owner::default()
        };
        (listed, owner, None)
    } else if let Some(tolerated) = tolerated(line) {
        let listed = ListParser::parse_posix(&tolerated.line).ok()?;
        // The owner read from the line the parser read, as for a strict line.
        let owner = Owner {
            name: unix_owner(&tolerated.line),
            ..Owner::default()
        };
        (listed, owner, Some(tolerated))
    } else {
        // A DOS line names no owner.
        (ListParser::parse_dos(line).ok()?, Owner::default(), None)
    };
    let mut entry = item(&listed);
    entry.owner = entry.owner.or(owner.number);
    entry.owner_name = owner.name;
    if let Some(tolerated) = tolerated {
        if let Some(special) = tolerated.special {
            entry.kind = ItemKind::Other(special);
        }
        if tolerated.device {
            // A device's line gives its major and minor numbers, not a size.
            entry.size = None;
        }
    }
    Some(entry)
}

/// Whether `line` is the `total N` summary a UNIX listing starts with, no entry.
fn summary(line: &str) -> bool {
    line.trim().is_empty() || line.split(' ').next() == Some("total")
}

/// A UNIX `LIST` line the parser refuses, rewritten into one it reads, as the C#
/// (`FluentFTP`) lists it: what the rewrite took out of the line is kept here.
#[derive(Debug, PartialEq, Eq)]
struct Tolerated {
    /// The line rewritten, its date and name left exactly as the server gave them.
    line: String,
    /// What the type character the parser does not read names.
    special: Option<crate::Special>,
    /// Whether the line gave a device's major and minor numbers where a size goes.
    device: bool,
}

/// The fields read before the date of a line the parser refuses: mode, links, owner, group,
/// a device's two numbers, then month, day and time, and the start of the name. Bounds the
/// work on a server's line.
const TOLERATED_FIELDS: usize = 12;

/// The length of a mode with its type character: `-rw-r--r--`.
const MODE_LENGTH: usize = 10;

/// The group a line without one is given: not a number, so no group number is read.
const NO_GROUP: &str = "-";

/// A UNIX `LIST` line the strict parser refuses, rewritten when what it refuses is one of
/// the shapes real servers send:
/// - a mode followed by an ACL or extended attribute mark: `-rw-r--r--+`, `drwxr-xr-x@`,
///   `-rw-r--r--.` (`SELinux`);
/// - a type it does not read: a device (`b`, `c`), a pipe (`p`), a socket (`s`), with a
///   device's `major, minor` where the size goes;
/// - no group column.
///
/// None when the line is none of these. The fields are parted by spaces only, as the
/// parser's `([^ ]+)` parts them, and the line from its date on is kept as is, so the name
/// (spaces, a link's `->`) is read exactly as the parser reads it.
fn tolerated(line: &str) -> Option<Tolerated> {
    let mut fields: Vec<(usize, &str)> = Vec::with_capacity(TOLERATED_FIELDS);
    let mut start = 0usize;
    for field in line.split(' ') {
        if !field.is_empty() {
            fields.push((start, field));
            if fields.len() == TOLERATED_FIELDS {
                break;
            }
        }
        start += field.len() + 1;
    }
    let names: Vec<&str> = fields.iter().map(|(_, field)| *field).collect();
    let field = |at: usize| names.get(at).copied();
    // The date: a month, a day, a time or a year, then a name. Never before the group.
    let date = (3..fields.len()).find(|&at| {
        field(at).is_some_and(month)
            && field(at + 1).is_some_and(|day| digits(day, 1, 2))
            && field(at + 2).is_some_and(time_or_year)
            && field(at + 3).is_some()
    })?;
    let (date_start, _) = *fields.get(date)?;
    let (mode, links, owner) = (field(0)?, field(1)?, field(2)?);
    let middle = names.get(3..date)?;

    let (kind, special) = mode_type(mode.chars().next()?)?;
    let permissions = mode.get(1..MODE_LENGTH)?;
    let marked = match mode.get(MODE_LENGTH..)? {
        "" => false,
        "+" | "@" | "." => true,
        _ => return None,
    };

    // A size, or a device's `major, minor` or `major,minor`, then what is before it.
    let (rest, size, device) = match middle {
        [rest @ .., major, minor]
            if major
                .strip_suffix(',')
                .is_some_and(|major| digits(major, 1, usize::MAX))
                && digits(minor, 1, usize::MAX) =>
        {
            (rest, "0", true)
        }
        [rest @ .., numbers] if device_numbers(numbers) => (rest, "0", true),
        [rest @ .., size] if digits(size, 1, usize::MAX) => (rest, *size, false),
        _ => return None,
    };
    let group = match rest {
        [] => NO_GROUP,
        [group] => group,
        _ => return None,
    };
    if !(marked || special.is_some() || device || rest.is_empty()) {
        // None of the shapes this reads: the parser's refusal stands.
        return None;
    }
    let rewritten = [mode, links, owner, group, size];
    if !digits(links, 1, usize::MAX)
        || rewritten
            .iter()
            .any(|field| field.contains(char::is_whitespace))
    {
        return None;
    }
    Some(Tolerated {
        line: format!(
            "{kind}{permissions} {links} {owner} {group} {size} {}",
            line.get(date_start..)?
        ),
        special,
        device,
    })
}

/// The type character the parser reads for a mode's first one, and what an entry of a type
/// it does not read is, as the C# `ClassifyFtpEntry` names it (`FtpBrowser.cs`): such an
/// entry is read as a file, then named for what it is.
fn mode_type(first: char) -> Option<(char, Option<crate::Special>)> {
    use crate::Special;

    match first {
        '-' | 'd' | 'l' => Some((first, None)),
        'b' | 'c' => Some(('-', Some(Special::Device))),
        'p' => Some(('-', Some(Special::Pipe))),
        's' => Some(('-', Some(Special::Socket))),
        // A type stated the C# does not recognise either: never a regular file.
        _ if first.is_ascii_alphabetic() => Some(('-', Some(Special::Unknown))),
        _ => None,
    }
}

/// Whether `said` is a month as `ls` writes it, `Sep`: three ASCII letters.
fn month(said: &str) -> bool {
    said.len() == 3 && said.bytes().all(|byte| byte.is_ascii_alphabetic())
}

/// Whether `said` is a time, `10:00`, or a year, `2025`, as `ls` writes a date's last part.
fn time_or_year(said: &str) -> bool {
    match said.split_once(':') {
        Some((hours, minutes)) => digits(hours, 1, 2) && digits(minutes, 1, 2),
        None => digits(said, 4, 4),
    }
}

/// Whether `said` is a device's `major,minor` given as one field.
fn device_numbers(said: &str) -> bool {
    said.split_once(',')
        .is_some_and(|(major, minor)| digits(major, 1, usize::MAX) && digits(minor, 1, usize::MAX))
}

/// Whether `said` is between `least` and `most` ASCII digits.
fn digits(said: &str, least: usize, most: usize) -> bool {
    (least..=most).contains(&said.len()) && said.bytes().all(|byte| byte.is_ascii_digit())
}

/// What a listing line says of its owner beyond what the parser reads.
#[derive(Debug, Default, PartialEq, Eq)]
struct Owner {
    /// A user number the parser does not read: `MLSD`'s `UNIX.owner` given as a number, as
    /// `ProFTPD` sends it.
    number: Option<u32>,
    /// The owner's name.
    name: Option<String>,
}

/// The facts of an `MLSD` line naming its owner, the most explicit first: `UNIX.ownername`,
/// as `ProFTPD` writes it, then `UNIX.owner`.
const OWNER_FACTS: [&str; 2] = ["unix.ownername", "unix.owner"];

/// The owner's name a `LIST` line in UNIX format gives, as the C# keeps it in
/// `FtpListItem.RawOwner`: its third field, after the mode and the link count, the fields
/// parted by spaces only, as the parser's `([^ ]+)` parts them. A number is the owner's user
/// number, which the parser already reads: no name then.
fn unix_owner(line: &str) -> Option<String> {
    owner_name(line.split(' ').filter(|field| !field.is_empty()).nth(2)?)
}

/// What an `MLSD` line says of its owner in its facts, before the space that starts the
/// name of the entry; facts are named whatever their case. Its name, the most explicit
/// fact first; and `UNIX.owner` as a number, the user number.
fn machine_owner(line: &str) -> Owner {
    let Some((facts, _)) = line.split_once(' ') else {
        return Owner::default();
    };
    let facts: Vec<(&str, &str)> = facts
        .split(';')
        .filter_map(|fact| fact.split_once('='))
        .collect();
    let value = |wanted: &str| {
        facts
            .iter()
            .find(|(fact, _)| fact.eq_ignore_ascii_case(wanted))
            .map(|(_, value)| *value)
    };
    Owner {
        number: value(OWNER_FACTS[1]).and_then(|said| said.parse().ok()),
        name: OWNER_FACTS
            .iter()
            .find_map(|wanted| value(wanted).and_then(owner_name)),
    }
}

/// `said` as an owner's name: none when empty, or a number, which is a user number.
fn owner_name(said: &str) -> Option<String> {
    (!said.is_empty() && said.parse::<u32>().is_err()).then(|| said.to_owned())
}

/// Whether a delete goes into an entry of `kind` rather than deleting it: only a folder. A
/// link is deleted itself, never what it points to.
fn walks(kind: ItemKind) -> bool {
    kind == ItemKind::Directory
}

fn item(listed: &Listed) -> RemoteItem {
    use suppaftp::list::PosixPexQuery;

    let kind = if listed.is_directory() {
        ItemKind::Directory
    } else if listed.is_symlink() {
        ItemKind::Link
    } else if listed.is_file() {
        ItemKind::File
    } else {
        ItemKind::Other(crate::Special::Unknown)
    };
    let mut mode = 0u32;
    for (shift, who) in [
        (6, PosixPexQuery::Owner),
        (3, PosixPexQuery::Group),
        (0, PosixPexQuery::Others),
    ] {
        let bits = u32::from(listed.can_read(who)) << 2
            | u32::from(listed.can_write(who)) << 1
            | u32::from(listed.can_execute(who));
        mode |= bits << shift;
    }
    let modified = listed.modified();
    RemoteItem {
        name: listed.name().as_bytes().to_vec(),
        kind,
        size: u64::try_from(listed.size()).ok(),
        // A listing without a date gives the epoch: none said.
        modified: (modified > UNIX_EPOCH).then_some(modified),
        permissions: Some(mode),
        owner: listed.uid(),
        owner_name: None,
        group: listed.gid(),
    }
}

fn exists() -> RemoteError {
    RemoteError::Refused {
        refusal: Refusal::Failure,
        message: b"already exists".to_vec(),
    }
}

fn refused(refusal: Refusal) -> RemoteError {
    RemoteError::Refused {
        refusal,
        message: Vec::new(),
    }
}

fn local_error(error: &std::io::Error) -> RemoteError {
    RemoteError::local(error)
}

/// A failure while bytes move: the data connection, or the local file.
fn transfer_io(error: &std::io::Error) -> RemoteError {
    RemoteError::local(error)
}

/// The refusal an FTP reply stands for, its text kept as the server's words.
fn ftp_error(error: &FtpError) -> RemoteError {
    match error {
        FtpError::UnexpectedResponse(response) => RemoteError::Refused {
            refusal: match response.status.code() {
                // 550: not found, or not allowed; FTP does not tell them apart.
                550 | 450 => Refusal::NoSuchFile,
                530 | 532 => Refusal::PermissionDenied,
                500..=504 => Refusal::Unsupported,
                _ => Refusal::Failure,
            },
            message: response.body.clone(),
        },
        FtpError::BadResponse => refused(Refusal::Failure),
        _ => RemoteError::SessionClosed,
    }
}

#[cfg(test)]
mod tests {
    use suppaftp::types::Response;
    use suppaftp::{FtpError, Status};

    use super::{Owner, ftp_error, items, machine_owner, tolerated, unix_owner, walks};
    use crate::{ItemKind, Refusal, RemoteError};

    fn kinds(lines: &[&str], machine: bool) -> Vec<(String, ItemKind)> {
        let lines: Vec<String> = lines.iter().map(|line| (*line).to_owned()).collect();
        items(&lines, machine)
            .into_iter()
            .map(|item| (String::from_utf8_lossy(&item.name).into_owned(), item.kind))
            .collect()
    }

    #[test]
    fn a_unix_listing_tells_folders_files_and_links_and_leaves_the_dots_out() {
        let listed = kinds(
            &[
                "drwxr-xr-x    2 ftp      ftp          4096 Sep 29 10:00 .",
                "drwxr-xr-x    5 ftp      ftp          4096 Sep 29 10:00 ..",
                "drwxr-xr-x    2 ftp      ftp          4096 Sep 29 10:00 logs",
                "-rw-r--r--    1 ftp      ftp            12 Sep 29 10:00 notes.txt",
                "lrwxrwxrwx    1 ftp      ftp             4 Sep 29 10:00 link -> logs",
                "total 3",
            ],
            false,
        );
        assert_eq!(
            listed,
            [
                ("logs".to_owned(), ItemKind::Directory),
                ("notes.txt".to_owned(), ItemKind::File),
                ("link".to_owned(), ItemKind::Link),
            ]
        );
    }

    #[test]
    fn a_machine_listing_leaves_the_current_and_parent_folders_out() {
        let listed = kinds(
            &[
                "type=cdir;modify=20260929100000; .",
                "type=pdir;modify=20260929100000; ..",
                "type=dir;modify=20260929100000; logs",
                "type=file;size=12;modify=20260929100000; notes.txt",
            ],
            true,
        );
        assert_eq!(
            listed,
            [
                ("logs".to_owned(), ItemKind::Directory),
                ("notes.txt".to_owned(), ItemKind::File),
            ]
        );
    }

    fn owners(lines: &[&str], machine: bool) -> Vec<(Option<u32>, Option<String>)> {
        let lines: Vec<String> = lines.iter().map(|line| (*line).to_owned()).collect();
        items(&lines, machine)
            .into_iter()
            .map(|item| (item.owner, item.owner_name))
            .collect()
    }

    #[test]
    fn a_unix_listing_keeps_the_owner_name_the_server_gives() {
        let listed = owners(
            &[
                "drwxr-xr-x    2 www-data ftp          4096 Sep 29 10:00 logs",
                "-rw-r--r--    1 1000     1000           12 Sep 29 10:00 notes.txt",
                "lrwxrwxrwx    1 Admin    ftp             4 Sep 29 10:00 link -> logs",
            ],
            false,
        );
        assert_eq!(
            listed,
            [
                (None, Some("www-data".to_owned())),
                // A number is the user number, read as such: no name.
                (Some(1000), None),
                (None, Some("Admin".to_owned())),
            ]
        );
    }

    #[test]
    fn a_dos_listing_names_no_owner() {
        let listed = owners(&["10-13-26  10:00AM       <DIR>          logs"], false);
        assert_eq!(listed, [(None, None)]);
    }

    #[test]
    fn a_machine_listing_names_the_owner_by_its_facts_the_most_explicit_first() {
        let listed = owners(
            &[
                "type=file;size=1;UNIX.owner=1000;UNIX.ownername=alice;UNIX.uid=1000; a",
                "type=file;size=1;unix.owner=bob; b",
                "type=file;size=1;unix.uid=1001;unix.owner=1001; c",
                "type=file;size=1; d",
                // ProFTPD's number, which the parser does not read: the user number.
                "type=file;UNIX.owner=1000; e",
                // A ";" in the name is no fact of its owner.
                "type=file;size=1; f;UNIX.owner=eve;UNIX.ownername=mallory",
                "type=file;size=1; g;UNIX.owner=7",
            ],
            true,
        );
        assert_eq!(
            listed[..5],
            [
                (Some(1000), Some("alice".to_owned())),
                (None, Some("bob".to_owned())),
                (Some(1001), None),
                (None, None),
                (Some(1000), None),
            ]
        );
        // Whatever the parser makes of the name itself, no owner is taken from it.
        assert_eq!(listed[5..], [(None, None), (None, None)]);
    }

    #[test]
    fn an_owner_is_taken_from_a_rewritten_line_where_the_server_put_it() {
        // An ACL mark after the mode and a device's two numbers: the line is rewritten for
        // the parser, and its owner is still the server's third field, never another one.
        for (line, owner) in [
            (
                "-rw-r--r--+  1 julien   staff          12 Sep 29 10:00 acl.txt",
                "julien",
            ),
            (
                "crw-r--r--   1 root     root       4,   1 Sep 29 10:00 tty1",
                "root",
            ),
        ] {
            assert_eq!(
                owners(&[line], false),
                [(None, Some(owner.to_owned()))],
                "{line}"
            );
        }
    }

    #[test]
    fn an_owner_field_is_read_where_its_line_puts_it() {
        assert_eq!(
            unix_owner("-rw-r--r-- 1 julien staff 12 Sep 29 10:00 a b c").as_deref(),
            Some("julien")
        );
        assert_eq!(unix_owner("-rw-r--r-- 1"), None);
        // Fields parted by spaces only, as the parser parts them: a tab or a no-break
        // space stays inside the owner.
        assert_eq!(
            unix_owner("-rw-r--r-- 1 a\tb staff 12 Sep 29 10:00 f").as_deref(),
            Some("a\tb")
        );
        assert_eq!(
            unix_owner("-rw-r--r-- 1 j\u{a0}d staff 12 Sep 29 10:00 f").as_deref(),
            Some("j\u{a0}d")
        );
        assert_eq!(
            owners(&["-rw-r--r-- 1 a\tb staff 12 Sep 29 10:00 f"], false),
            [(None, Some("a\tb".to_owned()))],
            "the parser reads the same line"
        );
        // The entry's name is never read as a fact, even with a "=" in it.
        assert_eq!(
            machine_owner("type=file;size=1; unix.owner=eve"),
            Owner::default()
        );
        assert_eq!(machine_owner("no-facts"), Owner::default());
    }

    type Read = (String, ItemKind, Option<u64>, Option<u32>, Option<String>);

    fn read(line: &str) -> Vec<Read> {
        items(&[line.to_owned()], false)
            .into_iter()
            .map(|item| {
                (
                    String::from_utf8_lossy(&item.name).into_owned(),
                    item.kind,
                    item.size,
                    item.group,
                    item.owner_name,
                )
            })
            .collect()
    }

    fn named(
        name: &str,
        kind: ItemKind,
        size: Option<u64>,
        group: Option<u32>,
        owner: &str,
    ) -> Read {
        (name.to_owned(), kind, size, group, Some(owner.to_owned()))
    }

    #[test]
    fn a_unix_listing_keeps_the_entries_the_strict_parser_refuses_as_the_c_sharp_lists_them() {
        use crate::Special;

        let device = ItemKind::Other(Special::Device);
        for (line, expected) in [
            // A Linux ACL mark, a macOS extended attribute one, an SELinux context one.
            (
                "-rw-r--r--+ 1 alice staff 12 Sep 29 10:00 f",
                named("f", ItemKind::File, Some(12), None, "alice"),
            ),
            (
                "drwxr-xr-x@ 3 bob staff 96 Oct 1 2025 d",
                named("d", ItemKind::Directory, Some(96), None, "bob"),
            ),
            (
                "-rw-r--r--.  1 carol    50     5 Sep 29 10:00 se",
                named("se", ItemKind::File, Some(5), Some(50), "carol"),
            ),
            // Devices: their major and minor numbers are no size.
            (
                "crw-rw-rw- 1 root root 1, 3 Oct 1 10:00 null",
                named("null", device, None, None, "root"),
            ),
            (
                "brw-rw----   1 root     disk       8,   0 Oct  1 10:00 sda",
                named("sda", device, None, None, "root"),
            ),
            (
                "crw--w----+ 1 root tty 4,1 Oct 1 10:00 tty1",
                named("tty1", device, None, None, "root"),
            ),
            (
                "crw-rw-rw- 1 root 1, 3 Oct 1 10:00 null",
                named("null", device, None, None, "root"),
            ),
            // A pipe, a socket, and a type the C# does not name either.
            (
                "prw-r--r-- 1 dave staff 0 Oct 1 10:00 fifo",
                named(
                    "fifo",
                    ItemKind::Other(Special::Pipe),
                    Some(0),
                    None,
                    "dave",
                ),
            ),
            (
                "srwxrwxrwx 1 dave staff 0 Oct 1 10:00 sock",
                named(
                    "sock",
                    ItemKind::Other(Special::Socket),
                    Some(0),
                    None,
                    "dave",
                ),
            ),
            (
                "Drw-r--r-- 1 dave staff 0 Oct 1 10:00 door",
                named(
                    "door",
                    ItemKind::Other(Special::Unknown),
                    Some(0),
                    None,
                    "dave",
                ),
            ),
            // No group column; the name keeps its spaces.
            (
                "-rw-r--r-- 1 erin 12 Sep 29 10:00 no group.txt",
                named("no group.txt", ItemKind::File, Some(12), None, "erin"),
            ),
            (
                "-rw-r--r--    1 erin          12 Sep 29 10:00  two  spaces ",
                named("two  spaces ", ItemKind::File, Some(12), None, "erin"),
            ),
            // A link with a mark: its name stops at the "->", as for a strict line.
            (
                "lrwxrwxrwx@ 1 frank staff 4 Sep 29 10:00 my link -> some target",
                named("my link", ItemKind::Link, Some(4), None, "frank"),
            ),
        ] {
            assert_eq!(read(line), [expected], "{line}");
        }
        // A number for the owner is its user number, as for a strict line.
        let numbered = items(
            &["-rw-r--r--+ 1 1000 1000 12 Sep 29 10:00 f".to_owned()],
            false,
        );
        assert_eq!(
            numbered
                .iter()
                .map(|item| (item.owner, item.group, item.owner_name.clone()))
                .collect::<Vec<_>>(),
            [(Some(1000), Some(1000), None)]
        );
    }

    #[test]
    fn a_rewritten_line_keeps_its_date_and_name_exactly() {
        let line = "-rw-r--r--+   1 alice  staff   12 Sep 29 10:00   a  b -> c";
        assert_eq!(
            tolerated(line).map(|tolerated| tolerated.line).as_deref(),
            Some("-rw-r--r-- 1 alice staff 12 Sep 29 10:00   a  b -> c")
        );
        // A line the strict parser reads is never rewritten into another.
        assert_eq!(
            tolerated("-rw-r--r-- 1 alice staff 12 Sep 29 10:00 f"),
            None
        );
    }

    #[test]
    fn a_line_no_shape_reads_stays_left_out() {
        for line in [
            "",
            "total 3",
            "-rw-r--r--+",
            "-rw-r--r--! 1 alice staff 12 Sep 29 10:00 f",
            "-rw-r--r--++ 1 alice staff 12 Sep 29 10:00 f",
            "-rw-r--r--+ 1 alice staff twelve Sep 29 10:00 f",
            "-rw-r--r--+ x alice staff 12 Sep 29 10:00 f",
            "-rw-r--r--+ 1 alice staff 12 Sep 29 10:00",
            "-rw-r--r--+ 1 alice two groups 12 Sep 29 10:00 f",
            "crw-rw-rw- 1 root root 1, Oct 1 10:00 null",
            "crw-rw-rw- 1 root root ,3 Oct 1 10:00 null",
            "-rw-r--r--+ 1 alice staff 12 Sep 99 10:00 f",
            "-rw-r--r--+ 1 alice staff 12 Sép 29 10:00 f",
            "-rw-r--r--+ 1 a\tb staff 12 Sep 29 10:00 f",
            "-rwXr--r--+ 1 alice staff 12 Sep 29 10:00 f",
            "é-rw-r--r-- 1 alice staff 12 Sep 29 10:00 f",
            "-rw-r--r-é 1 alice staff 12 Sep 29 10:00 f",
        ] {
            assert_eq!(read(line), [], "{line:?}");
        }
    }

    #[test]
    fn no_listing_line_panics_the_parser() {
        // Fragments of real lines, and characters that cut them where no parser expects:
        // a seeded xorshift picks them, the same lines every run.
        const PIECES: [&str; 24] = [
            "-rw-r--r--",
            "crw-rw-rw-",
            "drwxr-xr-x",
            "+",
            "@",
            ".",
            " ",
            "  ",
            "1",
            "1,",
            "3",
            "root",
            "staff",
            "12",
            "Sep",
            "29",
            "10:00",
            "2025",
            "->",
            "é",
            "\t",
            "\u{a0}",
            "total",
            ",",
        ];
        const LINES: usize = 20_000;
        const MOST_PIECES: u64 = 24;
        const READS: &str = "crw-rw-rw-+ 1 root 1, 3 Oct 1 10:00 a b -> c";
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut read_lines = 0usize;
        for round in 0..LINES {
            let pick = |at: u64| {
                usize::try_from(at)
                    .ok()
                    .and_then(|at| PIECES.get(at % PIECES.len()))
            };
            let line: String = if round % 2 == 0 {
                // Pieces put together.
                let count = next() % MOST_PIECES;
                (0..count).filter_map(|_| pick(next()).copied()).collect()
            } else {
                // A line that reads, a piece put in it anywhere.
                let mut line = String::from(READS);
                let at = usize::try_from(next()).unwrap_or_default() % (READS.len() + 1);
                let at = (0..=at)
                    .rev()
                    .find(|&at| line.is_char_boundary(at))
                    .unwrap_or_default();
                line.insert_str(at, pick(next()).copied().unwrap_or_default());
                line
            };
            read_lines += items(&[line], false).len();
        }
        // The lines a piece left readable did read: the loop did reach the parsers.
        assert!(read_lines > 0);
    }

    #[test]
    fn a_delete_goes_into_a_folder_only_never_a_link() {
        assert!(walks(ItemKind::Directory));
        for kind in [
            ItemKind::Link,
            ItemKind::File,
            ItemKind::Other(crate::Special::Unknown),
        ] {
            assert!(!walks(kind), "{kind:?}");
        }
    }

    fn reply(status: Status) -> RemoteError {
        ftp_error(&FtpError::UnexpectedResponse(Response::new(
            status,
            b"why".to_vec(),
        )))
    }

    #[test]
    fn every_ftp_reply_keeps_the_refusal_the_user_is_told() {
        let refusal = |status| match reply(status) {
            RemoteError::Refused { refusal, message } => {
                assert_eq!(message, b"why");
                refusal
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(refusal(Status::FileUnavailable), Refusal::NoSuchFile);
        assert_eq!(refusal(Status::NotLoggedIn), Refusal::PermissionDenied);
        assert_eq!(refusal(Status::NotImplemented), Refusal::Unsupported);
        assert_eq!(refusal(Status::BadFilename), Refusal::Failure);
        assert_eq!(
            ftp_error(&FtpError::ConnectionError(std::io::Error::other("gone"))),
            RemoteError::SessionClosed
        );
    }
}
