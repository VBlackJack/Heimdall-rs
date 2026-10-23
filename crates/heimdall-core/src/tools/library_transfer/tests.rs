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

use std::collections::BTreeMap;

use super::*;
use crate::tools::command_library::{self, CommandIssue};
use crate::tools::runbooks::{FailureMode, Step, StepIssue};
use crate::tools::user_commands::{
    Draft, MAX_FILE_BYTES, MAX_IMPORT_ACTIONS, MAX_TITLE_LENGTH, TemplatePlatform, UserLibrary,
    UserPlatform,
};

/// What the C# writes for a library with one action of the user and two runbooks
/// (`CommandLibraryTransferService.ExportAsync` and `RunbookTransferModel`): camel-case keys,
/// enums by name, `Both` on an action step, a null `actionId` on a literal one, a nil
/// description, a variable whose name is not a name, and an unreadable failure mode.
const CSHARP_EXPORT: &str = r#"{
  "schemaVersion": "1.0",
  "exportDate": "2026-10-21T05:00:00.0000000Z",
  "totalActions": 1,
  "actions": [
    {
      "id": "6f1c2d3e-0000-4000-8000-0000000000a1",
      "publicId": "6f1c2d3e-0000-4000-8000-0000000000a2",
      "title": "Echo a word",
      "description": "Prints it",
      "category": "System",
      "platform": 1,
      "level": 0,
      "tags": ["demo"],
      "linuxCommandTemplate": {
        "id": "t1",
        "platform": 1,
        "name": "echo",
        "commandPattern": "echo {text}",
        "parameters": [
          { "name": "text", "label": "Text", "type": "string", "required": true, "defaultValue": "hi" }
        ]
      },
      "isUserCreated": true
    }
  ],
  "runbooks": [
    {
      "publicId": "0b0e8f2c-5a1d-4c53-9a54-2d4f3f7c9e11",
      "name": "Say it twice",
      "description": null,
      "executionMode": "ContinueOnError",
      "variables": { "word": "hello", "bad name": "x" },
      "tags": ["demo"],
      "steps": [
        { "actionId": "6f1c2d3e-0000-4000-8000-0000000000a1", "actionTitle": "Echo a word",
          "command": "echo hello", "platform": "Both", "description": null,
          "parameters": { "text": "{{word}}" } },
        { "actionId": null, "actionTitle": "", "command": "uptime",
          "platform": "Both", "description": "Then", "parameters": {} }
      ]
    },
    {
      "publicId": "7d2a1c50-0000-4000-8000-000000000001",
      "name": "Windows check",
      "executionMode": "SomethingElse",
      "steps": [ { "command": "Get-Service W32Time", "platform": "Windows" } ]
    }
  ]
}"#;

fn nothing<'a>(_: &str) -> Option<&'a UserCommand> {
    None
}

fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

fn one_literal(name: &str, command: &str) -> Runbook {
    let mut runbook = Runbook::new(name);
    runbook
        .steps
        .push(Step::literal(command, TemplatePlatform::Linux));
    runbook
}

/// A library holding `echo {text}` for Linux, and its identity.
fn library_with_echo() -> (UserLibrary, String) {
    let mut library = UserLibrary::default();
    let id = library
        .upsert(&Draft {
            title: "Echo".to_owned(),
            category: "System".to_owned(),
            platform: UserPlatform::Linux,
            linux_pattern: "echo {text}".to_owned(),
            ..Draft::default()
        })
        .expect("a command");
    (library, id)
}

/// Imports the file `text` as the pane does: precheck, then the actions, then the runbooks
/// against the library as the actions left it.
fn import_all(
    library: &mut UserLibrary,
    store: &mut RunbookStore,
    text: &str,
) -> Result<(crate::tools::user_commands::ImportReport, RunbookImport), LibraryError> {
    let values = precheck(text)?;
    let actions = library.import(text)?;
    let found = |id: &str| library.get(id);
    let runbooks = import_runbooks(store, &values, &found);
    Ok((actions, runbooks))
}

