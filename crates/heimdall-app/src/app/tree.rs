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

//! The profile tree, as the C# Heimdall's: a click selects, a double click connects, and a
//! right click offers the profile's menu (connect, edit, duplicate, copy, delete), whatever
//! its protocol.

use std::collections::HashSet;

use heimdall_core::profile::{ProfileId, display_address};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use super::gateways::is_missing;
use super::{App, Dialog, Effect, Message};
use crate::driver::Purpose;
use crate::profile_draft::new_id;
use crate::text::server_text;

/// Port an `ssh` command line leaves out.
const DEFAULT_SSH_PORT: u16 = 22;

/// What a profile connects with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileKind {
    /// SSH shell, and SFTP for its files.
    Ssh,
    /// SFTP: an SSH profile that opens its files.
    Sftp,
    /// Remote desktop.
    Rdp,
    /// Telnet terminal.
    Telnet,
    /// VNC desktop.
    Vnc,
    /// FTP, plain or explicit FTPS: a Files tab.
    Ftp,
    /// A program on this computer.
    Local,
    /// Remote `PowerShell`.
    WinRm,
}

impl ProfileKind {
    /// Every protocol, in the order the C# filter lists them.
    pub const ALL: [Self; 8] = [
        Self::Rdp,
        Self::Ssh,
        Self::WinRm,
        Self::Sftp,
        Self::Vnc,
        Self::Telnet,
        Self::Ftp,
        Self::Local,
    ];

    /// The protocol's name, as the tree shows it and its search finds it.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Ssh => "SSH",
            Self::Sftp => "SFTP",
            Self::Rdp => "RDP",
            Self::Telnet => "Telnet",
            Self::Vnc => "VNC",
            Self::Ftp => "FTP",
            Self::Local => "Local",
            Self::WinRm => "WinRM",
        }
    }
}

/// What the tree shows and copies of a profile, whatever its protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSummary {
    /// Identifier.
    pub id: ProfileId,
    /// Name.
    pub name: String,
    /// Folder.
    pub group: Option<String>,
    /// Protocol.
    pub kind: ProfileKind,
    /// Host and port, for the protocols that reach a server.
    pub endpoint: Option<(String, u16)>,
    /// Account.
    pub username: Option<String>,
    /// The gateway it goes through, when it does.
    pub gateway: Option<GatewayBadge>,
    /// Marked as a favorite, as the C# tree's star.
    pub favorite: bool,
    /// What it says of its server: its environment, tags and MAC address.
    pub metadata: heimdall_core::metadata::ProfileMetadata,
}

impl ProfileSummary {
    /// Whether the tree's search `term` finds this profile, as the C# Heimdall's does: every
    /// word of it, each in its name, host, folder, account or protocol, so that "web prod"
    /// finds web01 filed in Prod; whatever the case and the accents, so that "reseau" finds
    /// "Réseau" and "Réseau" finds "reseau". A word is never matched across two fields. An
    /// empty term finds every profile.
    #[must_use]
    pub fn matches(&self, term: &str) -> bool {
        let host = self.endpoint.as_ref().map(|(host, _)| host.as_str());
        let fields: Vec<String> = [
            Some(self.name.as_str()),
            host,
            self.group.as_deref(),
            self.username.as_deref(),
            Some(self.kind.label()),
            // As the C# search: the environment and the tags too.
            self.metadata
                .environment
                .map(heimdall_core::metadata::Environment::name),
            Some(self.metadata.tags.as_str()).filter(|tags| !tags.is_empty()),
        ]
        .into_iter()
        .flatten()
        .map(folded)
        .collect();
        folded(term)
            .split_whitespace()
            .all(|word| fields.iter().any(|field| field.contains(word)))
    }
}

/// `text` as the search compares it, as the C# `NormalizeSearchTerm`: trimmed, each
/// character decomposed and its combining marks dropped, so that an accented and a plain
/// spelling meet on the plain one, then upper-cased.
fn folded(text: &str) -> String {
    text.trim()
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .nfc()
        .collect::<String>()
        .to_uppercase()
}

