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

//! Local profiles run as administrator: never in a tab, never unelevated. On Windows the
//! application asks for the start in a window of its own, which these tests never carry out,
//! so no elevation prompt is ever raised; elsewhere nothing is started at all.

use std::path::{Path, PathBuf};

use heimdall_app::{App, AppConfig, Dialog, Effect, ElevatedState, Message, Phase};
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

fn app(dir: &Path, profile: &LocalProfile) -> App {
    let mut store = ProfileStore::open(profiles_file(dir)).expect("store");
    store.merge_local([profile.clone()]);
    if let Some(approval) = &profile.approved {
        store.approve_local(&profile.id, approval.clone());
    }
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file: profiles_file(dir),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn elevated(folder: Option<PathBuf>) -> LocalCommand {
    LocalCommand {
        program: Some(PROGRAM.to_owned()),
        arguments: LocalArguments::List(vec!["/k".to_owned(), "two words".to_owned()]),
        working_directory: folder,
        run_as_administrator: true,
    }
}

fn profile(command: LocalCommand, approved: bool) -> LocalProfile {
    LocalProfile {
        id: ProfileId::new("admin"),
        name: "Admin shell".to_owned(),
        group: None,
        approved: approved.then(|| LocalApproval {
            command: command.clone(),
            program_path: PathBuf::from(PROGRAM),
        }),
        command,
        session_logging: None,
    }
}

fn open(app: &mut App) -> Vec<Effect> {
    app.update(Message::OpenLocalProfile(ProfileId::new("admin")))
}

/// Whether any effect would start something in a tab.
fn runs_in_a_tab(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::ConnectLocal { .. }))
}

/// The requests to start elevated among `effects`, with their tab.
fn launches(effects: &[Effect]) -> Vec<(heimdall_app::TabId, ElevatedLaunchView)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::LaunchElevated { tab, request } => Some((
                *tab,
                ElevatedLaunchView {
                    program: request.program.clone(),
                    verb: request.verb,
                    parameters: request.parameters.clone(),
                    working_directory: request.working_directory.clone(),
                },
            )),
            _ => None,
        })
        .collect()
}

/// What a request asks of Windows.
#[derive(Debug, PartialEq, Eq)]
struct ElevatedLaunchView {
    program: PathBuf,
    verb: &'static str,
    parameters: String,
    working_directory: PathBuf,
}

fn state(app: &App) -> Option<ElevatedState> {
    app.tabs
        .last()
        .and_then(|tab| tab.elevated.as_ref())
        .map(|pane| pane.state.clone())
}

#[test]
fn an_approved_elevated_profile_never_starts_a_terminal_in_its_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &profile(elevated(None), true));
    let effects = open(&mut app);
    assert!(app.dialog.is_none(), "approved: no question");
    assert!(!runs_in_a_tab(&effects), "{effects:?}");
    assert_eq!(app.tabs.len(), 1);
    let tab = &app.tabs[0];
    assert_eq!(tab.title, "Admin shell");
    let pane = tab.elevated.as_ref().expect("an elevated pane");
    assert_eq!(pane.program.as_deref(), Some(PROGRAM));
    if cfg!(windows) {
        assert_eq!(pane.state, ElevatedState::Starting);
        assert_eq!(tab.phase, Phase::Connecting);
        assert_eq!(
            launches(&effects),
            [(
                tab.id,
                ElevatedLaunchView {
                    program: PathBuf::from(PROGRAM),
                    verb: "runas",
                    parameters: "/k \"two words\"".to_owned(),
                    // No folder named: the home folder, as the C#.
                    working_directory: dir.path().to_owned(),
                }
            )]
        );
    } else {
        // No elevation prompt here: nothing is started, and the tab says why.
        assert!(effects.is_empty(), "{effects:?}");
        assert_eq!(pane.state, ElevatedState::Unsupported);
        assert!(matches!(tab.phase, Phase::Failed(_)), "{:?}", tab.phase);
    }
}

#[test]
fn the_folder_the_profile_names_is_the_one_asked_for() {
    let dir = tempfile::tempdir().expect("dir");
    let folder = dir.path().join("work");
    let mut app = app(dir.path(), &profile(elevated(Some(folder.clone())), true));
    let effects = open(&mut app);
    assert!(!runs_in_a_tab(&effects));
    if cfg!(windows) {
        let asked = launches(&effects);
        assert_eq!(asked.len(), 1, "{effects:?}");
        assert_eq!(asked[0].1.working_directory, folder);
    } else {
        assert!(launches(&effects).is_empty());
    }
}

#[test]
fn an_unapproved_elevated_profile_says_it_runs_as_administrator_before_it_does() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &profile(elevated(None), false));
    let effects = open(&mut app);
    assert!(effects.is_empty(), "nothing before agreeing: {effects:?}");
    assert!(app.tabs.is_empty());
    let Some(Dialog::ConfirmLocalCommand(confirmation)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert!(confirmation.elevated);
    assert!(confirmation.approval.command.run_as_administrator);

    let effects = app.update(Message::ConfirmDialog);
    assert!(!runs_in_a_tab(&effects), "agreed, still never in a tab");
    assert_eq!(launches(&effects).len(), usize::from(cfg!(windows)));
}

