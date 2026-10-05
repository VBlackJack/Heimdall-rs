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

//! What the tree's drag and drop moves, as the C# tree moves it: sessions into a folder,
//! a folder into another or to the top, and the last move undone.

use heimdall_core::folder;
use heimdall_core::profile::ProfileId;

use super::folders::NO_FOLDER;
use super::{App, Dialog, Notice};

/// Where something is dropped in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropTarget {
    /// A folder, or [`NO_FOLDER`].
    Folder(String),
    /// A session: into its folder.
    Profile(ProfileId),
}

/// The last move made by a drop, to undo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum UndoMove {
    /// Sessions moved, each with the folder it was in.
    Profiles(Vec<(ProfileId, Option<String>)>),
    /// A folder moved, now at `now`, back under `was_in`, the top for an empty one.
    Folder {
        /// Where it is now.
        now: String,
        /// The folder it was in.
        was_in: String,
    },
}

impl App {
    /// The folder a drop on `target` puts things in, normalised: empty for no folder; `None`
    /// for a session no longer there.
    fn drop_folder(&self, target: &DropTarget) -> Option<String> {
        let path = match target {
            DropTarget::Folder(path) if path == NO_FOLDER => String::new(),
            DropTarget::Folder(path) => path.clone(),
            DropTarget::Profile(id) => self.profile_summary(id)?.group.unwrap_or_default(),
        };
        Some(folder::normal(&path))
    }

    /// Sessions `ids` dropped on `target`: moved into its folder, saved, the move kept to
    /// undo.
    pub(super) fn drop_profiles(&mut self, ids: &[ProfileId], target: &DropTarget) {
        let Some(to) = self.drop_folder(target) else {
            return;
        };
        let to = Some(to).filter(|path| !path.is_empty());
        let moved: Vec<(ProfileId, Option<String>)> = ids
            .iter()
            .filter_map(|id| {
                let was = self.profile_summary(id)?.group;
                let was = was
                    .map(|path| folder::normal(&path))
                    .filter(|path| !path.is_empty());
                (was != to).then(|| (id.clone(), was))
            })
            .collect();
        if moved.is_empty() {
            return;
        }
        let saved = self.store.apply(|store| {
            for (id, _) in &moved {
                store.set_group(id, to.clone());
            }
        });
        if let Err(error) = saved {
            self.dialog = Some(Dialog::StoreError {
                detail: error.to_string(),
            });
            return;
        }
        self.tell(Notice::DroppedProfiles {
            count: moved.len(),
            folder: to,
        });
        self.last_move = Some(UndoMove::Profiles(moved));
    }

    /// Folder `path` dropped on `target`: moved into its folder, the top for no folder, as
    /// "Move to" moves it; the move kept to undo. A folder dropped into itself, or one of its
    /// own, does not move.
    pub(super) fn drop_folder_on(&mut self, path: &str, target: &DropTarget) {
        let Some(to) = self.drop_folder(target) else {
            return;
        };
        let was_in = folder::parent(path);
        if folder::normal(&to) == folder::normal(&was_in) || folder::is_within(&to, path) {
            return;
        }
        let moved = self.store.apply(|store| store.move_folder(path, &to));
        match moved {
            Ok(Ok(now)) => {
                self.follow_folds(path, &now);
                self.tell(Notice::DroppedFolder(folder::name(path)));
                self.last_move = Some(UndoMove::Folder { now, was_in });
            }
            // A folder of that name there already: nothing moves.
            Ok(Err(_)) => self.tell(Notice::DropRefused),
            Err(error) => {
                self.dialog = Some(Dialog::StoreError {
                    detail: error.to_string(),
                });
            }
        }
    }

    /// Undoes the last move a drop made, as the C# tree's Ctrl+Z; once.
    pub(super) fn undo_move(&mut self) {
        let Some(last) = self.last_move.take() else {
            self.tell(Notice::NothingToUndo);
            return;
        };
        let undone = match &last {
            UndoMove::Profiles(moved) => self
                .store
                .apply(|store| {
                    for (id, was) in moved {
                        store.set_group(id, was.clone());
                    }
                })
                .map(|()| true),
            UndoMove::Folder { now, was_in } => {
                let back = self.store.apply(|store| store.move_folder(now, was_in));
                match back {
                    Ok(Ok(at)) => {
                        self.follow_folds(now, &at);
                        Ok(true)
                    }
                    Ok(Err(_)) => Ok(false),
                    Err(error) => Err(error),
                }
            }
        };
        match undone {
            Ok(true) => self.tell(Notice::MoveUndone),
            Ok(false) => self.tell(Notice::NothingToUndo),
            Err(error) => {
                self.dialog = Some(Dialog::StoreError {
                    detail: error.to_string(),
                });
            }
        }
    }
}
