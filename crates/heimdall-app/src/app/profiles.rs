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

//! Creating, editing and deleting saved profiles, of every protocol but a local program, as
//! the C# Heimdall's session dialog does: a new profile's protocol is chosen first, a saved
//! one is edited in its own protocol. Every change is saved before it is kept: a save that
//! fails leaves the list as its file is.

use heimdall_core::credentials::{CredentialProtocol, Endpoint, rdp_account};
use heimdall_core::profile::{LocalApproval, ProfileId};
use heimdall_core::winrm;
use heimdall_ssh::Secret;
use heimdall_term::local;

use super::{App, Dialog, Message};
use crate::profile_draft::{
    DraftError, DraftProfile, DraftProtocol, ProfileDraft, ProfileField, ProfileToggle, SavedSecret,
};
use crate::text::server_text;

impl App {
    /// Applies a message about the profile form.
    pub(super) fn profile_message(&mut self, message: Message) {
        match message {
            Message::NewProfile => self.new_profile(),
            Message::EditProfile(id) => self.edit_profile(&id),
            Message::ProfileField { field, value } => self.profile_field(field, value),
            Message::DeleteProfile => self.ask_delete_profile(),
            Message::SaveProfile {
                password,
                passphrase,
            } => match self.dialog.take() {
                Some(Dialog::EditProfile { draft, .. }) => {
                    self.save_profile(draft, password.as_ref(), passphrase.as_ref());
                }
                // Not the form: whatever is open stays.
                other => self.dialog = other,
            },
            Message::ClearPassword => {
                if let Some(Dialog::EditProfile { draft, .. }) = self.dialog.as_mut() {
                    draft.clear_password = true;
                    draft.password_saved = false;
                }
            }
            Message::ClearPassphrase => {
                if let Some(Dialog::EditProfile { draft, .. }) = self.dialog.as_mut() {
                    draft.passphrase = SavedSecret::Cleared;
                }
            }
            Message::ChooseProtocol(protocol) => {
                let (ssh_mode, rdp_mode) = (
                    self.settings.ssh_default_mode,
                    self.settings.rdp_default_mode,
                );
                let gateway = self.last_used_gateway(protocol);
                if let Some(Dialog::EditProfile { draft, error }) = self.dialog.as_mut()
                    && draft.editing.is_none()
                {
                    **draft = ProfileDraft::new_for(protocol);
                    // As the C# "Add server" starts with the settings' default SSH and RDP
                    // modes (`ServerListViewModel.cs:1527-1531`).
                    draft.ssh_mode = ssh_mode;
                    if let Some(gateway) = gateway {
                        draft.choose_gateway(gateway);
                    }
                    draft.rdp_extras.external = rdp_mode.is_external();
                    *error = None;
                }
            }
            Message::ProfileToggle { toggle, on } => {
                if let Some(Dialog::EditProfile { draft, error }) = self.dialog.as_mut() {
                    draft.toggle(toggle, on);
                    *error = None;
                }
            }
            Message::ProfileChoice(choice) => {
                if let Some(Dialog::EditProfile { draft, .. }) = self.dialog.as_mut() {
                    draft.choose(choice);
                }
            }
            Message::PostConnectEdit(edit) => {
                if let Some(Dialog::EditProfile { draft, error }) = self.dialog.as_mut() {
                    draft.post_connect.apply(edit);
                    *error = None;
                }
            }
            // The gateway dialog opens from a session's form and returns to it.
            other => self.gateway_message(other),
        }
    }

    /// Opens an empty profile form, on its protocol picker.
    pub(super) fn new_profile(&mut self) {
        self.dialog = Some(Dialog::EditProfile {
            draft: Box::new(self.blank_draft()),
            error: None,
        });
    }

    /// An empty profile form, with the settings' default SSH and RDP modes, as the C# "Add
    /// server" starts (`ServerListViewModel.cs:1527-1531`). Quick connect and the imports
    /// keep their own, as the C# ones do.
    pub(super) fn blank_draft(&self) -> ProfileDraft {
        let mut draft = ProfileDraft {
            ssh_mode: self.settings.ssh_default_mode,
            ..ProfileDraft::default()
        };
        draft.rdp_extras.external = self.settings.rdp_default_mode.is_external();
        draft
    }

