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

//! The profile tree as the C# Heimdall draws it: folders nested by their `/`-separated path,
//! each opened or closed, its sub-folders first then its profiles, by name; the profiles in
//! no folder under "(No Folder)", last. A search shows the matching profiles, every folder
//! on the way open.

use std::collections::{BTreeMap, HashSet};

use heimdall_core::folder;

use super::App;
use super::tree::ProfileSummary;

/// The key of the "(No Folder)" node among the closed folders, as the C# `::nogroup`: never
/// a folder path, which has no `:` at its start.
pub const NO_FOLDER: &str = "::nogroup";

/// A row of the profile tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeRow {
    /// A folder: [`NO_FOLDER`] for the profiles in none.
    Folder {
        /// Its full path, or [`NO_FOLDER`].
        path: String,
        /// Its own name, the last part of its path.
        name: String,
        /// How deep: 0 at the top.
        depth: usize,
        /// Its content is shown.
        open: bool,
        /// The profiles it holds, in its sub-folders too, as the C# tree counts them; only
        /// those found while searching.
        count: usize,
    },
    /// A profile.
    Profile {
        /// The profile.
        profile: ProfileSummary,
        /// How deep: its folder's depth plus one.
        depth: usize,
    },
}

/// A folder and what it holds.
#[derive(Debug, Default)]
struct Node {
    folders: BTreeMap<(String, String), Node>,
    profiles: Vec<ProfileSummary>,
}

impl Node {
    /// The profiles in this folder and under it.
    fn count(&self) -> usize {
        self.profiles.len() + self.folders.values().map(Node::count).sum::<usize>()
    }

    /// The node of `parts` under this one, made on the way.
    fn at(&mut self, parts: &[&str]) -> &mut Node {
        parts.iter().fold(self, |node, part| {
            node.folders
                .entry((part.to_lowercase(), (*part).to_owned()))
                .or_default()
        })
    }
}

impl App {
    /// The rows of the tree, searched for `search` and passed through its filters: all of
    /// them when neither leaves any out. While either does, as in the C# tree, every folder
    /// shown is open and only the folders holding a profile found are shown.
    #[must_use]
    pub fn tree_rows(&self, search: &str) -> Vec<TreeRow> {
        let searching = !search.trim().is_empty() || self.tree_filter().is_active();
        let mut root = Node::default();
        let mut loose = Vec::new();
        for profile in self.profile_summaries() {
            if !profile.matches(search)
                || !self
                    .tree_filter()
                    .accepts(&profile, self.profile_state(&profile.id))
            {
                continue;
            }
            let parts = profile
                .group
                .as_deref()
                .map(folder::parts)
                .unwrap_or_default();
            if parts.is_empty() {
                loose.push(profile);
            } else {
                root.at(&parts).profiles.push(profile);
            }
        }
        // The folders kept for themselves, empty ones included; a search shows only what it
        // found.
        if !searching {
            for path in self.store.folder_paths() {
                root.at(&folder::parts(&path));
            }
        }
        let mut rows = Vec::new();
        walk(&root, "", 0, searching, &self.closed_folders, &mut rows);
        if !loose.is_empty() {
            let open = searching || !self.closed_folders.contains(NO_FOLDER);
            rows.push(TreeRow::Folder {
                path: NO_FOLDER.to_owned(),
                name: String::new(),
                depth: 0,
                open,
                count: loose.len(),
            });
            if open {
                sort_by_name(&mut loose);
                rows.extend(
                    loose
                        .into_iter()
                        .map(|profile| TreeRow::Profile { profile, depth: 1 }),
                );
            }
        }
        rows
    }

    /// The tree's folders folded, by path, to keep how it was left.
    #[must_use]
    pub fn folded_folders(&self) -> Vec<String> {
        let mut folded: Vec<String> = self.closed_folders.iter().cloned().collect();
        folded.sort();
        folded
    }

    /// The tree as it was left: `folded` folded, and `selected` selected when it is still
    /// there.
    pub fn restore_tree(
        &mut self,
        folded: &[String],
        selected: Option<heimdall_core::profile::ProfileId>,
    ) {
        self.closed_folders = folded.iter().cloned().collect();
        if let Some(id) = selected.filter(|id| self.profile_summary(id).is_some()) {
            self.select_only(Some(id));
        }
    }

    /// Opens folder `path` if closed, closes it if open.
    pub(super) fn toggle_folder(&mut self, path: &str) {
        let key = if path == NO_FOLDER {
            NO_FOLDER.to_owned()
        } else {
            folder::normal(path)
        };
        if !self.closed_folders.remove(&key) {
            self.closed_folders.insert(key);
        }
    }
}

/// The rows under `node`, at `depth`, its path `path`.
fn walk(
    node: &Node,
    path: &str,
    depth: usize,
    searching: bool,
    closed: &HashSet<String>,
    rows: &mut Vec<TreeRow>,
) {
    for ((_, name), child) in &node.folders {
        let child_path = if path.is_empty() {
            name.clone()
        } else {
            format!("{path}/{name}")
        };
        let open = searching || !closed.contains(&child_path);
        rows.push(TreeRow::Folder {
            path: child_path.clone(),
            name: name.clone(),
            depth,
            open,
            count: child.count(),
        });
        if open {
            walk(child, &child_path, depth + 1, searching, closed, rows);
        }
    }
    let mut profiles = node.profiles.clone();
    sort_by_name(&mut profiles);
    rows.extend(
        profiles
            .into_iter()
            .map(|profile| TreeRow::Profile { profile, depth }),
    );
}

/// By name, whatever the case, as the C# tree sorts.
fn sort_by_name(profiles: &mut [ProfileSummary]) {
    profiles.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
}
