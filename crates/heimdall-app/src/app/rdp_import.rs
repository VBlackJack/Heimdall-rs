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

//! "Import .rdp files", as the C# Heimdall's: files picked or dropped on the window, a
//! preview of each with what it carries and what it does not, a choice for each name a
//! profile already has, then the profiles written.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use heimdall_core::import::rdp_file::{self, Conflict, Patch, Refusal};
use heimdall_core::metadata::ProfileOrigin;
use heimdall_core::profile::{ProfileId, RdpProfile};

use super::{App, Dialog, Effect};

/// Extension of the files the import reads, as the C# filters them.
pub const RDP_EXTENSION: &str = "rdp";

/// The words the import writes into names, in the user's language: a renamed profile's
/// template, with `{name}` and `{n}` in it, and the name of a file that gives none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdpNames {
    /// As the C# "{0} (Imported {1})".
    pub rename: String,
    /// As the C# "Imported RDP".
    pub fallback: String,
}

/// A file of the preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdpRow {
    /// Where it was read.
    pub source: PathBuf,
    /// The name it proposes.
    pub name: String,
    /// What it gives, or why it gives nothing.
    pub patch: Result<Patch, Refusal>,
    /// Chosen for the import: every file that gives a profile, at first.
    pub chosen: bool,
    /// The profile, or file of the same batch, whose name it has.
    pub conflict_with: Option<String>,
    /// What to do about it: auto-rename at first, as in the C#.
    pub conflict: Conflict,
    /// A stored password was there, and is not imported.
    pub password: bool,
    /// Settings of the file the profile does not carry.
    pub partial: bool,
    /// Keys not known.
    pub unknown: usize,
}

/// The preview of the files read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdpPreview {
    /// The files, in the order read.
    pub rows: Vec<RdpRow>,
    /// Files that could not be read, with why.
    pub unreadable: Vec<String>,
    /// The words for names.
    pub names: RdpNames,
}

impl RdpPreview {
    /// Chosen rows, all rows, rows in conflict, rows with a password: the C# summary.
    #[must_use]
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let count =
            |test: &dyn Fn(&RdpRow) -> bool| self.rows.iter().filter(|row| test(row)).count();
        (
            count(&|row| row.chosen),
            self.rows.len(),
            count(&|row| row.conflict_with.is_some()),
            count(&|row| row.password),
        )
    }

    /// Whether the import has something to do.
    #[must_use]
    pub fn can_import(&self) -> bool {
        self.rows.iter().any(|row| row.chosen)
    }
}

/// A change to the `.rdp` import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RdpMessage {
    /// "RDP files...": pick them.
    Start,
    /// Files dropped on the window where no Files tab takes them: the `.rdp` ones are read.
    Dropped(Vec<PathBuf>),
    /// Files read, or why each could not be; added to an open preview.
    Read {
        /// Each file and its text.
        files: Vec<(PathBuf, Result<String, String>)>,
        /// The words for names.
        names: RdpNames,
    },
    /// This file chosen, or no longer.
    Choose(usize),
    /// Every file that gives a profile chosen, or none.
    ChooseAll(bool),
    /// What to do about this file's name.
    Conflict(usize, Conflict),
    /// The same for every file in conflict.
    ConflictAll(Conflict),
}

