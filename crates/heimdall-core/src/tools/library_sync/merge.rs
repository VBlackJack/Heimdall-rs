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

//! The runbooks of a pulled file against the runbooks of the user.
//!
//! The C# keeps a runbook as a batch with an `UpdatedAt`, and the sync import keeps the local
//! copy when it is newer (`JsonSyncService.cs:838-854`). Two fixes there shape the rule: a
//! run must not count as an edit (`e8e95001`, it writes `LastExecutedAt` alone) and a copy
//! taken from the repository must keep the repository's timestamp (`07571868`), or the next
//! sync reads it as a local edit. A runbook here has no timestamp at all, and a run writes
//! nothing into it, so neither can happen; the same question, "did the user change it since
//! the repository last agreed with it", is answered by comparing it with a digest of what it
//! was at that moment (the `bases`):
//!
//! - the repository's copy is the user's: nothing to do, the base is refreshed;
//! - the user's copy is as it was at the last agreement: the repository's replaces it;
//! - the repository's copy is as it was at the last agreement: the user's is kept, it is a
//!   local edit that a push will carry;
//! - both changed, or there is no base (never synchronized): the user's is kept and named,
//!   as the C# names the actions it kept.
//!
//! Nothing is run; a runbook is only kept, through the same checks as an import.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::tools::library_transfer::{self, RunbookRefusal, shown_text};
use crate::tools::runbooks::{ActionLookup, Runbook, RunbookStore};

/// FNV-1a, 64 bits: a digest to tell two texts apart, kept in a file, so it must not change
/// with the compiler (`std`'s hashers do). It guards nothing against an attacker.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// A digest of what a runbook says.
#[must_use]
pub fn digest(runbook: &Runbook) -> u64 {
    let text = serde_json::to_string(&runbook.to_value()).unwrap_or_default();
    text.bytes().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    })
}

/// The digests of `runbooks`, by identity: what the repository agrees with after a push wrote
/// them all.
#[must_use]
pub fn bases_of_runbooks(runbooks: &[Runbook]) -> BTreeMap<String, u64> {
    runbooks
        .iter()
        .map(|runbook| (key_of(&runbook.public_id), digest(runbook)))
        .collect()
}

/// [`bases_of_runbooks`] for the runbooks of a store.
#[must_use]
pub fn bases_of(store: &RunbookStore) -> BTreeMap<String, u64> {
    bases_of_runbooks(store.runbooks())
}

/// An identity without its hyphens: the plain and the hyphenated writings of a GUID are one.
fn key_of(identity: &str) -> String {
    identity.chars().filter(|c| *c != '-').collect()
}

/// What merging the runbooks of a pulled file did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunbookMerge {
    /// Runbooks added.
    pub imported: usize,
    /// Runbooks of the user replaced by the repository's newer copy.
    pub updated: usize,
    /// Runbooks of the file left out: they break a rule of an untrusted file.
    pub skipped: usize,
    /// The name of the first runbook left out and why.
    pub first_skipped: Option<(String, RunbookRefusal)>,
    /// The names of the runbooks the user changed that the repository changed too: kept.
    pub kept_local: Vec<String>,
}

impl RunbookMerge {
    /// Whether a runbook of the store changed, which the caller must then write.
    #[must_use]
    pub const fn changed(&self) -> bool {
        self.imported + self.updated > 0
    }
}

/// Merges the runbooks `values` of a pulled file into `store` by identity; `bases` is updated
/// for every runbook the repository now agrees with. Nothing is written: the caller keeps the
/// store, and the bases only if it did.
pub fn merge_runbooks(
    store: &mut RunbookStore,
    values: &[Value],
    lookup: &ActionLookup<'_>,
    bases: &mut BTreeMap<String, u64>,
) -> RunbookMerge {
    let mut report = RunbookMerge::default();
    for value in values {
        // A runbook that breaks a rule is judged (and counted as left out) by the import.
        let Some(remote) = Runbook::from_value(value)
            .ok()
            .filter(|runbook| runbook.validate(lookup).is_ok())
        else {
            take(&mut report, store, value, lookup, bases);
            continue;
        };
        let key = key_of(&remote.public_id);
        let local = store
            .runbooks()
            .iter()
            .find(|each| key_of(&each.public_id) == key)
            .cloned();
        // An empty identity (the C# `Guid.Empty`) names nothing: a new runbook.
        let named = !key.chars().all(|c| c == '0');
        let Some(local) = local.filter(|_| named) else {
            take(&mut report, store, value, lookup, bases);
            continue;
        };
        let remote_digest = digest(&remote);
        if local == remote {
            bases.insert(key, remote_digest);
            continue;
        }
        let local_digest = digest(&local);
        match bases.get(&key).copied() {
            Some(base) if base == local_digest => {
                take(&mut report, store, value, lookup, bases);
            }
            Some(base) if base == remote_digest => {}
            _ => report.kept_local.push(shown_text(&local.name)),
        }
    }
    report
}

/// Imports one runbook of the file through the checks of an import, and records what the
/// repository now agrees with.
fn take(
    report: &mut RunbookMerge,
    store: &mut RunbookStore,
    value: &Value,
    lookup: &ActionLookup<'_>,
    bases: &mut BTreeMap<String, u64>,
) {
    let done = library_transfer::import_runbooks(store, std::slice::from_ref(value), lookup);
    report.imported += done.imported;
    report.updated += done.updated;
    report.skipped += done.skipped;
    if report.first_skipped.is_none() {
        report.first_skipped = done.first_skipped;
    }
    if done.imported + done.updated > 0
        && let Ok(runbook) = Runbook::from_value(value)
    {
        let key = key_of(&runbook.public_id);
        // The store may have given an empty identity a new one: the digest is of the copy kept.
        if let Some(kept) = store
            .runbooks()
            .iter()
            .find(|each| key_of(&each.public_id) == key)
        {
            bases.insert(key, digest(kept));
        }
    }
}
