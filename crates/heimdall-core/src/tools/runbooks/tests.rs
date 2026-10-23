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

use super::*;
use crate::tools::user_commands::{Draft, UserLibrary, UserPlatform};

/// What a C# export of two runbooks looks like (`RunbookTransferModel`): an action step with a
/// variable, a literal step, a failure mode and a name that is not a variable.
const CSHARP_SAMPLE: &str = r#"{
  "schemaVersion": "1.0",
  "exportDate": "2026-10-21T05:00:00Z",
  "totalActions": 0,
  "actions": [],
  "runbooks": [
    {
      "publicId": "0b0e8f2c-5a1d-4c53-9a54-2d4f3f7c9e11",
      "name": "Restart the web service",
      "description": "Status, restart, status",
      "executionMode": "ContinueOnError",
      "variables": { "unit": "nginx", "bad name": "x" },
      "tags": ["web", "incident"],
      "steps": [
        { "actionId": "svc-status", "actionTitle": "Service status", "command": "systemctl status nginx",
          "platform": "Both", "description": "Before", "parameters": { "unit": "{{unit}}" } },
        { "actionId": null, "actionTitle": "", "command": "systemctl restart nginx",
          "platform": "Linux", "description": null, "parameters": {} },
        null
      ]
    },
    {
      "publicId": "7d2a1c50-0000-4000-8000-000000000001",
      "name": "Windows check",
      "executionMode": "SomethingElse",
      "steps": [
        { "command": "Get-Service W32Time", "platform": "windows" }
      ]
    }
  ]
}"#;

fn nothing<'a>(_: &str) -> Option<&'a UserCommand> {
    None
}

fn one_literal(command: &str) -> Runbook {
    let mut runbook = Runbook::new("Check");
    runbook
        .steps
        .push(Step::literal(command, TemplatePlatform::Linux));
    runbook
}

/// A library holding one user command, `echo {text}` for Linux, and its identity.
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

fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn a_csharp_export_is_read_and_written_back_in_the_same_shape() {
    let store = RunbookStore::parse(CSHARP_SAMPLE, &nothing).expect("an envelope");
    assert_eq!(store.runbooks().len(), 2);
    assert_eq!(store.unreadable_count(), 0);

    let web = &store.runbooks()[0];
    assert_eq!(web.public_id, "0b0e8f2c-5a1d-4c53-9a54-2d4f3f7c9e11");
    assert_eq!(web.name, "Restart the web service");
    assert_eq!(web.failure_mode, FailureMode::Continue);
    // The variable whose name is not a name is dropped, as the C# `ToRunbook` does.
    assert_eq!(web.variables, values(&[("unit", "nginx")]));
    assert_eq!(web.tags, ["web", "incident"]);
    assert_eq!(web.steps.len(), 2);
    assert_eq!(
        web.steps[0],
        Step::Action {
            action_id: "svc-status".to_owned(),
            action_title: "Service status".to_owned(),
            parameters: values(&[("unit", "{{unit}}")]),
            description: Some("Before".to_owned()),
        }
    );
    assert_eq!(
        web.steps[1],
        Step::literal("systemctl restart nginx", TemplatePlatform::Linux)
    );

    let windows = &store.runbooks()[1];
    assert_eq!(windows.failure_mode, FailureMode::Stop);
    assert_eq!(
        windows.steps,
        [Step::literal(
            "Get-Service W32Time",
            TemplatePlatform::Windows
        )]
    );

    // Written and read again: nothing is lost, and the keys are the C# ones.
    let text = store.to_text();
    let json: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(json["schemaVersion"], "1.0");
    let first = &json["runbooks"][0];
    assert_eq!(first["executionMode"], "ContinueOnError");
    assert_eq!(first["steps"][0]["platform"], "Both");
    assert_eq!(first["steps"][1]["platform"], "Linux");
    assert_eq!(first["steps"][1]["actionId"], Value::Null);
    let again = RunbookStore::parse(&text, &nothing).expect("an envelope");
    assert_eq!(again, store);
}

#[test]
fn the_store_is_kept_in_a_file_and_read_back() {
    let folder = tempfile::tempdir().expect("folder");
    let path = folder.path().join("sub").join(RUNBOOKS_FILE);
    assert_eq!(
        RunbookStore::load(&path, &nothing).expect("none yet"),
        RunbookStore::default()
    );
    let mut store = RunbookStore::default();
    store.keep(one_literal("uptime"), &nothing).expect("kept");
    store.save(&path).expect("saved");
    let read = RunbookStore::load(&path, &nothing).expect("read");
    assert_eq!(read, store);
}

