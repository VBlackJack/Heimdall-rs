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

//! The profile file of Heimdall-rs.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::folder::{self, FolderError};
use crate::profile::{
    LocalApproval, LocalProfile, ProfileId, RdpProfile, SshGateway, SshProfile, TelnetProfile,
    VncProfile, WinRmProfile,
};

/// Format version written into the profile file.
///
/// 2 added RDP profiles, 3 Telnet profiles, 4 VNC profiles, 5 local profiles, 6 SSH
/// gateways and the gateway an SSH profile goes through, 8 folders of their own, empty ones
/// included. A build that knows an older version refuses a newer file rather than reading
/// it, dropping what it does not know, and saving it back.
pub const PROFILE_FILE_VERSION: u32 = 8;

/// Oldest format version still read; its files hold SSH profiles only.
const OLDEST_READ_VERSION: u32 = 1;

/// On-disk shape of the profile file.
#[derive(Debug, Serialize, Deserialize)]
struct ProfileFile {
    version: u32,
    #[serde(default)]
    ssh: Vec<SshProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rdp: Vec<RdpProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    telnet: Vec<TelnetProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    vnc: Vec<VncProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    local: Vec<LocalProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    gateway: Vec<SshGateway>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    winrm: Vec<WinRmProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    folder: Vec<String>,
}

/// Why the profile file could not be read or written.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The file or its directory could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// File or directory concerned.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// The file is not valid TOML for this format.
    #[error("{path}: {source}")]
    Parse {
        /// File concerned.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: toml::de::Error,
    },
    /// The profiles could not be serialised.
    #[error("profiles could not be serialised: {0}")]
    Serialize(#[from] toml::ser::Error),
    /// The file was written by a newer or unknown format.
    #[error("{path}: format version {found}, expected {expected}")]
    UnsupportedVersion {
        /// File concerned.
        path: PathBuf,
        /// Version found in the file.
        found: u32,
        /// Version this build reads.
        expected: u32,
    },
}

/// Outcome of [`ProfileStore::merge`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// Profiles whose identifier was not in the store.
    pub added: usize,
    /// Profiles whose identifier was in the store with different content.
    pub updated: usize,
    /// Profiles already in the store with the same content.
    pub unchanged: usize,
}

/// Profiles held in one file.
#[derive(Debug, Clone)]
pub struct ProfileStore {
    path: PathBuf,
    ssh: Vec<SshProfile>,
    rdp: Vec<RdpProfile>,
    telnet: Vec<TelnetProfile>,
    vnc: Vec<VncProfile>,
    local: Vec<LocalProfile>,
    gateways: Vec<SshGateway>,
    winrm: Vec<WinRmProfile>,
    /// Folders kept for themselves, as the C# Heimdall's empty groups: normalised.
    folders: Vec<String>,
}

/// Why an SSH profile's gateways cannot be followed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RouteError {
    /// A gateway named on the way is not in the store.
    #[error("the SSH gateway {0} is not in the profiles")]
    MissingGateway(ProfileId),
    /// A gateway is reached through itself.
    #[error("the SSH gateway {0} is reached through itself")]
    Loop(ProfileId),
}

