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
use heimdall_core::profile::ProfileId;
use heimdall_ssh::Secret;

use super::{App, Dialog, Message};
use crate::profile_draft::{DraftError, DraftProfile, ProfileDraft, ProfileField};
use crate::text::server_text;

impl App {
    /// Applies a message about the profile form.
    pub(super) fn profile_message(&mut self, message: Message) {
        match message {
            Message::NewProfile => self.new_profile(),
            Message::EditProfile(id) => self.edit_profile(&id),
            Message::ProfileField { field, value } => self.profile_field(field, value),
            Message::DeleteProfile => self.ask_delete_profile(),
            Message::SaveProfile { password } => match self.dialog.take() {
                Some(Dialog::EditProfile { draft, .. }) => {
                    self.save_profile(draft, password.as_ref());
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
            Message::ChooseProtocol(protocol) => {
                if let Some(Dialog::EditProfile { draft, error }) = self.dialog.as_mut()
                    && draft.editing.is_none()
                {
                    **draft = ProfileDraft::new_for(protocol);
                    *error = None;
                }
            }
            Message::ProfileToggle { toggle, on } => {
                if let Some(Dialog::EditProfile { draft, error }) = self.dialog.as_mut() {
                    draft.toggle(toggle, on);
                    *error = None;
                }
            }
            _ => {}
        }
    }

    /// Opens an empty profile form, on its protocol picker.
    pub(super) fn new_profile(&mut self) {
        self.dialog = Some(Dialog::EditProfile {
            draft: Box::default(),
            error: None,
        });
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
        } else {
            return;
        };
        let mut draft = Box::new(draft);
        draft.password_saved = draft.protocol.saves_password() && self.password_saved(id);
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

    /// Saves the form, with `password` typed into it, or puts it back with what to fix.
    pub(super) fn save_profile(&mut self, draft: Box<ProfileDraft>, password: Option<&Secret>) {
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
                Ok((profile, endpoint))
            }
        });
        let (profile, endpoint) = match saved {
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
        let result = self.store.apply(|store| {
            match profile {
                DraftProfile::Ssh(profile) => store.merge([profile]),
                DraftProfile::Rdp(profile) => store.merge_rdp([profile]),
                DraftProfile::Vnc(profile) => store.merge_vnc([profile]),
                DraftProfile::WinRm(profile) => store.merge_winrm([profile]),
                DraftProfile::Telnet(profile) => store.merge_telnet([profile]),
            };
        });
        if let Err(error) = result {
            self.dialog = Some(Dialog::StoreError {
                detail: error.to_string(),
            });
            return;
        }
        if let Some(endpoint) = endpoint
            && self.can_save_passwords()
        {
            self.save_edited_password(&id, endpoint, typed, draft.clear_password);
        }
    }

    /// Deletes a profile. Its open tabs keep their own copy and stay as they are.
    pub(super) fn delete_profile(&mut self, id: &ProfileId) {
        match self.store.apply(|store| store.remove(id)) {
            Ok(_) => self.forget_password(id),
            Err(error) => {
                self.dialog = Some(Dialog::StoreError {
                    detail: error.to_string(),
                });
            }
        }
    }
}

fn saved_id(profile: &DraftProfile) -> &ProfileId {
    match profile {
        DraftProfile::Ssh(profile) => &profile.id,
        DraftProfile::Rdp(profile) => &profile.id,
        DraftProfile::Vnc(profile) => &profile.id,
        DraftProfile::WinRm(profile) => &profile.id,
        DraftProfile::Telnet(profile) => &profile.id,
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
        DraftProfile::WinRm(_) | DraftProfile::Telnet(_) => None,
    }
}