#[test]
fn what_is_exported_imports_back_with_its_runbooks() {
    let (library, id) = library_with_echo();
    let mut origin = RunbookStore::default();
    let mut runbook = Runbook::new("Say it");
    runbook.description = Some("Twice".to_owned());
    runbook.failure_mode = FailureMode::Continue;
    runbook
        .variables
        .insert("word".to_owned(), "hello".to_owned());
    runbook.tags = vec!["demo".to_owned()];
    runbook
        .steps
        .push(Step::action(&id, "Echo", values(&[("text", "{{word}}")])));
    runbook
        .steps
        .push(Step::literal("Get-Date", TemplatePlatform::Windows));
    let find = |wanted: &str| library.get(wanted);
    origin.keep(runbook.clone(), &find).expect("kept");

    let all: Vec<ExportAction> = library.commands().iter().map(action_value).collect();
    let text = export_text(&all, origin.runbooks());

    let mut copy = UserLibrary::default();
    let mut store = RunbookStore::default();
    let (actions, runbooks) = import_all(&mut copy, &mut store, &text).expect("imported");
    assert_eq!((actions.imported, actions.skipped), (1, 0));
    assert_eq!(
        (runbooks.imported, runbooks.updated, runbooks.skipped),
        (1, 0, 0)
    );
    assert_eq!(store.runbooks(), [runbook]);

    // The same file again replaces both, in place.
    let (actions, runbooks) = import_all(&mut copy, &mut store, &text).expect("imported");
    assert_eq!(actions.updated, 1);
    assert_eq!((runbooks.imported, runbooks.updated), (0, 1));
    assert_eq!(store.runbooks().len(), 1);
}

#[test]
fn an_export_without_runbooks_has_no_runbooks_key() {
    let (library, _) = library_with_echo();
    let text = library.export();
    let json: Value = serde_json::from_str(&text).expect("json");
    assert!(json.get("runbooks").is_none());
    assert_eq!(json["schemaVersion"], "1.0");
    assert_eq!(json["totalActions"], 1);

    let none = export_text(&[], &[one_literal("R", "uptime")]);
    let json: Value = serde_json::from_str(&none).expect("json");
    assert_eq!(json["totalActions"], 0);
    assert_eq!(json["runbooks"][0]["steps"][0]["platform"], "Linux");
    assert_eq!(json["runbooks"][0]["executionMode"], "StopOnError");
}

#[test]
fn a_csharp_export_imports_actions_and_runbooks_and_both_reads_as_linux() {
    let mut library = UserLibrary::default();
    let mut store = RunbookStore::default();
    let (actions, runbooks) =
        import_all(&mut library, &mut store, CSHARP_EXPORT).expect("imported");
    assert_eq!((actions.imported, actions.skipped), (1, 0));
    assert_eq!(
        (runbooks.imported, runbooks.updated, runbooks.skipped),
        (2, 0, 0)
    );
    assert_eq!(library.commands().len(), 1);

    let say = store
        .get("0b0e8f2c-5a1d-4c53-9a54-2d4f3f7c9e11")
        .expect("kept under its identity");
    assert_eq!(say.name, "Say it twice");
    assert_eq!(say.failure_mode, FailureMode::Continue);
    // The variable whose name is not a name is dropped, as the C# `ToRunbook` does.
    assert_eq!(say.variables, values(&[("word", "hello")]));
    assert_eq!(
        say.steps[0],
        Step::action(
            "6f1c2d3e-0000-4000-8000-0000000000a1",
            "Echo a word",
            values(&[("text", "{{word}}")])
        )
    );
    // A literal step written for Both reads as Linux.
    assert_eq!(
        say.steps[1],
        Step::Literal {
            command: "uptime".to_owned(),
            platform: TemplatePlatform::Linux,
            description: Some("Then".to_owned()),
        }
    );
    let windows = store
        .get("7d2a1c50-0000-4000-8000-000000000001")
        .expect("kept");
    assert_eq!(windows.failure_mode, FailureMode::Stop);
    assert_eq!(
        windows.steps,
        [Step::literal(
            "Get-Service W32Time",
            TemplatePlatform::Windows
        )]
    );

    // And what this build writes of them is what the C# reads: its own keys.
    let all: Vec<ExportAction> = library.commands().iter().map(action_value).collect();
    let back: Value = serde_json::from_str(&export_text(&all, store.runbooks())).expect("json");
    let first = back["runbooks"]
        .as_array()
        .and_then(|list| {
            list.iter()
                .find(|each| each["publicId"] == "0b0e8f2c-5a1d-4c53-9a54-2d4f3f7c9e11")
        })
        .expect("runbook");
    assert_eq!(first["executionMode"], "ContinueOnError");
    assert_eq!(
        first["steps"][0]["actionId"],
        "6f1c2d3e-0000-4000-8000-0000000000a1"
    );
    assert_eq!(first["steps"][0]["parameters"]["text"], "{{word}}");
}

