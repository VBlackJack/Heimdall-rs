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

//! A Heimdall session document picked for "Import Sessions", previewed as the C#
//! `ProfileImportService` previews it (`ProfileImportService.cs:219-309`): a row for each
//! profile, what it clashes with (a saved profile with its identifier or its name, or another
//! profile of the file with its name), a choice for each clash between skipping it, writing
//! it onto the profile it clashes with and importing it under a free name, then the rows
//! chosen applied in one save (`ProfileImportService.cs:311-430`).
//!
//! A profile replaced keeps its identifier and nothing else of what the file describes, as
//! the C# writes the file's profile in its place: its folder, its favorite star, its tags and
//! its password manager entry are the file's. What the store keeps beside the profile, by its
//! identifier, is not touched: a password saved for it stays, and is still given only to the
//! server and account it was saved for.
//!
//! Two things go further than the C#: a profile the file holds twice under one identifier is
//! shown as a clash, where the C# preview says nothing and the second is skipped; and the
//! file's own profiles the import refuses are shown as rows that cannot be chosen, with why.

use std::collections::{HashMap, HashSet, VecDeque};

use heimdall_core::import::csharp::{ImportReport, SkipReason};
use heimdall_core::import::rdp_file::{self, Conflict};
use heimdall_core::profile::ProfileId;
use heimdall_core::store::ProfileStore;

use super::{App, Dialog, ProfileKind};

/// Placeholders of the rename template, as [`super::RdpNames::rename`] holds them.
const RENAME_NAME: &str = "{name}";
const RENAME_NUMBER: &str = "{n}";

/// Keys of a C# session document, read without case as the C# reads them.
const SERVERS_KEY: &str = "servers";
const ID_KEY: &str = "id";

/// Which list of the import a profile is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum List {
    Ssh,
    Rdp,
    Telnet,
    Vnc,
    Local,
    WinRm,
    Ftp,
    Citrix,
}

impl List {
    /// The list of the store that holds profile `id`, if any.
    fn in_store(store: &ProfileStore, id: &ProfileId) -> Option<Self> {
        fn has<'a>(mut ids: impl Iterator<Item = &'a ProfileId>, id: &ProfileId) -> bool {
            ids.any(|held| held == id)
        }
        [
            (
                Self::Ssh,
                has(store.ssh_profiles().iter().map(|p| &p.id), id),
            ),
            (
                Self::Rdp,
                has(store.rdp_profiles().iter().map(|p| &p.id), id),
            ),
            (
                Self::Telnet,
                has(store.telnet_profiles().iter().map(|p| &p.id), id),
            ),
            (
                Self::Vnc,
                has(store.vnc_profiles().iter().map(|p| &p.id), id),
            ),
            (
                Self::Local,
                has(store.local_profiles().iter().map(|p| &p.id), id),
            ),
            (
                Self::WinRm,
                has(store.winrm_profiles().iter().map(|p| &p.id), id),
            ),
            (
                Self::Ftp,
                has(store.ftp_profiles().iter().map(|p| &p.id), id),
            ),
            (
                Self::Citrix,
                has(store.citrix_profiles().iter().map(|p| &p.id), id),
            ),
        ]
        .into_iter()
        .find_map(|(list, held)| held.then_some(list))
    }
}

/// Where a row's profile is in the import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Slot {
    list: List,
    index: usize,
}

/// A profile of the file, as the preview shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileImportRow {
    /// Its place in the file, from 0, as the C# source column numbers it; `None` when the
    /// file could not be read for it.
    pub position: Option<usize>,
    /// Its name, trimmed as the C# trims it.
    pub name: String,
    /// Its protocol; `None` for a profile the import refuses.
    pub kind: Option<ProfileKind>,
    /// Its host and port, for the protocols that reach a server.
    pub endpoint: Option<(String, u16)>,
    /// Why the import refuses it: it cannot be chosen.
    pub refused: Option<SkipReason>,
    /// Chosen for the import: every profile the import takes, at first, as in the C#.
    pub chosen: bool,
    /// The saved profile, or the name in the file, it clashes with.
    pub conflict_with: Option<String>,
    /// What to do about the clash: auto-rename at first, as in the C#.
    pub conflict: Conflict,
    slot: Option<Slot>,
}

