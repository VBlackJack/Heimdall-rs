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

//! Folding the tabs of a folder away behind their chip, as the C# folder groups
//! (`ConnectionViewModel.TabGroups.cs`): a view state of the running application, like pinning,
//! never saved. The group of a tab is derived from its profile's folder every time, so a fold
//! is only the set of folder keys the user folded.

use super::{App, Effect, Tab};
use crate::ids::TabId;

impl App {
    /// The key a tab's folder is folded under: its folder path without case, as the tree and
    /// the strip's gathering compare folders. `None` for a tab in no group.
    fn fold_key(&self, tab: &Tab) -> Option<String> {
        self.tab_folder(tab).map(|folder| folder.to_lowercase())
    }

    /// Whether the folder group of `tab` is folded away behind its chip: its tabs leave the
    /// strip, the chip standing alone for them.
    #[must_use]
    pub fn folder_folded(&self, tab: &Tab) -> bool {
        self.fold_key(tab)
            .is_some_and(|key| self.folded_tab_folders.contains(&key))
    }

    /// Whether the tab shown is in the folder group `key`.
    fn shown_in_folder(&self, key: &str) -> bool {
        self.shown_tab()
            .is_some_and(|shown| self.fold_key(shown).as_deref() == Some(key))
    }

    /// Whether the strip holds a folder group other than the one of `tab_id`, as the C#
    /// `HasOtherTabGroup`.
    #[must_use]
    pub fn has_other_folder(&self, tab_id: TabId) -> bool {
        let Some(key) = self.tab(tab_id).and_then(|tab| self.fold_key(tab)) else {
            return false;
        };
        self.strip()
            .into_iter()
            .filter_map(|each| self.fold_key(each))
            .any(|other| other != key)
    }

    /// Whether a click on the chip of the folder group of `tab_id` does something: unfolding
    /// always does; folding does unless the group holds the tab shown and every reachable tab
    /// is in it, as the C# chip says ("only tabs open") when it stays open.
    #[must_use]
    pub fn can_fold_folder(&self, tab_id: TabId) -> bool {
        let Some(key) = self.tab(tab_id).and_then(|tab| self.fold_key(tab)) else {
            return false;
        };
        self.folded_tab_folders.contains(&key)
            || !self.shown_in_folder(&key)
            || self.nearest_tab_outside(&key).is_some()
    }

    /// The tabs Ctrl+Tab and the ordinals can reach: the strip's, the tabs of a folded folder
    /// skipped whole, as the C# `NextVisibleTab`, because landing on one would unfold the
    /// folder and undo the fold.
    #[must_use]
    pub fn reachable_tabs(&self) -> Vec<TabId> {
        self.strip()
            .into_iter()
            .filter(|tab| !self.folder_folded(tab))
            .map(|tab| tab.id)
            .collect()
    }

    /// The tab to show when the folder `key` folds away: the first reachable tab after its
    /// group, else the last one before it; `None` when every reachable tab is in the group.
    fn nearest_tab_outside(&self, key: &str) -> Option<TabId> {
        let strip = self.strip();
        let inside = |tab: &Tab| self.fold_key(tab).as_deref() == Some(key);
        let first = strip.iter().position(|tab| inside(tab))?;
        let last = strip.iter().rposition(|tab| inside(tab))?;
        let reachable = |tab: &&&Tab| !inside(tab) && !self.folder_folded(tab);
        strip[last + 1..]
            .iter()
            .find(reachable)
            .or_else(|| strip[..first].iter().rev().find(reachable))
            .map(|tab| tab.id)
    }

    /// Folds the folder group of `tab_id` behind its chip, or unfolds it, as the C#
    /// `ToggleTabGroupCollapsed`. Folding the group that holds the tab shown first shows the
    /// nearest tab outside it; with none, the group stays open, as a strip with nothing to
    /// show would leave the page on a tab the strip hides.
    pub(super) fn toggle_folder_fold(&mut self, tab_id: TabId) -> Vec<Effect> {
        let Some(key) = self.tab(tab_id).and_then(|tab| self.fold_key(tab)) else {
            return Vec::new();
        };
        if self.folded_tab_folders.remove(&key) {
            return Vec::new();
        }
        let mut effects = Vec::new();
        if self.shown_in_folder(&key) {
            let Some(fallback) = self.nearest_tab_outside(&key) else {
                return Vec::new();
            };
            effects = self.select_tab(fallback);
        }
        self.folded_tab_folders.insert(key);
        effects
    }

    /// Folds every folder group but the one of `tab_id`, which is opened, as the C#
    /// `CollapseOtherTabGroups`: the tab shown moves into the folder first when it was
    /// elsewhere, so no folded folder holds it. Tabs in no folder stay on the strip.
    pub(super) fn fold_other_folders(&mut self, tab_id: TabId) -> Vec<Effect> {
        let Some(key) = self.tab(tab_id).and_then(|tab| self.fold_key(tab)) else {
            return Vec::new();
        };
        let mut effects = Vec::new();
        if !self.shown_in_folder(&key) {
            effects = self.select_tab(tab_id);
        }
        self.folded_tab_folders.remove(&key);
        let others: Vec<String> = self
            .strip()
            .into_iter()
            .filter_map(|tab| self.fold_key(tab))
            .filter(|other| *other != key)
            .collect();
        self.folded_tab_folders.extend(others);
        effects
    }

    /// Keeps the folds true to the strip after every change, as the C# does when the selection
    /// or the tabs change: a tab that is shown is never folded away, so showing one unfolds its
    /// folder; a folder with no tab left forgets it was folded, so its next tab opens afresh.
    pub(super) fn settle_tab_folds(&mut self) {
        if self.folded_tab_folders.is_empty() {
            return;
        }
        let present: std::collections::HashSet<String> = self
            .strip()
            .into_iter()
            .filter_map(|tab| self.fold_key(tab))
            .collect();
        let shown = self.shown_tab().and_then(|tab| self.fold_key(tab));
        self.folded_tab_folders
            .retain(|key| present.contains(key) && shown.as_ref() != Some(key));
    }
}
