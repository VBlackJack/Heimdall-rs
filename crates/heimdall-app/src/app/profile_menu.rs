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

//! A profile's Rename and "Move to folder", as the C# tree menu has them.

use heimdall_core::folder;
use heimdall_core::profile::ProfileId;

use super::{App, Dialog, Effect};

/// Something from a profile's menu about its name or its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileMenuMessage {
    /// Ask for a new name for a profile, its present one written in.
    Rename(ProfileId),
    /// The name typed so far.
    NameEdited(String),
    /// Put a profile in a folder, none for `None`.
    Move {
        /// The profile.
        id: ProfileId,
        /// The folder.
        to: Option<String>,
    },
}

impl App {
    /// Applies a message from a profile's menu about its name or its folder.
    pub(super) fn profile_menu(&mut self, message: ProfileMenuMessage) -> Vec<Effect> {
        match message {
            ProfileMenuMessage::Rename(id) => {
                if let Some(profile) = self.profile_summary(&id) {
                    self.dialog = Some(Dialog::RenameProfile {
                        id,
                        value: profile.name,
                    });
                }
            }
            ProfileMenuMessage::NameEdited(value) => {
                if let Some(Dialog::RenameProfile { value: typed, .. }) = self.dialog.as_mut() {
                    *typed = value;
                }
            }
            ProfileMenuMessage::Move { id, to } => {
                if let Err(error) = self.store.apply(|store| store.set_group(&id, to)) {
                    self.dialog = Some(Dialog::StoreError {
                        detail: error.to_string(),
                    });
                }
            }
        }
        Vec::new()
    }

    /// Names profile `id` as typed; an empty name leaves it as it was, as the C# inline
    /// rename does.
    pub(super) fn confirm_rename_profile(&mut self, id: &ProfileId, value: &str) {
        if let Err(error) = self.store.apply(|store| store.rename_profile(id, value)) {
            self.dialog = Some(Dialog::StoreError {
                detail: error.to_string(),
            });
        }
    }

    /// Where profile `id` can move, as the C# "Move to folder" lists them: "(No Folder)"
    /// first, as `None`, then every folder; with whether it is another than its own.
    #[must_use]
    pub fn profile_move_targets(&self, id: &ProfileId) -> Vec<(Option<String>, bool)> {
        let current = self
            .profile_summary(id)
            .and_then(|profile| profile.group)
            .map(|group| folder::normal(&group))
            .filter(|group| !group.is_empty());
        std::iter::once(None)
            .chain(self.store.folder_paths().into_iter().map(Some))
            .map(|target| {
                let other = target != current;
                (target, other)
            })
            .collect()
    }
}