/// How a session reaches its server, as the C# tree's badge says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayBadge {
    /// Through this gateway, by name.
    Via(String),
    /// Through a gateway that is not saved: it cannot connect.
    Missing,
}

/// What of a profile the menu copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileCopy {
    /// The host name.
    Hostname,
    /// The account.
    Username,
    /// `host:port`.
    Address,
    /// An `ssh` command line reaching it.
    SshCommand,
}

impl App {
    /// Every saved profile, whatever its protocol, in no particular order.
    #[must_use]
    pub fn profile_summaries(&self) -> Vec<ProfileSummary> {
        let mut all = Vec::new();
        for profile in self.store.ssh_profiles() {
            all.push(ProfileSummary {
                id: profile.id.clone(),
                favorite: self.store.is_favorite(&profile.id),
                metadata: self.metadata_of(&profile.id),
                name: profile.name.clone(),
                group: profile.group.clone(),
                kind: if profile.sftp {
                    ProfileKind::Sftp
                } else {
                    ProfileKind::Ssh
                },
                endpoint: Some((profile.host.clone(), profile.port)),
                username: profile.username.clone(),
                gateway: self.badge(profile.gateway.as_ref()),
            });
        }
        for profile in self.store.rdp_profiles() {
            all.push(ProfileSummary {
                id: profile.id.clone(),
                favorite: self.store.is_favorite(&profile.id),
                metadata: self.metadata_of(&profile.id),
                name: profile.name.clone(),
                group: profile.group.clone(),
                kind: ProfileKind::Rdp,
                endpoint: Some((profile.host.clone(), profile.port)),
                username: profile.username.clone(),
                gateway: self.badge(profile.gateway.as_ref()),
            });
        }
        for profile in self.store.telnet_profiles() {
            all.push(ProfileSummary {
                id: profile.id.clone(),
                favorite: self.store.is_favorite(&profile.id),
                metadata: self.metadata_of(&profile.id),
                name: profile.name.clone(),
                group: profile.group.clone(),
                kind: ProfileKind::Telnet,
                endpoint: Some((profile.host.clone(), profile.port)),
                username: None,
                gateway: None,
            });
        }
        for profile in self.store.vnc_profiles() {
            all.push(ProfileSummary {
                id: profile.id.clone(),
                favorite: self.store.is_favorite(&profile.id),
                metadata: self.metadata_of(&profile.id),
                name: profile.name.clone(),
                group: profile.group.clone(),
                kind: ProfileKind::Vnc,
                endpoint: Some((profile.host.clone(), profile.port)),
                username: None,
                gateway: None,
            });
        }
        for profile in self.store.ftp_profiles() {
            all.push(ProfileSummary {
                id: profile.id.clone(),
                favorite: self.store.is_favorite(&profile.id),
                metadata: self.metadata_of(&profile.id),
                name: profile.name.clone(),
                group: profile.group.clone(),
                kind: ProfileKind::Ftp,
                endpoint: Some((profile.host.clone(), profile.port)),
                username: profile.username.clone(),
                gateway: None,
            });
        }
        for profile in self.store.local_profiles() {
            all.push(ProfileSummary {
                id: profile.id.clone(),
                favorite: self.store.is_favorite(&profile.id),
                metadata: self.metadata_of(&profile.id),
                name: profile.name.clone(),
                group: profile.group.clone(),
                kind: ProfileKind::Local,
                endpoint: None,
                username: None,
                gateway: None,
            });
        }
        for profile in self.store.winrm_profiles() {
            all.push(ProfileSummary {
                id: profile.id.clone(),
                favorite: self.store.is_favorite(&profile.id),
                metadata: self.metadata_of(&profile.id),
                name: profile.name.clone(),
                group: profile.group.clone(),
                kind: ProfileKind::WinRm,
                endpoint: Some((profile.host.clone(), profile.port)),
                username: profile.username.clone(),
                gateway: self.badge(profile.gateway.as_ref()),
            });
        }
        all
    }

