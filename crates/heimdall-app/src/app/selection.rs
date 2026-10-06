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

//! Several profiles selected in the tree, as the C# Heimdall's: Ctrl adds or takes one, Shift
//! takes all from the last one clicked; a right click on one of them offers what applies to
//! them all: connect, duplicate, move to a folder, delete once asked.

use heimdall_core::profile::ProfileId;

use super::tree::ProfileKind;
use super::{App, Dialog, Effect, Notice};

/// Longest list of names the deletion question shows, as the C# one.
const LISTED_NAMES: usize = 10;

/// Something about the profiles selected together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionMessage {
    /// Ctrl+click: this profile added to the selection, or taken out.
    Toggle(ProfileId),
    /// Shift+click: every profile from the last one clicked to this one, in `order`, the
    /// order the tree shows them.
    Range {
        /// The profile clicked.
        to: ProfileId,
        /// The profiles as the tree shows them.
        order: Vec<ProfileId>,
    },
    /// Connect every one selected.
    Connect,
    /// Duplicate every one selected, each named with `suffix`.
    Duplicate {
        /// What a copy's name ends with.
        suffix: String,
    },
    /// Put every one selected in a folder, none for `None`.
    Move(Option<String>),
    /// Route every one selected that can be through this gateway, or directly for `None`,
    /// as the C# bulk "Set gateway".
    SetGateway(Option<ProfileId>),
    /// Mark every one selected as a favorite, or none of them.
    Favorite(bool),
    /// Set a field of every one selected that has it, as the C# "Edit" menu.
    Edit(super::BulkField),
    /// The value typed for them.
    BulkEdited(String),
    /// Delete every one selected, once asked.
    RequestDelete,
}

impl App {
    /// Routes the profiles selected that can be through `gateway`, or directly, and says
    /// how many changed.
    fn set_selection_gateway(&mut self, gateway: Option<&ProfileId>) {
        let ids = self.selected_profiles();
        let changed = self.store.apply(|store| {
            ids.iter()
                .filter(|id| store.set_gateway(id, gateway))
                .count()
        });
        match changed {
            Ok(changed) => self.tell(Notice::BulkGatewayUpdated(changed)),
            Err(error) => {
                self.dialog = Some(Dialog::StoreError {
                    detail: error.to_string(),
                });
            }
        }
    }

    /// Applies a message about the profiles selected together.
    pub(super) fn selection_message(&mut self, message: SelectionMessage) -> Vec<Effect> {
        match message {
            SelectionMessage::Toggle(id) => {
                if self.profile_summary(&id).is_some() {
                    if self.selection.is_empty()
                        && let Some(anchor) = self.selected_profile.clone()
                    {
                        self.selection.insert(anchor);
                    }
                    if !self.selection.remove(&id) {
                        self.selection.insert(id.clone());
                    }
                    self.selected_profile = Some(id);
                }
                Vec::new()
            }
            SelectionMessage::Range { to, order } => {
                let anchor = self.selected_profile.clone().unwrap_or_else(|| to.clone());
                let (Some(from), Some(until)) = (
                    order.iter().position(|id| *id == anchor),
                    order.iter().position(|id| *id == to),
                ) else {
                    return Vec::new();
                };
                let (first, last) = (from.min(until), from.max(until));
                self.selection = order[first..=last].iter().cloned().collect();
                Vec::new()
            }
            SelectionMessage::Connect => {
                let targets: Vec<ProfileId> = self
                    .selected_profiles()
                    .into_iter()
                    .filter(|id| self.connects_in_bulk(id))
                    .collect();
                targets
                    .iter()
                    .flat_map(|id| self.connect_profile(id))
                    .collect()
            }
            SelectionMessage::Duplicate { suffix } => {
                for id in self.selected_profiles() {
                    self.duplicate_profile(&id, &suffix);
                }
                Vec::new()
            }
            SelectionMessage::Move(to) => {
                let ids = self.selected_profiles();
                let before = self.organizations(&ids);
                let moved = self.store.apply(|store| {
                    for id in &ids {
                        store.set_group(id, to.clone());
                    }
                });
                if let Err(error) = moved {
                    self.dialog = Some(Dialog::StoreError {
                        detail: error.to_string(),
                    });
                } else {
                    self.record_organization(super::tree_drag::OrganizationChange::Move, before);
                }
                Vec::new()
            }
            SelectionMessage::SetGateway(gateway) => {
                self.set_selection_gateway(gateway.as_ref());
                Vec::new()
            }
            SelectionMessage::Edit(field) => {
                self.open_bulk_edit(field);
                Vec::new()
            }
            SelectionMessage::BulkEdited(value) => {
                self.bulk_edited(value);
                Vec::new()
            }
            SelectionMessage::Favorite(favorite) => {
                let ids = self.selected_profiles();
                self.set_favorites(&ids, favorite);
                Vec::new()
            }
            SelectionMessage::RequestDelete => {
                let ids = self.selected_profiles();
                if ids.len() > 1 {
                    let mut names: Vec<String> = ids
                        .iter()
                        .filter_map(|id| self.profile_summary(id))
                        .map(|profile| profile.name)
                        .collect();
                    names.sort_by_key(|name| name.to_lowercase());
                    if names.len() > LISTED_NAMES {
                        names.clear();
                    }
                    self.dialog = Some(Dialog::ConfirmDeleteProfiles { ids, names });
                }
                Vec::new()
            }
        }
    }

    /// The profiles selected together, still saved, in their selection's order; empty when
    /// fewer than two are.
    #[must_use]
    pub fn selected_profiles(&self) -> Vec<ProfileId> {
        let ids: Vec<ProfileId> = self
            .selection
            .iter()
            .filter(|id| self.profile_summary(id).is_some())
            .cloned()
            .collect();
        if ids.len() > 1 { ids } else { Vec::new() }
    }

    /// Whether profile `id` shows selected in the tree.
    #[must_use]
    pub fn is_selected(&self, id: &ProfileId) -> bool {
        if self.selection.is_empty() {
            self.selected_profile.as_ref() == Some(id)
        } else {
            self.selection.contains(id)
        }
    }

    /// Whether "Connect selected" opens profile `id`: not a local program, which runs on
    /// this computer and may ask to be approved first.
    #[must_use]
    pub fn connects_in_bulk(&self, id: &ProfileId) -> bool {
        self.profile_summary(id)
            .is_some_and(|profile| profile.kind != ProfileKind::Local)
    }

    /// Selects `id` alone, as a plain click does.
    pub(super) fn select_only(&mut self, id: Option<ProfileId>) {
        self.selection.clear();
        if id.is_some() {
            self.selected_folder = None;
        }
        self.selected_profile = id;
    }

    /// Deletes the profiles of a confirmed bulk deletion; gone, they leave the selection.
    pub(super) fn confirm_delete_profiles(&mut self, ids: &[ProfileId]) {
        for id in ids {
            self.delete_profile(id);
        }
    }
}