    /// The gateway a new profile of `protocol` starts on, as the C# "Add server" preselects
    /// the last one used (`ServerListViewModel.cs:2290-2297`): only for a protocol that goes
    /// through one, and only while that gateway still exists, matched as the C# matches it,
    /// whatever the case.
    fn last_used_gateway(&self, protocol: DraftProtocol) -> Option<ProfileId> {
        if !protocol.routes_through_gateway() {
            return None;
        }
        let last = self.settings.last_used_gateway.as_ref()?;
        self.gateways()
            .iter()
            .find(|gateway| gateway.id.as_str().eq_ignore_ascii_case(last.as_str()))
            .map(|gateway| gateway.id.clone())
    }

    /// Keeps `gateway`, the one of the profile just saved, as the one a new profile starts
    /// on, as the C# writes `LastUsedGatewayId` after "Add server", "Save as profile" and
    /// "Edit server" (`ServerListViewModel.cs:1547-1552`, `1606-1610`, `1712-1717`): none
    /// when the profile has none. A save that fails is logged; the profile itself is saved
    /// all the same.
    fn remember_gateway(&mut self, gateway: Option<ProfileId>) {
        if self.settings.last_used_gateway == gateway {
            return;
        }
        self.settings.last_used_gateway = gateway;
        if let Err(error) = self.settings.save(&self.settings_file) {
            log::warn!("the last gateway used was not saved: {error}");
        }
    }

    /// Opens the form of a saved profile, in its own protocol.
    pub(super) fn edit_profile(&mut self, id: &ProfileId) {
        let draft = if let Some(profile) = self.profiles().iter().find(|p| p.id == *id) {
            ProfileDraft::from_profile(profile)
        } else if let Some(profile) = self.rdp_profiles().iter().find(|p| p.id == *id) {
            ProfileDraft::from_rdp(profile)
        } else if let Some(profile) = self.vnc_profiles().iter().find(|p| p.id == *id) {
            ProfileDraft::from_vnc(profile)
        } else if let Some(profile) = self.winrm_profiles().iter().find(|p| p.id == *id) {
            ProfileDraft::from_winrm(profile)
        } else if let Some(profile) = self.telnet_profiles().iter().find(|p| p.id == *id) {
            ProfileDraft::from_telnet(profile)
        } else if let Some(profile) = self.local_profiles().iter().find(|p| p.id == *id) {
            ProfileDraft::from_local(profile)
        } else if let Some(profile) = self.store.ftp_profiles().iter().find(|p| p.id == *id) {
            ProfileDraft::from_ftp(profile)
        } else if let Some(profile) = self.store.citrix_profiles().iter().find(|p| p.id == *id) {
            ProfileDraft::from_citrix(profile)
        } else {
            return;
        };
        let mut draft = Box::new(draft);
        draft.password_saved = draft.protocol.saves_password() && self.password_saved(id);
        draft.passphrase =
            SavedSecret::from_saved(draft.protocol.has_key_file() && self.passphrase_saved(id));
        if self.store.is_favorite(id) {
            draft.toggle(ProfileToggle::Favorite, true);
        }
        if let Some(metadata) = self.store.metadata(id) {
            draft.show_metadata(metadata);
        }
        self.dialog = Some(Dialog::EditProfile { draft, error: None });
    }

    /// A field of the open form changed.
    pub(super) fn profile_field(&mut self, field: ProfileField, value: String) {
        if let Some(Dialog::EditProfile { draft, error }) = self.dialog.as_mut() {
            draft.set(field, value);
            // The message named what to fix; once the user fixes something, it goes.
            *error = None;
        }
    }

    /// From the form of a saved profile, asks to delete it.
    pub(super) fn ask_delete_profile(&mut self) {
        let Some(Dialog::EditProfile { draft, .. }) = self.dialog.as_ref() else {
            return;
        };
        let Some(id) = draft.editing.clone() else {
            return;
        };
        let name = self
            .profile_summary(&id)
            .map_or_else(String::new, |profile| server_text(&profile.name));
        self.dialog = Some(Dialog::ConfirmDeleteProfile { id, name });
    }

