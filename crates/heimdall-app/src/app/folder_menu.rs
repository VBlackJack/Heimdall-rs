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

//! A folder's menu, as the C# Heimdall's: connect all it holds, add a session or a folder
//! in it, rename it, move it, delete it with its profiles going to no folder.

use heimdall_core::folder::{self, FolderColor, FolderError};
use heimdall_core::store::ProfileStore;

use super::folders::NO_FOLDER;
use super::tree::{ProfileKind, ProfileSummary};
use super::{App, Dialog, Effect};
use crate::profile_draft::ProfileDraft;

/// Something from a folder's menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderMessage {
    /// Ask for the name of a new folder in `parent`, the top level for an empty one.
    New {
        /// Where.
        parent: String,
    },
    /// Ask for a new name for a folder.
    Rename(String),
    /// The folder name typed so far.
    NameEdited(String),
    /// Move a folder under another, the top level for an empty one.
    Move {
        /// The folder.
        path: String,
        /// Where to.
        to: String,
    },
    /// Delete a folder, once asked.
    RequestDelete(String),
    /// Connect every session a folder holds, once asked.
    RequestConnectAll(String),
    /// The session form, its folder written in.
    NewProfileIn(String),
    /// Give a folder a colour, or take its own away for `None`, as the C# "Colour" menu.
    Color {
        /// The folder.
        path: String,
        /// Its colour.
        color: Option<FolderColor>,
    },
}

/// What a folder name is typed for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderNaming {
    /// A new folder in this one, the top level for an empty one.
    New(String),
    /// A new name for this folder.
    Rename(String),
}

/// The profiles `path` holds, its folders' included: those in no folder for [`NO_FOLDER`].
fn held(profiles: Vec<ProfileSummary>, path: &str) -> Vec<ProfileSummary> {
    profiles
        .into_iter()
        .filter(|profile| {
            let group = profile
                .group
                .as_deref()
                .map(folder::normal)
                .unwrap_or_default();
            if path == NO_FOLDER {
                group.is_empty()
            } else {
                folder::is_within(&group, path)
            }
        })
        .collect()
}

/// Whether "Connect all" opens `profile`: not a local program, which runs on this computer
/// and may ask to be approved first.
fn connectable(profile: &ProfileSummary) -> bool {
    profile.kind != ProfileKind::Local
}

impl App {
    /// Applies a message from a folder's menu.
    pub(super) fn folder_menu(&mut self, message: FolderMessage) -> Vec<Effect> {
        match message {
            FolderMessage::New { parent } => {
                self.dialog = Some(Dialog::FolderName {
                    naming: FolderNaming::New(parent),
                    value: String::new(),
                    error: None,
                });
            }
            FolderMessage::Rename(path) => {
                self.dialog = Some(Dialog::FolderName {
                    value: folder::name(&path),
                    naming: FolderNaming::Rename(path),
                    error: None,
                });
            }
            FolderMessage::NameEdited(value) => {
                if let Some(Dialog::FolderName {
                    value: typed,
                    error,
                    ..
                }) = self.dialog.as_mut()
                {
                    *typed = value;
                    *error = None;
                }
            }
            FolderMessage::Move { path, to } => {
                self.change_folders(&path, |store| store.move_folder(&path, &to));
            }
            FolderMessage::Color { path, color } => {
                // The menu offers only folders that are there.
                if let Err(error) = self
                    .store
                    .apply(|store| store.set_folder_color(&path, color))
                {
                    self.dialog = Some(Dialog::StoreError {
                        detail: error.to_string(),
                    });
                }
            }
            FolderMessage::RequestDelete(path) => {
                let count = held(self.profile_summaries(), &path).len();
                self.dialog = Some(Dialog::ConfirmDeleteFolder {
                    name: folder::name(&path),
                    path,
                    count,
                });
            }
            FolderMessage::RequestConnectAll(path) => {
                let count = self.folder_connectable(&path);
                if count > 0 {
                    self.dialog = Some(Dialog::ConfirmConnectFolder { path, count });
                }
            }
            FolderMessage::NewProfileIn(path) => {
                let mut draft = ProfileDraft::default();
                if path != NO_FOLDER {
                    draft.group = path;
                }
                self.dialog = Some(Dialog::EditProfile {
                    draft: Box::new(draft),
                    error: None,
                });
            }
        }
        Vec::new()
    }

    /// Every folder the tree shows, empty ones included.
    #[must_use]
    pub fn folder_paths(&self) -> Vec<String> {
        self.store.folder_paths()
    }