impl ProfileStore {
    /// An empty store that will save to `path`, without reading it. For when the existing
    /// file is unreadable: saving then writes to `path`, never over the unreadable file,
    /// provided the caller passes another path.
    #[must_use]
    pub fn empty(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            ssh: Vec::new(),
            rdp: Vec::new(),
            telnet: Vec::new(),
            vnc: Vec::new(),
            local: Vec::new(),
            gateways: Vec::new(),
            winrm: Vec::new(),
            folders: Vec::new(),
        }
    }

    /// Opens the store at `path`; a missing file is an empty store.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file exists and cannot be read, does not parse, or has
    /// another format version.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let path = path.into();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self::empty(path));
            }
            Err(source) => return Err(StoreError::Io { path, source }),
        };
        let file: ProfileFile = match toml::from_str(&text) {
            Ok(file) => file,
            Err(source) => return Err(StoreError::Parse { path, source }),
        };
        if !(OLDEST_READ_VERSION..=PROFILE_FILE_VERSION).contains(&file.version) {
            return Err(StoreError::UnsupportedVersion {
                path,
                found: file.version,
                expected: PROFILE_FILE_VERSION,
            });
        }
        Ok(Self {
            path,
            ssh: file.ssh,
            rdp: file.rdp,
            telnet: file.telnet,
            vnc: file.vnc,
            local: file.local,
            gateways: file.gateway,
            winrm: file.winrm,
            folders: file
                .folder
                .iter()
                .map(|path| folder::normal(path))
                .filter(|path| !path.is_empty())
                .collect(),
        })
    }

    /// File this store reads and writes.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// SSH profiles, in file order.
    #[must_use]
    pub fn ssh_profiles(&self) -> &[SshProfile] {
        &self.ssh
    }

    /// RDP profiles, in file order.
    #[must_use]
    pub fn rdp_profiles(&self) -> &[RdpProfile] {
        &self.rdp
    }

    /// Telnet profiles, in file order.
    #[must_use]
    pub fn telnet_profiles(&self) -> &[TelnetProfile] {
        &self.telnet
    }

    /// VNC profiles, in file order.
    #[must_use]
    pub fn vnc_profiles(&self) -> &[VncProfile] {
        &self.vnc
    }

    /// Local profiles, in file order.
    #[must_use]
    pub fn local_profiles(&self) -> &[LocalProfile] {
        &self.local
    }

    /// `WinRM` profiles, in file order.
    #[must_use]
    pub fn winrm_profiles(&self) -> &[WinRmProfile] {
        &self.winrm
    }

    /// Adds or replaces `WinRM` profiles by identifier; the order of existing ones is kept.
    pub fn merge_winrm(&mut self, incoming: impl IntoIterator<Item = WinRmProfile>) -> MergeReport {
        merge_into(&mut self.winrm, incoming, |profile| &profile.id)
    }

    /// SSH gateways, in file order.
    #[must_use]
    pub fn gateways(&self) -> &[SshGateway] {
        &self.gateways
    }

    /// The gateways to go through to reach a server whose gateway is `gateway`, nearest to
    /// this machine first: the gateway's parents from the farthest back, then the gateway.
    ///
    /// # Errors
    ///
    /// [`RouteError`] when a gateway on the way is not in the store, or comes back.
    pub fn route(&self, gateway: Option<&ProfileId>) -> Result<Vec<SshGateway>, RouteError> {
        let mut route = Vec::new();
        let mut next = gateway;
        while let Some(id) = next {
            if route.iter().any(|seen: &SshGateway| &seen.id == id) {
                return Err(RouteError::Loop(id.clone()));
            }
            let found = self
                .gateways
                .iter()
                .find(|candidate| &candidate.id == id)
                .ok_or_else(|| RouteError::MissingGateway(id.clone()))?;
            route.push(found.clone());
            next = found.parent.as_ref();
        }
        route.reverse();
        Ok(route)
    }

    /// Adds or replaces SSH gateways by identifier; the order of existing ones is kept.
    pub fn merge_gateways(
        &mut self,
        incoming: impl IntoIterator<Item = SshGateway>,
    ) -> MergeReport {
        merge_into(&mut self.gateways, incoming, |gateway| &gateway.id)
    }

    /// Adds or replaces SSH profiles by identifier; the order of existing profiles is kept.
    pub fn merge(&mut self, incoming: impl IntoIterator<Item = SshProfile>) -> MergeReport {
        merge_into(&mut self.ssh, incoming, |profile| &profile.id)
    }

    /// Adds or replaces RDP profiles by identifier; the order of existing profiles is kept.
    pub fn merge_rdp(&mut self, incoming: impl IntoIterator<Item = RdpProfile>) -> MergeReport {
        merge_into(&mut self.rdp, incoming, |profile| &profile.id)
    }

    /// Adds or replaces Telnet profiles by identifier; the order of existing profiles is
    /// kept.
    pub fn merge_telnet(
        &mut self,
        incoming: impl IntoIterator<Item = TelnetProfile>,
    ) -> MergeReport {
        merge_into(&mut self.telnet, incoming, |profile| &profile.id)
    }

    /// Adds or replaces VNC profiles by identifier; the order of existing profiles is kept.
    pub fn merge_vnc(&mut self, incoming: impl IntoIterator<Item = VncProfile>) -> MergeReport {
        merge_into(&mut self.vnc, incoming, |profile| &profile.id)
    }

    /// Adds or replaces local profiles by identifier; the order of existing profiles is kept.
    ///
    /// A profile already in the store keeps its approval: an approval names what it approved,
    /// so it still holds only if the incoming profile runs the same thing. An approval is
    /// never taken from the incoming profile.
    pub fn merge_local(&mut self, incoming: impl IntoIterator<Item = LocalProfile>) -> MergeReport {
        let incoming: Vec<LocalProfile> = incoming
            .into_iter()
            .map(|mut profile| {
                profile.approved = self
                    .local
                    .iter()
                    .find(|existing| existing.id == profile.id)
                    .and_then(|existing| existing.approved.clone());
                profile
            })
            .collect();
        merge_into(&mut self.local, incoming, |profile| &profile.id)
    }

    /// Records what the user approved for the local profile `id`; whether it was there.
    pub fn approve_local(&mut self, id: &ProfileId, approval: LocalApproval) -> bool {
        match self.local.iter_mut().find(|profile| profile.id == *id) {
            Some(profile) => {
                profile.approved = Some(approval);
                true
            }
            None => false,
        }
    }

    /// Removes the profile `id`, of any protocol; whether it was there.
    pub fn remove(&mut self, id: &ProfileId) -> bool {
        let before = self.len();
        self.ssh.retain(|profile| profile.id != *id);
        self.rdp.retain(|profile| profile.id != *id);
        self.telnet.retain(|profile| profile.id != *id);
        self.vnc.retain(|profile| profile.id != *id);
        self.local.retain(|profile| profile.id != *id);
        self.winrm.retain(|profile| profile.id != *id);
        self.len() != before
    }

    /// The folders kept for themselves, empty ones included.
    #[must_use]
    pub fn folders(&self) -> &[String] {
        &self.folders
    }

    /// The folder of every profile, of any protocol.
    fn groups(&self) -> impl Iterator<Item = Option<&str>> {
        self.ssh
            .iter()
            .map(|p| p.group.as_deref())
            .chain(self.rdp.iter().map(|p| p.group.as_deref()))
            .chain(self.telnet.iter().map(|p| p.group.as_deref()))
            .chain(self.vnc.iter().map(|p| p.group.as_deref()))
            .chain(self.local.iter().map(|p| p.group.as_deref()))
            .chain(self.winrm.iter().map(|p| p.group.as_deref()))
    }

    /// The folder of every profile, of any protocol, to change.
    fn groups_mut(&mut self) -> impl Iterator<Item = &mut Option<String>> {
        self.ssh
            .iter_mut()
            .map(|p| &mut p.group)
            .chain(self.rdp.iter_mut().map(|p| &mut p.group))
            .chain(self.telnet.iter_mut().map(|p| &mut p.group))
            .chain(self.vnc.iter_mut().map(|p| &mut p.group))
            .chain(self.local.iter_mut().map(|p| &mut p.group))
            .chain(self.winrm.iter_mut().map(|p| &mut p.group))
    }

    /// Every folder the tree shows: those kept for themselves, those of the profiles, and
    /// every folder on the way to them; each once, normalised.
    #[must_use]
    pub fn folder_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .groups()
            .filter_map(|group| group.map(folder::normal))
            .chain(self.folders.iter().cloned())
            .filter(|path| !path.is_empty())
            .flat_map(|path| {
                let parts: Vec<String> = folder::parts(&path)
                    .into_iter()
                    .map(str::to_owned)
                    .collect();
                (1..=parts.len())
                    .map(|end| parts[..end].join("/"))
                    .collect::<Vec<_>>()
            })
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// Keeps folder `name` under `parent` for itself; its path.
    ///
    /// # Errors
    ///
    /// [`FolderError::InvalidName`] for an empty name or one with a `/`,
    /// [`FolderError::Collision`] when a folder of that name is already there.
    pub fn add_folder(&mut self, parent: &str, name: &str) -> Result<String, FolderError> {
        let path = folder::child(parent, name)?;
        if self
            .folder_paths()
            .iter()
            .any(|existing| folder::same_folder(existing, &path))
        {
            return Err(FolderError::Collision);
        }
        self.folders.push(path.clone());
        Ok(path)
    }

    /// Renames folder `path` to `name`, at the same level: its profiles and its folders
    /// follow. Its new path.
    ///
    /// # Errors
    ///
    /// As [`Self::add_folder`], and [`FolderError::Missing`] for no such folder.
    pub fn rename_folder(&mut self, path: &str, name: &str) -> Result<String, FolderError> {
        let renamed = folder::child(&folder::parent(path), name)?;
        self.relocate(path, &renamed)
    }

    /// Moves folder `path` under `parent`, the top level for an empty one: its profiles and
    /// its folders follow. Its new path.
    ///
    /// # Errors
    ///
    /// [`FolderError::IntoItself`] when `parent` is the folder or inside it,
    /// [`FolderError::Collision`] when a folder of its name is already there,
    /// [`FolderError::Missing`] for no such folder.
    pub fn move_folder(&mut self, path: &str, parent: &str) -> Result<String, FolderError> {
        if folder::is_within(parent, path) {
            return Err(FolderError::IntoItself);
        }
        let moved = folder::child(parent, &folder::name(path))?;
        self.relocate(path, &moved)
    }

    /// Gives folder `path`, and all it holds, the path `to`.
    fn relocate(&mut self, path: &str, to: &str) -> Result<String, FolderError> {
        let path = folder::normal(path);
        if path.is_empty() || !self.folder_paths().contains(&path) {
            return Err(FolderError::Missing);
        }
        if path == to {
            return Ok(path);
        }
        let taken = self.folder_paths().iter().any(|existing| {
            folder::same_folder(existing, to) && !folder::same_folder(existing, &path)
        });
        if taken {
            return Err(FolderError::Collision);
        }
        for group in self.groups_mut() {
            if let Some(current) = group
                .as_deref()
                .filter(|current| folder::is_within(current, &path))
            {
                *group = Some(folder::relabel(current, &path, to));
            }
        }
        for kept in &mut self.folders {
            if folder::is_within(kept, &path) {
                *kept = folder::relabel(kept, &path, to);
            }
        }
        Ok(to.to_owned())
    }

    /// Deletes folder `path` and the folders in it; their profiles go to no folder, as the
    /// C# Heimdall moves them. How many profiles moved.
    pub fn delete_folder(&mut self, path: &str) -> usize {
        let mut moved = 0;
        for group in self.groups_mut() {
            if group
                .as_deref()
                .is_some_and(|current| folder::is_within(current, path))
            {
                *group = None;
                moved += 1;
            }
        }
        self.folders.retain(|kept| !folder::is_within(kept, path));
        moved
    }

    /// Number of profiles, all protocols together.
    fn len(&self) -> usize {
        self.ssh.len()
            + self.rdp.len()
            + self.telnet.len()
            + self.vnc.len()
            + self.local.len()
            + self.winrm.len()
    }

    /// Applies `change` to a copy, saves the copy, and only then keeps it: a save that fails
    /// leaves the store as it was, the same as its file.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the save fails; the store is then unchanged.
    pub fn apply<T>(&mut self, change: impl FnOnce(&mut Self) -> T) -> Result<T, StoreError> {
        let mut next = self.clone();
        let result = change(&mut next);
        next.save()?;
        *self = next;
        Ok(result)
    }

    /// Writes the store to its file, creating the directory if needed.
    ///
    /// The content goes to a temporary file in the same directory, which then replaces the
    /// profile file, so an interruption never leaves a truncated file behind.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when serialisation or any file operation fails.
    pub fn save(&self) -> Result<(), StoreError> {
        let text = toml::to_string_pretty(&ProfileFile {
            version: PROFILE_FILE_VERSION,
            ssh: self.ssh.clone(),
            rdp: self.rdp.clone(),
            telnet: self.telnet.clone(),
            vnc: self.vnc.clone(),
            local: self.local.clone(),
            gateway: self.gateways.clone(),
            winrm: self.winrm.clone(),
            folder: self.folders.clone(),
        })?;
        let dir = self
            .path
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let io_error = |path: &Path| {
            let path = path.to_owned();
            move |source| StoreError::Io { path, source }
        };
        fs::create_dir_all(dir).map_err(io_error(dir))?;
        let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(io_error(dir))?;
        temp.write_all(text.as_bytes())
            .map_err(io_error(temp.path()))?;
        temp.as_file().sync_all().map_err(io_error(temp.path()))?;
        temp.persist(&self.path)
            .map_err(|error| io_error(&self.path)(error.error))?;
        Ok(())
    }
}

/// Adds or replaces `incoming` in `list` by identifier, keeping the order of `list`.
fn merge_into<T: PartialEq>(
    list: &mut Vec<T>,
    incoming: impl IntoIterator<Item = T>,
    id: impl Fn(&T) -> &ProfileId,
) -> MergeReport {
    let mut report = MergeReport::default();
    for profile in incoming {
        match list
            .iter_mut()
            .find(|existing| id(existing) == id(&profile))
        {
            Some(existing) if *existing == profile => report.unchanged += 1,
            Some(existing) => {
                *existing = profile;
                report.updated += 1;
            }
            None => {
                list.push(profile);
                report.added += 1;
            }
        }
    }
    report
}