impl ProfileImportRow {
    /// Whether it can be chosen: a refused profile never is, as in the C#.
    #[must_use]
    pub fn choosable(&self) -> bool {
        self.refused.is_none()
    }
}

/// The preview of a Heimdall session document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileImportPreview {
    /// The file's name, which the source column shows.
    pub source: String,
    /// The profiles, in the file's order.
    pub rows: Vec<ProfileImportRow>,
    /// The words a renamed profile takes, `{name}` and `{n}` in them.
    pub rename: String,
    report: ImportReport,
}

impl ProfileImportPreview {
    /// Chosen rows, all rows, rows in a clash: the C# summary.
    #[must_use]
    pub fn counts(&self) -> (usize, usize, usize) {
        (
            self.rows.iter().filter(|row| row.chosen).count(),
            self.rows.len(),
            self.rows
                .iter()
                .filter(|row| row.conflict_with.is_some())
                .count(),
        )
    }

    /// Whether the import has something to do.
    #[must_use]
    pub fn can_import(&self) -> bool {
        self.rows.iter().any(|row| row.chosen)
    }
}

/// What the import did with the rows chosen, as the C# summary counts it
/// (`ImportSummaryText.Profiles`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportActions {
    /// Profiles added, renamed ones included.
    pub imported: usize,
    /// Saved profiles the file's profile was written onto.
    pub replaced: usize,
    /// Profiles added under another name.
    pub renamed: usize,
    /// Profiles left out for their clash.
    pub skipped: usize,
}

/// A change to the preview of a Heimdall session document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileImportMessage {
    /// This profile chosen, or no longer.
    Choose(usize),
    /// Every profile that can be chosen, or none.
    ChooseAll(bool),
    /// What to do about this profile's clash.
    Conflict(usize, Conflict),
    /// The same for every profile in a clash.
    ConflictAll(Conflict),
}

/// What reads and renames a profile of any protocol.
trait Entry {
    fn id_mut(&mut self) -> &mut ProfileId;
    fn name_mut(&mut self) -> &mut String;
    fn row(&self) -> (String, ProfileKind, Option<(String, u16)>);
}

macro_rules! entry {
    ($type:ty, $kind:expr, $endpoint:expr) => {
        impl Entry for $type {
            fn id_mut(&mut self) -> &mut ProfileId {
                &mut self.id
            }
            fn name_mut(&mut self) -> &mut String {
                &mut self.name
            }
            fn row(&self) -> (String, ProfileKind, Option<(String, u16)>) {
                (self.name.trim().to_owned(), $kind(self), $endpoint(self))
            }
        }
    };
}

entry!(
    heimdall_core::profile::SshProfile,
    |p: &heimdall_core::profile::SshProfile| if p.sftp {
        ProfileKind::Sftp
    } else {
        ProfileKind::Ssh
    },
    |p: &heimdall_core::profile::SshProfile| Some((p.host.clone(), p.port))
);
entry!(
    heimdall_core::profile::RdpProfile,
    |_: &heimdall_core::profile::RdpProfile| ProfileKind::Rdp,
    |p: &heimdall_core::profile::RdpProfile| Some((p.host.clone(), p.port))
);
entry!(
    heimdall_core::profile::TelnetProfile,
    |_: &heimdall_core::profile::TelnetProfile| ProfileKind::Telnet,
    |p: &heimdall_core::profile::TelnetProfile| Some((p.host.clone(), p.port))
);
entry!(
    heimdall_core::profile::VncProfile,
    |_: &heimdall_core::profile::VncProfile| ProfileKind::Vnc,
    |p: &heimdall_core::profile::VncProfile| Some((p.host.clone(), p.port))
);
entry!(
    heimdall_core::profile::LocalProfile,
    |_: &heimdall_core::profile::LocalProfile| ProfileKind::Local,
    |_: &heimdall_core::profile::LocalProfile| None
);
entry!(
    heimdall_core::profile::WinRmProfile,
    |_: &heimdall_core::profile::WinRmProfile| ProfileKind::WinRm,
    |p: &heimdall_core::profile::WinRmProfile| Some((p.host.clone(), p.port))
);
entry!(
    heimdall_core::profile::FtpProfile,
    |_: &heimdall_core::profile::FtpProfile| ProfileKind::Ftp,
    |p: &heimdall_core::profile::FtpProfile| Some((p.host.clone(), p.port))
);
entry!(
    heimdall_core::profile::CitrixProfile,
    |_: &heimdall_core::profile::CitrixProfile| ProfileKind::Citrix,
    |_: &heimdall_core::profile::CitrixProfile| None
);

