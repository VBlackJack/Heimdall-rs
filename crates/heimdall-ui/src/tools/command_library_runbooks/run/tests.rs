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

use std::path::Path;

use heimdall_core::tools::runbooks::{RUNBOOKS_FILE, RunbookStore};
use heimdall_core::tools::user_commands::{TemplatePlatform, ValueError};

use super::*;

/// The route tracer of the catalogue: a value to ask for, a template for each shell.
const TRACE: &str = "network-trace-route";

/// A runbook of that name with `variables` and `steps`.
fn runbook(name: &str, variables: &[(&str, &str)], steps: Vec<Step>) -> Runbook {
    let mut runbook = Runbook::new(name);
    runbook.variables = variables
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    runbook.steps = steps;
    runbook
}

/// The route tracer with its value written as `value`.
fn trace(value: &str) -> Step {
    Step::action(
        TRACE,
        "Trace",
        BTreeMap::from([("target".to_owned(), value.to_owned())]),
    )
}

/// A pane over the runbooks kept in `folder`; their identities.
fn pane_with(folder: &Path, runbooks: Vec<Runbook>) -> (RunbooksPane, Vec<String>) {
    let mut store = RunbookStore::default();
    let ids = runbooks
        .into_iter()
        .map(|runbook| {
            let id = runbook.public_id.clone();
            store.keep(runbook, &|_: &str| None).expect("kept");
            id
        })
        .collect();
    store.save(&folder.join(RUNBOOKS_FILE)).expect("saved");
    (
        RunbooksPane::new(Some(&folder.join("profiles.toml")), &UserLibrary::default()),
        ids,
    )
}

fn update(pane: &mut RunbooksPane, message: RunbookMessage) {
    pane.update(message, &UserLibrary::default());
}

#[test]
fn every_variable_is_asked_once_prefilled_from_the_session_and_the_steps_wait_for_it() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(
        folder.path(),
        vec![runbook(
            "Check",
            &[
                ("host", "default.lab"),
                ("user", "nobody"),
                ("ticket", "INC-1"),
            ],
            vec![trace("{{host}}")],
        )],
    );
    pane.set_session(
        Some(" web.lab ".to_owned()),
        Some("bob".to_owned()),
        RemoteOs::Linux,
    );
    update(&mut pane, RunbookMessage::Run(ids[0].clone()));

    // The session fills what is named for it, the default stays where the name fits nothing.
    assert!(pane.is_asking());
    assert_eq!(
        pane.run_values(),
        [
            ("host".to_owned(), "web.lab".to_owned()),
            ("ticket".to_owned(), "INC-1".to_owned()),
            ("user".to_owned(), "bob".to_owned()),
        ]
    );
    // No step is made before the values are confirmed.
    assert_eq!(pane.ready_step(&UserLibrary::default()), None);

    update(&mut pane, RunbookMessage::RunValue(0, "db.lab".to_owned()));
    update(&mut pane, RunbookMessage::RunBegin);
    assert!(!pane.is_asking());
    let ready = pane.ready_step(&UserLibrary::default()).expect("ready");
    assert_eq!(ready.step.command, "traceroute db.lab");
    // Asked once: the values do not change after the start.
    update(
        &mut pane,
        RunbookMessage::RunValue(0, "other.lab".to_owned()),
    );
    assert_eq!(pane.run_values()[0].1, "db.lab");
}

#[test]
fn a_runbook_with_no_variable_starts_on_its_first_step() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(
        folder.path(),
        vec![runbook(
            "Plain",
            &[],
            vec![Step::literal("uptime", TemplatePlatform::Linux)],
        )],
    );
    update(&mut pane, RunbookMessage::Run(ids[0].clone()));
    assert!(!pane.is_asking());
    assert_eq!(pane.current_index(), Some(0));
    assert_eq!(
        pane.ready_step(&UserLibrary::default())
            .map(|ready| ready.step.command),
        Some("uptime".to_owned())
    );
}

#[test]
fn the_platform_of_the_session_picks_the_template_of_an_action_step() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(
        folder.path(),
        vec![runbook("Trace", &[], vec![trace("web.lab")])],
    );
    let library = UserLibrary::default();
    for (remote, expected) in [
        (RemoteOs::Linux, "traceroute web.lab"),
        (RemoteOs::Windows, "tracert web.lab"),
        (RemoteOs::Auto, "tracert web.lab"),
    ] {
        pane.set_session(None, None, remote);
        update(&mut pane, RunbookMessage::Run(ids[0].clone()));
        assert_eq!(
            pane.ready_step(&library).map(|ready| ready.step.command),
            Some(expected.to_owned()),
            "{remote:?}"
        );
    }
}

