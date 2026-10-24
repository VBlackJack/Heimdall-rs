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

//! The optional Windows workstation debloat pack (`data/packs`), copied from the C# build:
//! it is never part of the shipped catalogue, and imported by the library's Import button it
//! goes through the untrusted path, where a destructive command is marked dangerous.

use std::path::PathBuf;

use heimdall_app::dangerous_command;
use heimdall_core::tools::command_library;
use heimdall_core::tools::library_transfer;
use heimdall_core::tools::user_commands::{Criticality, UserLibrary};

/// The actions the C# pack holds.
const PACK_ACTIONS: usize = 72;

/// The action of the pack the hardened import refuses: the default value of its `Path`
/// parameter holds parentheses (`Program Files (x86)`), which the value generator will not
/// put in a command. The pack stays as the C# ships it; the refusal is the import doing its job.
const REFUSED_TITLE: &str = "Configurer exclusions Windows Defender";

/// Actions the pack marks dangerous itself (level 2 in the C# file).
const PACK_DANGEROUS: usize = 12;

/// Dangerous after the import: the pack's own, and the one more whose command the paste
/// guard's list of destructive commands recognizes.
const DANGEROUS_AFTER_IMPORT: usize = 13;

fn pack_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/packs/windows-workstation-debloat.json");
    std::fs::read_to_string(path).expect("the pack is in the repository")
}

/// The identities of the pack's actions.
fn pack_ids(text: &str) -> Vec<String> {
    let root: serde_json::Value = serde_json::from_str(text).expect("the pack is JSON");
    root["actions"]
        .as_array()
        .expect("the pack holds actions")
        .iter()
        .map(|action| action["id"].as_str().expect("an identity").to_owned())
        .collect()
}

#[test]
fn the_pack_is_not_in_the_shipped_catalogue() {
    let ids = pack_ids(&pack_text());
    assert_eq!(ids.len(), PACK_ACTIONS);
    assert!(
        ids.iter()
            .all(|id| command_library::shipped().get(id).is_none()),
        "the pack is only ever imported by the user"
    );
}

#[test]
fn the_pack_imports_through_the_untrusted_path_but_for_one_default() {
    let text = pack_text();
    library_transfer::precheck(&text).expect("the envelope is accepted");
    let mut library = UserLibrary::default();
    let report = library.import(&text).expect("the pack is read");
    assert_eq!(
        (report.imported, report.updated, report.skipped),
        (PACK_ACTIONS - 1, 0, 1)
    );
    assert_eq!(
        report
            .first_skipped
            .as_ref()
            .map(|(title, _)| title.as_str()),
        Some(REFUSED_TITLE)
    );
    let ids = pack_ids(&text);
    assert!(library.commands().iter().all(|c| ids.contains(&c.id)));
}

#[test]
fn the_dangerous_actions_stay_dangerous_and_the_guard_raises_one_more() {
    let text = pack_text();
    let mut library = UserLibrary::default();
    let report = library.import(&text).unwrap();
    let dangerous = |library: &UserLibrary| {
        library
            .commands()
            .iter()
            .filter(|command| command.level == Criticality::Dangerous)
            .count()
    };
    // A level the file gives is kept as read by the import.
    let before = library.clone();
    assert_eq!(dangerous(&before), PACK_DANGEROUS);
    // The confirmation before running reads the stored level and the guard's list both; the
    // import also stores what the guard finds, whatever level the file gave.
    let raised = library.raise_dangerous(&report.touched, |pattern| {
        dangerous_command(pattern).is_some()
    });
    assert_eq!(raised, DANGEROUS_AFTER_IMPORT - PACK_DANGEROUS);
    assert_eq!(dangerous(&library), DANGEROUS_AFTER_IMPORT);
    for command in before.commands() {
        let after = library.get(&command.id).expect("kept").level;
        assert!(after >= command.level, "{} was lowered", command.id);
    }
}
