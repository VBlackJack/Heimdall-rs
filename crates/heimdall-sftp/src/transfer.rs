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

//! File downloads with many reads in flight, and resume.
//!
//! A download writes `<name>.heimdall-part`, never the real name, and renames it once
//! complete and flushed: a crash never leaves a truncated file under the name the user
//! expects. Reads complete in any order and each lands at its offset; the *mark* is the
//! length below which every byte is written. On any stop the part file is cut back to the
//! mark and the mark is saved beside it (`<name>.heimdall-part.resume`), so a resume never
//! starts after a hole. A resume first checks that the remote file is the one it started
//! from: same path, size and modification time, and the last bytes before the mark read
//! again and compared; otherwise it starts over.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use thiserror::Error;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _, AsyncWriteExt as _};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::client::{Handle, SftpClient, SftpError};
use crate::path::RemotePath;
use crate::protocol::{Attributes, PLAIN_PERMISSIONS, StatusCode, open_flags};

/// Suffix of a file being downloaded.
pub const PART_SUFFIX: &str = ".heimdall-part";

/// Suffix of the resume record beside a part file.
pub const RESUME_SUFFIX: &str = ".heimdall-part.resume";

/// First line of a resume record, with its format version.
const RESUME_HEADER: &str = "heimdall-resume 1";

/// Mode of an upload's temporary file until it is complete: owner only.
const UPLOAD_TEMP_MODE: u32 = 0o600;

/// Mode given to an upload when the local system has no POSIX mode (Windows).
#[cfg(not(unix))]
const DEFAULT_UPLOAD_MODE: u32 = 0o644;

/// Bytes re-read before the mark to check the remote file is unchanged.
const TAIL_CHECK: u64 = 64 * 1024;

/// Bytes written between two saves of the mark.
const SAVE_EVERY: u64 = 4 * 1024 * 1024;

/// Settings of a transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferConfig {
    /// Bytes per read or write request, lowered to the server's limits.
    pub chunk: u32,
    /// Requests in flight at once.
    pub in_flight: usize,
    /// Copy the modification time to the new file.
    pub preserve_times: bool,
}

impl Default for TransferConfig {
    fn default() -> Self {
        Self {
            chunk: 32 * 1024,
            in_flight: 64,
            preserve_times: true,
        }
    }
}

/// Why a transfer stopped.
#[derive(Debug, Error)]
pub enum TransferError {
    /// The server or the session failed.
    #[error(transparent)]
    Sftp(#[from] SftpError),
    /// The local file system failed.
    #[error("{path}: {source}")]
    Local {
        /// File concerned.
        path: PathBuf,
        /// Cause.
        #[source]
        source: io::Error,
    },
    /// The remote path is not a regular file (a directory, a device such as `/dev/zero`).
    #[error("not a regular file")]
    NotARegularFile,
    /// The file to replace is not a regular file (a folder, a link, a device): an upload does
    /// not replace it.
    #[error("the destination is not a regular file")]
    DestinationNotAFile,
    /// The file to replace was left as it is: the server cannot replace it in one step, or the
    /// replacing file could not keep its group.
    #[error("the destination could not be replaced safely")]
    ReplaceNotSafe,
    /// Cancelled; `kept` bytes stay in the part file for a resume.
    #[error("cancelled after {kept} bytes")]
    Cancelled {
        /// Bytes kept.
        kept: u64,
    },
}

/// How a download went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadReport {
    /// Size of the file downloaded.
    pub bytes: u64,
    /// Bytes taken from an earlier attempt; 0 for a fresh download.
    pub resumed_from: u64,
}

fn local(path: &Path) -> impl FnOnce(io::Error) -> TransferError + '_ {
    move |source| TransferError::Local {
        path: path.to_owned(),
        source,
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// What a resume record holds.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResumeRecord {
    remote: Vec<u8>,
    size: u64,
    mtime: u32,
    mark: u64,
}

impl ResumeRecord {
    fn to_text(&self) -> String {
        let mut hex = String::with_capacity(self.remote.len() * 2);
        for byte in &self.remote {
            let _ = write!(hex, "{byte:02x}");
        }
        format!(
            "{RESUME_HEADER}\nremote {hex}\nsize {}\nmtime {}\nmark {}\n",
            self.size, self.mtime, self.mark
        )
    }