#[test]
fn approved_unelevated_the_same_command_elevated_is_asked_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut plain = elevated(None);
    plain.run_as_administrator = false;
    let mut approved = profile(elevated(None), false);
    approved.approved = Some(LocalApproval {
        command: plain,
        program_path: PathBuf::from(PROGRAM),
    });
    let mut app = app(dir.path(), &approved);
    let effects = open(&mut app);
    assert!(effects.is_empty(), "{effects:?}");
    assert!(matches!(app.dialog, Some(Dialog::ConfirmLocalCommand(_))));
}

#[test]
fn a_program_found_nowhere_is_said_and_nothing_runs() {
    let dir = tempfile::tempdir().expect("dir");
    let mut command = elevated(None);
    command.program = Some("no-such-shell-anywhere".to_owned());
    let mut app = app(dir.path(), &profile(command, false));
    let effects = open(&mut app);
    assert!(effects.is_empty(), "{effects:?}");
    let failed = state(&app).expect("a tab says why");
    if cfg!(windows) {
        assert!(matches!(failed, ElevatedState::Failed(_)), "{failed:?}");
    } else {
        assert_eq!(failed, ElevatedState::Unsupported);
    }
}

#[cfg(windows)]
mod on_windows {
    use heimdall_app::elevated_shell::ElevatedOutcome;
    use heimdall_app::{Effect, ElevatedState, Message, Phase, UiError};

    use super::{app, elevated, launches, open, profile, runs_in_a_tab, state};

    /// Opens the approved profile, then answers its start with `outcome`, as the UI would once
    /// Windows answered: the effects of that answer.
    fn answered(
        dir: &std::path::Path,
        outcome: ElevatedOutcome,
    ) -> (heimdall_app::App, Vec<Effect>) {
        let mut app = app(dir, &profile(elevated(None), true));
        let asked = launches(&open(&mut app));
        let tab = asked[0].0;
        let effects = app.update(Message::ElevatedLaunched { tab, outcome });
        (app, effects)
    }

    #[test]
    fn the_prompt_declined_is_said_cancelled_and_nothing_else_runs() {
        let dir = tempfile::tempdir().expect("dir");
        let (app, effects) = answered(dir.path(), ElevatedOutcome::Cancelled);
        assert!(effects.is_empty(), "{effects:?}");
        assert_eq!(state(&app), Some(ElevatedState::Cancelled));
        assert_eq!(app.tabs[0].phase, Phase::Failed(UiError::Cancelled));
    }

    #[test]
    fn a_start_refused_is_said_failed_and_nothing_runs_unelevated() {
        let dir = tempfile::tempdir().expect("dir");
        let (app, effects) = answered(dir.path(), ElevatedOutcome::Failed("refused".to_owned()));
        assert!(effects.is_empty(), "{effects:?}");
        assert!(!runs_in_a_tab(&effects));
        assert_eq!(
            state(&app),
            Some(ElevatedState::Failed("refused".to_owned()))
        );
        assert!(matches!(app.tabs[0].phase, Phase::Failed(_)));
    }

    #[test]
    fn once_started_the_tab_offers_to_open_it_again_in_another_window() {
        let dir = tempfile::tempdir().expect("dir");
        let (mut app, effects) = answered(dir.path(), ElevatedOutcome::Started);
        assert!(effects.is_empty(), "{effects:?}");
        assert_eq!(state(&app), Some(ElevatedState::Started));
        let tab = app.tabs[0].id;
        assert!(!app.tabs[0].is_live(), "nothing is lost by closing it");
        assert!(app.can_reconnect(&app.tabs[0]));
        let again = app.update(Message::ReconnectTab(tab));
        assert!(!runs_in_a_tab(&again), "{again:?}");
        assert_eq!(launches(&again).len(), 1);
        assert_eq!(app.tabs.len(), 1, "in its place");
        assert_eq!(state(&app), Some(ElevatedState::Starting));
    }

    #[test]
    fn a_late_answer_changes_nothing() {
        let dir = tempfile::tempdir().expect("dir");
        let (mut app, _) = answered(dir.path(), ElevatedOutcome::Cancelled);
        let tab = app.tabs[0].id;
        let effects = app.update(Message::ElevatedLaunched {
            tab,
            outcome: ElevatedOutcome::Started,
        });
        assert!(effects.is_empty());
        assert_eq!(state(&app), Some(ElevatedState::Cancelled));
    }
}

#[cfg(not(windows))]
#[test]
fn elsewhere_an_answer_for_the_tab_changes_nothing() {
    use heimdall_app::elevated_shell::ElevatedOutcome;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &profile(elevated(None), true));
    let _ = open(&mut app);
    let tab = app.tabs[0].id;
    let effects = app.update(Message::ElevatedLaunched {
        tab,
        outcome: ElevatedOutcome::Started,
    });
    assert!(effects.is_empty());
    assert_eq!(state(&app), Some(ElevatedState::Unsupported));
    assert!(matches!(
        app.tabs[0].phase,
        Phase::Failed(heimdall_app::UiError::LocalShell { .. })
    ));
}
