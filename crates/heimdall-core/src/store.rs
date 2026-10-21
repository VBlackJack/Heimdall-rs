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

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::folder::{self, FolderColor, FolderError};
use crate::folder_defaults::{DefaultsEntry, FolderDefaults, InheritedField, ResolvedDefaults};
use crate::gateway_parents::MAX_GATEWAY_CHAIN_DEPTH;
use crate::last_good_copy::{self, Document};
use crate::metadata::{Environment, MacAddress, ProfileMetadata, ProfileOrigin};
use crate::post_connect::PostConnectStep;
use crate::profile::{
    CitrixProfile, FtpProfile, LocalApproval, LocalProfile, ProfileId, RdpMode, RdpProfile,
    SshGateway, SshMode, SshProfile, TelnetProfile, ToolProfile, VncProfile, WebProfile,
    WinRmProfile,
};

/// Format version written into the profile file.
///
/// 2 added RDP profiles, 3 Telnet profiles, 4 VNC profiles, 5 local profiles, 6 SSH
/// gateways and the gateway an SSH profile goes through, 8 folders of their own, empty ones
/// included, 9 the favorites, 10 the folders' colours and a profile's own session logging,
/// 11 the profiles' metadata, 12 Citrix profiles, 13 web consoles, 14 tool entries, 15 the
/// defaults a folder gives its sessions. A build that knows an
/// older version refuses a newer file rather than reading it, dropping what it does not
/// know, and saving it back.
pub const PROFILE_FILE_VERSION: u32 = 15;

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
    ftp: Vec<FtpProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    citrix: Vec<CitrixProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    web: Vec<WebProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool: Vec<ToolProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    folder: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    favorite: Vec<ProfileId>,
    /// A folder's colour by its path, the colour by name; one not known is left out.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    folder_color: BTreeMap<String, String>,
    /// What a folder gives the sessions put in it, by its path.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    folder_default: BTreeMap<String, DefaultsEntry>,
    /// What each profile says of its server, by identifier.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    metadata: BTreeMap<String, MetadataEntry>,
}

/// A profile's metadata as the file keeps it: words, read leniently.
#[derive(Debug, Default, Serialize, Deserialize)]
struct MetadataEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    environment: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    tags: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mac_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sort_order: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tunnels_expanded: Option<bool>,
}

impl MetadataEntry {
    /// What it says that is known: an environment or an address that does not read is
    /// left out, the rest kept.
    fn read(&self) -> ProfileMetadata {
        ProfileMetadata {
            environment: self.environment.as_deref().and_then(Environment::named),
            tags: self.tags.trim().to_owned(),
            mac_address: self
                .mac_address
                .as_deref()
                .and_then(|typed| typed.parse::<MacAddress>().ok()),
            origin: self.origin.as_deref().and_then(ProfileOrigin::named),
            sort_order: self.sort_order,
            tunnels_expanded: self.tunnels_expanded,
        }
    }

    fn of(metadata: &ProfileMetadata) -> Self {
        Self {
            environment: metadata.environment.map(|e| e.name().to_owned()),
            tags: metadata.tags.clone(),
            mac_address: metadata.mac_address.map(|mac| mac.to_string()),
            origin: metadata.origin.map(|origin| origin.name().to_owned()),
            sort_order: metadata.sort_order,
            tunnels_expanded: metadata.tunnels_expanded,
        }
    }
}

/// What the profile file held when a store last read or wrote it: a content found there
/// since was written by something else, another instance or an editor.
#[derive(Clone)]
enum OnDisk {
    /// Not known, the store having been made without reading its file: whatever is there
    /// is replaced, as by a store saving beside an unreadable file.
    Unknown,
    /// There was no file.
    Absent,
    /// This text.
    Text(String),
}

impl fmt::Debug for OnDisk {
    /// Its kind and length only: the text is the whole profile file.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => f.write_str("Unknown"),
            Self::Absent => f.write_str("Absent"),
            Self::Text(text) => write!(f, "Text({} bytes)", text.len()),
        }
    }
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
    /// The file's content could not be serialised.
    #[error("could not be serialised: {0}")]
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
    /// The file was changed by something else since this store read or wrote it, and is
    /// left as it is rather than overwritten with what this store holds.
    #[error("{path}: changed outside this instance since it was read, so it was not overwritten")]
    ChangedOutside {
        /// File concerned.
        path: PathBuf,
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
    /// The gateways of the shared sources, read-only and in memory: reached by [`Self::route`]
    /// so that a shared session crosses the bastion its file names, and in no list, no
    /// export and no save.
    shared_gateways: Vec<SshGateway>,
    winrm: Vec<WinRmProfile>,
    ftp: Vec<FtpProfile>,
    citrix: Vec<CitrixProfile>,
    web: Vec<WebProfile>,
    tools: Vec<ToolProfile>,
    /// Folders kept for themselves, as the C# Heimdall's empty groups: normalised.
    folders: Vec<String>,
    /// The profiles marked as favorites, as the C# `IsFavorite`: each once.
    favorites: Vec<ProfileId>,
    /// What each profile says of its server, as the C# Metadata section; none empty.
    metadata: BTreeMap<ProfileId, ProfileMetadata>,
    /// The colours given to folders, by normalised path, as the C# group colour.
    folder_colors: BTreeMap<String, FolderColor>,
    /// What folders give the sessions put in them, by normalised path, as the C#
    /// `GroupDefaultsDto`; none empty.
    folder_defaults: BTreeMap<String, FolderDefaults>,
    /// What the file held when last read or written here, for a change made outside to be
    /// kept rather than overwritten.
    on_disk: OnDisk,
    /// The file was unreadable and its last good copy was read instead.
    recovered: bool,
}