    /// The record, or `None` for anything malformed: a resume then starts over.
    fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()? != RESUME_HEADER {
            return None;
        }
        let hex = lines.next()?.strip_prefix("remote ")?;
        if hex.len() % 2 != 0 {
            return None;
        }
        let remote = (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(hex.get(index..index + 2)?, 16).ok())
            .collect::<Option<Vec<u8>>>()?;
        let size = lines.next()?.strip_prefix("size ")?.parse().ok()?;
        let mtime = lines.next()?.strip_prefix("mtime ")?.parse().ok()?;
        let mark = lines.next()?.strip_prefix("mark ")?.parse().ok()?;
        (lines.next().is_none() && mark <= size).then_some(Self {
            remote,
            size,
            mtime,
            mark,
        })
    }
}

/// A download in progress.
struct Download<'a> {
    client: &'a SftpClient,
    handle: Handle,
    part_path: PathBuf,
    resume_path: PathBuf,
    record: ResumeRecord,
    part: File,
}

impl Download<'_> {
    async fn save_mark(&mut self, mark: u64) -> Result<(), TransferError> {
        self.record.mark = mark;
        tokio::fs::write(&self.resume_path, self.record.to_text())
            .await
            .map_err(local(&self.resume_path))
    }

    /// Cuts the part file back to `mark` and records it: nothing after a hole survives.
    async fn keep(&mut self, mark: u64) {
        let _ = self.part.set_len(mark).await;
        let _ = self.part.sync_all().await;
        let _ = self.save_mark(mark).await;
    }

    async fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<(), TransferError> {
        self.part
            .seek(io::SeekFrom::Start(offset))
            .await
            .map_err(local(&self.part_path))?;
        self.part
            .write_all(data)
            .await
            .map_err(local(&self.part_path))
    }

    /// Reads `length` bytes at `offset` in full, following short reads.
    async fn read_exact_remote(&self, offset: u64, length: u64) -> Result<Vec<u8>, TransferError> {
        let mut data = Vec::new();
        while (data.len() as u64) < length {
            let wanted = u32::try_from(length - data.len() as u64).unwrap_or(u32::MAX);
            match self
                .client
                .read(&self.handle, offset + data.len() as u64, wanted)
                .await?
            {
                Some(chunk) if !chunk.is_empty() => data.extend_from_slice(&chunk),
                _ => break,
            }
        }
        Ok(data)
    }
}

fn mtime_of(attributes: &Attributes) -> u32 {
    attributes.times.map_or(0, |(_, modified)| modified)
}

/// Where a download starts: from a valid earlier attempt, or from zero.
async fn resume_point(download: &mut Download<'_>, earlier: Option<ResumeRecord>) -> u64 {
    let Some(earlier) = earlier else {
        return 0;
    };
    let same_file = earlier.remote == download.record.remote
        && earlier.size == download.record.size
        && earlier.mtime == download.record.mtime;
    let part_length = download.part.metadata().await.map_or(0, |meta| meta.len());
    if !same_file || earlier.mark == 0 || part_length < earlier.mark {
        return 0;
    }
    let tail = TAIL_CHECK.min(earlier.mark);
    let start = earlier.mark - tail;
    let Ok(remote_tail) = download.read_exact_remote(start, tail).await else {
        return 0;
    };
    let mut local_tail = vec![0; usize::try_from(tail).unwrap_or(0)];
    let read = async {
        download.part.seek(io::SeekFrom::Start(start)).await?;
        download.part.read_exact(&mut local_tail).await
    };
    if read.await.is_err() || local_tail != remote_tail {
        return 0;
    }
    earlier.mark
}