    /// Whether profile `id` is marked as a favorite.
    #[must_use]
    pub fn is_favorite(&self, id: &ProfileId) -> bool {
        self.store.is_favorite(id)
    }

    /// The badge of a session routed through `gateway`.
    fn badge(&self, gateway: Option<&ProfileId>) -> Option<GatewayBadge> {
        let id = gateway?;
        if is_missing(self.gateways(), Some(id)) {
            return Some(GatewayBadge::Missing);
        }
        self.gateways()
            .iter()
            .find(|known| known.id == *id)
            .map(|known| GatewayBadge::Via(known.name.clone()))
    }

    /// What profile `id` says of its server; empty when it says nothing.
    fn metadata_of(&self, id: &ProfileId) -> heimdall_core::metadata::ProfileMetadata {
        self.store.metadata(id).cloned().unwrap_or_default()
    }

    /// One profile, whatever its protocol.
    #[must_use]
    pub fn profile_summary_note(&self, id: &ProfileId) -> Option<crate::notes::NoteContext> {
        let profile = self.profile_summary(id)?;
        let (host, port) = profile
            .endpoint
            .map_or((None, None), |(host, port)| (Some(host), Some(port)));
        Some(crate::notes::NoteContext {
            name: profile.name,
            host,
            port,
            user: profile.username,
            group: profile.group,
            protocol: Some(profile.kind.label().to_owned()),
        })
    }

    /// The notes folder, beside the profiles.
    #[must_use]
    pub fn notes_dir(&self) -> std::path::PathBuf {
        crate::notes::notes_dir(&self.config.profiles_file)
    }

    /// The summary of profile `id`, when there is one.
    #[must_use]
    pub fn profile_summary(&self, id: &ProfileId) -> Option<ProfileSummary> {
        self.profile_summaries()
            .into_iter()
            .find(|profile| profile.id == *id)
    }

    /// Whether the editor opens `id`: every protocol but a local program, whose command has
    /// an editor of its own to come.
    #[must_use]
    pub fn can_edit(&self, id: &ProfileId) -> bool {
        self.profile_summary(id)
            .is_some_and(|profile| profile.kind != ProfileKind::Local)
    }

