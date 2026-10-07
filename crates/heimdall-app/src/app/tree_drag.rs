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

//! What the tree's drag and drop moves, as the C# tree moves it: sessions into a folder or
//! before or after another, a folder into another or to the top, a session nudged up or
//! down; and the last change of the tree's organization undone.

use std::time::{Duration, Instant};

use heimdall_core::folder;
use heimdall_core::profile::ProfileId;

use super::folders::{NO_FOLDER, sort_in_order};
use super::{App, Dialog, Notice, ProfileSummary};

/// How long the Undo bar offers the last change, as the C# one's 30 seconds; Ctrl+Z undoes
/// it after too.
const UNDO_OFFER: Duration = Duration::from_secs(30);

/// The step between two sessions' places once a folder is put in order, as the C#
/// `SessionOrdering`: room to insert without renumbering is not relied on, but the values
/// stay those the C# writes.
const ORDER_STEP: i32 = 10;

/// Where something is dropped in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropTarget {
    /// A folder, or [`NO_FOLDER`].
    Folder(String),
    /// A session: into its folder.
    Profile(ProfileId),
    /// Just before a session, in its folder.
    Before(ProfileId),
    /// Just after a session, in its folder.
    After(ProfileId),
}

/// What changed the tree's organization, as the C# Undo bar names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizationChange {
    /// Sessions moved to another folder.
    Move,
    /// Sessions put in another order.
    Reorder,
    /// A session renamed.
    Rename,
    /// A folder moved.
    FolderMove,
    /// A folder renamed.
    FolderRename,
}

/// What places a session in the tree: its name, folder and rank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Organization {
    name: String,
    group: Option<String>,
    sort_order: Option<i32>,
}

/// The last change of the tree's organization, to undo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum UndoMove {
    /// Sessions changed, each as it was and as it was left: undone only while what changed
    /// is still as it was left, as the C# `TreeOrganizationHistory`.
    Sessions {
        /// What it was.
        change: OrganizationChange,
        /// The sessions before.
        before: Vec<(ProfileId, Organization)>,
        /// The same sessions after.
        after: Vec<(ProfileId, Organization)>,
    },
    /// A folder renamed, now at `now`, back to the name it had, `was`.
    FolderRename {
        /// Where it is now.
        now: String,
        /// Its name before.
        was: String,
    },
    /// A folder moved, now at `now`, back under `was_in`, the top for an empty one.
    Folder {
        /// Where it is now.
        now: String,
        /// The folder it was in.
        was_in: String,
    },
}

/// The folder a session is in, normalised: `None` for none.
fn folder_of(profile: &ProfileSummary) -> Option<String> {
    profile
        .group
        .as_deref()
        .map(folder::normal)
        .filter(|path| !path.is_empty())
}

impl App {
    /// The change the Undo bar offers to undo: the last one, for 30 seconds.
    #[must_use]
    pub fn undo_offer(&self) -> Option<OrganizationChange> {
        let (last, at) = self.last_move.as_ref()?;
        (at.elapsed() < UNDO_OFFER).then_some(match last {
            UndoMove::Sessions { change, .. } => *change,
            UndoMove::Folder { .. } => OrganizationChange::FolderMove,
            UndoMove::FolderRename { .. } => OrganizationChange::FolderRename,
        })
    }

    /// What places session `id` in the tree now.
    fn organization(&self, id: &ProfileId) -> Option<Organization> {
        let profile = self.profile_summary(id)?;
        Some(Organization {
            name: profile.name,
            group: profile.group,
            sort_order: profile.metadata.sort_order,
        })
    }

    /// Each of `ids` as it is placed now.
    pub(super) fn organizations(&self, ids: &[ProfileId]) -> Vec<(ProfileId, Organization)> {
        ids.iter()
            .filter_map(|id| Some((id.clone(), self.organization(id)?)))
            .collect()
    }

    /// Keeps `change` to undo: the sessions `before` it, and as they are now; nothing when
    /// none changed, the change kept before staying.
    pub(super) fn record_organization(
        &mut self,
        change: OrganizationChange,
        before: Vec<(ProfileId, Organization)>,
    ) {
        let (before, after): (Vec<_>, Vec<_>) = before
            .into_iter()
            .filter_map(|(id, was)| {
                let now = self.organization(&id)?;
                (now != was).then(|| ((id.clone(), was), (id, now)))
            })
            .unzip();
        if before.is_empty() {
            return;
        }
        self.last_move = Some((
            UndoMove::Sessions {
                change,
                before,
                after,
            },
            Instant::now(),
        ));
    }