/// Downloads `remote` to `target`, resuming an earlier attempt when it is safe to.
/// `progress` receives the number of bytes contiguously written.
///
/// The caller has decided what to do with an existing `target`: it is replaced.
///
/// # Errors
///
/// [`TransferError`]; on [`TransferError::Cancelled`] and on server or network errors the
/// part file and its resume record stay for a later attempt.
pub async fn download(
    client: &SftpClient,
    remote: &RemotePath,
    target: &Path,
    config: &TransferConfig,
    cancel: &CancellationToken,
    mut progress: impl FnMut(u64) + Send,
) -> Result<DownloadReport, TransferError> {
    let attributes = client.stat(remote).await?;
    if !attributes.is_regular_file() {
        return Err(TransferError::NotARegularFile);
    }
    let part_path = with_suffix(target, PART_SUFFIX);
    let resume_path = with_suffix(target, RESUME_SUFFIX);
    let earlier = match tokio::fs::read_to_string(&resume_path).await {
        Ok(text) => ResumeRecord::parse(&text),
        Err(_) => None,
    };

    // A part file is reused only with a record; anything else in its place, a planted
    // link included, is removed and replaced by a file created here.
    let reusable = earlier.is_some()
        && tokio::fs::symlink_metadata(&part_path)
            .await
            .is_ok_and(|meta| meta.file_type().is_file());
    if !reusable {
        let _ = tokio::fs::remove_file(&part_path).await;
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if reusable {
        options.create(false);
    } else {
        options.create_new(true);
    }
    let part = options.open(&part_path).await.map_err(local(&part_path))?;

    let handle = client
        .open(remote, open_flags::READ, Attributes::default())
        .await?;
    let mut download = Download {
        client,
        handle,
        part_path,
        resume_path,
        record: ResumeRecord {
            remote: remote.as_bytes().to_vec(),
            size: attributes.size.unwrap_or(0),
            mtime: mtime_of(&attributes),
            mark: 0,
        },
        part,
    };
    let start = resume_point(&mut download, earlier.filter(|_| reusable)).await;
    let result = fetch(&mut download, start, config, cancel, &mut progress).await;
    let _ = client.close(&download.handle).await;
    let bytes = match result {
        Ok(bytes) => bytes,
        Err((mark, error)) => {
            download.keep(mark).await;
            return Err(error);
        }
    };

    download
        .part
        .set_len(bytes)
        .await
        .map_err(local(&download.part_path))?;
    download
        .part
        .sync_all()
        .await
        .map_err(local(&download.part_path))?;
    let Download {
        part,
        part_path,
        resume_path,
        ..
    } = download;
    drop(part);
    tokio::fs::rename(&part_path, target)
        .await
        .map_err(local(target))?;
    let _ = tokio::fs::remove_file(&resume_path).await;
    if config.preserve_times
        && let Some((_, modified)) = attributes.times
    {
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(u64::from(modified));
        if let Ok(file) = std::fs::File::options().write(true).open(target) {
            let _ = file.set_modified(time);
        }
    }
    Ok(DownloadReport {
        bytes,
        resumed_from: start,
    })
}

/// Reads from `start` to the end of the file; returns the file size, or the mark reached
/// and the error.
async fn fetch(
    download: &mut Download<'_>,
    start: u64,
    config: &TransferConfig,
    cancel: &CancellationToken,
    progress: &mut (impl FnMut(u64) + Send),
) -> Result<u64, (u64, TransferError)> {
    let chunk = chunk_size(download.client, config);
    let mut reads: JoinSet<ReadOutcome> = JoinSet::new();
    let mut next = start;
    let mut mark = start;
    let mut saved = start;
    let mut eof_at: Option<u64> = None;
    // Written ranges beyond the mark: start to end.
    let mut ahead: BTreeMap<u64, u64> = BTreeMap::new();
    progress(mark);

    let reader = download.client.clone();
    let file = download.handle.clone();
    let spawn = |reads: &mut JoinSet<_>, offset: u64, length: u32| {
        let client = reader.clone();
        let handle = file.clone();
        reads.spawn(async move { (offset, length, client.read(&handle, offset, length).await) });
    };

    loop {
        while reads.len() < config.in_flight.max(1) && eof_at.is_none_or(|end| next < end) {
            spawn(&mut reads, next, chunk);
            next += u64::from(chunk);
        }
        let joined = tokio::select! {
            () = cancel.cancelled() => {
                reads.abort_all();
                return Err((mark, TransferError::Cancelled { kept: mark }));
            }
            joined = reads.join_next() => joined,
        };
        let Some(joined) = joined else { break };
        let Ok((offset, length, result)) = joined else {
            reads.abort_all();
            return Err((
                mark,
                TransferError::Local {
                    path: download.part_path.clone(),
                    source: io::Error::other("a read task failed"),
                },
            ));
        };
        match result {
            Ok(Some(data)) if !data.is_empty() => {
                let end = offset + data.len() as u64;
                if let Err(error) = download.write_at(offset, &data).await {
                    reads.abort_all();
                    return Err((mark, error));
                }
                ahead.insert(offset, end);
                let got = u32::try_from(data.len()).unwrap_or(length);
                if got < length && eof_at.is_none_or(|eof| end < eof) {
                    // A short read is not the end: ask for the rest.
                    spawn(&mut reads, end, length - got);
                }
            }
            Ok(_) => eof_at = Some(eof_at.map_or(offset, |eof| eof.min(offset))),
            Err(error) => {
                reads.abort_all();
                return Err((mark, error.into()));
            }
        }
        while let Some(end) = ahead.remove(&mark) {
            mark = end;
        }
        progress(mark);
        if mark - saved >= SAVE_EVERY {
            saved = mark;
            if let Err(error) = download.save_mark(mark).await {
                reads.abort_all();
                return Err((mark, error));
            }
        }
    }
    let end = eof_at.unwrap_or(mark);
    if mark < end {
        // Every read answered and a gap remains: the server broke its own answers.
        return Err((
            mark,
            TransferError::Sftp(SftpError::Closed(crate::client::Closed::Protocol(
                "a gap in the file".to_owned(),
            ))),
        ));
    }
    Ok(end)
}

/// A read's offset, the length asked, and what came back.
type ReadOutcome = (u64, u32, Result<Option<Vec<u8>>, SftpError>);

/// The request size: the configured chunk, lowered to what the server announced.
fn chunk_size(client: &SftpClient, config: &TransferConfig) -> u32 {
    let limit = client.limits().max_read;
    if limit == 0 {
        config.chunk.max(1)
    } else {
        config
            .chunk
            .min(u32::try_from(limit).unwrap_or(u32::MAX))
            .max(1)
    }
}

/// How an upload went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadReport {
    /// Bytes sent.
    pub bytes: u64,
    /// Whether the server confirmed the data reached its storage (`fsync@openssh.com`).
    pub flushed: bool,
}