impl App {
    pub(super) fn rdp_message(&mut self, message: RdpMessage) -> Vec<Effect> {
        match message {
            RdpMessage::Start => return vec![Effect::PickRdpFiles],
            RdpMessage::Dropped(paths) => {
                let rdp: Vec<PathBuf> = paths
                    .into_iter()
                    .filter(|path| {
                        path.extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case(RDP_EXTENSION))
                    })
                    .collect();
                return if rdp.is_empty() {
                    Vec::new()
                } else {
                    vec![Effect::ReadRdpFiles(rdp)]
                };
            }
            RdpMessage::Read { files, names } => self.rdp_read(files, names),
            RdpMessage::Choose(index) => self.rdp_rows(|rows| {
                if let Some(row) = rows.get_mut(index).filter(|row| row.patch.is_ok()) {
                    row.chosen = !row.chosen;
                }
            }),
            RdpMessage::ChooseAll(on) => self.rdp_rows(|rows| {
                for row in rows.iter_mut().filter(|row| row.patch.is_ok()) {
                    row.chosen = on;
                }
            }),
            RdpMessage::Conflict(index, conflict) => self.rdp_rows(|rows| {
                if let Some(row) = rows.get_mut(index) {
                    row.conflict = conflict;
                }
            }),
            RdpMessage::ConflictAll(conflict) => self.rdp_rows(|rows| {
                for row in rows.iter_mut().filter(|row| row.conflict_with.is_some()) {
                    row.conflict = conflict;
                }
            }),
        }
        Vec::new()
    }

    fn rdp_rows(&mut self, change: impl FnOnce(&mut Vec<RdpRow>)) {
        if let Some(Dialog::RdpPreview(preview)) = &mut self.dialog {
            change(&mut preview.rows);
        }
    }

    /// Adds the files read to the preview, opening it when none is: files dropped one by
    /// one gather in one preview.
    fn rdp_read(&mut self, files: Vec<(PathBuf, Result<String, String>)>, names: RdpNames) {
        let mut preview = match self.dialog.take() {
            Some(Dialog::RdpPreview(preview)) => *preview,
            other => {
                self.dialog = other;
                if self.dialog.is_some() {
                    // Another question is open: the files wait for it to be answered.
                    return;
                }
                RdpPreview {
                    rows: Vec::new(),
                    unreadable: Vec::new(),
                    names,
                }
            }
        };
        for (source, text) in files {
            if preview.rows.iter().any(|row| row.source == source) {
                continue;
            }
            match text {
                Ok(text) => preview
                    .rows
                    .push(row_of(source, &text, &preview.names.fallback)),
                Err(detail) => preview.unreadable.push(detail),
            }
        }
        self.mark_conflicts(&mut preview.rows);
        self.dialog = Some(if preview.rows.is_empty() {
            Dialog::RdpNothing {
                unreadable: preview.unreadable,
            }
        } else {
            Dialog::RdpPreview(Box::new(preview))
        });
    }

    /// Marks the rows whose name a profile or another row of the batch has, as the C#
    /// preview; a row newly in conflict is auto-renamed at first.
    fn mark_conflicts(&self, rows: &mut [RdpRow]) {
        let existing: HashMap<String, String> = self
            .profile_summaries()
            .into_iter()
            .map(|profile| (profile.name.to_lowercase(), profile.name))
            .collect();
        let mut batch: HashMap<String, usize> = HashMap::new();
        for row in rows.iter().filter(|row| row.patch.is_ok()) {
            *batch.entry(row.name.to_lowercase()).or_default() += 1;
        }
        for row in rows.iter_mut().filter(|row| row.patch.is_ok()) {
            let key = row.name.to_lowercase();
            let with = existing
                .get(&key)
                .cloned()
                .or_else(|| (batch.get(&key) > Some(&1)).then(|| row.name.clone()));
            if row.conflict_with.is_none() && with.is_some() {
                row.conflict = Conflict::AutoRename;
            }
            row.conflict_with = with;
        }
    }

    /// Imports the chosen files of `preview`, saved before kept.
    pub(super) fn import_rdp(&mut self, preview: &RdpPreview) {
        let mut taken: HashSet<String> = self
            .profile_summaries()
            .into_iter()
            .map(|profile| profile.name.to_lowercase())
            .collect();
        let mut ids: HashSet<ProfileId> = self
            .profile_summaries()
            .into_iter()
            .map(|profile| profile.id)
            .collect();
        let mut new_id = || loop {
            let id = crate::profile_draft::new_id(&[]);
            if ids.insert(id.clone()) {
                return id;
            }
        };
        let rename = |base: &str, n: u32| {
            preview
                .names
                .rename
                .replace("{name}", base)
                .replace("{n}", &n.to_string())
        };
        let mut done = RdpOutcome::default();
        let mut written: Vec<RdpProfile> = Vec::new();
        for row in preview.rows.iter().filter(|row| row.chosen) {
            let Ok(patch) = &row.patch else {
                done.skipped += 1;
                continue;
            };
            if row.password {
                done.passwords += 1;
            }
            let key = row.name.to_lowercase();
            if !taken.contains(&key) {
                taken.insert(key);
                written.push(patch.new_profile(new_id(), row.name.clone()));
                done.imported += 1;
                continue;
            }
            // A name taken by an earlier file of the batch, not shown as a conflict, is
            // renamed: the file is never dropped for it, as in the C#.
            let conflict = if row.conflict_with.is_some() {
                row.conflict
            } else {
                Conflict::AutoRename
            };
            let replaced = (conflict == Conflict::Replace)
                .then(|| self.rdp_profile_named(&row.name, &written))
                .flatten();
            match (conflict, replaced) {
                (Conflict::Skip, _) => done.skipped += 1,
                (Conflict::Replace, Some(mut profile)) => {
                    patch.apply(&mut profile);
                    written.retain(|kept| kept.id != profile.id);
                    written.push(profile);
                    done.replaced += 1;
                }
                // Replace only turns an RDP profile into this file: a profile of another
                // protocol with the name is kept, and the file renamed.
                (Conflict::Replace | Conflict::AutoRename, _) => {
                    let name = rdp_file::auto_rename(&row.name, &taken, &rename);
                    taken.insert(name.to_lowercase());
                    written.push(patch.new_profile(new_id(), name));
                    done.imported += 1;
                    done.renamed += 1;
                }
            }
        }
        let saved = self.store.apply(|store| {
            let ids: Vec<ProfileId> = written.iter().map(|profile| profile.id.clone()).collect();
            store.merge_rdp(written);
            // As the C# marks a profile a file wrote, replaced ones included.
            for id in &ids {
                store.set_origin(id, ProfileOrigin::RdpFile);
            }
        });
        self.dialog = Some(match saved {
            Ok(()) => Dialog::RdpDone(done),
            Err(error) => Dialog::save_failed(&error),
        });
    }

    /// The RDP profile named `name`: one written earlier in this import, else a saved one.
    fn rdp_profile_named(&self, name: &str, written: &[RdpProfile]) -> Option<RdpProfile> {
        written
            .iter()
            .rev()
            .chain(self.store.rdp_profiles())
            .find(|profile| profile.name.eq_ignore_ascii_case(name))
            .cloned()
    }
}

/// What the import did, as the C# summary counts it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RdpOutcome {
    /// Profiles added, renamed ones included.
    pub imported: usize,
    /// Profiles a file was written onto.
    pub replaced: usize,
    /// Profiles added under another name.
    pub renamed: usize,
    /// Files left out: refused, or skipped for their name.
    pub skipped: usize,
    /// Stored passwords not imported.
    pub passwords: usize,
}

fn row_of(source: PathBuf, text: &str, fallback: &str) -> RdpRow {
    let file = rdp_file::parse(text);
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let patch = Patch::of(&file);
    RdpRow {
        name: rdp_file::proposed_name(&stem, &file, fallback),
        chosen: patch.is_ok(),
        partial: patch.as_ref().is_ok_and(|patch| patch.is_partial(&file)),
        patch,
        conflict_with: None,
        conflict: Conflict::AutoRename,
        password: file.has_password,
        unknown: file.unknown,
        source,
    }
}
