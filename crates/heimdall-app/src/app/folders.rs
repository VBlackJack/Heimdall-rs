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
    },
    /// A profile.
    Profile {
        /// The profile.
        profile: ProfileSummary,
        /// How deep: its folder's depth plus one.
        depth: usize,
    },
}

/// The parts of a folder path: trimmed, the empty ones dropped.
#[must_use]
pub(super) fn folder_parts(path: &str) -> Vec<&str> {
    path.split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

/// A folder path written the one way: its parts joined by `/`.
#[must_use]
pub(super) fn normal_folder(path: &str) -> String {
    folder_parts(path).join("/")
}

/// A folder and what it holds.
#[derive(Debug, Default)]
struct Node {
    folders: BTreeMap<(String, String), Node>,
    profiles: Vec<ProfileSummary>,
}

impl Node {
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
    /// The rows of the tree, searched for `search`: all of them when it is empty.
    #[must_use]
    pub fn tree_rows(&self, search: &str) -> Vec<TreeRow> {
        let searching = !search.trim().is_empty();
        let mut root = Node::default();
        let mut loose = Vec::new();
        for profile in self.profile_summaries() {
            if !profile.matches(search) {
                continue;
            }
            let parts = profile
                .group
                .as_deref()
                .map(folder_parts)
                .unwrap_or_default();
            if parts.is_empty() {
                loose.push(profile);
            } else {
                root.at(&parts).profiles.push(profile);
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

    /// Opens folder `path` if closed, closes it if open.
    pub(super) fn toggle_folder(&mut self, path: &str) {
        let key = if path == NO_FOLDER {
            NO_FOLDER.to_owned()
        } else {
            normal_folder(path)
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