    /// The sessions of `group`, in the tree's order, the ones a filter hides included.
    fn folder_sessions(&self, group: Option<&str>) -> Vec<ProfileSummary> {
        let mut sessions: Vec<ProfileSummary> = self
            .profile_summaries()
            .into_iter()
            .filter(|profile| folder_of(profile).as_deref() == group)
            .collect();
        sort_in_order(&mut sessions);
        sessions
    }

    /// Writes `order`, sessions of folder `group`, as their places: every tenth, as the C#
    /// `PersistOrderAsync`, only those whose place or folder changes.
    fn write_order(&mut self, group: Option<&str>, order: &[ProfileSummary]) -> bool {
        let group = group.map(str::to_owned);
        let saved = self.store.apply(|store| {
            let mut rank = 0_i32;
            for profile in order {
                rank = rank.saturating_add(ORDER_STEP);
                if folder_of(profile) != group {
                    store.set_group(&profile.id, group.clone());
                }
                if profile.metadata.sort_order != Some(rank) {
                    let metadata = heimdall_core::metadata::ProfileMetadata {
                        sort_order: Some(rank),
                        ..store.metadata(&profile.id).cloned().unwrap_or_default()
                    };
                    store.set_metadata(&profile.id, metadata);
                }
            }
        });
        if let Err(error) = saved {
            self.dialog = Some(Dialog::save_failed(&error));
            return false;
        }
        true
    }

    /// The folder a drop on `target` puts things in, normalised: empty for no folder; `None`
    /// for a session no longer there.
    fn drop_folder(&self, target: &DropTarget) -> Option<String> {
        let path = match target {
            DropTarget::Folder(path) if path == NO_FOLDER => String::new(),
            DropTarget::Folder(path) => path.clone(),
            DropTarget::Profile(id) | DropTarget::Before(id) | DropTarget::After(id) => {
                self.profile_summary(id)?.group.unwrap_or_default()
            }
        };
        Some(folder::normal(&path))
    }

    /// Sessions `ids` dropped on `target`: moved into its folder, or placed before or after
    /// a session; saved, the change kept to undo.
    pub(super) fn drop_profiles(&mut self, ids: &[ProfileId], target: &DropTarget) {
        match target {
            DropTarget::Before(anchor) => return self.reorder_profiles(ids, anchor, false),
            DropTarget::After(anchor) => return self.reorder_profiles(ids, anchor, true),
            DropTarget::Folder(_) | DropTarget::Profile(_) => {}
        }
        let Some(to) = self.drop_folder(target) else {
            return;
        };
        let to = Some(to).filter(|path| !path.is_empty());
        let moved: Vec<ProfileId> = ids
            .iter()
            .filter(|id| {
                self.profile_summary(id)
                    .is_some_and(|profile| folder_of(&profile) != to)
            })
            .cloned()
            .collect();
        if moved.is_empty() {
            return;
        }
        let before = self.organizations(&moved);
        let saved = self.store.apply(|store| {
            for id in &moved {
                store.set_group(id, to.clone());
            }
        });
        if let Err(error) = saved {
            self.dialog = Some(Dialog::save_failed(&error));
            return;
        }
        self.tell(Notice::DroppedProfiles {
            count: moved.len(),
            folder: to,
        });
        self.record_organization(OrganizationChange::Move, before);
    }

    /// Sessions `ids` placed just before `anchor`, or just after, in its folder, as the C#
    /// `ReorderServersAsync`: in the tree's order whatever the order they were picked in,
    /// moved into that folder when they were in another. Dropped on one of themselves,
    /// nothing moves.
    fn reorder_profiles(&mut self, ids: &[ProfileId], anchor: &ProfileId, after: bool) {
        if ids.contains(anchor) {
            return;
        }
        let Some(anchor_profile) = self.profile_summary(anchor) else {
            return;
        };
        let group = folder_of(&anchor_profile);
        let mut moving: Vec<ProfileSummary> = ids
            .iter()
            .filter_map(|id| self.profile_summary(id))
            .collect();
        if moving.is_empty() {
            return;
        }
        sort_moving(&mut moving);
        let mut order = self.folder_sessions(group.as_deref());
        order.retain(|profile| !ids.contains(&profile.id));
        let Some(at) = order.iter().position(|profile| &profile.id == anchor) else {
            return;
        };
        let at = at + usize::from(after);
        let count = moving.len();
        let single = (count == 1).then(|| moving[0].name.clone());
        let touched: Vec<ProfileId> = order
            .iter()
            .chain(&moving)
            .map(|profile| profile.id.clone())
            .collect();
        let before = self.organizations(&touched);
        order.splice(at..at, moving);
        if !self.write_order(group.as_deref(), &order) {
            return;
        }
        self.tell(Notice::Reordered {
            count,
            name: single,
            folder: group,
        });
        self.record_organization(OrganizationChange::Reorder, before);
    }

