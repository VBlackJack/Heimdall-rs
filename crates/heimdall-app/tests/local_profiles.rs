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

//! Saved local profiles: nothing runs that the user has not seen and agreed to, and what
//! runs is what was agreed to.

use std::path::{Path, PathBuf};

use heimdall_app::local_driver::LocalShell;
use heimdall_app::{App, AppConfig, Dialog, Effect, Message};
use heimdall_core::profile::{
    LocalApproval, LocalArguments, LocalCommand, LocalProfile, ProfileId,
};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

/// A program found on every machine the tests run on, by its full path.
#[cfg(unix)]
const PROGRAM: &str = "/bin/sh";
#[cfg(windows)]
const PROGRAM: &str = r"C:\Windows\System32\cmd.exe";

fn profiles_file(dir: &Path) -> PathBuf {
    dir.join("profiles.toml")
}

fn app(dir: &Path, profiles: &[LocalProfile]) -> App {
    let mut store = ProfileStore::open(profiles_file(dir)).expect("store");
    store.merge_local(profiles.iter().cloned());
    for profile in profiles {
        if let Some(approval) = &profile.approved {
            store.approve_local(&profile.id, approval.clone());
        }
    }
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file: profiles_file(dir),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
    })
}

fn command(argument: &str) -> LocalCommand {
    LocalCommand {
        program: Some(PROGRAM.to_owned()),
        arguments: LocalArguments::List(vec![argument.to_owned()]),
        working_directory: None,
    }
}

fn profile(command: LocalCommand, approved: Option<LocalApproval>) -> LocalProfile {
    LocalProfile {
        id: ProfileId::new("tool"),
        name: "Tool".to_owned(),
        group: None,
        command,
        approved,
    }
}

fn approval(command: LocalCommand) -> LocalApproval {
    LocalApproval {
        command,
        program_path: PathBuf::from(PROGRAM),
    }
}

fn open(app: &mut App) -> Vec<Effect> {
    app.update(Message::OpenLocalProfile(ProfileId::new("tool")))
}

/// The shell a tab was started with, when opening started one.
fn started(effects: &[Effect]) -> Option<LocalShell> {
    effects.iter().find_map(|effect| match effect {
        Effect::ConnectLocal { request, .. } => Some(request.shell.clone()),
        _ => None,
    })
}

/// The program a tab was started with; `None` when none was started.
fn program(effects: &[Effect]) -> Option<String> {
    started(effects).and_then(|shell| shell.program)
}

#[test]
fn an_unapproved_command_waits_for_agreement_and_then_runs_as_approved() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[profile(command("-x"), None)]);
    let effects = open(&mut app);
    assert!(started(&effects).is_none(), "nothing runs before agreeing");
    assert!(app.tabs.is_empty());
    let Some(Dialog::ConfirmLocalCommand(confirmation)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(confirmation.approval, approval(command("-x")));
    assert!(
        confirmation.command.contains("-x"),
        "{}",
        confirmation.command
    );

    let effects = app.update(Message::ConfirmDialog);
    assert_eq!(program(&effects), Some(PROGRAM.to_owned()));
    // Recorded in the file, so the next opening does not ask.
    let stored = ProfileStore::open(profiles_file(dir.path())).expect("reopen");
    assert_eq!(
        stored.local_profiles()[0].approved,
        Some(approval(command("-x")))
    );
    assert!(app.dialog.is_none());
    let effects = open(&mut app);
    assert!(app.dialog.is_none(), "approved: no question");
    assert_eq!(program(&effects), Some(PROGRAM.to_owned()));
}

#[test]
fn an_approved_command_runs_at_once_and_a_changed_one_asks_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(
        dir.path(),
        &[profile(command("-x"), Some(approval(command("-x"))))],
    );
    assert!(started(&open(&mut app)).is_some());
    assert!(app.dialog.is_none());

    let changed = tempfile::tempdir().expect("dir");
    let mut app = app_with_changed_command(changed.path());
    assert!(started(&open(&mut app)).is_none());
    assert!(matches!(app.dialog, Some(Dialog::ConfirmLocalCommand(_))));
}

/// A profile approved for `-x` that now runs `-y`.
fn app_with_changed_command(dir: &Path) -> App {
    app(
        dir,
        &[profile(command("-y"), Some(approval(command("-x"))))],
    )
}

/// A program by its bare name, found in `PATH` on every machine the tests run on.
#[cfg(unix)]
const BARE: &str = "sh";
#[cfg(windows)]
const BARE: &str = "cmd.exe";

#[test]
fn what_runs_is_the_file_approved_not_whatever_the_name_finds_later() {
    let dir = tempfile::tempdir().expect("dir");
    let mut bare = command("-x");
    bare.program = Some(BARE.to_owned());
    let mut app = app(dir.path(), &[profile(bare, None)]);
    let _ = open(&mut app);
    let Some(Dialog::ConfirmLocalCommand(confirmation)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    let found = confirmation.approval.program_path.clone();
    assert!(found.is_absolute(), "{found:?}");
    let effects = app.update(Message::ConfirmDialog);
    assert_eq!(
        program(&effects),
        Some(found.to_string_lossy().into_owned()),
        "the full path agreed to, not the name"
    );
}

#[test]
fn dismissing_runs_nothing_and_records_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[profile(command("-x"), None)]);
    let _ = open(&mut app);
    let effects = app.update(Message::DismissDialog);
    assert!(started(&effects).is_none());
    assert!(app.tabs.is_empty());
    let stored = ProfileStore::open(profiles_file(dir.path())).expect("reopen");
    assert_eq!(stored.local_profiles()[0].approved, None);
}

#[test]
fn the_default_shell_runs_without_asking() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[profile(LocalCommand::default(), None)]);
    let effects = open(&mut app);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let program = program(&effects).expect("a program");
    assert!(
        Path::new(&program).is_absolute(),
        "run by its full path: {program}"
    );
}

#[test]
fn a_program_found_nowhere_opens_a_tab_that_fails_rather_than_asking() {
    let dir = tempfile::tempdir().expect("dir");
    let mut missing = command("-x");
    missing.program = Some("heimdall-no-such-program".to_owned());
    let mut app = app(dir.path(), &[profile(missing, None)]);
    let effects = open(&mut app);
    assert!(app.dialog.is_none(), "nothing to agree to: nothing can run");
    assert_eq!(
        program(&effects),
        Some("heimdall-no-such-program".to_owned())
    );
}
