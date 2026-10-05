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

//! What the C# detail panel says of the session selected beside its address: the
//! credentials kept for it. Reading them may ask the system's credential store, so it is
//! read once a selection, and again when a dialog that may have saved one closes, never
//! each time the window is drawn.

use heimdall_core::profile::ProfileId;

use super::App;

/// The credentials kept for a session, as the C# "Saved credentials" line says them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SavedCredentials {
    /// A password is saved.
    pub password: bool,
    /// The key file it signs in with, by its name.
    pub key_file: Option<String>,
    /// That key's passphrase is saved.
    pub passphrase: bool,
}

impl SavedCredentials {
    /// Whether nothing is kept.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.password && self.key_file.is_none() && !self.passphrase
    }
}

/// The credentials read for the session selected, and whether a dialog was open when
/// they were.
#[derive(Debug, Default)]
pub(super) struct DetailCache {
    read: Option<(ProfileId, SavedCredentials)>,
    dialog_open: bool,
}

impl App {
    /// The credentials kept for the session selected, as last read.
    #[must_use]
    pub fn selected_credentials(&self) -> Option<&SavedCredentials> {
        let (id, saved) = self.detail.read.as_ref()?;
        (self.selected_profile.as_ref() == Some(id)).then_some(saved)
    }

    /// Reads the credentials of the session selected again when it changed, or when a
    /// dialog, which may have saved one, just closed.
    pub fn refresh_detail(&mut self) {
        let dialog_open = self.dialog.is_some();
        let closed = std::mem::replace(&mut self.detail.dialog_open, dialog_open) && !dialog_open;
        let known = self.detail.read.as_ref().map(|(id, _)| id);
        if !closed && known == self.selected_profile.as_ref() {
            return;
        }
        self.detail.read = self.selected_profile.clone().map(|id| {
            let saved = self.saved_credentials(&id);
            (id, saved)
        });
    }

    /// The credentials kept for profile `id`.
    fn saved_credentials(&self, id: &ProfileId) -> SavedCredentials {
        let key_file = self
            .store
            .ssh_profiles()
            .iter()
            .find(|profile| profile.id == *id)
            .and_then(|profile| profile.key_path.as_ref())
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned());
        SavedCredentials {
            password: self.password_saved(id),
            passphrase: key_file.is_some() && self.passphrase_saved(id),
            key_file,
        }
    }
}