#[test]
fn variables_are_extracted_in_order_once_and_only_when_well_formed() {
    assert_eq!(
        variable_references("{{host}} and {{ user }} {{host}} {{_a1}} {{1x}} {{a-b}} {{"),
        ["host", "_a1"]
    );
    assert!(variable_references("no braces {x} {{}}").is_empty());
    // A name is read greedily: `{{a}}}` holds `a` and one stray brace.
    assert_eq!(variable_references("{{a}}}"), ["a"]);
    assert!(is_valid_variable_name("host_2"));
    assert!(!is_valid_variable_name("2host"));
    assert!(!is_valid_variable_name(""));
    assert!(!is_valid_variable_name("a b"));
    assert!(!is_valid_variable_name(
        &"a".repeat(MAX_VARIABLE_NAME_LENGTH + 1)
    ));
    assert_eq!(format_reference("host"), "{{host}}");
}

#[test]
fn a_runbook_lists_the_variables_its_steps_use_and_the_ones_it_does_not_declare() {
    let mut runbook = Runbook::new("R");
    runbook.variables.insert("host".to_owned(), String::new());
    runbook.steps.push(Step::action(
        "a",
        "A",
        values(&[("x", "{{host}}:{{port}}"), ("y", "{{user}}")]),
    ));
    runbook
        .steps
        .push(Step::literal("echo {{ignored}}", TemplatePlatform::Linux));
    assert_eq!(runbook.referenced_variables(), ["host", "port", "user"]);
    assert_eq!(runbook.undeclared_variables(), ["port", "user"]);
}

#[test]
fn a_reference_is_replaced_by_its_variable_and_an_unknown_one_is_left_and_reported() {
    let variables = values(&[("host", "db1"), ("empty", "")]);
    let mut unknown = Vec::new();
    assert_eq!(
        substitute(
            "{{host}}/{{empty}}/{{nope}}/{{nope}}/{x}",
            &variables,
            &mut unknown
        ),
        "db1//{{nope}}/{{nope}}/{x}"
    );
    assert_eq!(unknown, ["nope"]);
    // The value is not read again: a variable holding a reference is not expanded.
    let nested = values(&[("a", "{{b}}"), ("b", "x")]);
    assert_eq!(substitute("{{a}}", &nested, &mut Vec::new()), "{{b}}");
}

#[test]
fn a_literal_step_with_a_control_or_invisible_character_is_refused() {
    for hidden in [
        '\n',
        '\r',
        '\u{1b}',
        '\0',
        '\u{202E}',
        '\u{200B}',
        '\u{FEFF}',
        '\u{E0041}',
    ] {
        let runbook = one_literal(&format!("ls{hidden}-la"));
        assert_eq!(
            runbook.validate(&nothing),
            Err(RunbookIssue::Step(1, StepIssue::Hidden)),
            "{hidden:?}"
        );
    }
    assert_eq!(one_literal("ls -la").validate(&nothing), Ok(()));
}

#[test]
fn hidden_characters_are_refused_in_every_text() {
    let mut named = one_literal("ls");
    named.name = "Che\u{202E}ck".to_owned();
    assert_eq!(named.validate(&nothing), Err(RunbookIssue::Hidden));

    let mut variable = one_literal("ls");
    variable.variables.insert("v".to_owned(), "a\nb".to_owned());
    assert_eq!(variable.validate(&nothing), Err(RunbookIssue::Hidden));

    let mut tagged = one_literal("ls");
    tagged.tags.push("a\u{200B}".to_owned());
    assert_eq!(tagged.validate(&nothing), Err(RunbookIssue::Hidden));

    let mut valued = Runbook::new("R");
    valued
        .steps
        .push(Step::action("a", "A", values(&[("x", "a\u{202E}b")])));
    assert_eq!(
        valued.validate(&nothing),
        Err(RunbookIssue::Step(1, StepIssue::Hidden))
    );

    // A note may run over several lines; it may not hide anything.
    let mut noted = one_literal("ls");
    noted.description = Some("line one\nline two".to_owned());
    assert_eq!(noted.validate(&nothing), Ok(()));
    noted.description = Some("line\u{202E}".to_owned());
    assert_eq!(noted.validate(&nothing), Err(RunbookIssue::Hidden));
}

