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

//! Transfers planned whole before they start, as the C# plans its uploads: every file and
//! folder a selection would write is listed first, then checked against what the
//! destination holds, so that the user answers every conflict at once and nothing is
//! replaced unasked, inside a folder as much as at the top.
//!
//! The walks never follow a link and keep the limits of the SFTP ones ([`MAX_DEPTH`],
//! [`MAX_ENTRIES`]); a download's names are checked as local names, a folder's entries never
//! taking the same local name twice.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use heimdall_sftp::local_name::{FolderNames, LocalName, Rules};
use heimdall_sftp::tree::{MAX_DEPTH, MAX_ENTRIES};
use tokio_util::sync::CancellationToken;

use crate::conflict::{self, Checked, Choice, Kind, Outcome, PlanError, Planned, Policy, Target};
use crate::{FolderReport, ItemKind, RemoteError, RemotePath, RemoteSession};

/// One entry the user picked, with both ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Root {
    /// On the server.
    pub remote: RemotePath,
    /// On this computer.
    pub local: PathBuf,
    /// A folder, transferred with everything in it, or a file.
    pub kind: Kind,
}

/// One file or folder a transfer writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Which picked entry it is part of.
    pub root: usize,
    /// On the server.
    pub remote: RemotePath,
    /// On this computer.
    pub local: PathBuf,
    /// What it is.
    pub kind: Kind,
    /// Its names from the destination folder down, as written.
    pub target: Target,
}

/// A step once conflicts are answered: where it goes, and whether it may replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ready {
    /// On the server.
    pub remote: RemotePath,
    /// On this computer.
    pub local: PathBuf,
    /// What it is.
    pub kind: Kind,
    /// Replace what is there; only when the user chose so.
    pub replace: bool,
}

/// A selection's transfer, planned and checked, waiting for the user's answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    download: bool,
    roots: usize,
    steps: Vec<Step>,
    checked: Vec<Checked>,
    /// Every entry found in the destination folders looked at, folded, and what it is.
    taken: HashMap<Target, Kind>,
    /// Entries each picked entry's walk left out: links, special files, unusable names.
    left_out: Vec<usize>,
}

impl Plan {
    /// The entries the user must answer for: their index and what they are.
    pub fn conflicts(&self) -> impl Iterator<Item = (usize, &Step, &Checked)> {
        self.steps
            .iter()
            .zip(&self.checked)
            .enumerate()
            .filter(|(_, (_, checked))| checked.is_conflict())
            .map(|(index, (step, checked))| (index, step, checked))
    }

    /// Whether the user must answer anything.
    #[must_use]
    pub fn has_conflicts(&self) -> bool {
        self.checked.iter().any(Checked::is_conflict)
    }

    /// A download, rather than an upload.
    #[must_use]
    pub const fn is_download(&self) -> bool {
        self.download
    }

    /// Entries the walk of picked entry `root` left out.
    #[must_use]
    pub fn left_out(&self, root: usize) -> usize {
        self.left_out.get(root).copied().unwrap_or(0)
    }

    /// Applies the user's answers, `(index, choice)` for every conflict; returns, for each
    /// picked entry in order, the steps to run, skipped ones left out.
    ///
    /// # Errors
    ///
    /// [`PlanError`] for answers missing, extra, or not allowed.
    pub fn resolve(&self, answers: &[(usize, Choice)]) -> Result<Vec<Vec<Ready>>, PlanError> {
        let fold = self.fold();
        let outcomes = conflict::resolve(
            &self.checked,
            answers,
            |target| self.taken.contains_key(&folded(target, fold)),
            fold,
        )?;
        let mut ready = vec![Vec::new(); self.roots];
        for (step, outcome) in self.steps.iter().zip(outcomes) {
            let (replace, renamed) = match outcome {
                Outcome::Skip => continue,
                Outcome::Write { replace } => (replace, None),
                Outcome::WriteAs(target) => (false, target.last().cloned()),
            };
            let (remote, local) = match renamed {
                None => (step.remote.clone(), step.local.clone()),
                // Only the written end takes the free name.
                Some(name) if self.download => {
                    let Some(name) = os_name(&name) else {
                        continue;
                    };
                    (step.remote.clone(), step.local.with_file_name(name))
                }
                Some(name) => (step.remote.parent().join(&name), step.local.clone()),
            };
            if let Some(list) = ready.get_mut(step.root) {
                list.push(Ready {
                    remote,
                    local,
                    kind: step.kind,
                    replace,
                });
            }
        }
        Ok(ready)
    }

