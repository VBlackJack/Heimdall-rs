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

use super::{App, Dialog, Effect, Notice};

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
    /// Mark a profile as a favorite, or no longer, as the C# tree's "Add to favorites".
    Favorite {
        /// The profile.
        id: ProfileId,
        /// Whether it is one.
        favorite: bool,
    },
    /// Test whether a profile's address answers, as the C# tree's "Test reachability".
    TestReachability(ProfileId),
    /// Wake a profile's server with its MAC address, as the C# tree's "Wake on LAN".
    WakeOnLan(ProfileId),
    /// The magic packet was sent, or why not.
    WakeOnLanSent(Result<(), String>),
    /// What that test found.
    Tested {
        /// The address tested.
        host: String,
        /// Its port.
        port: u16,
        /// What it found.
        result: Result<crate::reachability::Reached, crate::reachability::Unreached>,
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
            ProfileMenuMessage::WakeOnLan(id) => {
                let Some(mac) = self
                    .profile_summary(&id)
                    .and_then(|profile| profile.metadata.mac_address)
                else {
                    return Vec::new();
                };
                return vec![Effect::WakeOnLan(mac)];
            }
            ProfileMenuMessage::WakeOnLanSent(result) => {
                self.tell(Notice::WakeOnLan(result));
            }
            ProfileMenuMessage::TestReachability(id) => {
                let Some((host, port)) = self
                    .profile_summary(&id)
                    .and_then(|profile| profile.endpoint)
                else {
                    return Vec::new();
                };
                self.tell(Notice::ReachabilityTesting {
                    host: host.clone(),
                    port,
                });
                return vec![Effect::TestReachability { host, port }];
            }
            ProfileMenuMessage::Tested { host, port, result } => {
                self.tell(match result {
                    Ok(reached) => Notice::Reachable {
                        host,
                        port,
                        millis: reached.millis,
                    },
                    Err(failure) => Notice::Unreachable {
                        host,
                        port,
                        failure,
                    },
                });
            }
            ProfileMenuMessage::Favorite { id, favorite } => self.set_favorites(&[id], favorite),
            ProfileMenuMessage::Move { id, to } => {
                let before = self.organizations(std::slice::from_ref(&id));
                if let Err(error) = self.store.apply(|store| store.set_group(&id, to)) {
                    self.dialog = Some(Dialog::save_failed(&error));
                } else {
                    // As the C# "Move to folder": undone from the Undo bar.
                    self.record_organization(super::tree_drag::OrganizationChange::Move, before);
                }
            }
        }
        Vec::new()
    }

    /// Marks `ids` as favorites, or none of them, saved at once, as the C# does them in one
    /// write; a failure said as the C# says it.
    pub(super) fn set_favorites(&mut self, ids: &[ProfileId], favorite: bool) {
        let saved = self.store.apply(|store| {
            for id in ids {
                store.set_favorite(id, favorite);
            }
        });
        if saved.is_err() {
            self.tell(Notice::FavoriteSaveFailed);
        }
    }

    /// Whether every one of `ids` is a favorite: the menu then offers to remove them, as
    /// the C# one does.
    #[must_use]
    pub fn all_favorites(&self, ids: &[ProfileId]) -> bool {
        !ids.is_empty() && ids.iter().all(|id| self.store.is_favorite(id))
    }

    /// Names profile `id` as typed; an empty name leaves it as it was, as the C# inline
    /// rename does.
    pub(super) fn confirm_rename_profile(&mut self, id: &ProfileId, value: &str) {
        let before = self.organizations(std::slice::from_ref(id));
        if let Err(error) = self.store.apply(|store| store.rename_profile(id, value)) {
            self.dialog = Some(Dialog::save_failed(&error));
            return;
        }
        self.record_organization(super::tree_drag::OrganizationChange::Rename, before);
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