#[test]
fn the_empty_and_the_unnamed_are_refused() {
    let mut unnamed = one_literal("ls");
    unnamed.name = "   ".to_owned();
    assert_eq!(unnamed.validate(&nothing), Err(RunbookIssue::NameMissing));
    assert_eq!(
        one_literal("   ").validate(&nothing),
        Err(RunbookIssue::Step(1, StepIssue::CommandMissing))
    );
    let mut no_action = Runbook::new("R");
    no_action.steps.push(Step::action(" ", "", BTreeMap::new()));
    assert_eq!(
        no_action.validate(&nothing),
        Err(RunbookIssue::Step(1, StepIssue::ActionMissing))
    );
}

#[test]
fn a_literal_step_for_both_platforms_or_an_unreadable_one_reads_as_linux() {
    for platform in ["Both", "2", "Solaris", ""] {
        let json =
            format!(r#"{{"name": "R", "steps": [{{"command": "ls", "platform": "{platform}"}}]}}"#);
        let value: Value = serde_json::from_str(&json).expect("json");
        let runbook = Runbook::from_value(&value).expect("a runbook");
        assert_eq!(
            runbook.steps,
            [Step::literal("ls", TemplatePlatform::Linux)],
            "{platform}"
        );
    }
    let json = r#"{"name": "R", "steps": [{"command": "dir", "platform": "Windows"}]}"#;
    let value: Value = serde_json::from_str(json).expect("json");
    assert_eq!(
        Runbook::from_value(&value).expect("a runbook").steps,
        [Step::literal("dir", TemplatePlatform::Windows)]
    );
}

#[test]
fn an_identity_that_is_not_a_guid_is_replaced() {
    let bad = [
        "abc",
        "1234-",
        "zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz",
        "00000000-0000-4000-8000-00000000000",
        "0000000000004000800000000000000-",
    ];
    for id in bad {
        let json = format!(r#"{{"publicId": "{id}", "name": "R"}}"#);
        let value: Value = serde_json::from_str(&json).expect("json");
        let runbook = Runbook::from_value(&value).expect("a runbook");
        assert_ne!(runbook.public_id, id, "{id}");
        assert_eq!(runbook.public_id.len(), 36, "{id}");
    }
    for id in [
        "00000000-0000-4000-8000-000000000001",
        "0000000000004000800000000000000A",
    ] {
        let json = format!(r#"{{"publicId": "{id}", "name": "R"}}"#);
        let value: Value = serde_json::from_str(&json).expect("json");
        let runbook = Runbook::from_value(&value).expect("a runbook");
        assert_eq!(runbook.public_id, id.to_ascii_lowercase());
    }
}

#[test]
fn a_generated_command_too_long_is_refused_unless_a_variable_is_still_to_come() {
    let mut library = UserLibrary::default();
    let id = library
        .upsert(&Draft {
            title: "Echo".to_owned(),
            category: "System".to_owned(),
            platform: UserPlatform::Linux,
            linux_pattern: format!("echo {{a}} {{b}} {{c}} {{d}} {{e}} {}", "x".repeat(300)),
            ..Draft::default()
        })
        .expect("a command");
    let lookup = |wanted: &str| library.get(wanted);
    let long = "v".repeat(MAX_PARAMETER_LENGTH);
    let mut runbook = Runbook::new("R");
    runbook.steps.push(Step::action(
        &id,
        "Echo",
        values(&[
            ("a", &long),
            ("b", &long),
            ("c", &long),
            ("d", &long),
            ("e", &long),
        ]),
    ));
    assert_eq!(
        runbook.validate(&lookup),
        Err(RunbookIssue::Step(1, StepIssue::CommandTooLong))
    );
    runbook.steps[0] = Step::action(
        &id,
        "Echo",
        values(&[
            ("a", &long),
            ("b", &long),
            ("c", &long),
            ("d", &long),
            ("e", "{{later}}"),
        ]),
    );
    assert_eq!(runbook.validate(&lookup), Ok(()));
}

#[test]
fn lengths_and_counts_are_bounded() {
    let mut long_name = one_literal("ls");
    long_name.name = "n".repeat(MAX_TITLE_LENGTH + 1);
    assert_eq!(long_name.validate(&nothing), Err(RunbookIssue::NameTooLong));

    assert_eq!(
        one_literal(&"c".repeat(MAX_COMMAND_LENGTH + 1)).validate(&nothing),
        Err(RunbookIssue::Step(1, StepIssue::CommandTooLong))
    );
    assert_eq!(
        one_literal(&"c".repeat(MAX_COMMAND_LENGTH)).validate(&nothing),
        Ok(())
    );

    let mut many_steps = Runbook::new("R");
    for _ in 0..=MAX_STEPS {
        many_steps
            .steps
            .push(Step::literal("ls", TemplatePlatform::Linux));
    }
    assert_eq!(
        many_steps.validate(&nothing),
        Err(RunbookIssue::TooManySteps)
    );
    many_steps.steps.pop();
    assert_eq!(many_steps.validate(&nothing), Ok(()));

    let mut many_variables = one_literal("ls");
    for index in 0..=MAX_VARIABLES {
        many_variables
            .variables
            .insert(format!("v{index}"), String::new());
    }
    assert_eq!(
        many_variables.validate(&nothing),
        Err(RunbookIssue::TooManyVariables)
    );

    let mut many_tags = one_literal("ls");
    many_tags.tags = vec!["t".to_owned(); MAX_TAGS + 1];
    assert_eq!(many_tags.validate(&nothing), Err(RunbookIssue::TooManyTags));

    let mut long_value = Runbook::new("R");
    long_value.steps.push(Step::action(
        "a",
        "A",
        values(&[("x", &"v".repeat(MAX_PARAMETER_LENGTH + 1))]),
    ));
    assert_eq!(
        long_value.validate(&nothing),
        Err(RunbookIssue::Step(
            1,
            StepIssue::ValueTooLong("x".to_owned())
        ))
    );

    let mut long_default = one_literal("ls");
    long_default
        .variables
        .insert("v".to_owned(), "d".repeat(MAX_PARAMETER_LENGTH + 1));
    assert_eq!(
        long_default.validate(&nothing),
        Err(RunbookIssue::VariableValueTooLong("v".to_owned()))
    );

    let mut bad_parameter = Runbook::new("R");
    bad_parameter
        .steps
        .push(Step::action("a", "A", values(&[("a b", "1")])));
    assert_eq!(
        bad_parameter.validate(&nothing),
        Err(RunbookIssue::Step(
            1,
            StepIssue::ParameterName("a b".to_owned())
        ))
    );
}

#[test]
fn the_values_of_an_action_step_go_through_the_generator() {
    let (library, id) = library_with_echo();
    let lookup = |wanted: &str| library.get(wanted);
    let mut runbook = Runbook::new("R");
    runbook
        .steps
        .push(Step::action(&id, "Echo", values(&[("text", "hello")])));
    assert_eq!(runbook.validate(&lookup), Ok(()));

    // A shell character in a value the command would take bare is refused, as in the pane.
    runbook.steps[0] = Step::action(&id, "Echo", values(&[("text", "a; rm -rf /")]));
    let refused = runbook.validate(&lookup);
    assert!(
        matches!(
            refused,
            Err(RunbookIssue::Step(1, StepIssue::Values(ref errors)))
                if matches!(errors.as_slice(), [ValueError::Dangerous(_)])
        ),
        "{refused:?}"
    );

    // A value that refers to a variable is judged when the run has substituted it.
    runbook.steps[0] = Step::action(&id, "Echo", values(&[("text", "{{what}}; x")]));
    assert_eq!(runbook.validate(&lookup), Ok(()));

    // An action that is gone is kept: the step is shown as missing, not dropped.
    assert_eq!(runbook.validate(&nothing), Ok(()));
}

#[test]
fn a_hostile_file_keeps_its_bad_runbooks_unread_and_writes_them_back() {
    let json = format!(
        r#"{{"schemaVersion": "1.0", "runbooks": [
          {{"publicId": "00000000-0000-4000-8000-000000000001", "name": "Good",
            "steps": [{{"command": "uptime", "platform": "Linux"}}]}},
          {{"publicId": "00000000-0000-4000-8000-000000000002", "name": "Hidden",
            "steps": [{{"command": "ls\u202e -la", "platform": "Linux"}}]}},
          {{"publicId": "00000000-0000-4000-8000-000000000001", "name": "Same identity",
            "steps": []}},
          {{"name": "", "steps": []}},
          "not an object",
          {{"name": "{}", "steps": []}}
        ]}}"#,
        "n".repeat(MAX_TITLE_LENGTH + 1)
    );
    let store = RunbookStore::parse(&json, &nothing).expect("an envelope");
    // The runbook that repeats an identity is kept under a new one.
    assert_eq!(store.runbooks().len(), 2);
    assert_eq!(store.runbooks()[0].name, "Good");
    assert_eq!(store.runbooks()[1].name, "Same identity");
    assert_ne!(store.runbooks()[0].public_id, store.runbooks()[1].public_id);
    assert_eq!(store.unreadable_count(), 4);
    let written: Value = serde_json::from_str(&store.to_text()).expect("json");
    assert_eq!(written["runbooks"].as_array().map(Vec::len), Some(6));
    assert_eq!(written["runbooks"][2]["name"], "Hidden");
}