    fn fold(&self) -> fn(&[u8]) -> Vec<u8> {
        if self.download {
            local_fold
        } else {
            <[u8]>::to_vec
        }
    }
}

impl RemoteSession {
    /// Plans downloading `roots` into local folder `into`: every file and folder below them,
    /// checked against what `into` holds.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] when the session ends, the walk is cancelled or too large.
    pub async fn plan_download(
        &self,
        roots: &[Root],
        cancel: &CancellationToken,
    ) -> Result<Plan, RemoteError> {
        let mut walk = Walk::new(roots.len());
        for (index, root) in roots.iter().enumerate() {
            let target = vec![os_bytes(root.local.file_name().unwrap_or_default())];
            walk.push(
                index,
                root.remote.clone(),
                root.local.clone(),
                root.kind,
                target,
            );
            if root.kind == Kind::Folder {
                self.walk_remote(&mut walk, index, root, cancel).await?;
            }
        }
        let taken = local_names(&walk.steps).await;
        Ok(walk.finish(true, taken))
    }

    /// Plans uploading `roots` into remote folder `into`: every file and folder below them,
    /// checked against what `into` holds.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] when the session ends, the walk is cancelled or too large.
    pub async fn plan_upload(
        &self,
        roots: &[Root],
        cancel: &CancellationToken,
    ) -> Result<Plan, RemoteError> {
        let mut walk = Walk::new(roots.len());
        for (index, root) in roots.iter().enumerate() {
            let target = vec![root.remote.file_name().unwrap_or_default().to_vec()];
            walk.push(
                index,
                root.remote.clone(),
                root.local.clone(),
                root.kind,
                target,
            );
            if root.kind == Kind::Folder {
                walk_local(&mut walk, index, root, cancel).await?;
            }
        }
        let taken = self.remote_names(&walk.steps).await?;
        Ok(walk.finish(false, taken))
    }

    /// Runs the steps of one picked entry in order; returns how many failed alone.
    ///
    /// # Errors
    ///
    /// [`RemoteError`] when the session ends or the transfer is cancelled; the steps run
    /// before stay done.
    pub async fn run(
        &self,
        download: bool,
        steps: &[Ready],
        cancel: &CancellationToken,
        mut progress: impl FnMut(u64) + Send,
    ) -> Result<FolderReport, RemoteError> {
        let mut skipped = 0usize;
        let mut done = 0u64;
        for step in steps {
            if cancel.is_cancelled() {
                return Err(RemoteError::Cancelled);
            }
            let before = done;
            let moved = match (step.kind, download) {
                (Kind::Folder, true) => tokio::fs::create_dir_all(&step.local)
                    .await
                    .map(|()| 0)
                    .map_err(|e| RemoteError::Local {
                        detail: e.to_string(),
                    }),
                (Kind::Folder, false) => self.ensure_folder(&step.remote).await.map(|()| 0),
                (Kind::File, true) => {
                    self.download_with(&step.remote, &step.local, step.replace, cancel, |n| {
                        progress(before + n);
                    })
                    .await
                }
                (Kind::File, false) => {
                    self.upload(&step.local, &step.remote, step.replace, cancel, |n| {
                        progress(before + n);
                    })
                    .await
                }
            };
            match moved {
                Ok(bytes) => done += bytes,
                Err(error) if stops(&error) => return Err(error),
                Err(_) => skipped += 1,
            }
        }
        Ok(FolderReport { skipped })
    }

    /// Creates remote folder `path`, accepting one already there as a folder.
    async fn ensure_folder(&self, path: &RemotePath) -> Result<(), RemoteError> {
        let Err(error) = self.make_folder(path).await else {
            return Ok(());
        };
        let parent = self.list(&path.parent()).await?;
        let name = path.file_name().unwrap_or_default();
        if parent
            .iter()
            .any(|item| item.name == name && item.kind == ItemKind::Directory)
        {
            Ok(())
        } else {
            Err(error)
        }
    }