/// The rows of `list`, each with its place in the import.
fn rows_of<T: Entry>(list: List, profiles: &[T]) -> Vec<ProfileImportRow> {
    profiles
        .iter()
        .enumerate()
        .map(|(index, profile)| {
            let (name, kind, endpoint) = profile.row();
            ProfileImportRow {
                position: None,
                name,
                kind: Some(kind),
                endpoint,
                refused: None,
                chosen: true,
                conflict_with: None,
                conflict: Conflict::Skip,
                slot: Some(Slot { list, index }),
            }
        })
        .collect()
}

/// The profiles of `list` written, each under the identifier and name `writes` gives it;
/// the others left out.
fn written<T: Entry>(
    list: List,
    profiles: Vec<T>,
    writes: &HashMap<Slot, (ProfileId, String)>,
) -> Vec<T> {
    profiles
        .into_iter()
        .enumerate()
        .filter_map(|(index, mut profile)| {
            let (id, name) = writes.get(&Slot { list, index })?;
            *profile.id_mut() = id.clone();
            profile.name_mut().clone_from(name);
            Some(profile)
        })
        .collect()
}

/// The identifier of each profile of a C# session document, folded, in the file's order;
/// empty for one without. Nothing when the text is not such a document.
fn file_order(text: &str) -> Vec<String> {
    use serde_json::Value;
    let key = |object: &serde_json::Map<String, Value>, wanted: &str| {
        object
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
            .map(|(_, value)| value.clone())
    };
    let servers = match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
        Ok(Value::Array(servers)) => servers,
        Ok(Value::Object(document)) => match key(&document, SERVERS_KEY) {
            Some(Value::Array(servers)) => servers,
            _ => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    servers
        .iter()
        .map(|server| {
            server
                .as_object()
                .and_then(|server| key(server, ID_KEY))
                .and_then(|id| id.as_str().map(fold))
                .unwrap_or_default()
        })
        .collect()
}

/// A name or identifier compared without case, as the C# compares them.
fn fold(text: &str) -> String {
    text.trim().to_lowercase()
}

/// A profile held while the import is applied: a saved one, or one the import writes.
#[derive(Debug, Clone)]
struct Held {
    id: ProfileId,
    name: String,
    /// The row that writes it; `None` for a saved profile.
    row: Option<usize>,
}

impl App {
    /// The preview of a Heimdall session document named `source`, of text `text`, whose
    /// sessions `report` holds: a row for each, in the file's order, its clash marked.
    pub(super) fn profile_preview(
        &self,
        source: String,
        text: &str,
        rename: String,
        mut report: ImportReport,
    ) -> ProfileImportPreview {
        let mut rows = Vec::new();
        rows.extend(rows_of(List::Ssh, &report.profiles));
        rows.extend(rows_of(List::Rdp, &report.rdp));
        rows.extend(rows_of(List::Telnet, &report.telnet));
        rows.extend(rows_of(List::Vnc, &report.vnc));
        rows.extend(rows_of(List::Local, &report.local));
        rows.extend(rows_of(List::WinRm, &report.winrm));
        rows.extend(rows_of(List::Ftp, &report.ftp));
        rows.extend(rows_of(List::Citrix, &report.citrix));
        for skipped in &report.skipped {
            rows.push(ProfileImportRow {
                position: None,
                name: skipped.name.trim().to_owned(),
                kind: None,
                endpoint: None,
                refused: Some(skipped.reason.clone()),
                chosen: false,
                conflict_with: None,
                conflict: Conflict::Skip,
                slot: None,
            });
        }
        // Each row placed where its identifier is in the file, the first free place of it.
        let mut places: HashMap<String, VecDeque<usize>> = HashMap::new();
        for (position, id) in file_order(text).into_iter().enumerate() {
            places.entry(id).or_default().push_back(position);
        }
        let ids: Vec<String> = rows
            .iter()
            .map(|row| match row.slot {
                Some(slot) => fold(Self::slot_id(&mut report, slot).as_str()),
                None => String::new(),
            })
            .collect();
        let skipped_ids = report.skipped.iter().map(|skipped| fold(&skipped.id));
        let mut skipped_ids = skipped_ids.collect::<VecDeque<_>>();
        for (row, id) in rows.iter_mut().zip(ids) {
            let id = if row.slot.is_some() {
                id
            } else {
                skipped_ids.pop_front().unwrap_or_default()
            };
            row.position = places.get_mut(&id).and_then(VecDeque::pop_front);
        }
        rows.sort_by_key(|row| row.position.unwrap_or(usize::MAX));
        // The names written are the names shown: trimmed, as the C# trims them.
        for row in &rows {
            if let Some(slot) = row.slot {
                Self::slot_name(&mut report, slot).clone_from(&row.name);
            }
        }
        self.mark_profile_conflicts(&mut rows, &mut report);
        ProfileImportPreview {
            source,
            rows,
            rename,
            report,
        }
    }

    fn slot_id(report: &mut ImportReport, slot: Slot) -> &mut ProfileId {
        Self::slot_entry(report, slot).id_mut()
    }

    fn slot_name(report: &mut ImportReport, slot: Slot) -> &mut String {
        Self::slot_entry(report, slot).name_mut()
    }

    fn slot_entry(report: &mut ImportReport, slot: Slot) -> &mut dyn Entry {
        let Slot { list, index } = slot;
        match list {
            List::Ssh => &mut report.profiles[index],
            List::Rdp => &mut report.rdp[index],
            List::Telnet => &mut report.telnet[index],
            List::Vnc => &mut report.vnc[index],
            List::Local => &mut report.local[index],
            List::WinRm => &mut report.winrm[index],
            List::Ftp => &mut report.ftp[index],
            List::Citrix => &mut report.citrix[index],
        }
    }

    /// Marks the rows that clash, as the C# preview: a saved profile with the identifier,
    /// else a saved profile with the name, else another profile of the file with the name
    /// or the identifier. A row in a clash is auto-renamed at first, the others skipped
    /// should a clash appear while the import runs, as the C# sets them.
    fn mark_profile_conflicts(&self, rows: &mut [ProfileImportRow], report: &mut ImportReport) {
        let saved = self.profile_summaries();
        let saved_ids: HashMap<String, String> = saved
            .iter()
            .map(|profile| (fold(profile.id.as_str()), profile.name.clone()))
            .collect();
        let saved_names: HashMap<String, String> = saved
            .iter()
            .map(|profile| (fold(&profile.name), profile.name.clone()))
            .collect();
        let ids: Vec<Option<String>> = rows
            .iter()
            .map(|row| {
                row.slot
                    .map(|slot| fold(Self::slot_id(report, slot).as_str()))
            })
            .collect();
        let mut batch_names: HashMap<String, usize> = HashMap::new();
        let mut batch_ids: HashMap<String, usize> = HashMap::new();
        for (row, id) in rows.iter().zip(&ids) {
            if let Some(id) = id {
                *batch_names.entry(fold(&row.name)).or_default() += 1;
                *batch_ids.entry(id.clone()).or_default() += 1;
            }
        }
        for (row, id) in rows.iter_mut().zip(&ids) {
            let Some(id) = id else {
                continue;
            };
            let name = fold(&row.name);
            row.conflict_with = saved_ids
                .get(id)
                .or_else(|| saved_names.get(&name))
                .cloned()
                .or_else(|| {
                    (batch_names.get(&name) > Some(&1) || batch_ids.get(id) > Some(&1))
                        .then(|| row.name.clone())
                });
            row.conflict = if row.conflict_with.is_some() {
                Conflict::AutoRename
            } else {
                Conflict::Skip
            };
        }
    }

    pub(super) fn profile_import_message(&mut self, message: ProfileImportMessage) {
        let Some(Dialog::ProfileImportPreview(preview)) = &mut self.dialog else {
            return;
        };
        let rows = &mut preview.rows;
        match message {
            ProfileImportMessage::Choose(index) => {
                if let Some(row) = rows.get_mut(index).filter(|row| row.choosable()) {
                    row.chosen = !row.chosen;
                }
            }
            ProfileImportMessage::ChooseAll(on) => {
                for row in rows.iter_mut().filter(|row| row.choosable()) {
                    row.chosen = on;
                }
            }
            ProfileImportMessage::Conflict(index, conflict) => {
                if let Some(row) = rows.get_mut(index) {
                    row.conflict = conflict;
                }
            }
            ProfileImportMessage::ConflictAll(conflict) => {
                for row in rows.iter_mut().filter(|row| row.conflict_with.is_some()) {
                    row.conflict = conflict;
                }
            }
        }
    }

    /// The rows chosen of `preview` applied, as the C# `ApplyJsonSelectionAsync`: each in
    /// turn met against the saved profiles and those the import wrote before it, by
    /// identifier then by name; then written in one save, and what each choice did said.
    pub(super) fn import_profiles(&mut self, preview: ProfileImportPreview) {
        if !preview.can_import() {
            // As the C# button, nothing chosen imports nothing: the preview stays.
            self.dialog = Some(Dialog::ProfileImportPreview(Box::new(preview)));
            return;
        }
        let ProfileImportPreview {
            rows,
            rename,
            mut report,
            ..
        } = preview;
        let mut held: Vec<Held> = self
            .profile_summaries()
            .into_iter()
            .map(|profile| Held {
                id: profile.id,
                name: profile.name,
                row: None,
            })
            .collect();
        let mut writes: Vec<Option<(ProfileId, String)>> = vec![None; rows.len()];
        let mut replaced: HashSet<ProfileId> = HashSet::new();
        let mut actions = ImportActions::default();
        for (at, row) in rows.iter().enumerate().filter(|(_, row)| row.chosen) {
            let Some(slot) = row.slot else {
                continue;
            };
            let id = Self::slot_id(&mut report, slot).clone();
            let found = held
                .iter()
                .position(|other| fold(other.id.as_str()) == fold(id.as_str()))
                .or_else(|| {
                    held.iter()
                        .position(|other| fold(&other.name) == fold(&row.name))
                });
            let Some(found) = found else {
                let id = self.unique_import_id(&id, &held);
                held.push(Held {
                    id: id.clone(),
                    name: row.name.clone(),
                    row: Some(at),
                });
                writes[at] = Some((id, row.name.clone()));
                actions.imported += 1;
                continue;
            };
            match row.conflict {
                Conflict::Skip => actions.skipped += 1,
                Conflict::Replace => {
                    // The file's profile in the place of the one found, under its identifier.
                    let target = held[found].clone();
                    match target.row {
                        Some(earlier) => writes[earlier] = None,
                        None => {
                            replaced.insert(target.id.clone());
                        }
                    }
                    held[found] = Held {
                        id: target.id.clone(),
                        name: row.name.clone(),
                        row: Some(at),
                    };
                    writes[at] = Some((target.id, row.name.clone()));
                    actions.replaced += 1;
                }
                Conflict::AutoRename => {
                    let id = self.unique_import_id(&id, &held);
                    let taken: HashSet<String> =
                        held.iter().map(|other| fold(&other.name)).collect();
                    let name = rdp_file::auto_rename(&row.name, &taken, &|base, n| {
                        rename
                            .replace(RENAME_NAME, base)
                            .replace(RENAME_NUMBER, &n.to_string())
                    });
                    held.push(Held {
                        id: id.clone(),
                        name: name.clone(),
                        row: Some(at),
                    });
                    writes[at] = Some((id, name));
                    actions.imported += 1;
                    actions.renamed += 1;
                }
            }
        }
        let (report, replacing) = self.chosen_report(report, &rows, &writes, &replaced);
        if let Some(mut summary) = self.merge_import_replacing(report, &replacing) {
            summary.actions = Some(actions);
            self.dialog = Some(Dialog::ImportDone(summary));
        }
    }

    /// The identifier a profile of the file is written under: its own when no profile held
    /// has it, else a fresh one, as the C# `BuildUniqueId`.
    fn unique_import_id(&self, id: &ProfileId, held: &[Held]) -> ProfileId {
        let taken = |id: &ProfileId| {
            held.iter()
                .any(|other| fold(other.id.as_str()) == fold(id.as_str()))
        };
        if !id.as_str().trim().is_empty() && !taken(id) {
            return id.clone();
        }
        loop {
            let fresh = self.fresh_id();
            if !taken(&fresh) {
                return fresh;
            }
        }
    }

    /// `report` with only the profiles written, each under its identifier and name, their
    /// favorite stars and descriptions following them; and the saved profiles replaced, each
    /// with whether the file's profile is of another protocol.
    fn chosen_report(
        &self,
        mut report: ImportReport,
        rows: &[ProfileImportRow],
        writes: &[Option<(ProfileId, String)>],
        replaced: &HashSet<ProfileId>,
    ) -> (ImportReport, Vec<(ProfileId, bool)>) {
        let mut by_slot: HashMap<Slot, (ProfileId, String)> = HashMap::new();
        let mut renamed: HashMap<ProfileId, ProfileId> = HashMap::new();
        let mut names: HashSet<String> = HashSet::new();
        let mut replacing = Vec::new();
        for (row, write) in rows.iter().zip(writes) {
            let (Some(slot), Some((id, name))) = (row.slot, write) else {
                continue;
            };
            let original = Self::slot_id(&mut report, slot).clone();
            renamed.entry(original).or_insert_with(|| id.clone());
            names.insert(row.name.clone());
            if replaced.contains(id) {
                let elsewhere =
                    List::in_store(&self.store, id).is_some_and(|list| list != slot.list);
                replacing.push((id.clone(), elsewhere));
            }
            by_slot.insert(slot, (id.clone(), name.clone()));
        }
        let favorites = report
            .favorites
            .iter()
            .filter_map(|id| renamed.get(id).cloned())
            .collect();
        let metadata = std::mem::take(&mut report.metadata)
            .into_iter()
            .filter_map(|(id, metadata)| Some((renamed.get(&id)?.clone(), metadata)))
            .collect();
        let written_any = !by_slot.is_empty();
        let chosen = ImportReport {
            profiles: written(List::Ssh, report.profiles, &by_slot),
            rdp: written(List::Rdp, report.rdp, &by_slot),
            telnet: written(List::Telnet, report.telnet, &by_slot),
            vnc: written(List::Vnc, report.vnc, &by_slot),
            local: written(List::Local, report.local, &by_slot),
            winrm: written(List::WinRm, report.winrm, &by_slot),
            ftp: written(List::Ftp, report.ftp, &by_slot),
            citrix: written(List::Citrix, report.citrix, &by_slot),
            // As the C#, the file's gateways are reconciled only when a profile is written.
            gateways: if written_any {
                report.gateways
            } else {
                Vec::new()
            },
            dropped: report
                .dropped
                .into_iter()
                .filter(|dropped| names.contains(dropped.name.trim()))
                .collect(),
            host_keys: Vec::new(),
            skipped: report.skipped,
            favorites,
            metadata,
            folder_colors: report.folder_colors,
        };
        (chosen, replacing)
    }
}