#[test]
fn a_file_that_is_not_ours_to_read_is_refused_whole() {
    for text in [
        "",
        "[]",
        "{",
        r#"{"runbooks": 3}"#,
        r#"{"schemaVersion": 1}"#,
    ] {
        assert!(
            matches!(RunbookStore::parse(text, &nothing), Err(StoreError::Format)),
            "{text:?}"
        );
    }
    assert!(matches!(
        RunbookStore::parse(r#"{"schemaVersion": "2.0", "runbooks": []}"#, &nothing),
        Err(StoreError::Version(version)) if version == "2.0"
    ));
    // A minor version of the major we read is read.
    assert!(RunbookStore::parse(r#"{"schemaVersion": "1.7"}"#, &nothing).is_ok());

    let entries = vec!["{\"name\": \"R\"}"; MAX_RUNBOOKS + 1].join(",");
    assert!(matches!(
        RunbookStore::parse(&format!(r#"{{"runbooks": [{entries}]}}"#), &nothing),
        Err(StoreError::TooMany)
    ));

    let folder = tempfile::tempdir().expect("folder");
    let path = folder.path().join(RUNBOOKS_FILE);
    let size = usize::try_from(MAX_FILE_BYTES).expect("size") + 1;
    std::fs::write(&path, vec![b' '; size]).expect("a big file");
    assert!(matches!(
        RunbookStore::load(&path, &nothing),
        Err(StoreError::TooLarge)
    ));
}

#[test]
fn the_number_of_runbooks_is_capped_and_a_refusal_changes_nothing() {
    let mut store = RunbookStore::default();
    for index in 0..MAX_RUNBOOKS {
        store
            .keep(one_literal(&format!("echo {index}")), &nothing)
            .expect("room");
    }
    assert_eq!(
        store.keep(one_literal("one more"), &nothing),
        Err(KeepError::TooMany)
    );
    assert_eq!(
        store.duplicate(&store.runbooks()[0].public_id.clone(), "Copy"),
        None
    );
    assert_eq!(store.runbooks().len(), MAX_RUNBOOKS);

    // A runbook already there can still be changed.
    let mut changed = store.runbooks()[5].clone();
    changed.name = "Renamed".to_owned();
    store.keep(changed.clone(), &nothing).expect("changed");
    assert_eq!(store.runbooks().len(), MAX_RUNBOOKS);
    assert_eq!(store.runbooks()[0], changed);

    let mut bad = one_literal("ls");
    bad.steps.push(Step::literal("", TemplatePlatform::Linux));
    let before = store.clone();
    assert!(matches!(
        store.keep(bad, &nothing),
        Err(KeepError::Refused(RunbookIssue::Step(
            2,
            StepIssue::CommandMissing
        )))
    ));
    assert_eq!(store, before);
}

#[test]
fn a_runbook_is_duplicated_under_a_new_identity_and_deleted() {
    let mut store = RunbookStore::default();
    let original = one_literal("uptime");
    let id = original.public_id.clone();
    store.keep(original.clone(), &nothing).expect("kept");
    let copy_id = store.duplicate(&id, "  Check (copy) ").expect("a copy");
    assert_ne!(copy_id, id);
    assert_eq!(store.runbooks()[0].public_id, copy_id);
    assert_eq!(store.runbooks()[0].name, "Check (copy)");
    assert_eq!(store.runbooks()[0].steps, original.steps);
    assert_eq!(store.duplicate("unknown", "x"), None);

    assert!(store.delete(&id));
    assert!(!store.delete(&id));
    assert_eq!(store.runbooks().len(), 1);
}