    /// Saves the form, with `password` and `passphrase` typed into it, or puts it back with
    /// what to fix.
    pub(super) fn save_profile(
        &mut self,
        draft: Box<ProfileDraft>,
        password: Option<&Secret>,
        passphrase: Option<&Secret>,
    ) {
        let id = draft.editing.clone().unwrap_or_else(|| self.fresh_id());
        let typed = password.filter(|typed| {
            !typed.expose().is_empty()
                && draft.protocol.saves_password()
                && self.can_save_passwords()
        });
        let saved = draft.to_saved(id).and_then(|profile| {
            let endpoint = password_endpoint(&profile);
            let has_account = endpoint
                .as_ref()
                .is_some_and(|endpoint| endpoint.username.is_some());
            if typed.is_some() && draft.protocol.password_needs_username() && !has_account {
                Err(DraftError::UsernameForPassword)
            } else {
                Ok((profile, endpoint, draft.metadata()?))
            }
        });
        let (profile, endpoint, metadata) = match saved {
            Ok(saved) => saved,
            Err(error) => {
                self.dialog = Some(Dialog::EditProfile {
                    draft,
                    error: Some(error),
                });
                return;
            }
        };
        let id = saved_id(&profile).clone();
        // The current Windows identity has no password: one stored before goes.
        let drops_password =
            matches!(&profile, DraftProfile::WinRm(winrm) if winrm.username.is_none());
        let favorite = draft.is_on(ProfileToggle::Favorite);
        let gateway = saved_gateway(&draft);
        // The key whose passphrase the form saves: an SSH profile's, which may have none.
        let key_file = match &profile {
            DraftProfile::Ssh(profile) => Some(profile.key_path.clone()),
            _ => None,
        };
        // Renamed, it keeps its password manager's entry under the old name, as in C#.
        let old_name = draft
            .editing
            .as_ref()
            .and_then(|editing| self.profile_summary(editing))
            .map(|summary| summary.name.clone())
            .filter(|old| *old != draft.name.trim());
        let result = self.store.apply(|store| {
            match profile {
                // Steps written in the form are approved by the one who wrote them, as the
                // C# dialog confirms them when it saves.
                DraftProfile::Ssh(profile) => {
                    let steps = profile.post_connect.steps.clone();
                    let saved = profile.id.clone();
                    let report = store.merge([profile]);
                    store.approve_post_connect(&saved, &steps);
                    report
                }
                DraftProfile::Rdp(profile) => store.merge_rdp([profile]),
                DraftProfile::Vnc(profile) => store.merge_vnc([profile]),
                DraftProfile::WinRm(profile) => store.merge_winrm([profile]),
                DraftProfile::Telnet(profile) => store.merge_telnet([profile]),
                DraftProfile::Ftp(profile) => store.merge_ftp([profile]),
                DraftProfile::Citrix(profile) => store.merge_citrix([profile]),
                // Written in the form: approved by the one who wrote it, the program as it is
                // found now, as the C# dialog confirms it when it saves. A program found
                // nowhere is saved unapproved; opening it says why it cannot run.
                DraftProfile::Local(profile) => {
                    let approval = local::program_path(profile.command.program.as_deref())
                        .ok()
                        .map(|program_path| LocalApproval {
                            command: profile.command.clone(),
                            program_path,
                        });
                    let saved = profile.id.clone();
                    let report = store.merge_local([profile]);
                    if let Some(approval) = approval {
                        store.approve_local(&saved, approval);
                    }
                    report
                }
            };
            store.set_favorite(&id, favorite);
            store.set_metadata(&id, metadata);
            if let Some(old_name) = &old_name {
                store.freeze_vault_entry(&id, old_name);
            }
        });
        if let Err(error) = result {
            self.dialog = Some(Dialog::save_failed(&error));
            return;
        }
        self.remember_gateway(gateway);
        if let Some(endpoint) = endpoint
            && self.can_save_passwords()
        {
            self.save_edited_password(&id, endpoint, typed, draft.clear_password);
        } else if drops_password && self.can_save_passwords() {
            self.drop_password(&id);
        }
        if let Some(key_path) = key_file
            && self.can_save_passwords()
        {
            self.save_edited_passphrase(
                &id,
                key_path.as_deref(),
                passphrase,
                draft.passphrase == SavedSecret::Cleared,
            );
        }
    }