    async fn walk_remote(
        &self,
        walk: &mut Walk,
        root: usize,
        top: &Root,
        cancel: &CancellationToken,
    ) -> Result<(), RemoteError> {
        let target = walk
            .steps
            .last()
            .map(|step| step.target.clone())
            .unwrap_or_default();
        let mut pending = vec![(top.remote.clone(), top.local.clone(), target, 0usize)];
        while let Some((remote_dir, local_dir, target, depth)) = pending.pop() {
            if cancel.is_cancelled() {
                return Err(RemoteError::Cancelled);
            }
            let mut names = FolderNames::new(Rules::native());
            for entry in self.list(&remote_dir).await? {
                walk.visit()?;
                let Ok(local) = LocalName::from_remote(&entry.name, Rules::native()) else {
                    walk.leave_out(root);
                    continue;
                };
                if names.claim(&local).is_err() {
                    walk.leave_out(root);
                    continue;
                }
                let remote = remote_dir.join(&entry.name);
                let local_path = local_dir.join(&local.name);
                let mut below = target.clone();
                below.push(os_bytes(&local.name));
                match entry.kind {
                    ItemKind::Directory if depth < MAX_DEPTH => {
                        walk.push(
                            root,
                            remote.clone(),
                            local_path.clone(),
                            Kind::Folder,
                            below.clone(),
                        );
                        pending.push((remote, local_path, below, depth + 1));
                    }
                    ItemKind::File => {
                        walk.push(root, remote, local_path, Kind::File, below);
                    }
                    // Too deep, a link, anything else: left out.
                    _ => walk.leave_out(root),
                }
            }
        }
        Ok(())
    }

