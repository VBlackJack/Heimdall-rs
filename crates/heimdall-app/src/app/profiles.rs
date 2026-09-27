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

use heimdall_core::profile::ProfileId;

use super::{App, Dialog};
use crate::profile_draft::{ProfileDraft, ProfileField, new_id};
use crate::text::server_text;

impl App {
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
            self.dialog = Some(Dialog::EditProfile {
                draft: Box::new(ProfileDraft::from_profile(profile)),
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

    /// Saves the form, or puts it back with what to fix.
    pub(super) fn save_profile(&mut self, draft: Box<ProfileDraft>) {
        let id = draft
            .editing
            .clone()
            .unwrap_or_else(|| new_id(self.profiles()));
        let profile = match draft.to_profile(id) {
            Ok(profile) => profile,
            Err(error) => {
                self.dialog = Some(Dialog::EditProfile {
                    draft,
                    error: Some(error),
                });
                return;
            }
        };
        if let Err(error) = self.store.apply(|store| store.merge([profile])) {
            self.dialog = Some(Dialog::StoreError {
                detail: error.to_string(),
            });
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