    /// The server and account a password saved now for profile `id` is for, from the
    /// profile as it is saved, as the editor computes it; `None` for a protocol whose
    /// password is not saved, or a profile not found.
    pub(super) fn stored_password_endpoint(&self, id: &ProfileId) -> Option<Endpoint> {
        let store = &self.store;
        let profile = if let Some(found) = store.ssh_profiles().iter().find(|p| p.id == *id) {
            DraftProfile::Ssh(found.clone())
        } else if let Some(found) = store.rdp_profiles().iter().find(|p| p.id == *id) {
            DraftProfile::Rdp(found.clone())
        } else if let Some(found) = store.vnc_profiles().iter().find(|p| p.id == *id) {
            DraftProfile::Vnc(found.clone())
        } else if let Some(found) = store.winrm_profiles().iter().find(|p| p.id == *id) {
            DraftProfile::WinRm(found.clone())
        } else {
            DraftProfile::Ftp(store.ftp_profiles().iter().find(|p| p.id == *id)?.clone())
        };
        password_endpoint(&profile)
    }

    /// Deletes a profile. Its open tabs keep their own copy and stay as they are.
    pub(super) fn delete_profile(&mut self, id: &ProfileId) {
        match self.store.apply(|store| store.remove(id)) {
            Ok(_) => self.forget_password(id),
            Err(error) => {
                self.dialog = Some(Dialog::save_failed(&error));
            }
        }
    }
}

/// The gateway `draft` saves its profile with: none for a protocol that goes through none.
fn saved_gateway(draft: &ProfileDraft) -> Option<ProfileId> {
    draft
        .protocol
        .routes_through_gateway()
        .then(|| draft.routed_gateway())
        .flatten()
}

fn saved_id(profile: &DraftProfile) -> &ProfileId {
    match profile {
        DraftProfile::Ssh(profile) => &profile.id,
        DraftProfile::Rdp(profile) => &profile.id,
        DraftProfile::Vnc(profile) => &profile.id,
        DraftProfile::WinRm(profile) => &profile.id,
        DraftProfile::Telnet(profile) => &profile.id,
        DraftProfile::Local(profile) => &profile.id,
        DraftProfile::Ftp(profile) => &profile.id,
        DraftProfile::Citrix(profile) => &profile.id,
    }
}

/// The server and account a password saved with `profile` is for; `None` for a protocol
/// whose password is not saved.
fn password_endpoint(profile: &DraftProfile) -> Option<Endpoint> {
    match profile {
        DraftProfile::Ssh(profile) => Some(Endpoint {
            protocol: CredentialProtocol::Ssh,
            host: profile.host.clone(),
            port: profile.port,
            username: profile.username.clone(),
        }),
        DraftProfile::Rdp(profile) => Some(Endpoint {
            protocol: CredentialProtocol::Rdp,
            host: profile.host.clone(),
            port: profile.port,
            username: profile
                .username
                .as_deref()
                .map(|user| rdp_account(profile.domain.as_deref(), user)),
        }),
        DraftProfile::Vnc(profile) => Some(Endpoint {
            protocol: CredentialProtocol::Vnc,
            host: profile.host.clone(),
            port: profile.port,
            username: None,
        }),
        DraftProfile::Ftp(profile) => Some(Endpoint {
            protocol: CredentialProtocol::Ftp,
            host: profile.host.clone(),
            port: profile.port,
            username: profile.username.clone(),
        }),
        // The current Windows identity has none: only an account's is saved.
        DraftProfile::WinRm(profile) => winrm::password_endpoint(profile),
        DraftProfile::Telnet(_) | DraftProfile::Local(_) | DraftProfile::Citrix(_) => None,
    }
}