/// A temporary name beside `target`, hidden and unique enough not to meet another upload.
fn upload_temp(target: &RemotePath) -> RemotePath {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut name = b".".to_vec();
    name.extend_from_slice(target.file_name().unwrap_or(b"upload"));
    name.extend_from_slice(format!(".heimdall-upload-{}-{stamp}", std::process::id()).as_bytes());
    target.parent().join(&name)
}

/// The mode an uploaded file gets: the local permission bits, never set-user-id,
/// set-group-id or sticky.
fn upload_mode(metadata: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        metadata.permissions().mode() & PLAIN_PERMISSIONS
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        DEFAULT_UPLOAD_MODE & PLAIN_PERMISSIONS
    }
}

/// Uploads `source` to `target`. The data goes to a hidden temporary file beside the
/// target, readable by its owner only, then gets its mode and date, and replaces the
/// target in one rename: nobody ever sees a partial file under the real name. An upload
/// does not resume: a new attempt starts over.
///
/// With `replace`, an existing target is replaced, as the C# `SftpAtomicUpload` does: only a
/// regular file, only in one step (`posix-rename@openssh.com`), and keeping its permissions
/// and its group; otherwise it is left as it is and the upload fails. A server without the
/// atomic rename can still create a file that does not exist. Without `replace`, an existing
/// target makes the upload fail and stay untouched.
///
/// # Errors
///
/// [`TransferError`]; [`TransferError::DestinationNotAFile`] and
/// [`TransferError::ReplaceNotSafe`] for a target left as it is. The temporary file is removed
/// whatever stopped the upload.
pub async fn upload(
    client: &SftpClient,
    source: &Path,
    target: &RemotePath,
    replace: bool,
    config: &TransferConfig,
    cancel: &CancellationToken,
    mut progress: impl FnMut(u64) + Send,
) -> Result<UploadReport, TransferError> {
    // A link is not followed: a folder upload must not leave its folder through one.
    let metadata = tokio::fs::symlink_metadata(source)
        .await
        .map_err(local(source))?;
    if !metadata.file_type().is_file() {
        return Err(TransferError::NotARegularFile);
    }
    let replaced = if replace {
        match client.lstat(target).await {
            Ok(existing) if !existing.is_regular_file() => {
                return Err(TransferError::DestinationNotAFile);
            }
            Ok(existing) => Some(existing),
            Err(error) if is_missing(&error) => None,
            Err(error) => return Err(error.into()),
        }
    } else {
        None
    };
    let file = File::open(source).await.map_err(local(source))?;
    let temp = upload_temp(target);
    let handle = client
        .open(
            &temp,
            open_flags::WRITE | open_flags::CREATE | open_flags::EXCLUSIVE,
            Attributes {
                permissions: Some(UPLOAD_TEMP_MODE),
                ..Attributes::default()
            },
        )
        .await?;
    let sent = send(client, &handle, file, source, config, cancel, &mut progress).await;
    let finished = match sent {
        Ok(bytes) => finish(
            client,
            &handle,
            &temp,
            target,
            &metadata,
            if replace {
                Commit::Replace(replaced.as_ref())
            } else {
                Commit::Create
            },
            config,
        )
        .await
        .map(|flushed| UploadReport { bytes, flushed }),
        Err(error) => {
            let _ = client.close(&handle).await;
            Err(error)
        }
    };
    if finished.is_err() {
        let _ = client.remove(&temp).await;
    }
    finished
}