#[test]
fn a_file_of_runbooks_alone_imports() {
    let text = r#"{"schemaVersion":"1.0","runbooks":[{"publicId":"0b0e8f2c-5a1d-4c53-9a54-2d4f3f7c9e11","name":"Solo","steps":[{"command":"id","platform":"Linux"}]}]}"#;
    let mut library = UserLibrary::default();
    let mut store = RunbookStore::default();
    let (actions, runbooks) = import_all(&mut library, &mut store, text).expect("imported");
    assert_eq!(actions.imported + actions.updated + actions.skipped, 0);
    assert_eq!(runbooks.imported, 1);
    assert_eq!(store.runbooks()[0].name, "Solo");
}

#[test]
fn a_file_that_breaks_the_envelope_is_refused_before_anything_is_kept() {
    let too_big = format!("{{\"pad\":\"{}\"}}", "a".repeat(11 * 1024 * 1024));
    assert!(matches!(precheck(&too_big), Err(LibraryError::TooLarge)));
    assert!(matches!(precheck("nope"), Err(LibraryError::Format)));
    assert!(matches!(precheck("[]"), Err(LibraryError::Format)));
    assert!(matches!(
        precheck(r#"{"schemaVersion":2,"actions":[]}"#),
        Err(LibraryError::Format)
    ));
    assert!(matches!(
        precheck(r#"{"schemaVersion":"2.0","runbooks":[]}"#),
        Err(LibraryError::Version(version)) if version == "2.0"
    ));
    assert!(matches!(
        precheck(r#"{"schemaVersion":"1.7","runbooks":"x"}"#),
        Err(LibraryError::Format)
    ));
    let many = format!(
        "{{\"runbooks\":[{}]}}",
        vec!["{}"; MAX_RUNBOOKS + 1].join(",")
    );
    assert!(matches!(
        precheck(&many),
        Err(LibraryError::TooManyRunbooks)
    ));
    let at_most = format!("{{\"runbooks\":[{}]}}", vec!["{}"; MAX_RUNBOOKS].join(","));
    assert_eq!(precheck(&at_most).expect("allowed").len(), MAX_RUNBOOKS);

    let actions = format!(
        "{{\"actions\":[{}]}}",
        vec![r#"{"id":"x","title":"t","category":"c"}"#; MAX_IMPORT_ACTIONS + 1].join(",")
    );
    assert!(matches!(
        UserLibrary::default().import(&actions),
        Err(LibraryError::TooManyActions)
    ));
}

#[test]
fn a_hostile_runbook_is_left_out_with_its_reason_and_the_others_are_kept() {
    let (library, id) = library_with_echo();
    let hostile = serde_json::json!({
        "runbooks": [
            { "name": "Bell\u{7}", "steps": [ { "command": "id", "platform": "Linux" } ] },
            { "name": "Hidden\u{200b}command",
              "steps": [ { "command": "id\u{202e}", "platform": "Linux" } ] },
            { "name": "Newline step",
              "steps": [ { "command": "id\nrm -rf /", "platform": "Linux" } ] },
            { "name": "Injected value",
              "steps": [ { "actionId": id, "actionTitle": "Echo",
                           "parameters": { "text": "a; rm -rf /" } } ] },
            { "name": "", "steps": [] },
            { "name": "Odd parameter",
              "steps": [ { "actionId": id, "parameters": { "te xt": "a" } } ] },
            { "name": "Steps", "steps": vec![serde_json::json!({"command":"id"}); 101] },
            "not an object",
            { "name": "Fine", "steps": [ { "actionId": id, "parameters": { "text": "ok" } } ] }
        ]
    })
    .to_string();
    let values = precheck(&hostile).expect("an envelope");
    let find = |wanted: &str| library.get(wanted);
    let mut store = RunbookStore::default();
    let report = import_runbooks(&mut store, &values, &find);
    assert_eq!((report.imported, report.updated, report.skipped), (1, 0, 8));
    assert_eq!(store.runbooks().len(), 1);
    assert_eq!(store.runbooks()[0].name, "Fine");
    // The first reason, and the name shown without the control character.
    let (name, why) = report.first_skipped.expect("a first reason");
    assert_eq!(name, "Bell");
    assert_eq!(why, RunbookRefusal::Issue(RunbookIssue::Hidden));
}

#[test]
fn each_hostile_text_is_refused_by_the_rules_of_the_editor() {
    let (library, id) = library_with_echo();
    let find = |wanted: &str| library.get(wanted);
    let refused = |runbook: Value| {
        let mut store = RunbookStore::default();
        let report = import_runbooks(&mut store, &[runbook], &find);
        assert!(store.runbooks().is_empty(), "nothing kept");
        report.first_skipped.expect("refused").1
    };
    assert_eq!(
        refused(serde_json::json!({ "name": "A", "description": "x\u{0}",
            "steps": [ { "command": "id" } ] })),
        RunbookRefusal::Issue(RunbookIssue::Hidden)
    );
    assert_eq!(
        refused(serde_json::json!({ "name": "A", "variables": { "v": "a\u{feff}" } })),
        RunbookRefusal::Issue(RunbookIssue::Hidden)
    );
    assert_eq!(
        refused(serde_json::json!({ "name": "A", "tags": ["t\u{1b}"] })),
        RunbookRefusal::Issue(RunbookIssue::Hidden)
    );
    assert!(matches!(
        refused(serde_json::json!({ "name": "A",
            "steps": [ { "command": "id" },
                       { "actionId": id, "parameters": { "text": "a; rm -rf /" } } ] })),
        RunbookRefusal::Issue(RunbookIssue::Step(2, StepIssue::Values(_)))
    ));
}

#[test]
fn a_runbook_of_a_known_identity_replaces_it_in_place_and_keeps_that_identity() {
    let mut store = RunbookStore::default();
    store
        .keep(one_literal("Mine", "uptime"), &nothing)
        .expect("kept");
    let mine = store.runbooks()[0].public_id.clone();
    store
        .keep(one_literal("Other", "id"), &nothing)
        .expect("kept");

    // The plain writing of the same GUID, upper case: the same runbook.
    let plain = mine.replace('-', "").to_ascii_uppercase();
    let file = serde_json::json!({ "runbooks": [
        { "publicId": plain, "name": "Theirs",
          "steps": [ { "command": "df", "platform": "Linux" } ] } ] })
    .to_string();
    let values = precheck(&file).expect("an envelope");
    let report = import_runbooks(&mut store, &values, &nothing);
    assert_eq!((report.imported, report.updated, report.skipped), (0, 1, 0));
    assert_eq!(store.runbooks().len(), 2);
    let replaced = store.get(&mine).expect("still under its identity");
    assert_eq!(replaced.name, "Theirs");
    assert_eq!(
        replaced.steps,
        [Step::literal("df", TemplatePlatform::Linux)]
    );
}

#[test]
fn a_runbook_without_a_usable_identity_is_added_and_never_replaces_one() {
    let mut store = RunbookStore::default();
    store
        .keep(one_literal("Mine", "uptime"), &nothing)
        .expect("kept");
    let file = serde_json::json!({ "runbooks": [
        { "publicId": "00000000-0000-0000-0000-000000000000", "name": "Nil",
          "steps": [ { "command": "a" } ] },
        { "publicId": "not a guid", "name": "Bad", "steps": [ { "command": "b" } ] },
        { "name": "None", "steps": [ { "command": "c" } ] } ] })
    .to_string();
    let values = precheck(&file).expect("an envelope");
    let report = import_runbooks(&mut store, &values, &nothing);
    assert_eq!((report.imported, report.updated, report.skipped), (3, 0, 0));
    assert_eq!(store.runbooks().len(), 4);
    let mut identities: Vec<&str> = store
        .runbooks()
        .iter()
        .map(|each| each.public_id.as_str())
        .collect();
    identities.sort_unstable();
    identities.dedup();
    assert_eq!(identities.len(), 4, "four identities");
    assert!(
        store
            .runbooks()
            .iter()
            .all(|each| !each.public_id.starts_with("00000000-0000"))
    );
}

#[test]
fn a_full_store_takes_no_new_runbook_but_still_updates() {
    let mut store = RunbookStore::default();
    for number in 0..MAX_RUNBOOKS {
        store
            .keep(one_literal(&format!("R{number}"), "id"), &nothing)
            .expect("kept");
    }
    let known = store.runbooks()[0].public_id.clone();
    let file = serde_json::json!({ "runbooks": [
        { "name": "New", "steps": [ { "command": "a" } ] },
        { "publicId": known, "name": "Again", "steps": [ { "command": "b" } ] } ] })
    .to_string();
    let values = precheck(&file).expect("an envelope");
    let report = import_runbooks(&mut store, &values, &nothing);
    assert_eq!((report.imported, report.updated, report.skipped), (0, 1, 1));
    assert_eq!(
        report.first_skipped,
        Some(("New".to_owned(), RunbookRefusal::Full))
    );
    assert_eq!(store.runbooks().len(), MAX_RUNBOOKS);
}

#[test]
fn an_export_of_everything_does_not_copy_the_catalogue_back() {
    let shipped = command_library::shipped();
    let entry = shipped.entries().first().expect("a shipped action");
    let (library, _) = library_with_echo();
    let mut all: Vec<ExportAction> = library.commands().iter().map(action_value).collect();
    all.push(shipped_value(entry));
    let text = export_text(&all, &[]);

    let mut copy = UserLibrary::default();
    let report = copy.import(&text).expect("imported");
    assert_eq!((report.imported, report.skipped), (1, 1));
    assert_eq!(
        report.first_skipped.map(|(_, why)| why),
        Some(CommandIssue::InCatalogue)
    );
    assert_eq!(copy.commands().len(), 1);
    assert_eq!(copy.commands()[0].title, "Echo");
}

#[test]
fn a_runbook_may_use_an_action_of_the_catalogue_or_of_the_same_file() {
    let file = r#"{"runbooks":[{"name":"Ghost","steps":[{"actionId":"not-anywhere","parameters":{"a":"b"}}]}]}"#;
    let values = precheck(file).expect("an envelope");
    let mut store = RunbookStore::default();
    // An action nothing knows is not judged here: the run says so.
    let report = import_runbooks(&mut store, &values, &nothing);
    assert_eq!(report.imported, 1);
}

/// Every shipped action, as an export of everything writes them.
fn everything() -> String {
    let mut all: Vec<ExportAction> = command_library::shipped()
        .entries()
        .iter()
        .map(shipped_value)
        .collect();
    all.extend(library_with_echo().0.commands().iter().map(action_value));
    export_text(&all, &[])
}

#[test]
fn an_export_of_everything_leaves_no_empty_identity_or_null_date_for_the_csharp() {
    let json: Value = serde_json::from_str(&everything()).expect("json");
    let actions = json["actions"].as_array().expect("actions");
    assert_eq!(
        actions.len(),
        command_library::shipped().entries().len() + 1
    );
    for action in actions {
        // An empty `publicId` would parse as a nil GUID and a null date as a bad date: the key
        // is left out and the C# falls back to its defaults.
        if let Some(id) = action.get("publicId") {
            assert!(id.as_str().is_some_and(|id| !id.is_empty()), "{action}");
        }
        for date in ["createdAt", "updatedAt"] {
            assert!(
                action.get(date).is_none_or(|value| !value.is_null()),
                "{action}"
            );
        }
    }
}

#[test]
fn the_shipped_translations_go_out_with_their_action_and_everything_re_imports() {
    let text = everything();
    assert!(
        text.contains("\"translations\""),
        "the French text is exported"
    );
    assert!(text.len() < usize::try_from(MAX_FILE_BYTES).expect("small"));
    let json: Value = serde_json::from_str(&text).expect("json");
    let translated = json["actions"]
        .as_array()
        .expect("actions")
        .iter()
        .filter(|action| action.get("translations").is_some())
        .count();
    assert!(translated > 0);

    // The shipped ones are not counted against the cap: the file always re-imports here.
    assert!(precheck(&text).is_ok());
    let mut copy = UserLibrary::default();
    let report = copy
        .import(&text)
        .expect("an export of everything re-imports");
    assert_eq!(report.imported, 1);
    assert_eq!(report.skipped, command_library::shipped().entries().len());
}

#[test]
fn only_actions_of_the_user_count_against_the_import_cap() {
    let shipped = command_library::shipped()
        .entries()
        .first()
        .expect("an action");
    let mut all: Vec<ExportAction> = (0..=MAX_IMPORT_ACTIONS)
        .map(|n| {
            ExportAction(serde_json::json!({
                "id": format!("mine-{n}"), "title": "T", "category": "c", "platform": 1,
                "linuxCommandTemplate": {"platform": 1, "commandPattern": "id", "parameters": []}
            }))
        })
        .collect();
    assert!(matches!(
        UserLibrary::default().import(&export_text(&all, &[])),
        Err(LibraryError::TooManyActions)
    ));
    all.pop();
    all.push(shipped_value(shipped));
    let report = UserLibrary::default()
        .import(&export_text(&all, &[]))
        .expect("the shipped one is not counted");
    assert_eq!(report.imported, MAX_IMPORT_ACTIONS);
}

#[test]
fn what_a_file_shows_of_itself_is_filtered_and_cut() {
    let version = format!("2.\u{202e}{}", "9".repeat(500));
    let file = serde_json::json!({ "schemaVersion": version }).to_string();
    let Err(LibraryError::Version(shown)) = precheck(&file) else {
        panic!("another version");
    };
    assert!(!shown.contains('\u{202e}'));
    assert!(shown.chars().count() <= MAX_TITLE_LENGTH);

    let mut library = UserLibrary::default();
    let file = serde_json::json!({ "actions": [
        { "id": "x", "title": "Bad\u{7}\u{200b}title", "category": "" } ] })
    .to_string();
    let report = library.import(&file).expect("read");
    assert_eq!(
        report.first_skipped.map(|(title, _)| title),
        Some("Badtitle".to_owned())
    );
}

#[test]
fn the_first_replaced_runbook_is_named_and_a_repeated_identity_in_one_file_is_not_replaced_twice() {
    let mut store = RunbookStore::default();
    store
        .keep(one_literal("Mine", "uptime"), &nothing)
        .expect("kept");
    let mine = store.runbooks()[0].public_id.clone();
    let file = serde_json::json!({ "runbooks": [
        { "publicId": mine, "name": "First", "steps": [ { "command": "a" } ] },
        { "publicId": mine, "name": "Second", "steps": [ { "command": "b" } ] },
        { "publicId": "11111111-1111-4111-8111-111111111111", "name": "N1",
          "steps": [ { "command": "c" } ] },
        { "publicId": "11111111-1111-4111-8111-111111111111", "name": "N2",
          "steps": [ { "command": "d" } ] } ] })
    .to_string();
    let values = precheck(&file).expect("an envelope");
    let report = import_runbooks(&mut store, &values, &nothing);
    assert_eq!((report.imported, report.updated, report.skipped), (1, 1, 2));
    assert_eq!(
        report.first_updated,
        Some(("Mine".to_owned(), "First".to_owned()))
    );
    assert_eq!(
        report.first_skipped,
        Some(("Second".to_owned(), RunbookRefusal::DuplicateInFile))
    );
    assert_eq!(store.get(&mine).expect("kept").name, "First");
}