    /// Session `id` moved one place up, or down, in its folder, as the C# Alt+Up and
    /// Alt+Down; at either end of its folder, nothing moves: it never leaves it.
    pub(super) fn nudge_profile(&mut self, id: &ProfileId, down: bool) {
        let Some(profile) = self.profile_summary(id) else {
            return;
        };
        let group = folder_of(&profile);
        let mut order = self.folder_sessions(group.as_deref());
        let Some(at) = order.iter().position(|profile| &profile.id == id) else {
            return;
        };
        let to = if down { at + 1 } else { at.wrapping_sub(1) };
        if to >= order.len() {
            return;
        }
        let touched: Vec<ProfileId> = order.iter().map(|profile| profile.id.clone()).collect();
        let before = self.organizations(&touched);
        order.swap(at, to);
        if self.write_order(group.as_deref(), &order) {
            self.record_organization(OrganizationChange::Reorder, before);
        }
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
                self.last_move = Some((UndoMove::Folder { now, was_in }, Instant::now()));
            }
            // A folder of that name there already: nothing moves.
            Ok(Err(_)) => self.tell(Notice::DropRefused),
            Err(error) => {
                self.dialog = Some(Dialog::save_failed(&error));
            }
        }
    }

    /// Undoes the last change of the tree's organization, as the C# Undo bar and Ctrl+Z;
    /// once. Sessions changed since are left as they are, and that is said.
    pub(super) fn undo_move(&mut self) {
        let Some((last, _)) = self.last_move.take() else {
            self.tell(Notice::NothingToUndo);
            return;
        };
        let undone = match &last {
            UndoMove::Sessions { before, after, .. } => self.undo_sessions(before, after),
            // Renamed back, unless a folder of its old name came there since, or it went.
            UndoMove::FolderRename { now, was } => {
                let back = self.store.apply(|store| store.rename_folder(now, was));
                match back {
                    Ok(Ok(at)) => {
                        self.follow_folds(now, &at);
                        Ok(true)
                    }
                    Ok(Err(_)) => Ok(false),
                    Err(error) => Err(error),
                }
            }
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
            Ok(false) => self.tell(Notice::UndoConflict),
            Err(error) => {
                self.dialog = Some(Dialog::save_failed(&error));
            }
        }
    }

    /// Puts back what changed from `before` to `after`, when every field that changed is
    /// still as it was left; otherwise nothing, and `Ok(false)`.
    fn undo_sessions(
        &mut self,
        before: &[(ProfileId, Organization)],
        after: &[(ProfileId, Organization)],
    ) -> Result<bool, heimdall_core::store::StoreError> {
        let unchanged = before.iter().zip(after).all(|((id, was), (_, left))| {
            self.organization(id).is_some_and(|now| {
                (was.name == left.name || now.name == left.name)
                    && (was.group == left.group || now.group == left.group)
                    && (was.sort_order == left.sort_order || now.sort_order == left.sort_order)
            })
        });
        if !unchanged {
            return Ok(false);
        }
        self.store.apply(|store| {
            for ((id, was), (_, left)) in before.iter().zip(after) {
                if was.name != left.name {
                    store.rename_profile(id, &was.name);
                }
                if was.group != left.group {
                    store.set_group(id, was.group.clone());
                }
                if was.sort_order != left.sort_order {
                    let metadata = heimdall_core::metadata::ProfileMetadata {
                        sort_order: was.sort_order,
                        ..store.metadata(id).cloned().unwrap_or_default()
                    };
                    store.set_metadata(id, metadata);
                }
            }
        })?;
        Ok(true)
    }
}

/// Sessions picked from several places, in the tree's order: folder by folder as the tree
/// walks them, then as each folder orders them.
fn sort_moving(moving: &mut [ProfileSummary]) {
    moving.sort_by(|a, b| tree_cmp(folder_of(a).as_deref(), folder_of(b).as_deref()));
    let mut start = 0;
    while start < moving.len() {
        let group = folder_of(&moving[start]);
        let end = moving[start..]
            .iter()
            .position(|profile| folder_of(profile) != group)
            .map_or(moving.len(), |offset| start + offset);
        sort_in_order(&mut moving[start..end]);
        start = end;
    }
}

/// Which of two folders the tree walks first: by name, whatever the case; a sub-folder
/// before the sessions of the folder holding it; no folder last.
fn tree_cmp(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => {
            let (left, right): (Vec<&str>, Vec<&str>) =
                (a.split('/').collect(), b.split('/').collect());
            left.iter()
                .zip(&right)
                .map(|(x, y)| {
                    x.to_lowercase()
                        .cmp(&y.to_lowercase())
                        .then_with(|| x.cmp(y))
                })
                .find(|order| order.is_ne())
                .unwrap_or_else(|| right.len().cmp(&left.len()))
        }
    }
}