/// Sends the file's bytes with writes in flight; returns the size sent.
async fn send(
    client: &SftpClient,
    handle: &Handle,
    mut file: File,
    source: &Path,
    config: &TransferConfig,
    cancel: &CancellationToken,
    progress: &mut (impl FnMut(u64) + Send),
) -> Result<u64, TransferError> {
    let chunk = write_chunk(client, config) as usize;
    let mut writes: JoinSet<(u64, Result<(), SftpError>)> = JoinSet::new();
    let mut offset = 0u64;
    let mut acknowledged = 0u64;
    let mut at_end = false;
    progress(0);
    loop {
        while !at_end && writes.len() < config.in_flight.max(1) {
            let mut data = vec![0; chunk];
            let mut filled = 0;
            while filled < chunk {
                let read = file
                    .read(&mut data[filled..])
                    .await
                    .map_err(local(source))?;
                if read == 0 {
                    at_end = true;
                    break;
                }
                filled += read;
            }
            if filled == 0 {
                break;
            }
            data.truncate(filled);
            let (client, handle, at) = (client.clone(), handle.clone(), offset);
            offset += filled as u64;
            writes.spawn(async move {
                let length = data.len() as u64;
                (length, client.write(&handle, at, data).await)
            });
        }
        let joined = tokio::select! {
            () = cancel.cancelled() => {
                writes.abort_all();
                return Err(TransferError::Cancelled { kept: 0 });
            }
            joined = writes.join_next() => joined,
        };
        let Some(joined) = joined else { break };
        let Ok((length, result)) = joined else {
            writes.abort_all();
            return Err(local(source)(io::Error::other("a write task failed")));
        };
        if let Err(error) = result {
            writes.abort_all();
            return Err(error.into());
        }
        acknowledged += length;
        progress(acknowledged);
    }
    Ok(offset)
}

/// Whether `error` says the path does not exist.
fn is_missing(error: &SftpError) -> bool {
    matches!(
        error,
        SftpError::Status {
            code: StatusCode::NoSuchFile,
            ..
        }
    )
}

/// How an upload lands on its target.
#[derive(Clone, Copy)]
enum Commit<'a> {
    /// Only where nothing is: the plain rename refuses an existing name.
    Create,
    /// Replacing allowed, with the file found there when there was one: the new file keeps
    /// its permissions and group.
    Replace(Option<&'a Attributes>),
}