#[test]
fn a_value_is_judged_by_the_generator_when_the_step_is_reached() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(
        folder.path(),
        vec![runbook(
            "Trace",
            &[("target", "web.lab")],
            vec![trace("{{target}}")],
        )],
    );
    pane.set_session(None, None, RemoteOs::Linux);
    let library = UserLibrary::default();
    update(&mut pane, RunbookMessage::Run(ids[0].clone()));

    // A value that would run more than the command is refused, and the step cannot be sent.
    for refused in [
        "web.lab; rm -rf /",
        "web.lab\nrm -rf /",
        "web\u{200B}.lab",
        "",
    ] {
        update(&mut pane, RunbookMessage::RunValue(0, refused.to_owned()));
        update(&mut pane, RunbookMessage::RunBegin);
        assert_eq!(pane.ready_step(&library), None, "{refused:?}");
        assert!(
            matches!(pane.current_problem(&library), Some(RunProblem::Values(_))),
            "{refused:?}: {:?}",
            pane.current_problem(&library)
        );
        update(&mut pane, RunbookMessage::RunStop);
        update(&mut pane, RunbookMessage::Run(ids[0].clone()));
    }

    // The same step, with a value that is fit.
    update(&mut pane, RunbookMessage::RunBegin);
    assert_eq!(
        pane.ready_step(&library).map(|ready| ready.step.command),
        Some("traceroute web.lab".to_owned())
    );
}

#[test]
fn a_step_with_a_variable_the_runbook_does_not_declare_says_which() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(
        folder.path(),
        vec![runbook("Trace", &[], vec![trace("{{where}}")])],
    );
    let library = UserLibrary::default();
    update(&mut pane, RunbookMessage::Run(ids[0].clone()));
    assert_eq!(
        pane.current_problem(&library),
        Some(RunProblem::UnknownVariables(vec!["where".to_owned()]))
    );
    assert_eq!(pane.ready_step(&library), None);
}

#[test]
fn an_action_gone_from_the_library_cannot_be_sent() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(
        folder.path(),
        vec![runbook(
            "Gone",
            &[],
            vec![Step::action(
                "user-command-that-is-gone",
                "Gone",
                BTreeMap::new(),
            )],
        )],
    );
    update(&mut pane, RunbookMessage::Run(ids[0].clone()));
    assert_eq!(
        pane.current_problem(&UserLibrary::default()),
        Some(RunProblem::ActionMissing)
    );
}

#[test]
fn a_literal_step_with_a_control_or_invisible_character_is_refused_at_the_run() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(
        folder.path(),
        vec![runbook(
            "Plain",
            &[],
            vec![Step::literal("uptime", TemplatePlatform::Linux)],
        )],
    );
    update(&mut pane, RunbookMessage::Run(ids[0].clone()));
    let library = UserLibrary::default();
    // A file edited after the load, or a model that let one through: the run does not trust
    // either.
    for (hidden, problem) in [
        ("ls\nrm -rf x", RunProblem::Hidden),
        ("ls\u{202E}x", RunProblem::Hidden),
        ("ls\u{200B}x", RunProblem::Hidden),
        ("ls\u{1b}[2J", RunProblem::Hidden),
        ("   ", RunProblem::CommandMissing),
    ] {
        if let Some(run) = pane.run.as_mut() {
            run.runbook.steps[0] = Step::literal(hidden, TemplatePlatform::Linux);
        }
        assert_eq!(pane.current_problem(&library), Some(problem), "{hidden:?}");
        assert_eq!(pane.ready_step(&library), None, "{hidden:?}");
    }
}

#[test]
fn a_refused_value_names_its_parameter_for_the_message() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(
        folder.path(),
        vec![runbook(
            "Trace",
            &[("t", "web.lab; reboot")],
            vec![trace("{{t}}")],
        )],
    );
    update(&mut pane, RunbookMessage::Run(ids[0].clone()));
    update(&mut pane, RunbookMessage::RunBegin);
    let Some(RunProblem::Values(errors)) = pane.current_problem(&UserLibrary::default()) else {
        panic!("refused");
    };
    assert!(
        matches!(errors.first(), Some(ValueError::Dangerous(label) | ValueError::Hostname(label)) if !label.is_empty())
    );
}