    /// The colour folder `path` is shown in: its own, else that of the nearest folder it is
    /// in.
    #[must_use]
    pub fn folder_color(&self, path: &str) -> Option<FolderColor> {
        self.store.folder_color(path)
    }

    /// The colour folder `path` was given itself.
    #[must_use]
    pub fn own_folder_color(&self, path: &str) -> Option<FolderColor> {
        self.store.own_folder_color(path)
    }

    /// How many sessions "Connect all" of `path` opens.
    #[must_use]
    pub fn folder_connectable(&self, path: &str) -> usize {
        held(self.profile_summaries(), path)
            .iter()
            .filter(|profile| connectable(profile))
            .count()
    }

    /// The folders a folder can move to: the top level, as an empty path, then every folder
    /// but itself, those it holds and the one it is in.
    #[must_use]
    pub fn folder_targets(&self, path: &str) -> Vec<String> {
        let parent = folder::parent(path);
        let mut targets = Vec::new();
        if !parent.is_empty() {
            targets.push(String::new());
        }
        targets.extend(
            self.store
                .folder_paths()
                .into_iter()
                .filter(|target| !folder::is_within(target, path) && *target != parent),
        );
        targets
    }

    /// Names a folder as the open dialog asks; the dialog stays, saying why, when the name
    /// cannot be taken.
    pub(super) fn confirm_folder_name(&mut self, naming: FolderNaming, value: String) {
        let result = match &naming {
            FolderNaming::New(parent) => {
                let result = self.store.apply(|store| store.add_folder(parent, &value));
                if let Ok(Ok(path)) = &result {
                    self.tell(super::Notice::FolderCreated(path.clone()));
                }
                result.map(|added| added.map(|_| ()))
            }
            FolderNaming::Rename(path) => {
                let path = path.clone();
                return self.rename_folder(naming, &path, value);
            }
        };
        self.after_folder_change(result, naming, value);
    }

    /// Renames folder `path` to `value`, its folds following it.
    fn rename_folder(&mut self, naming: FolderNaming, path: &str, value: String) {
        let result = self
            .store
            .apply(|store| store.rename_folder(path, &value))
            .map(|renamed| {
                renamed.map(|renamed| {
                    self.follow_folds(path, &renamed);
                })
            });
        self.after_folder_change(result, naming, value);
    }

    /// Shows what a folder change came to: the dialog again with its reason when refused.
    fn after_folder_change(
        &mut self,
        result: Result<Result<(), FolderError>, heimdall_core::store::StoreError>,
        naming: FolderNaming,
        value: String,
    ) {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                self.dialog = Some(Dialog::FolderName {
                    naming,
                    value,
                    error: Some(error),
                });
            }
            Err(error) => {
                self.dialog = Some(Dialog::StoreError {
                    detail: error.to_string(),
                });
            }
        }
    }

    /// Applies `change` to the folders, which moves folder `path`; its folds follow it.
    fn change_folders(
        &mut self,
        path: &str,
        change: impl FnOnce(&mut ProfileStore) -> Result<String, FolderError>,
    ) {
        match self.store.apply(change) {
            Ok(Ok(moved)) => self.follow_folds(path, &moved),
            // The menu offers only the moves that can be made.
            Ok(Err(_)) => {}
            Err(error) => {
                self.dialog = Some(Dialog::StoreError {
                    detail: error.to_string(),
                });
            }
        }
    }

    /// The folds of folder `path` and those it holds, carried to its new path `to`.
    pub(super) fn follow_folds(&mut self, path: &str, to: &str) {
        let closed: Vec<String> = self.closed_folders.iter().cloned().collect();
        for key in closed {
            if key != NO_FOLDER && folder::is_within(&key, path) {
                self.closed_folders.remove(&key);
                self.closed_folders.insert(folder::relabel(&key, path, to));
            }
        }
    }

    /// Deletes folder `path` as asked; its profiles go to no folder.
    pub(super) fn confirm_delete_folder(&mut self, path: &str) {
        if let Err(error) = self.store.apply(|store| store.delete_folder(path)) {
            self.dialog = Some(Dialog::StoreError {
                detail: error.to_string(),
            });
        }
    }

    /// Opens every session folder `path` holds, as asked.
    pub(super) fn confirm_connect_folder(&mut self, path: &str) -> Vec<Effect> {
        let targets: Vec<_> = held(self.profile_summaries(), path)
            .into_iter()
            .filter(connectable)
            .map(|profile| profile.id)
            .collect();
        targets
            .iter()
            .flat_map(|id| self.connect_profile(id))
            .collect()
    }
}