/// Flushes and closes the temporary file, gives it its mode and date, and renames it over
/// the target as `commit` says. Returns whether the server flushed to storage.
async fn finish(
    client: &SftpClient,
    handle: &Handle,
    temp: &RemotePath,
    target: &RemotePath,
    metadata: &std::fs::Metadata,
    commit: Commit<'_>,
    config: &TransferConfig,
) -> Result<bool, TransferError> {
    let flushed = match client.fsync(handle).await {
        Ok(flushed) => flushed,
        Err(error) => {
            let _ = client.close(handle).await;
            return Err(error.into());
        }
    };
    // A server may report a failed write (quota, NFS) only when the file is closed.
    client.close(handle).await?;
    let times = if config.preserve_times {
        metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
            .and_then(|since| u32::try_from(since.as_secs()).ok())
            .map(|seconds| (seconds, seconds))
    } else {
        None
    };
    let existing = match commit {
        Commit::Replace(existing) => existing,
        Commit::Create => None,
    };
    // A replaced file keeps its permissions: a private file stays private. Set-user-id,
    // set-group-id and sticky are never copied, as for any transfer.
    let permissions = existing
        .and_then(|existing| existing.permissions)
        .map_or_else(|| upload_mode(metadata), |mode| mode & PLAIN_PERMISSIONS);
    client
        .setstat(
            temp,
            Attributes {
                permissions: Some(permissions),
                times,
                ..Attributes::default()
            },
        )
        .await?;
    if let Some((_, group)) = existing.and_then(|existing| existing.uid_gid) {
        keep_group(client, temp, group).await?;
    }
    if matches!(commit, Commit::Create) {
        client.rename(temp, target, false).await?;
        return Ok(flushed);
    }
    if client.rename(temp, target, true).await.is_ok() {
        return Ok(flushed);
    }
    // No atomic replace: the file is created only where nothing is, as the C# CommitRename
    // does; an existing one is never removed first, which would lose it if the rename failed.
    match client.lstat(target).await {
        Ok(_) => Err(TransferError::ReplaceNotSafe),
        Err(error) if is_missing(&error) => {
            client.rename(temp, target, false).await?;
            Ok(flushed)
        }
        Err(error) => Err(error.into()),
    }
}

/// Gives `temp` the group `group` when it has another: the group a replaced file had, so its
/// members keep their access. Refused with [`TransferError::ReplaceNotSafe`] when the server
/// will not.
async fn keep_group(
    client: &SftpClient,
    temp: &RemotePath,
    group: u32,
) -> Result<(), TransferError> {
    let Some((owner, current)) = client.lstat(temp).await?.uid_gid else {
        return Ok(());
    };
    if current == group {
        return Ok(());
    }
    client
        .setstat(
            temp,
            Attributes {
                uid_gid: Some((owner, group)),
                ..Attributes::default()
            },
        )
        .await
        .map_err(|_| TransferError::ReplaceNotSafe)
}

/// The write size: the configured chunk, lowered to what the server announced.
fn write_chunk(client: &SftpClient, config: &TransferConfig) -> u32 {
    let limit = client.limits().max_write;
    if limit == 0 {
        config.chunk.max(1)
    } else {
        config
            .chunk
            .min(u32::try_from(limit).unwrap_or(u32::MAX))
            .max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::ResumeRecord;

    #[test]
    fn a_resume_record_round_trips_and_garbage_is_refused() {
        let record = ResumeRecord {
            remote: b"/srv/caf\xE9".to_vec(),
            size: 10,
            mtime: 42,
            mark: 7,
        };
        assert_eq!(ResumeRecord::parse(&record.to_text()), Some(record.clone()));
        let mut beyond = record;
        beyond.mark = 11;
        assert_eq!(
            ResumeRecord::parse(&beyond.to_text()),
            None,
            "mark past the size"
        );
        for garbage in ["", "heimdall-resume 2\n", "heimdall-resume 1\nremote zz\n"] {
            assert_eq!(ResumeRecord::parse(garbage), None, "{garbage:?}");
        }
    }
}