    /// The entries in every remote folder the steps write into.
    async fn remote_names(&self, steps: &[Step]) -> Result<HashMap<Target, Kind>, RemoteError> {
        let mut taken = HashMap::new();
        let mut looked = HashSet::new();
        for step in steps {
            let folder = step.remote.parent();
            if !looked.insert(folder.clone()) {
                continue;
            }
            let parent = &step.target[..step.target.len().saturating_sub(1)];
            match self.list(&folder).await {
                Ok(items) => {
                    for item in items {
                        let mut name = parent.to_vec();
                        name.push(item.name);
                        // A link is never entered: it is in the way as a file.
                        let kind = if item.kind == ItemKind::Directory {
                            Kind::Folder
                        } else {
                            Kind::File
                        };
                        taken.insert(name, kind);
                    }
                }
                // A folder the transfer creates holds nothing yet.
                Err(RemoteError::Refused { .. }) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(taken)
    }
}

/// What a walk has gathered so far.
struct Walk {
    steps: Vec<Step>,
    left_out: Vec<usize>,
    visited: usize,
}

impl Walk {
    fn new(roots: usize) -> Self {
        Self {
            steps: Vec::new(),
            left_out: vec![0; roots],
            visited: 0,
        }
    }

    fn push(
        &mut self,
        root: usize,
        remote: RemotePath,
        local: PathBuf,
        kind: Kind,
        target: Target,
    ) {
        self.steps.push(Step {
            root,
            remote,
            local,
            kind,
            target,
        });
    }

    fn visit(&mut self) -> Result<(), RemoteError> {
        self.visited += 1;
        if self.visited > MAX_ENTRIES {
            Err(RemoteError::TooLarge)
        } else {
            Ok(())
        }
    }

    fn leave_out(&mut self, root: usize) {
        if let Some(count) = self.left_out.get_mut(root) {
            *count += 1;
        }
    }

    /// Checks the steps against `taken`, the entries found where they go.
    fn finish(self, download: bool, taken: HashMap<Target, Kind>) -> Plan {
        let fold: fn(&[u8]) -> Vec<u8> = if download { local_fold } else { <[u8]>::to_vec };
        let taken: HashMap<Target, Kind> = taken
            .into_iter()
            .map(|(target, kind)| (folded(&target, fold), kind))
            .collect();
        let planned: Vec<Planned> = self
            .steps
            .iter()
            .map(|step| Planned {
                target: step.target.clone(),
                kind: step.kind,
            })
            .collect();
        let checked = conflict::check(
            &planned,
            |target| taken.get(&folded(target, fold)).copied(),
            fold,
            Policy::Transfer,
        );
        Plan {
            download,
            roots: self.left_out.len(),
            steps: self.steps,
            checked,
            taken,
            left_out: self.left_out,
        }
    }
}

async fn walk_local(
    walk: &mut Walk,
    root: usize,
    top: &Root,
    cancel: &CancellationToken,
) -> Result<(), RemoteError> {
    let local_error = |e: std::io::Error| RemoteError::Local {
        detail: e.to_string(),
    };
    let target = walk
        .steps
        .last()
        .map(|step| step.target.clone())
        .unwrap_or_default();
    let mut pending = vec![(top.local.clone(), top.remote.clone(), target, 0usize)];
    while let Some((local_dir, remote_dir, target, depth)) = pending.pop() {
        if cancel.is_cancelled() {
            return Err(RemoteError::Cancelled);
        }
        let mut entries = tokio::fs::read_dir(&local_dir).await.map_err(local_error)?;
        while let Some(entry) = entries.next_entry().await.map_err(local_error)? {
            walk.visit()?;
            // Not followed: the link itself.
            let kind = entry.file_type().await.map_err(local_error)?;
            let Some(name) = os_name_bytes(&entry.file_name()) else {
                walk.leave_out(root);
                continue;
            };
            let remote = remote_dir.join(&name);
            let mut below = target.clone();
            below.push(name);
            if kind.is_dir() && depth < MAX_DEPTH {
                walk.push(
                    root,
                    remote.clone(),
                    entry.path(),
                    Kind::Folder,
                    below.clone(),
                );
                pending.push((entry.path(), remote, below, depth + 1));
            } else if kind.is_file() {
                walk.push(root, remote, entry.path(), Kind::File, below);
            } else {
                walk.leave_out(root);
            }
        }
    }
    Ok(())
}

/// The entries in every local folder the steps write into.
async fn local_names(steps: &[Step]) -> HashMap<Target, Kind> {
    let mut taken = HashMap::new();
    let mut looked = HashSet::new();
    for step in steps {
        let Some(folder) = step.local.parent() else {
            continue;
        };
        if !looked.insert(folder.to_owned()) {
            continue;
        }
        let parent = &step.target[..step.target.len().saturating_sub(1)];
        // A folder the transfer creates holds nothing yet.
        let Ok(mut entries) = tokio::fs::read_dir(folder).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let mut name = parent.to_vec();
            name.push(os_bytes(&entry.file_name()));
            // Not followed: a link is in the way as a file.
            let folder = entry.file_type().await.is_ok_and(|kind| kind.is_dir());
            taken.insert(name, if folder { Kind::Folder } else { Kind::File });
        }
    }
    taken
}

/// Whether a failure stops the whole transfer rather than one entry.
fn stops(error: &RemoteError) -> bool {
    matches!(error, RemoteError::SessionClosed | RemoteError::Cancelled)
}

fn folded(target: &[Vec<u8>], fold: fn(&[u8]) -> Vec<u8>) -> Target {
    target.iter().map(|name| fold(name)).collect()
}

/// How this computer compares names: without case where its file systems do.
#[cfg(any(windows, target_os = "macos"))]
fn local_fold(name: &[u8]) -> Vec<u8> {
    String::from_utf8_lossy(name).to_lowercase().into_bytes()
}

#[cfg(not(any(windows, target_os = "macos")))]
fn local_fold(name: &[u8]) -> Vec<u8> {
    name.to_vec()
}

#[cfg(unix)]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the same form as on Windows, where a name can fail"
)]
fn os_name_bytes(name: &std::ffi::OsStr) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt as _;
    Some(name.as_bytes().to_vec())
}

/// A name that is not Unicode has no form a server takes: left out.
#[cfg(not(unix))]
fn os_name_bytes(name: &std::ffi::OsStr) -> Option<Vec<u8>> {
    name.to_str().map(|name| name.as_bytes().to_vec())
}

fn os_bytes(name: &std::ffi::OsStr) -> Vec<u8> {
    os_name_bytes(name).unwrap_or_else(|| name.to_string_lossy().into_owned().into_bytes())
}

#[cfg(unix)]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the same form as on Windows, where a name can fail"
)]
fn os_name(name: &[u8]) -> Option<std::ffi::OsString> {
    use std::os::unix::ffi::OsStrExt as _;
    Some(std::ffi::OsStr::from_bytes(name).to_owned())
}

#[cfg(not(unix))]
fn os_name(name: &[u8]) -> Option<std::ffi::OsString> {
    std::str::from_utf8(name).ok().map(std::ffi::OsString::from)
}