    /// Applies a message about the tree.
    pub(super) fn tree_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::SelectProfile(id) => {
                let found = self.profile_summary(&id).map(|profile| profile.id);
                self.select_only(found);
                Vec::new()
            }
            Message::ToggleFolder(path) => {
                self.toggle_folder(&path);
                Vec::new()
            }
            Message::FoldAll(fold) => {
                self.closed_folders.clear();
                if fold {
                    self.closed_folders.extend(self.store.folder_paths());
                    self.closed_folders.insert(super::NO_FOLDER.to_owned());
                }
                Vec::new()
            }
            Message::Filter(message) => {
                self.filter_message(message);
                Vec::new()
            }
            Message::Folder(message) => self.folder_menu(message),
            Message::ProfileMenu(message) => self.profile_menu(message),
            Message::Selection(message) => self.selection_message(message),
            Message::ConnectProfile(id) => self.connect_profile(&id),
            Message::DuplicateProfile { id, suffix } => {
                self.duplicate_profile(&id, &suffix);
                Vec::new()
            }
            Message::RequestDeleteProfile(id) => {
                self.request_delete(&id);
                Vec::new()
            }
            Message::CopyProfile { id, what } => {
                let Some(text) = self
                    .profile_summary(&id)
                    .and_then(|profile| copied_text(&profile, what))
                else {
                    return Vec::new();
                };
                self.tell(super::Notice::Copied(text.clone()));
                vec![Effect::WriteClipboard(text)]
            }
            _ => Vec::new(),
        }
    }

    /// Opens `id` with its own protocol.
    pub(super) fn connect_profile(&mut self, id: &ProfileId) -> Vec<Effect> {
        if self.profile_summary(id).is_none() {
            return Vec::new();
        }
        self.selected_profile = Some(id.clone());
        self.open_saved(id, Purpose::Shell)
    }

    /// Asks to delete `id`, whatever its protocol.
    fn request_delete(&mut self, id: &ProfileId) {
        if let Some(profile) = self.profile_summary(id) {
            self.dialog = Some(Dialog::ConfirmDeleteProfile {
                id: profile.id,
                name: server_text(&profile.name),
            });
        }
    }

    /// Saves a copy of `id` named as the C# Heimdall names one: `suffix` (" (copy)" in
    /// English) added, then a number when that name is taken. Its saved password comes
    /// along, except an RDP one, as there.
    pub(super) fn duplicate_profile(&mut self, id: &ProfileId, suffix: &str) {
        let Some(source) = self.profile_summary(id) else {
            return;
        };
        let names: HashSet<String> = self
            .profile_summaries()
            .into_iter()
            .map(|profile| profile.name)
            .collect();
        let name = copy_name(&source.name, suffix, &names);
        let new = self.fresh_id();
        let copy = new.clone();
        let result = self.store.apply(|store| {
            match source.kind {
                ProfileKind::Ssh | ProfileKind::Sftp => {
                    let found = store.ssh_profiles().iter().find(|p| p.id == *id).cloned();
                    if let Some(mut profile) = found {
                        profile.id = copy.clone();
                        profile.name.clone_from(&name);
                        store.merge([profile]);
                    }
                }
                ProfileKind::Rdp => {
                    let found = store.rdp_profiles().iter().find(|p| p.id == *id).cloned();
                    if let Some(mut profile) = found {
                        profile.id = copy.clone();
                        profile.name.clone_from(&name);
                        store.merge_rdp([profile]);
                    }
                }
                ProfileKind::Telnet => {
                    let found = store
                        .telnet_profiles()
                        .iter()
                        .find(|p| p.id == *id)
                        .cloned();
                    if let Some(mut profile) = found {
                        profile.id = copy.clone();
                        profile.name.clone_from(&name);
                        store.merge_telnet([profile]);
                    }
                }
                ProfileKind::Vnc => {
                    let found = store.vnc_profiles().iter().find(|p| p.id == *id).cloned();
                    if let Some(mut profile) = found {
                        profile.id = copy.clone();
                        profile.name.clone_from(&name);
                        store.merge_vnc([profile]);
                    }
                }
                ProfileKind::Ftp => {
                    let found = store.ftp_profiles().iter().find(|p| p.id == *id).cloned();
                    if let Some(mut profile) = found {
                        profile.id = copy.clone();
                        profile.name.clone_from(&name);
                        store.merge_ftp([profile]);
                    }
                }
                ProfileKind::Local => {
                    let found = store.local_profiles().iter().find(|p| p.id == *id).cloned();
                    if let Some(mut profile) = found {
                        profile.id = copy.clone();
                        profile.name.clone_from(&name);
                        // Unapproved: the store never takes an approval from an incoming
                        // profile, so the copy runs only once approved in its own right.
                        store.merge_local([profile]);
                    }
                }
                ProfileKind::WinRm => {
                    let found = store.winrm_profiles().iter().find(|p| p.id == *id).cloned();
                    if let Some(mut profile) = found {
                        profile.id = copy.clone();
                        profile.name.clone_from(&name);
                        store.merge_winrm([profile]);
                    }
                }
            }
            // What it says of its server comes along, as the C# copy carries it.
            store.set_metadata(&copy, source.metadata.clone());
        });
        if let Err(error) = result {
            self.dialog = Some(Dialog::StoreError {
                detail: error.to_string(),
            });
            return;
        }
        if source.kind != ProfileKind::Rdp {
            self.copy_password(id, &new);
        }
        self.selected_profile = Some(new);
    }

    /// An identifier no saved profile has.
    pub(super) fn fresh_id(&self) -> ProfileId {
        let taken: HashSet<ProfileId> = self
            .profile_summaries()
            .into_iter()
            .map(|profile| profile.id)
            .collect();
        loop {
            let id = new_id(self.profiles());
            if !taken.contains(&id) {
                return id;
            }
        }
    }
}

