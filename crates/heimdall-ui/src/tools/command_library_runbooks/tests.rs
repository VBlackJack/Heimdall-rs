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

/// The route tracer of the catalogue, with a value to ask for.
const TRACE: &str = "network-trace-route";

fn pane_in(folder: &Path) -> RunbooksPane {
    RunbooksPane::new(Some(&folder.join("profiles.toml")), &UserLibrary::default())
}

fn send(pane: &mut RunbooksPane, message: RunbookMessage) {
    pane.update(message, &UserLibrary::default());
}

/// A new runbook of that name with one literal command, kept.
fn keep_literal(pane: &mut RunbooksPane, name: &str, command: &str) {
    send(pane, RunbookMessage::New);
    send(pane, RunbookMessage::Name(name.to_owned()));
    send(pane, RunbookMessage::AddLiteral);
    send(pane, RunbookMessage::Command(0, command.to_owned()));
    send(pane, RunbookMessage::Save);
}

#[test]
fn nothing_ships_and_the_panel_opens_closed() {
    let folder = tempfile::tempdir().expect("folder");
    let pane = pane_in(folder.path());
    assert!(pane.store.runbooks().is_empty());
    assert!(!pane.is_open());
    assert!(!folder.path().join(RUNBOOKS_FILE).exists());
}

#[test]
fn a_runbook_is_made_with_steps_kept_in_its_file_and_found_again() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane_in(folder.path());
    send(&mut pane, RunbookMessage::Toggle);
    assert!(pane.is_open());

    send(&mut pane, RunbookMessage::New);
    send(&mut pane, RunbookMessage::Name("  Restart  ".to_owned()));
    send(
        &mut pane,
        RunbookMessage::Description("Status, restart".to_owned()),
    );
    send(&mut pane, RunbookMessage::Mode(FailureMode::Continue));
    send(&mut pane, RunbookMessage::AddVariable);
    send(
        &mut pane,
        RunbookMessage::VariableName(0, "target".to_owned()),
    );
    send(
        &mut pane,
        RunbookMessage::VariableValue(0, "example.com".to_owned()),
    );
    send(&mut pane, RunbookMessage::AddAction(TRACE.to_owned()));
    send(
        &mut pane,
        RunbookMessage::Param(0, 0, "{{target}}".to_owned()),
    );
    send(&mut pane, RunbookMessage::AddLiteral);
    send(
        &mut pane,
        RunbookMessage::Platform(1, TemplatePlatform::Linux),
    );
    send(&mut pane, RunbookMessage::Command(1, " uptime ".to_owned()));
    send(&mut pane, RunbookMessage::Note(1, "then".to_owned()));
    send(&mut pane, RunbookMessage::Save);

    assert!(pane.editor.is_none(), "{:?}", pane.editor);
    let runbook = &pane.store.runbooks()[0];
    assert_eq!(runbook.name, "Restart");
    assert_eq!(runbook.failure_mode, FailureMode::Continue);
    assert_eq!(
        runbook.variables.get("target").map(String::as_str),
        Some("example.com")
    );
    assert_eq!(runbook.steps.len(), 2);
    assert!(matches!(
        &runbook.steps[0],
        Step::Action { action_id, parameters, .. }
            if action_id == TRACE
                && parameters.values().any(|value| value == "{{target}}")
    ));
    assert_eq!(
        runbook.steps[1],
        Step::Literal {
            command: "uptime".to_owned(),
            platform: TemplatePlatform::Linux,
            description: Some("then".to_owned()),
        }
    );

    let again = pane_in(folder.path());
    assert_eq!(again.store, pane.store);
}

#[test]
fn a_runbook_is_edited_in_place_and_its_steps_are_reordered_and_removed() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane_in(folder.path());
    keep_literal(&mut pane, "R", "first");
    let id = pane.store.runbooks()[0].public_id.clone();

    send(&mut pane, RunbookMessage::Edit(id.clone()));
    send(&mut pane, RunbookMessage::AddLiteral);
    send(&mut pane, RunbookMessage::Command(1, "second".to_owned()));
    send(&mut pane, RunbookMessage::AddLiteral);
    send(&mut pane, RunbookMessage::Command(2, "third".to_owned()));
    send(&mut pane, RunbookMessage::MoveUp(2));
    send(&mut pane, RunbookMessage::MoveDown(0));
    // Nothing moves past either end.
    send(&mut pane, RunbookMessage::MoveUp(0));
    send(&mut pane, RunbookMessage::MoveDown(2));
    send(&mut pane, RunbookMessage::RemoveStep(0));
    send(&mut pane, RunbookMessage::Save);

    assert_eq!(pane.store.runbooks().len(), 1);
    let commands: Vec<&str> = pane.store.runbooks()[0]
        .steps
        .iter()
        .map(|step| match step {
            Step::Literal { command, .. } => command.as_str(),
            Step::Action { .. } => "",
        })
        .collect();
    assert_eq!(commands, ["first", "second"]);
    assert_eq!(pane.store.runbooks()[0].public_id, id);
}