/// What removing a gateway changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayRemoval {
    /// Servers that went through it, now connecting directly.
    pub servers: usize,
    /// Gateways reached through it, now reached directly.
    pub gateways: usize,
    /// Folders whose defaults named it, now naming none, as the C# delete clears them.
    pub folders: usize,
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
    /// The chain is deeper than [`MAX_GATEWAY_CHAIN_DEPTH`] gateways, as the C#
    /// `GatewayChainResolver` refuses it; carries the gateway that would be one too many.
    #[error("the SSH gateway {0} makes a chain deeper than {MAX_GATEWAY_CHAIN_DEPTH} gateways")]
    TooDeep(ProfileId),
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
            shared_gateways: Vec::new(),
            winrm: Vec::new(),
            ftp: Vec::new(),
            citrix: Vec::new(),
            web: Vec::new(),
            tools: Vec::new(),
            folders: Vec::new(),
            favorites: Vec::new(),
            metadata: BTreeMap::new(),
            folder_colors: BTreeMap::new(),
            folder_defaults: BTreeMap::new(),
            on_disk: OnDisk::Unknown,
            recovered: false,
        }
    }

    /// Opens the store at `path`; a missing file is an empty store. A file that is not
    /// readable is read from its last good copy when there is one, as the C# `ConfigManager`
    /// reads `servers.json`: [`Self::recovered_from_backup`] then says so, and the next save
    /// replaces the damaged file.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file exists and cannot be read, does not parse, or has
    /// another format version.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let path = path.into();
        let (text, recovered) = match last_good_copy::read_document(&path) {
            Ok(Document::Read(text)) => (text, false),
            Ok(Document::Recovered(text)) => (text, true),
            Ok(Document::Missing) => {
                return Ok(Self {
                    on_disk: OnDisk::Absent,
                    ..Self::empty(path)
                });
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
            shared_gateways: Vec::new(),
            winrm: file.winrm,
            ftp: file.ftp,
            citrix: file.citrix,
            web: file.web,
            tools: file.tool,
            folders: file
                .folder
                .iter()
                .map(|path| folder::normal(path))
                .filter(|path| !path.is_empty())
                .collect(),
            favorites: {
                let mut favorites = file.favorite;
                favorites.sort();
                favorites.dedup();
                favorites
            },
            metadata: file
                .metadata
                .iter()
                .map(|(id, entry)| (ProfileId::new(id.clone()), entry.read()))
                .filter(|(_, metadata)| !metadata.is_empty())
                .collect(),
            folder_colors: file
                .folder_color
                .iter()
                .filter_map(|(path, name)| {
                    let path = folder::normal(path);
                    let color = FolderColor::named(name)?;
                    (!path.is_empty()).then_some((path, color))
                })
                .collect(),
            folder_defaults: file
                .folder_default
                .iter()
                .filter_map(|(path, entry)| {
                    let path = folder::normal(path);
                    let defaults = entry.read();
                    (!path.is_empty() && !defaults.is_empty()).then_some((path, defaults))
                })
                .collect(),
            // The damaged file is not what the store holds: the next save replaces it.
            on_disk: if recovered {
                OnDisk::Unknown
            } else {
                OnDisk::Text(text)
            },
            recovered,
        })
    }

    /// Whether the file was unreadable when opened and its last good copy was read instead,
    /// as the C# `DocumentsRecoveredFromBackup` says it.
    #[must_use]
    pub fn recovered_from_backup(&self) -> bool {
        self.recovered
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

    /// FTP profiles, in file order.
    #[must_use]
    pub fn ftp_profiles(&self) -> &[FtpProfile] {
        &self.ftp
    }

    /// Adds or replaces FTP profiles by identifier; the order of existing ones is kept.
    pub fn merge_ftp(&mut self, incoming: impl IntoIterator<Item = FtpProfile>) -> MergeReport {
        merge_into(&mut self.ftp, incoming, |profile| &profile.id)
    }

    /// Citrix profiles, in file order.
    #[must_use]
    pub fn citrix_profiles(&self) -> &[CitrixProfile] {
        &self.citrix
    }

    /// Adds or replaces Citrix profiles by identifier; the order of existing ones is kept.
    pub fn merge_citrix(
        &mut self,
        incoming: impl IntoIterator<Item = CitrixProfile>,
    ) -> MergeReport {
        merge_into(&mut self.citrix, incoming, |profile| &profile.id)
    }

    /// Web consoles, in file order.
    #[must_use]
    pub fn web_profiles(&self) -> &[WebProfile] {
        &self.web
    }

    /// Adds or replaces web consoles by identifier; the order of existing ones is kept.
    pub fn merge_web(&mut self, incoming: impl IntoIterator<Item = WebProfile>) -> MergeReport {
        merge_into(&mut self.web, incoming, |profile| &profile.id)
    }

    /// Tool entries, in file order.
    #[must_use]
    pub fn tool_profiles(&self) -> &[ToolProfile] {
        &self.tools
    }

    /// Adds or replaces tool entries by identifier; the order of existing ones is kept.
    pub fn merge_tools(&mut self, incoming: impl IntoIterator<Item = ToolProfile>) -> MergeReport {
        merge_into(&mut self.tools, incoming, |profile| &profile.id)
    }

    /// SSH gateways, in file order.
    #[must_use]
    pub fn gateways(&self) -> &[SshGateway] {
        &self.gateways
    }

    /// Replaces the gateways of the shared sources. Only those in the shared namespace are
    /// kept, and a parent outside it is dropped, as the C# `SharedProfileSanitizer`: a shared
    /// file can neither name nor shadow a gateway of the user's own. Nothing is saved.
    pub fn set_shared_gateways(&mut self, gateways: impl IntoIterator<Item = SshGateway>) {
        self.shared_gateways = gateways
            .into_iter()
            .filter(|gateway| crate::shared_sessions::is_shared(gateway.id.as_str()))
            .map(|mut gateway| {
                if gateway
                    .parent
                    .as_ref()
                    .is_some_and(|parent| !crate::shared_sessions::is_shared(parent.as_str()))
                {
                    gateway.parent = None;
                }
                gateway
            })
            .collect();
    }

    /// The gateways to go through to reach a server whose gateway is `gateway`, nearest to
    /// this machine first: the gateway's parents from the farthest back, then the gateway.
    ///
    /// # Errors
    ///
    /// [`RouteError`] when a gateway on the way is not in the store, or comes back, or when
    /// there are more than [`MAX_GATEWAY_CHAIN_DEPTH`] of them, as the C# refuses at connect.
    pub fn route(&self, gateway: Option<&ProfileId>) -> Result<Vec<SshGateway>, RouteError> {
        let mut route = Vec::new();
        let mut next = gateway;
        while let Some(id) = next {
            if route.iter().any(|seen: &SshGateway| &seen.id == id) {
                return Err(RouteError::Loop(id.clone()));
            }
            if route.len() >= MAX_GATEWAY_CHAIN_DEPTH {
                return Err(RouteError::TooDeep(id.clone()));
            }
            // A shared identifier is never a saved gateway's: the saved ones are looked at
            // first all the same.
            let found = self
                .gateways
                .iter()
                .chain(&self.shared_gateways)
                .find(|candidate| &candidate.id == id)
                .ok_or_else(|| RouteError::MissingGateway(id.clone()))?;
            route.push(found.clone());
            next = found.parent.as_ref();
        }
        route.reverse();
        Ok(route)
    }

    /// Removes the SSH gateway `id`, as the C# delete does: the servers that went through
    /// it connect directly, and the gateways reached through it are reached directly. What
    /// it changed; `None` when there was no such gateway.
    pub fn remove_gateway(&mut self, id: &ProfileId) -> Option<GatewayRemoval> {
        if !self.gateways.iter().any(|gateway| gateway.id == *id) {
            return None;
        }
        let mut servers = 0;
        let routes = self
            .ssh
            .iter_mut()
            .map(|profile| &mut profile.gateway)
            .chain(self.rdp.iter_mut().map(|profile| &mut profile.gateway))
            .chain(self.winrm.iter_mut().map(|profile| &mut profile.gateway))
            .chain(self.web.iter_mut().map(|profile| &mut profile.gateway));
        for route in routes.filter(|route| route.as_ref() == Some(id)) {
            *route = None;
            servers += 1;
        }
        let mut gateways = 0;
        for parent in self
            .gateways
            .iter_mut()
            .map(|gateway| &mut gateway.parent)
            .filter(|parent| parent.as_ref() == Some(id))
        {
            *parent = None;
            gateways += 1;
        }
        let mut folders = 0;
        for defaults in self.folder_defaults.values_mut() {
            if defaults.gateway.as_ref() == Some(id) {
                defaults.gateway = None;
                folders += 1;
            }
        }
        self.folder_defaults
            .retain(|_, defaults| !defaults.is_empty());
        self.gateways.retain(|gateway| gateway.id != *id);
        Some(GatewayRemoval {
            servers,
            gateways,
            folders,
        })
    }

    /// Adds or replaces SSH gateways by identifier; the order of existing ones is kept.
    pub fn merge_gateways(
        &mut self,
        incoming: impl IntoIterator<Item = SshGateway>,
    ) -> MergeReport {
        merge_into(&mut self.gateways, incoming, |gateway| &gateway.id)
    }

    /// Adds or replaces SSH profiles by identifier; the order of existing profiles is kept.
    ///
    /// A profile already in the store keeps the approval of its post-connect steps: an
    /// approval names what it approved, so it still holds only if the incoming steps are the
    /// same. An approval is never taken from the incoming profile.
    pub fn merge(&mut self, incoming: impl IntoIterator<Item = SshProfile>) -> MergeReport {
        let incoming: Vec<SshProfile> = incoming
            .into_iter()
            .map(|mut profile| {
                profile.post_connect.approved = self
                    .ssh
                    .iter()
                    .find(|existing| existing.id == profile.id)
                    .and_then(|existing| existing.post_connect.approved.clone());
                profile
            })
            .collect();
        merge_into(&mut self.ssh, incoming, |profile| &profile.id)
    }

    /// Records that the user approved `steps` as the post-connect steps of the SSH profile
    /// `id`; whether it was there. Steps changed since the user saw them are not approved by
    /// this: the approval names the steps shown.
    pub fn approve_post_connect(&mut self, id: &ProfileId, steps: &[PostConnectStep]) -> bool {
        match self.ssh.iter_mut().find(|profile| profile.id == *id) {
            Some(profile) => {
                // What each step does is approved; a step that arrived from outside keeps
                // its mark until the user says yes to it (`trust_post_connect`).
                profile.post_connect.approved = Some(
                    steps
                        .iter()
                        .map(|step| PostConnectStep {
                            untrusted: false,
                            ..step.clone()
                        })
                        .collect(),
                );
                true
            }
            None => false,
        }
    }

    /// Records the user's yes to the steps `steps` of profile `id`, shown to them as they
    /// are: approved, and none of them waits for a Run any more.
    pub fn trust_post_connect(&mut self, id: &ProfileId, steps: &[PostConnectStep]) -> bool {
        match self.ssh.iter_mut().find(|profile| profile.id == *id) {
            Some(profile) => {
                profile.post_connect =
                    crate::post_connect::PostConnect::approved_as(steps.to_vec());
                true
            }
            None => false,
        }
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

    /// Whether profile `id` is marked as a favorite.
    #[must_use]
    pub fn is_favorite(&self, id: &ProfileId) -> bool {
        self.favorites.binary_search(id).is_ok()
    }

    /// Marks profile `id` as a favorite, or no longer, as the C# tree menu and server form
    /// do; whether that changed anything. A profile not in the store is not marked.
    pub fn set_favorite(&mut self, id: &ProfileId, favorite: bool) -> bool {
        match (self.favorites.binary_search(id), favorite) {
            (Err(at), true) if self.holds(id) => {
                self.favorites.insert(at, id.clone());
                true
            }
            (Ok(at), false) => {
                self.favorites.remove(at);
                true
            }
            _ => false,
        }
    }

    /// What profile `id` says of its server; nothing for one that says nothing.
    #[must_use]
    pub fn metadata(&self, id: &ProfileId) -> Option<&ProfileMetadata> {
        self.metadata.get(id)
    }

    /// Keeps `metadata` for profile `id`, taken away when it says nothing; a profile not in
    /// the store keeps none. Whether it is kept.
    pub fn set_metadata(&mut self, id: &ProfileId, metadata: ProfileMetadata) -> bool {
        if metadata.is_empty() || !self.holds(id) {
            self.metadata.remove(id);
            return false;
        }
        self.metadata.insert(id.clone(), metadata);
        true
    }

    /// Marks profile `id` as come from `origin`, what else it says kept; whether it is kept.
    pub fn set_origin(&mut self, id: &ProfileId, origin: ProfileOrigin) -> bool {
        let metadata = ProfileMetadata {
            origin: Some(origin),
            ..self.metadata.get(id).cloned().unwrap_or_default()
        };
        self.set_metadata(id, metadata)
    }

    /// Whether a profile of any protocol is `id`.
    fn holds(&self, id: &ProfileId) -> bool {
        self.ssh.iter().any(|profile| profile.id == *id)
            || self.rdp.iter().any(|profile| profile.id == *id)
            || self.telnet.iter().any(|profile| profile.id == *id)
            || self.vnc.iter().any(|profile| profile.id == *id)
            || self.local.iter().any(|profile| profile.id == *id)
            || self.winrm.iter().any(|profile| profile.id == *id)
            || self.ftp.iter().any(|profile| profile.id == *id)
            || self.citrix.iter().any(|profile| profile.id == *id)
            || self.web.iter().any(|profile| profile.id == *id)
            || self.tools.iter().any(|profile| profile.id == *id)
    }

    /// Removes the profile `id`, of any protocol; whether it was there. A favorite no
    /// longer.
    pub fn remove(&mut self, id: &ProfileId) -> bool {
        self.set_favorite(id, false);
        self.metadata.remove(id);
        let before = self.len();
        self.ssh.retain(|profile| profile.id != *id);
        self.rdp.retain(|profile| profile.id != *id);
        self.telnet.retain(|profile| profile.id != *id);
        self.vnc.retain(|profile| profile.id != *id);
        self.local.retain(|profile| profile.id != *id);
        self.winrm.retain(|profile| profile.id != *id);
        self.ftp.retain(|profile| profile.id != *id);
        self.citrix.retain(|profile| profile.id != *id);
        self.web.retain(|profile| profile.id != *id);
        self.tools.retain(|profile| profile.id != *id);
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
            .chain(self.ftp.iter().map(|p| p.group.as_deref()))
            .chain(self.citrix.iter().map(|p| p.group.as_deref()))
            .chain(self.web.iter().map(|p| p.group.as_deref()))
            .chain(self.tools.iter().map(|p| p.group.as_deref()))
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
            .chain(self.ftp.iter_mut().map(|p| &mut p.group))
            .chain(self.citrix.iter_mut().map(|p| &mut p.group))
            .chain(self.web.iter_mut().map(|p| &mut p.group))
            .chain(self.tools.iter_mut().map(|p| &mut p.group))
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
        // Their colours follow them.
        let colors = std::mem::take(&mut self.folder_colors);
        self.folder_colors = colors
            .into_iter()
            .map(|(colored, color)| {
                if folder::is_within(&colored, &path) {
                    (folder::relabel(&colored, &path, to), color)
                } else {
                    (colored, color)
                }
            })
            .collect();
        // So do their defaults, as `FolderRenameService` moves the group defaults.
        let defaults = std::mem::take(&mut self.folder_defaults);
        self.folder_defaults = defaults
            .into_iter()
            .map(|(named, set)| {
                if folder::is_within(&named, &path) {
                    (folder::relabel(&named, &path, to), set)
                } else {
                    (named, set)
                }
            })
            .collect();
        Ok(to.to_owned())
    }

    /// Puts profile `id`, of any protocol, in folder `group`, none for `None`; whether it is
    /// saved.
    pub fn set_group(&mut self, id: &ProfileId, group: Option<String>) -> bool {
        let group = group
            .map(|path| folder::normal(&path))
            .filter(|path| !path.is_empty());
        self.profile_fields_mut(id)
            .map(|(_, current)| *current = group)
            .is_some()
    }

    /// Moves profile `id` into folder `group` as [`Self::set_group`] does and, when that
    /// changes its folder, fills what it leaves unset from the folder's defaults
    /// ([`Self::apply_folder_defaults`]); what it filled. `None` when the profile is not
    /// there, an empty list when nothing was filled.
    pub fn move_into_folder(
        &mut self,
        id: &ProfileId,
        group: Option<String>,
    ) -> Option<Vec<InheritedField>> {
        let target = group
            .as_deref()
            .map(folder::normal)
            .filter(|path| !path.is_empty());
        let (_, current) = self.profile_fields_mut(id)?;
        let unchanged = current.as_deref().map(folder::normal) == target;
        self.set_group(id, group);
        Some(if unchanged {
            Vec::new()
        } else {
            self.apply_folder_defaults(id)
        })
    }

    /// Sets the port profile `id` connects to, as the C# bulk edit does; whether it changed.
    /// A local shell has none.
    pub fn set_port(&mut self, id: &ProfileId, port: u16) -> bool {
        match self.port_mut(id) {
            Some(current) if *current != port => {
                *current = port;
                true
            }
            _ => false,
        }
    }

    /// Sets the account profile `id` logs in with, as the C# bulk edit does for SSH, SFTP,
    /// RDP, FTP and `WinRM`; whether it changed. The other protocols name none.
    pub fn set_username(&mut self, id: &ProfileId, username: &str) -> bool {
        let username = Some(username.to_owned());
        match self.username_mut(id) {
            Some(current) if *current != username => {
                *current = username;
                true
            }
            _ => false,
        }
    }

    /// Sets the gateway profile `id` goes through, none for a direct connection, as the C#
    /// bulk "Set gateway" does for SSH, SFTP, RDP, `WinRM` and web consoles; whether it
    /// changed. The other protocols go through none, and a gateway not in the store is not
    /// set.
    pub fn set_gateway(&mut self, id: &ProfileId, gateway: Option<&ProfileId>) -> bool {
        if gateway.is_some_and(|gateway| !self.gateways.iter().any(|known| known.id == *gateway)) {
            return false;
        }
        let gateway = gateway.cloned();
        let current = if let Some(profile) = self.ssh.iter_mut().find(|p| p.id == *id) {
            &mut profile.gateway
        } else if let Some(profile) = self.rdp.iter_mut().find(|p| p.id == *id) {
            &mut profile.gateway
        } else if let Some(profile) = self.winrm.iter_mut().find(|p| p.id == *id) {
            &mut profile.gateway
        } else if let Some(profile) = self.web.iter_mut().find(|p| p.id == *id) {
            &mut profile.gateway
        } else {
            return false;
        };
        if *current == gateway {
            return false;
        }
        *current = gateway;
        true
    }

    /// The SSH profiles that are not in `mode`, and the SSH profiles there are, as the C#
    /// "Apply to all" counts them: shells only, an SFTP profile always opening its files in
    /// a tab.
    #[must_use]
    pub fn ssh_mode_changes(&self, mode: SshMode) -> (usize, usize) {
        let shells = self.ssh.iter().filter(|profile| !profile.sftp);
        let total = shells.clone().count();
        let changes = shells.filter(|profile| profile.ssh_mode != mode).count();
        (changes, total)
    }

    /// Sets `mode` on every SSH profile, as the C# "Apply to all" rewrites every saved SSH
    /// session; SFTP profiles are left as they are. How many changed.
    pub fn set_ssh_modes(&mut self, mode: SshMode) -> usize {
        let mut changed = 0;
        for profile in &mut self.ssh {
            if !profile.sftp && profile.ssh_mode != mode {
                profile.ssh_mode = mode;
                changed += 1;
            }
        }
        changed
    }

    /// The RDP profiles that are not in `mode`, and the RDP profiles there are, as the C#
    /// "Apply to all" counts them (`SettingsViewModel.cs:1671-1675`): every saved RDP
    /// session.
    #[must_use]
    pub fn rdp_mode_changes(&self, mode: RdpMode) -> (usize, usize) {
        let changes = self
            .rdp
            .iter()
            .filter(|profile| RdpMode::of(profile.extras.external) != mode)
            .count();
        (changes, self.rdp.len())
    }

    /// Sets `mode` on every RDP profile, as the C# "Apply to all" rewrites every saved RDP
    /// session (`SettingsViewModel.cs:1690-1708`). How many changed.
    pub fn set_rdp_modes(&mut self, mode: RdpMode) -> usize {
        let mut changed = 0;
        for profile in &mut self.rdp {
            if RdpMode::of(profile.extras.external) != mode {
                profile.extras.external = mode.is_external();
                changed += 1;
            }
        }
        changed
    }

    fn port_mut(&mut self, id: &ProfileId) -> Option<&mut u16> {
        if let Some(profile) = self.ssh.iter_mut().find(|profile| profile.id == *id) {
            return Some(&mut profile.port);
        }
        if let Some(profile) = self.rdp.iter_mut().find(|profile| profile.id == *id) {
            return Some(&mut profile.port);
        }
        if let Some(profile) = self.telnet.iter_mut().find(|profile| profile.id == *id) {
            return Some(&mut profile.port);
        }
        if let Some(profile) = self.vnc.iter_mut().find(|profile| profile.id == *id) {
            return Some(&mut profile.port);
        }
        if let Some(profile) = self.winrm.iter_mut().find(|profile| profile.id == *id) {
            return Some(&mut profile.port);
        }
        if let Some(profile) = self.web.iter_mut().find(|profile| profile.id == *id) {
            return Some(&mut profile.port);
        }
        self.ftp
            .iter_mut()
            .find(|profile| profile.id == *id)
            .map(|profile| &mut profile.port)
    }

    fn username_mut(&mut self, id: &ProfileId) -> Option<&mut Option<String>> {
        if let Some(profile) = self.ssh.iter_mut().find(|profile| profile.id == *id) {
            return Some(&mut profile.username);
        }
        if let Some(profile) = self.rdp.iter_mut().find(|profile| profile.id == *id) {
            return Some(&mut profile.username);
        }
        if let Some(profile) = self.winrm.iter_mut().find(|profile| profile.id == *id) {
            return Some(&mut profile.username);
        }
        self.ftp
            .iter_mut()
            .find(|profile| profile.id == *id)
            .map(|profile| &mut profile.username)
    }

    /// Names profile `id`, of any protocol, `name`, trimmed; whether it is saved and the
    /// name is not empty. A profile the password manager knows by its name keeps being
    /// found: its old name becomes its vault entry name, as the C# rename freezes it.
    pub fn rename_profile(&mut self, id: &ProfileId, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let Some((current, _)) = self.profile_fields_mut(id) else {
            return false;
        };
        let old = std::mem::replace(current, name.to_owned());
        if old != name {
            self.freeze_vault_entry(id, &old);
        }
        true
    }

    /// Makes `old_name` the vault entry name of profile `id` when it has none: the password
    /// manager's entry keeps the name the profile had.
    pub fn freeze_vault_entry(&mut self, id: &ProfileId, old_name: &str) {
        let entry = if let Some(profile) = self.ssh.iter_mut().find(|p| p.id == *id) {
            &mut profile.vault_entry
        } else if let Some(profile) = self.rdp.iter_mut().find(|p| p.id == *id) {
            &mut profile.vault_entry
        } else if let Some(profile) = self.vnc.iter_mut().find(|p| p.id == *id) {
            &mut profile.vault_entry
        } else {
            return;
        };
        if entry.as_deref().is_none_or(|entry| entry.trim().is_empty()) {
            *entry = Some(old_name.to_owned());
        }
    }

    /// The name and the folder of profile `id`, of any protocol, to change.
    fn profile_fields_mut(&mut self, id: &ProfileId) -> Option<(&mut String, &mut Option<String>)> {
        macro_rules! find {
            ($list:expr) => {
                if let Some(profile) = $list.iter_mut().find(|profile| profile.id == *id) {
                    return Some((&mut profile.name, &mut profile.group));
                }
            };
        }
        find!(self.ssh);
        find!(self.rdp);
        find!(self.telnet);
        find!(self.vnc);
        find!(self.local);
        find!(self.winrm);
        find!(self.ftp);
        find!(self.citrix);
        find!(self.web);
        find!(self.tools);
        None
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
        self.folder_colors
            .retain(|colored, _| !folder::is_within(colored, path));
        // And their defaults, as `FolderDeletionService` removes the group defaults.
        self.folder_defaults
            .retain(|named, _| !folder::is_within(named, path));
        moved
    }

    /// The colour folder `path` was given itself; `None` when it takes its parent's, or none.
    #[must_use]
    pub fn own_folder_color(&self, path: &str) -> Option<FolderColor> {
        self.folder_colors.get(&folder::normal(path)).copied()
    }

    /// The colour folder `path` is shown in: its own, else the nearest one of the folders it
    /// is in, as the C# folders inherit theirs.
    #[must_use]
    pub fn folder_color(&self, path: &str) -> Option<FolderColor> {
        let mut path = folder::normal(path);
        while !path.is_empty() {
            if let Some(color) = self.folder_colors.get(&path) {
                return Some(*color);
            }
            path = folder::parent(&path);
        }
        None
    }

    /// Gives folder `path` `color`, or takes its own away for `None`, so it shows its
    /// parent's again.
    ///
    /// # Errors
    ///
    /// [`FolderError::Missing`] for no such folder.
    pub fn set_folder_color(
        &mut self,
        path: &str,
        color: Option<FolderColor>,
    ) -> Result<(), FolderError> {
        let path = folder::normal(path);
        if path.is_empty() || !self.folder_paths().contains(&path) {
            return Err(FolderError::Missing);
        }
        match color {
            Some(color) => self.folder_colors.insert(path, color),
            None => self.folder_colors.remove(&path),
        };
        Ok(())
    }

    /// What folder `path` gives itself; `None` when it sets nothing.
    #[must_use]
    pub fn own_folder_defaults(&self, path: &str) -> Option<&FolderDefaults> {
        self.folder_defaults.get(&folder::normal(path))
    }

    /// Every folder that sets defaults and what it sets, in path order.
    pub fn folder_defaults(&self) -> impl Iterator<Item = (&str, &FolderDefaults)> {
        self.folder_defaults
            .iter()
            .map(|(path, defaults)| (path.as_str(), defaults))
    }

    /// The defaults in force for folder `path`, each from the nearest folder that sets it,
    /// as the C# `GroupDefaultsDto.Resolve`.
    #[must_use]
    pub fn resolve_folder_defaults(&self, path: &str) -> ResolvedDefaults {
        ResolvedDefaults::resolve(path, |named| self.folder_defaults.get(named))
    }

    /// Gives folder `path` `defaults`, or takes them away when they set nothing.
    ///
    /// # Errors
    ///
    /// [`FolderError::Missing`] for no such folder.
    pub fn set_folder_defaults(
        &mut self,
        path: &str,
        defaults: FolderDefaults,
    ) -> Result<(), FolderError> {
        let path = folder::normal(path);
        if path.is_empty() || !self.folder_paths().contains(&path) {
            return Err(FolderError::Missing);
        }
        if defaults.is_empty() {
            self.folder_defaults.remove(&path);
        } else {
            self.folder_defaults.insert(path, defaults);
        }
        Ok(())
    }

    /// The folders whose own defaults send their sessions through gateway `id`, in path
    /// order, as the C# `GatewayReferenceAnalyzer` lists them.
    #[must_use]
    pub fn folders_using_gateway(&self, id: &ProfileId) -> Vec<String> {
        self.folder_defaults
            .iter()
            .filter(|(_, defaults)| defaults.gateway.as_ref() == Some(id))
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// Fills what profile `id` leaves unset from the folder it is in, each field from the
    /// nearest folder that sets it, as the C# `GroupDefaultsDto.ApplyTo`; what it filled.
    ///
    /// Unset is what the C# calls unset where the profile can tell: no account, no key, no
    /// gateway, no environment. The port is a plain number, so it is never touched here
    /// (the editor fills it while it was never edited). A profile in no folder, and one not
    /// in the store (a shared session is never in it), get nothing.
    pub fn apply_folder_defaults(&mut self, id: &ProfileId) -> Vec<InheritedField> {
        let Some(group) = self
            .profile_fields_mut(id)
            .and_then(|(_, group)| group.clone())
        else {
            return Vec::new();
        };
        let resolved = self.resolve_folder_defaults(&group);
        if resolved.is_empty() {
            return Vec::new();
        }
        let mut filled = Vec::new();
        if let Some(profile) = self.ssh.iter_mut().find(|p| p.id == *id) {
            if profile.username.as_deref().is_none_or(str::is_empty)
                && let Some(inherited) = &resolved.username
            {
                profile.username = Some(inherited.value.clone());
                filled.push(InheritedField::Username);
            }
            if profile
                .key_path
                .as_ref()
                .is_none_or(|path| path.as_os_str().is_empty())
                && let Some(inherited) = &resolved.key_path
            {
                profile.key_path = Some(inherited.value.clone());
                filled.push(InheritedField::KeyPath);
            }
        }
        if let Some(inherited) = &resolved.gateway
            && self.gateways.iter().any(|g| g.id == inherited.value)
            && let Some(current) = self.gateway_slot_mut(id)
            && current.is_none()
        {
            *current = Some(inherited.value.clone());
            filled.push(InheritedField::Gateway);
        }
        if let Some(inherited) = &resolved.environment
            && self.metadata.get(id).and_then(|m| m.environment).is_none()
        {
            let metadata = ProfileMetadata {
                environment: Some(inherited.value),
                ..self.metadata.get(id).cloned().unwrap_or_default()
            };
            self.set_metadata(id, metadata);
            filled.push(InheritedField::Environment);
        }
        filled
    }

    /// The gateway of profile `id`, for the protocols that go through one.
    fn gateway_slot_mut(&mut self, id: &ProfileId) -> Option<&mut Option<ProfileId>> {
        if let Some(profile) = self.ssh.iter_mut().find(|p| p.id == *id) {
            Some(&mut profile.gateway)
        } else if let Some(profile) = self.rdp.iter_mut().find(|p| p.id == *id) {
            Some(&mut profile.gateway)
        } else if let Some(profile) = self.winrm.iter_mut().find(|p| p.id == *id) {
            Some(&mut profile.gateway)
        } else {
            self.web
                .iter_mut()
                .find(|p| p.id == *id)
                .map(|profile| &mut profile.gateway)
        }
    }

    /// Number of profiles, all protocols together.
    fn len(&self) -> usize {
        self.ssh.len()
            + self.rdp.len()
            + self.telnet.len()
            + self.vnc.len()
            + self.local.len()
            + self.winrm.len()
            + self.ftp.len()
            + self.citrix.len()
            + self.web.len()
            + self.tools.len()
    }

    /// Applies `change` to a copy, saves the copy, and only then keeps it: a save that fails
    /// leaves the store as it was, the same as its file. A file changed by something else
    /// since this store read or wrote it is not overwritten: another instance on the same
    /// file, the single-instance guard notwithstanding, or an editor, would otherwise lose
    /// its changes without a word.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the save fails, [`StoreError::ChangedOutside`] when the
    /// file changed; the store is then unchanged.
    pub fn apply<T>(&mut self, change: impl FnOnce(&mut Self) -> T) -> Result<T, StoreError> {
        let mut next = self.clone();
        let result = change(&mut next);
        self.check_unchanged()?;
        next.on_disk = OnDisk::Text(next.write()?);
        *self = next;
        Ok(result)
    }

    /// Fails when the file holds something other than what this store last read or wrote:
    /// a file created since it was found missing, or one whose content changed. A file
    /// removed since, or one that cannot be read now, is written as before: nothing of it is
    /// lost, or the write tells what is wrong.
    fn check_unchanged(&self) -> Result<(), StoreError> {
        let Ok(now) = fs::read_to_string(&self.path) else {
            return Ok(());
        };
        let changed = match &self.on_disk {
            OnDisk::Unknown => false,
            OnDisk::Absent => true,
            OnDisk::Text(known) => *known != now,
        };
        if changed {
            return Err(StoreError::ChangedOutside {
                path: self.path.clone(),
            });
        }
        Ok(())
    }

    /// Writes the store to its file, creating the directory if needed, whatever the file
    /// holds now: [`Self::apply`] is what looks for a change made outside.
    ///
    /// The content goes to a temporary file in the same directory, which then replaces the
    /// profile file, so an interruption never leaves a truncated file behind.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when serialisation or any file operation fails.
    pub fn save(&self) -> Result<(), StoreError> {
        self.write().map(drop)
    }

    /// Writes the store to its file as [`Self::save`] does, and returns the text written.
    fn write(&self) -> Result<String, StoreError> {
        let text = toml::to_string_pretty(&ProfileFile {
            version: PROFILE_FILE_VERSION,
            ssh: self.ssh.clone(),
            rdp: self.rdp.clone(),
            telnet: self.telnet.clone(),
            vnc: self.vnc.clone(),
            local: self.local.clone(),
            gateway: self.gateways.clone(),
            winrm: self.winrm.clone(),
            ftp: self.ftp.clone(),
            citrix: self.citrix.clone(),
            web: self.web.clone(),
            tool: self.tools.clone(),
            folder: self.folders.clone(),
            favorite: self.favorites.clone(),
            metadata: self
                .metadata
                .iter()
                .map(|(id, metadata)| (id.as_str().to_owned(), MetadataEntry::of(metadata)))
                .collect(),
            folder_color: self
                .folder_colors
                .iter()
                .map(|(path, color)| (path.clone(), color.name().to_owned()))
                .collect(),
            folder_default: self
                .folder_defaults
                .iter()
                .map(|(path, defaults)| (path.clone(), DefaultsEntry::of(defaults)))
                .collect(),
        })?;
        last_good_copy::write_keeping_copy(&self.path, &text)?;
        Ok(text)
    }
}

/// Writes `text` to `path` through a temporary file beside it, so an interruption never
/// leaves a truncated file behind.
pub(crate) fn write_atomic(path: &Path, text: &str) -> Result<(), StoreError> {
    replace(path, text.as_bytes(), None).map_err(|source| StoreError::Io {
        path: path.to_owned(),
        source,
    })
}

/// Writes `bytes` to `path` through a temporary file beside it, synced, then renamed over
/// the file: an interruption leaves the old file or the new one, never a truncated one. The
/// folder is made when missing; the file written is private to its owner.
///
/// # Errors
///
/// The folder, the temporary file or the rename fails.
pub fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    replace(path, bytes, None)
}

/// [`replace_file`] for a file the user picked: written where a link at `path` leads, its
/// permissions (its mode, on Unix) kept, and a read-only one refused, as writing over it in
/// place would; only the truncation is gone.
///
/// # Errors
///
/// The file is read-only, or the folder, the temporary file or the rename fails.
pub fn replace_user_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)?,
        _ => path.to_owned(),
    };
    let permissions = match fs::metadata(&target) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if permissions.as_ref().is_some_and(fs::Permissions::readonly) {
        return Err(io::Error::from(io::ErrorKind::PermissionDenied));
    }
    replace(&target, bytes, permissions)
}

