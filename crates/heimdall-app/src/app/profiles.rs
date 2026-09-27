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

//! Creating, editing and deleting saved profiles. Every change is saved before it is kept:
//! a save that fails leaves the list as its file is.

use heimdall_core::credentials::{CredentialProtocol, Endpoint};
use heimdall_core::profile::ProfileId;
use heimdall_ssh::Secret;

use super::{App, Dialog, Message};
use crate::profile_draft::{DraftError, ProfileDraft, ProfileField, new_id};
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
            _ => {}
        }
    }

    /// Opens an empty profile form.
    pub(super) fn new_profile(&mut self) {
        self.dialog = Some(Dialog::EditProfile {
            draft: Box::default(),
            error: None,
        });
    }

    /// Opens the form of a saved profile.
    pub(super) fn edit_profile(&mut self, id: &ProfileId) {
        if let Some(profile) = self.profiles().iter().find(|profile| profile.id == *id) {
            let mut draft = ProfileDraft::from_profile(profile);
            draft.password_saved = self.password_saved(id);
            self.dialog = Some(Dialog::EditProfile {
                draft: Box::new(draft),
                error: None,
            });
        }
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
            .profiles()
            .iter()
            .find(|profile| profile.id == id)
            .map_or_else(String::new, |profile| server_text(&profile.name));
        self.dialog = Some(Dialog::ConfirmDeleteProfile { id, name });
    }

    /// Saves the form, with `password` typed into it, or puts it back with what to fix.
    pub(super) fn save_profile(&mut self, draft: Box<ProfileDraft>, password: Option<&Secret>) {
        let id = draft
            .editing
            .clone()
            .unwrap_or_else(|| new_id(self.profiles()));
        let typed =
            password.filter(|typed| !typed.expose().is_empty() && self.can_save_passwords());
        let profile = match draft.to_profile(id) {
            Ok(profile) if typed.is_some() && profile.username.is_none() => {
                Err(DraftError::UsernameForPassword)
            }
            other => other,
        };
        let profile = match profile {
            Ok(profile) => profile,
            Err(error) => {
                self.dialog = Some(Dialog::EditProfile {
                    draft,
                    error: Some(error),
                });
                return;
            }
        };
        let endpoint = Endpoint {
            protocol: CredentialProtocol::Ssh,
            host: profile.host.clone(),
            port: profile.port,
            username: profile.username.clone(),
        };
        let id = profile.id.clone();
        if let Err(error) = self.store.apply(|store| store.merge([profile])) {
            self.dialog = Some(Dialog::StoreError {
                detail: error.to_string(),
            });
            return;
        }
        if self.can_save_passwords() {
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