#[test]
fn a_runbook_that_breaks_a_rule_is_not_kept_and_says_why() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane_in(folder.path());

    send(&mut pane, RunbookMessage::New);
    send(&mut pane, RunbookMessage::Save);
    assert_eq!(
        pane.editor
            .as_ref()
            .and_then(|editor| editor.problem.clone()),
        Some(Problem::Issue(RunbookIssue::NameMissing))
    );

    send(&mut pane, RunbookMessage::Name("R".to_owned()));
    send(&mut pane, RunbookMessage::AddLiteral);
    send(&mut pane, RunbookMessage::Save);
    assert_eq!(
        pane.editor
            .as_ref()
            .and_then(|editor| editor.problem.clone()),
        Some(Problem::Issue(RunbookIssue::Step(
            1,
            StepIssue::CommandMissing
        )))
    );

    for hidden in ["a\nb", "a\u{202E}b", "a\u{200B}b"] {
        send(&mut pane, RunbookMessage::Command(0, hidden.to_owned()));
        send(&mut pane, RunbookMessage::Save);
        assert_eq!(
            pane.editor
                .as_ref()
                .and_then(|editor| editor.problem.clone()),
            Some(Problem::Issue(RunbookIssue::Step(1, StepIssue::Hidden))),
            "{hidden:?}"
        );
    }

    send(&mut pane, RunbookMessage::Command(0, "uptime".to_owned()));
    send(&mut pane, RunbookMessage::AddVariable);
    send(
        &mut pane,
        RunbookMessage::VariableName(0, "not a name".to_owned()),
    );
    send(&mut pane, RunbookMessage::Save);
    assert_eq!(
        pane.editor
            .as_ref()
            .and_then(|editor| editor.problem.clone()),
        Some(Problem::VariableName("not a name".to_owned()))
    );
    send(&mut pane, RunbookMessage::VariableName(0, "a".to_owned()));
    send(&mut pane, RunbookMessage::AddVariable);
    send(&mut pane, RunbookMessage::VariableName(1, "a".to_owned()));
    send(&mut pane, RunbookMessage::Save);
    assert_eq!(
        pane.editor
            .as_ref()
            .and_then(|editor| editor.problem.clone()),
        Some(Problem::VariableName("a".to_owned()))
    );

    assert!(pane.store.runbooks().is_empty());
    assert!(!folder.path().join(RUNBOOKS_FILE).exists());
}

#[test]
fn a_value_the_generator_refuses_is_not_kept_unless_it_is_a_variable() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane_in(folder.path());
    send(&mut pane, RunbookMessage::New);
    send(&mut pane, RunbookMessage::Name("R".to_owned()));
    send(&mut pane, RunbookMessage::AddAction(TRACE.to_owned()));
    send(
        &mut pane,
        RunbookMessage::Param(0, 0, "a; rm -rf /".to_owned()),
    );
    send(&mut pane, RunbookMessage::Save);
    assert!(
        matches!(
            pane.editor
                .as_ref()
                .and_then(|editor| editor.problem.as_ref()),
            Some(Problem::Issue(RunbookIssue::Step(1, StepIssue::Values(_))))
        ),
        "{:?}",
        pane.editor.as_ref().map(|editor| editor.problem.clone())
    );
    send(&mut pane, RunbookMessage::Param(0, 0, "{{x}}".to_owned()));
    send(&mut pane, RunbookMessage::Save);
    assert!(pane.editor.is_none());
    assert_eq!(pane.store.runbooks().len(), 1);
}

#[test]
fn a_variable_used_and_not_declared_is_listed() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane_in(folder.path());
    send(&mut pane, RunbookMessage::New);
    send(&mut pane, RunbookMessage::AddAction(TRACE.to_owned()));
    send(
        &mut pane,
        RunbookMessage::Param(0, 0, "{{host}}.{{zone}}".to_owned()),
    );
    send(&mut pane, RunbookMessage::AddVariable);
    send(
        &mut pane,
        RunbookMessage::VariableName(0, " host ".to_owned()),
    );
    let editor = pane.editor.as_ref().expect("an editor");
    assert_eq!(editor.undeclared(), ["zone"]);
}

