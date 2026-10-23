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

//! The library's export file and the runbooks it carries, as the C#
//! `CommandLibraryTransferService.cs` writes and reads it: the envelope of actions with an
//! optional `runbooks` array (`RunbookTransferModel`). A file exported here imports into the
//! C# build and the reverse.
//!
//! An import is untrusted. [`precheck`] holds the file to its size, its schema version and its
//! counts before anything is kept; every action then goes through
//! `UserLibrary::import` and every runbook through [`import_runbooks`], which apply the rules
//! the editors apply (control and invisible characters refused, lengths and counts bounded,
//! the values of an action step through the hardened generator, an unreadable platform read
//! as Linux). Nothing is run.

use serde_json::{Map, Value};

use super::command_library::CatalogueEntry;
use super::runbooks::{
    ActionLookup, KeepError, MAX_RUNBOOKS, Runbook, RunbookIssue, RunbookStore, is_hidden,
    new_identity,
};
use super::user_commands::{LibraryError, MAX_FILE_BYTES, MAX_TITLE_LENGTH, UserCommand, now_text};

/// The version of the envelope, as the `schemaVersion` of the C# export.
pub const SCHEMA_VERSION: &str = "1.0";

/// The major version of [`SCHEMA_VERSION`], the one this build reads.
const SUPPORTED_MAJOR: &str = "1";

/// What separates the major version from the rest in `schemaVersion`.
const VERSION_SEPARATOR: char = '.';

/// The keys of the envelope.
mod key {
    pub const SCHEMA_VERSION: &str = "schemaVersion";
    pub const EXPORT_DATE: &str = "exportDate";
    pub const TOTAL_ACTIONS: &str = "totalActions";
    pub const ACTIONS: &str = "actions";
    pub const RUNBOOKS: &str = "runbooks";
    pub const NAME: &str = "name";
}

/// An action as the export writes it.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportAction(Value);

/// An action of the user as the export writes it.
#[must_use]
pub fn action_value(action: &UserCommand) -> ExportAction {
    ExportAction(serde_json::to_value(action).unwrap_or_default())
}

/// A shipped action as the export writes it, its translations included.
#[must_use]
pub fn shipped_value(entry: &CatalogueEntry) -> ExportAction {
    ExportAction(entry.to_value())
}

/// The envelope of an export: `actions`, and `runbooks` only when there are some, so a file
/// without runbooks reads exactly as it did before they existed (as the C# writes it).
#[must_use]
pub fn export_text(actions: &[ExportAction], runbooks: &[Runbook]) -> String {
    let mut envelope = Map::new();
    envelope.insert(key::SCHEMA_VERSION.to_owned(), Value::from(SCHEMA_VERSION));
    envelope.insert(key::EXPORT_DATE.to_owned(), Value::from(now_text()));
    envelope.insert(key::TOTAL_ACTIONS.to_owned(), Value::from(actions.len()));
    envelope.insert(
        key::ACTIONS.to_owned(),
        Value::Array(actions.iter().map(|action| action.0.clone()).collect()),
    );
    if !runbooks.is_empty() {
        envelope.insert(
            key::RUNBOOKS.to_owned(),
            Value::Array(runbooks.iter().map(Runbook::to_value).collect()),
        );
    }
    serde_json::to_string_pretty(&Value::Object(envelope)).unwrap_or_default()
}

/// Holds the file `json` to what an import takes before anything is kept: its size, that it
/// is an object of a major version this build reads, and at most [`MAX_RUNBOOKS`] runbooks.
/// The runbooks it carries, unread (each is judged by [`import_runbooks`]).
///
/// # Errors
///
/// [`LibraryError::TooLarge`], [`LibraryError::Format`], [`LibraryError::Version`] or
/// [`LibraryError::TooManyRunbooks`].
pub fn precheck(json: &str) -> Result<FileRunbooks, LibraryError> {
    if u64::try_from(json.len()).map_or(true, |size| size > MAX_FILE_BYTES) {
        return Err(LibraryError::TooLarge);
    }
    let root: Value = serde_json::from_str(json).map_err(|_| LibraryError::Format)?;
    let envelope = root.as_object().ok_or(LibraryError::Format)?;
    if let Some(version) = envelope.get(key::SCHEMA_VERSION) {
        let version = version.as_str().ok_or(LibraryError::Format)?;
        let major = version.split(VERSION_SEPARATOR).next().unwrap_or_default();
        if major != SUPPORTED_MAJOR {
            return Err(LibraryError::Version(shown_text(version)));
        }
    }
    match envelope.get(key::RUNBOOKS) {
        None | Some(Value::Null) => Ok(FileRunbooks::default()),
        Some(Value::Array(listed)) if listed.len() > MAX_RUNBOOKS => {
            Err(LibraryError::TooManyRunbooks)
        }
        Some(Value::Array(listed)) => Ok(FileRunbooks(listed.clone())),
        Some(_) => Err(LibraryError::Format),
    }
}