/// The write of [`replace_file`], the temporary file given `permissions` before the rename.
fn replace(path: &Path, bytes: &[u8], permissions: Option<fs::Permissions>) -> io::Result<()> {
    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    temp.write_all(bytes)?;
    // Before the sync, so the mode change is on disk with the content.
    if let Some(permissions) = permissions {
        temp.as_file().set_permissions(permissions)?;
    }
    temp.as_file().sync_all()?;
    rename_over(temp, path)
}

/// Renames the written and synced `temp` over `target`, the temporary file removed when the
/// rename fails.
///
/// Through [`fs::rename`], not `NamedTempFile::persist`: on Windows persist is a bare
/// `MoveFileExW`, which refuses to replace a file any other program holds open (an editor, an
/// antivirus, a sync client, a second Heimdall), while std retries a refused move as a rename
/// with POSIX semantics (`FileRenameInfoEx`), which replaces a file opened with delete
/// sharing, the reader keeping the content it opened. The C# save does the same
/// (`PosixFileReplace.TryReplace`, used by `SecureFileWriter.WriteAllTextAtomicAsync`). On
/// Unix both are the same `rename(2)`, followed here by a sync of the folder so the new name
/// survives a power cut.
///
/// # Errors
///
/// The temporary file cannot be kept (made a plain file, on Windows) or the rename fails.
pub(crate) fn rename_over(temp: tempfile::NamedTempFile, target: &Path) -> io::Result<()> {
    // The handle stays open until the rename is done, as persist kept it: closed earlier, an
    // antivirus scanning the file on close could hold it without delete sharing and fail the
    // move. Opened with every sharing flag, it never blocks the rename itself.
    let (file, path) = temp.into_parts();
    // Kept, the path is no longer removed on drop: the rename below moves it away, and the
    // failure path removes it by hand. A failed keep drops it, which removes it.
    let kept = path.keep().map_err(|error| error.error)?;
    let renamed = fs::rename(&kept, target);
    drop(file);
    if renamed.is_err() {
        // Best effort: the rename's error is the one the caller needs.
        let _ = fs::remove_file(&kept);
    }
    #[cfg(unix)]
    if renamed.is_ok() {
        // Best effort too: the file is already replaced, a folder that cannot be synced (some
        // network file systems) must not report the save as failed.
        let dir = target
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let _ = fs::File::open(dir).and_then(|dir| dir.sync_all());
    }
    renamed
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