#[test]
fn a_delete_asks_first_and_a_duplicate_is_a_runbook_of_its_own() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane_in(folder.path());
    keep_literal(&mut pane, "Restart", "uptime");
    let id = pane.store.runbooks()[0].public_id.clone();

    send(&mut pane, RunbookMessage::Duplicate(id.clone()));
    assert_eq!(pane.store.runbooks().len(), 2);
    let copy = &pane.store.runbooks()[0];
    assert_ne!(copy.public_id, id);
    assert!(copy.name.contains("Restart"));
    assert_eq!(copy.steps, pane.store.runbooks()[1].steps);

    // Asking changes nothing; keeping cancels; only the confirmation deletes.
    send(&mut pane, RunbookMessage::AskDelete(id.clone()));
    assert_eq!(pane.store.runbooks().len(), 2);
    assert_eq!(pane.deleting.as_deref(), Some(id.as_str()));
    send(&mut pane, RunbookMessage::CancelDelete);
    assert!(pane.deleting.is_none());
    assert_eq!(pane.store.runbooks().len(), 2);
    send(&mut pane, RunbookMessage::ConfirmDelete(id.clone()));
    assert_eq!(pane.store.runbooks().len(), 1);
    assert!(pane.store.get(&id).is_none());
    assert_eq!(pane_in(folder.path()).store, pane.store);
}

#[test]
fn a_file_that_cannot_be_read_is_left_alone_and_nothing_is_changed() {
    let folder = tempfile::tempdir().expect("folder");
    let path = folder.path().join(RUNBOOKS_FILE);
    std::fs::write(&path, "not json").expect("a file");
    let mut pane = pane_in(folder.path());
    assert_eq!(pane.failure, Some(Failure::Unreadable));
    send(&mut pane, RunbookMessage::New);
    assert!(pane.editor.is_none());
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file"),
        "not json"
    );
}

#[test]
fn a_pane_without_a_place_to_keep_runbooks_makes_none() {
    let mut pane = RunbooksPane::new(None, &UserLibrary::default());
    send(&mut pane, RunbookMessage::New);
    assert!(pane.editor.is_none());
}

#[test]
fn a_runbook_that_could_not_be_written_is_not_kept() {
    let folder = tempfile::tempdir().expect("folder");
    let mut pane = pane_in(folder.path());
    // A folder where the file should be: the write fails.
    std::fs::create_dir(folder.path().join(RUNBOOKS_FILE)).expect("a folder");
    keep_literal(&mut pane, "R", "uptime");
    assert!(pane.store.runbooks().is_empty());
    assert_eq!(pane.failure, Some(Failure::Unsaved));
    assert_eq!(
        pane.editor
            .as_ref()
            .and_then(|editor| editor.problem.clone()),
        Some(Problem::Unsaved)
    );
}

#[test]
fn a_hostile_file_is_kept_unread_and_the_panel_says_so() {
    let folder = tempfile::tempdir().expect("folder");
    let json = r#"{"schemaVersion": "1.0", "runbooks": [
        {"name": "Hostile", "steps": [{"command": "ls\u202e", "platform": "Linux"}]},
        {"name": "Fine", "steps": [{"command": "ls", "platform": "Linux"}]}]}"#;
    std::fs::write(folder.path().join(RUNBOOKS_FILE), json).expect("a file");
    let mut pane = pane_in(folder.path());
    assert_eq!(pane.store.runbooks().len(), 1);
    assert_eq!(pane.store.unreadable_count(), 1);
    keep_literal(&mut pane, "Mine", "uptime");
    let written = std::fs::read_to_string(folder.path().join(RUNBOOKS_FILE)).expect("the file");
    assert!(written.contains("Hostile"));
}

#[test]
fn an_action_of_the_user_is_found_and_one_that_is_gone_is_said() {
    use heimdall_core::tools::user_commands::{Draft, UserPlatform};
    let folder = tempfile::tempdir().expect("folder");
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
    let mut pane = RunbooksPane::new(Some(&folder.path().join("profiles.toml")), &library);
    pane.update(RunbookMessage::New, &library);
    pane.update(RunbookMessage::Name("R".to_owned()), &library);
    pane.update(RunbookMessage::AddAction(id.clone()), &library);
    pane.update(RunbookMessage::Param(0, 0, "hi".to_owned()), &library);
    pane.update(RunbookMessage::Save, &library);
    assert_eq!(pane.store.runbooks().len(), 1);

    // The action is deleted: the runbook still opens and its step says so.
    let runbook_id = pane.store.runbooks()[0].public_id.clone();
    pane.update(RunbookMessage::Edit(runbook_id), &UserLibrary::default());
    let editor = pane.editor.as_ref().expect("an editor");
    assert!(matches!(
        &editor.steps[0],
        EditorStep::Action { found: false, fields, .. }
            if fields.iter().any(|field| field.value == "hi")
    ));
}