/// `name` with `suffix`, then a number from 2 when that is taken: " (copy)", " (copy) 2".
fn copy_name(name: &str, suffix: &str, taken: &HashSet<String>) -> String {
    let base = format!("{name}{suffix}");
    if !taken.contains(&base) {
        return base;
    }
    // Among taken.len() + 1 numbers one is free: the search ends.
    (2..=taken.len() + 2)
        .map(|number| format!("{base} {number}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or(base)
}

/// The text the menu copies, when the profile has it.
fn copied_text(profile: &ProfileSummary, what: ProfileCopy) -> Option<String> {
    let (host, port) = profile.endpoint.as_ref()?;
    match what {
        ProfileCopy::Hostname => Some(host.clone()),
        ProfileCopy::Username => profile.username.clone().filter(|name| !name.is_empty()),
        ProfileCopy::Address => Some(display_address(host, *port)),
        ProfileCopy::SshCommand => {
            let host = shell_word(host);
            let target = match profile.username.as_deref().filter(|name| !name.is_empty()) {
                Some(user) => format!("{}@{host}", shell_word(user)),
                None => host,
            };
            Some(if *port == DEFAULT_SSH_PORT {
                format!("ssh {target}")
            } else {
                format!("ssh {target} -p {port}")
            })
        }
    }
}

/// `text` as one word of a POSIX shell: as is when it holds nothing a shell reads, else
/// single-quoted. A copied command is pasted into a shell, and an imported user name may
/// hold anything.
fn shell_word(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '%'));
    if plain {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(username: Option<&str>, port: u16) -> ProfileSummary {
        ProfileSummary {
            id: ProfileId::new("a"),
            name: "a".to_owned(),
            group: None,
            kind: ProfileKind::Ssh,
            endpoint: Some(("web.lab".to_owned(), port)),
            username: username.map(str::to_owned),
            gateway: None,
            favorite: false,
            metadata: heimdall_core::metadata::ProfileMetadata::default(),
        }
    }

    #[test]
    fn the_ssh_command_is_the_csharp_one() {
        let copy = |username, port| copied_text(&summary(username, port), ProfileCopy::SshCommand);
        assert_eq!(
            copy(Some("admin"), 22).as_deref(),
            Some("ssh admin@web.lab")
        );
        assert_eq!(copy(None, 2222).as_deref(), Some("ssh web.lab -p 2222"));
        assert_eq!(
            copy(Some("x; rm -rf ~"), 22).as_deref(),
            Some("ssh 'x; rm -rf ~'@web.lab"),
            "a pasted command runs nothing it holds"
        );
        assert_eq!(
            copy(Some("o'b"), 22).as_deref(),
            Some(r"ssh 'o'\''b'@web.lab")
        );
    }

    #[test]
    fn address_host_and_account_copy_as_they_are() {
        let profile = summary(Some("admin"), 2222);
        let copy = |what| copied_text(&profile, what);
        assert_eq!(copy(ProfileCopy::Address).as_deref(), Some("web.lab:2222"));
        assert_eq!(copy(ProfileCopy::Hostname).as_deref(), Some("web.lab"));
        assert_eq!(copy(ProfileCopy::Username).as_deref(), Some("admin"));
        assert_eq!(
            copied_text(&summary(None, 22), ProfileCopy::Username),
            None,
            "nothing to copy"
        );
    }

    #[test]
    fn a_copy_is_named_as_in_the_csharp_app() {
        let taken: HashSet<String> = ["web (copy)", "web (copy) 2"]
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        assert_eq!(copy_name("db", " (copy)", &taken), "db (copy)");
        assert_eq!(copy_name("web", " (copy)", &taken), "web (copy) 3");
    }
}