#[test]
fn a_step_moves_on_when_delivered_and_a_stale_receipt_changes_nothing() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(
        folder.path(),
        vec![runbook(
            "Two",
            &[],
            vec![
                Step::literal("uptime", TemplatePlatform::Linux),
                Step::literal("df -h", TemplatePlatform::Linux),
            ],
        )],
    );
    update(&mut pane, RunbookMessage::Run(ids[0].clone()));
    // A receipt for a step the run is not at.
    pane.step_delivered(pane.run_id(), 1, Delivered::Sent);
    assert_eq!(pane.current_index(), Some(0));
    assert_eq!(pane.status_of(0), Some(StepStatus::Pending));

    pane.step_delivered(pane.run_id(), 0, Delivered::Inserted);
    assert_eq!(pane.status_of(0), Some(StepStatus::Inserted));
    assert_eq!(pane.current_index(), Some(1));
    // The same receipt twice does not move the run again.
    pane.step_delivered(pane.run_id(), 0, Delivered::Sent);
    assert_eq!(pane.current_index(), Some(1));

    pane.step_delivered(pane.run_id(), 1, Delivered::Copied);
    assert_eq!(pane.status_of(1), Some(StepStatus::Copied));
    assert!(pane.is_finished());
    assert_eq!(pane.ready_step(&UserLibrary::default()), None);
}

#[test]
fn skip_passes_over_a_step_without_sending_it_and_stop_leaves_the_run() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(
        folder.path(),
        vec![runbook(
            "Two",
            &[],
            vec![
                Step::literal("uptime", TemplatePlatform::Linux),
                Step::literal("df -h", TemplatePlatform::Linux),
            ],
        )],
    );
    update(&mut pane, RunbookMessage::Run(ids[0].clone()));
    pane.set_guard(Duration::ZERO);
    update(&mut pane, RunbookMessage::RunSkip);
    assert_eq!(pane.status_of(0), Some(StepStatus::Skipped));
    assert_eq!(pane.current_index(), Some(1));
    update(&mut pane, RunbookMessage::RunSkip);
    assert_eq!(pane.status_of(1), Some(StepStatus::Skipped));
    assert!(pane.is_finished());
    // Nothing left to skip.
    update(&mut pane, RunbookMessage::RunSkip);
    assert!(pane.is_finished());

    update(&mut pane, RunbookMessage::RunStop);
    assert!(!pane.is_running());
}

#[test]
fn a_skip_before_the_values_are_confirmed_does_nothing() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(
        folder.path(),
        vec![runbook(
            "Ask",
            &[("a", "1")],
            vec![Step::literal("uptime", TemplatePlatform::Linux)],
        )],
    );
    update(&mut pane, RunbookMessage::Run(ids[0].clone()));
    update(&mut pane, RunbookMessage::RunSkip);
    assert_eq!(pane.status_of(0), Some(StepStatus::Pending));
    assert!(pane.is_asking());
}

#[test]
fn a_run_of_a_runbook_with_no_step_is_finished_at_once() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(folder.path(), vec![runbook("Empty", &[], Vec::new())]);
    update(&mut pane, RunbookMessage::Run(ids[0].clone()));
    assert!(pane.is_finished());
    assert_eq!(pane.ready_step(&UserLibrary::default()), None);
}

#[test]
fn a_run_of_a_runbook_that_is_not_there_starts_nothing() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, _) = pane_with(folder.path(), Vec::new());
    update(&mut pane, RunbookMessage::Run("nope".to_owned()));
    assert!(!pane.is_running());
}

#[test]
fn the_dangerous_question_follows_the_stored_level_the_paste_guard_and_a_literal_step() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(
        folder.path(),
        vec![runbook(
            "Mixed",
            &[],
            vec![
                trace("web.lab"),
                Step::literal("echo hello", TemplatePlatform::Linux),
            ],
        )],
    );
    pane.set_session(None, None, RemoteOs::Linux);
    update(&mut pane, RunbookMessage::Run(ids[0].clone()));
    let library = UserLibrary::default();
    let action = pane.ready_step(&library).expect("action step");
    assert!(!action.is_dangerous(), "an info action asks nothing more");
    pane.step_delivered(pane.run_id(), 0, Delivered::Sent);
    let literal = pane.ready_step(&library).expect("literal step");
    assert!(literal.is_dangerous(), "a literal step always asks");
}

#[test]
fn a_skip_just_after_another_is_a_double_press_and_changes_nothing() {
    let folder = tempfile::tempdir().expect("folder");
    let (mut pane, ids) = pane_with(
        folder.path(),
        vec![runbook(
            "Two",
            &[],
            vec![
                Step::literal("uptime", TemplatePlatform::Linux),
                Step::literal("df -h", TemplatePlatform::Linux),
            ],
        )],
    );
    update(&mut pane, RunbookMessage::Run(ids[0].clone()));
    update(&mut pane, RunbookMessage::RunSkip);
    update(&mut pane, RunbookMessage::RunSkip);
    assert_eq!(pane.status_of(0), Some(StepStatus::Skipped));
    assert_eq!(pane.status_of(1), Some(StepStatus::Pending));
    assert_eq!(pane.current_index(), Some(1));
}