/// The runbooks a file carries, as it wrote them: not yet read, each is judged by
/// [`import_runbooks`]. Opaque to the callers, who only pass it on.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FileRunbooks(Vec<Value>);

impl std::ops::Deref for FileRunbooks {
    type Target = [Value];

    fn deref(&self) -> &[Value] {
        &self.0
    }
}

/// Why a runbook of a file was left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunbookRefusal {
    /// It breaks a rule of the editor.
    Issue(RunbookIssue),
    /// The store already holds as many runbooks as it can.
    Full,
    /// An earlier runbook of the same file has this identity: the first is kept.
    DuplicateInFile,
}

/// What an import of runbooks did, as the C# merge counts it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunbookImport {
    /// Runbooks added.
    pub imported: usize,
    /// Runbooks of the user replaced: the file carried one of their identity.
    pub updated: usize,
    /// Runbooks left out.
    pub skipped: usize,
    /// The name of the first runbook replaced, safe to show.
    pub first_updated: Option<String>,
    /// The name of the first runbook left out and why.
    pub first_skipped: Option<(String, RunbookRefusal)>,
}

/// Merges the runbooks `values` of a file into `store` by public identity, as the C#
/// `MergeRunbooksAsync`: one the user has is replaced in place (it keeps its identity, and
/// is counted as updated), a new one is added, one that breaks a rule is left out. Every
/// runbook is checked with the rules of the editor against `lookup`, which should already
/// know the actions of the same file. Nothing is written: the caller keeps the store.
pub fn import_runbooks(
    store: &mut RunbookStore,
    values: &[Value],
    lookup: &ActionLookup<'_>,
) -> RunbookImport {
    let mut report = RunbookImport::default();
    let mut seen: Vec<String> = Vec::new();
    for value in values {
        let parsed = Runbook::from_value(value)
            .and_then(|runbook| runbook.validate(lookup).map(|()| runbook));
        let mut runbook = match parsed {
            Ok(runbook) => runbook,
            Err(issue) => {
                skip(&mut report, value, RunbookRefusal::Issue(issue));
                continue;
            }
        };
        // An empty identity (the C# `Guid.Empty`) names nothing: the runbook is a new one.
        if bare(&runbook.public_id).chars().all(|c| c == '0') {
            runbook.public_id = new_identity();
        }
        // One file naming an identity twice replaces nothing twice: the first is kept.
        if seen.contains(&bare(&runbook.public_id)) {
            skip(&mut report, value, RunbookRefusal::DuplicateInFile);
            continue;
        }
        let known = store
            .runbooks()
            .iter()
            .find(|each| bare(&each.public_id) == bare(&runbook.public_id))
            .map(|each| each.public_id.clone());
        let replaced = known.is_some();
        if let Some(identity) = known {
            runbook.public_id = identity;
        }
        let identity = bare(&runbook.public_id);
        match store.keep(runbook, lookup) {
            Ok(()) if replaced => {
                report.updated += 1;
                report
                    .first_updated
                    .get_or_insert_with(|| shown_text(&name_of(value)));
                seen.push(identity);
            }
            Ok(()) => {
                report.imported += 1;
                seen.push(identity);
            }
            Err(KeepError::Refused(issue)) => {
                skip(&mut report, value, RunbookRefusal::Issue(issue));
            }
            Err(KeepError::TooMany) => skip(&mut report, value, RunbookRefusal::Full),
        }
    }
    report
}

/// An identity without its hyphens, so the plain and the hyphenated writings of one GUID
/// are the same runbook.
fn bare(identity: &str) -> String {
    identity.chars().filter(|c| *c != '-').collect()
}

/// Counts a runbook left out, remembering the first one.
fn skip(report: &mut RunbookImport, value: &Value, why: RunbookRefusal) {
    report.skipped += 1;
    if report.first_skipped.is_none() {
        report.first_skipped = Some((shown_text(&name_of(value)), why));
    }
}

/// The name a file wrote for a runbook.
fn name_of(value: &Value) -> String {
    value
        .get(key::NAME)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Text from a file made safe to show: no control or invisible character, cut to the length
/// of a title.
#[must_use]
pub fn shown_text(text: &str) -> String {
    text.chars()
        .filter(|c| !is_hidden(*c))
        .take(MAX_TITLE_LENGTH)
        .collect()
}

#[cfg(test)]
mod tests;
