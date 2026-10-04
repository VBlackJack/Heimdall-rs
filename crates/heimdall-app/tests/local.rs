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

//! A local shell tab: opened from a message, fed by a real shell on this machine (`sh` on
//! Unix, `cmd` on Windows), and shown in its terminal.

use std::path::Path;
use std::time::Duration;

use heimdall_app::local_driver::{LocalRequest, LocalShell, local_events};
use heimdall_app::{
    App, AppConfig, AttemptId, Effect, Message, Phase, Purpose, TabId, TabProfile, UiError,
};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_term::local::LocalArguments;
use tokio_stream::StreamExt as _;

/// Bound on anything the test waits for.
const WAIT: Duration = Duration::from_secs(15);

fn app(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn shell(program: &str, args: &[&str]) -> LocalShell {
    LocalShell {
        name: "Shell".to_owned(),
        program: Some(program.to_owned()),
        arguments: LocalArguments::List(args.iter().map(|arg| (*arg).to_owned()).collect()),
        working_directory: None,
        environment: Vec::new(),
    }
}

#[cfg(unix)]
fn script(script: &str) -> LocalShell {
    shell("/bin/sh", &["-c", script])
}

#[cfg(windows)]
fn script(script: &str) -> LocalShell {
    shell("cmd.exe", &["/C", script])
}

/// A local tab, its attempt and the request its opening made.
fn open(app: &mut App, shell: LocalShell) -> (TabId, AttemptId, LocalRequest) {
    let effects = app.update(Message::OpenLocal(shell));
    let Some(Effect::ConnectLocal {
        tab,
        attempt,
        request,
    }) = effects.into_iter().next()
    else {
        panic!("no local attempt");
    };
    (tab, attempt, *request)
}

/// Feeds every event of the attempt to the application. A silence that runs out says what
/// came before it: a shell still starting shows nothing, one whose exit was lost its output.
async fn follow(app: &mut App, tab: TabId, attempt: AttemptId, request: LocalRequest) {
    let started = std::time::Instant::now();
    let mut events = local_events(request);
    loop {
        let Ok(next) = tokio::time::timeout(WAIT, events.next()).await else {
            panic!(
                "silent for {WAIT:?}, {:?} after the first wait, screen so far:\n{}",
                started.elapsed(),
                screen(app, tab)
            );
        };
        let Some(event) = next else {
            return;
        };
        let _ = app.update(Message::Connection {
            tab,
            attempt,
            event,
        });
    }
}

/// The terminal's text, rows joined.
fn screen(app: &App, tab: TabId) -> String {
    let snapshot = app.tab(tab).expect("tab").terminal.snapshot();
    (0..24)
        .map(|row| snapshot.row_text(row).trim_end().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn opening_a_local_shell_starts_a_terminal_tab_in_the_home_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _, request) = open(&mut app, shell("sh", &[]));
    assert_eq!(request.shell.working_directory.as_deref(), Some(dir.path()));
    assert_eq!((request.size.cols, request.size.rows), (80, 24));
    let tab = app.tab(tab).expect("tab");
    assert!(matches!(tab.profile, TabProfile::Local(_)));
    assert_eq!(tab.profile.endpoint(), None);
    assert_eq!(tab.purpose, Purpose::Shell);
    assert_eq!(tab.phase, Phase::Connecting);
    assert_eq!(tab.title, "Shell");
}

#[test]
fn a_folder_the_shell_names_is_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let elsewhere = tempfile::tempdir().expect("elsewhere");
    let mut app = app(dir.path());
    let mut named = shell("sh", &[]);
    named.working_directory = Some(elsewhere.path().to_owned());
    let (_, _, request) = open(&mut app, named);
    assert_eq!(
        request.shell.working_directory.as_deref(),
        Some(elsewhere.path())
    );
}

#[tokio::test]
async fn a_local_shell_shows_in_its_tab_and_reports_its_exit_code() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt, request) = open(&mut app, script("echo hello&& exit 5"));
    follow(&mut app, tab, attempt, request).await;
    assert_eq!(
        app.tab(tab).expect("tab").phase,
        Phase::Closed {
            exit_status: Some(5)
        }
    );
    let screen = screen(&app, tab);
    assert!(screen.contains("hello"), "{screen}");
}

#[cfg(unix)]
#[tokio::test]
async fn a_folder_that_is_not_there_falls_back_to_the_home_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let mut gone = script("pwd -P");
    gone.working_directory = Some(dir.path().join("removed"));
    let (tab, attempt, request) = open(&mut app, gone);
    follow(&mut app, tab, attempt, request).await;
    let home = dir.path().canonicalize().expect("canonical");
    let screen = screen(&app, tab);
    assert!(
        screen.contains(home.to_str().expect("utf-8")),
        "{home:?} in {screen}"
    );
    assert_eq!(
        app.tab(tab).expect("tab").phase,
        Phase::Closed {
            exit_status: Some(0)
        },
        "started, not failed"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_local_shell_starts_in_the_home_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt, request) = open(&mut app, script("pwd -P"));
    follow(&mut app, tab, attempt, request).await;
    let home = dir.path().canonicalize().expect("canonical");
    let screen = screen(&app, tab);
    assert!(
        screen.contains(home.to_str().expect("utf-8")),
        "{home:?} in {screen}"
    );
}

#[tokio::test]
async fn a_program_that_does_not_exist_fails_the_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let missing = dir.path().join("no-such-shell");
    let (tab, attempt, request) = open(&mut app, shell(missing.to_str().expect("utf-8"), &[]));
    follow(&mut app, tab, attempt, request).await;
    let phase = &app.tab(tab).expect("tab").phase;
    assert!(
        matches!(phase, Phase::Failed(UiError::LocalShell { .. })),
        "{phase:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn closing_the_tab_ends_the_shell() {
    use heimdall_app::ConnectionEvent;
    use std::time::Instant;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt, request) = open(&mut app, script("echo ready; sleep 100"));
    let cancel = request.cancel.clone();
    let mut events = local_events(request);
    let started = loop {
        let event = tokio::time::timeout(WAIT, events.next())
            .await
            .expect("in time")
            .expect("an event");
        let ready = matches!(
            &event,
            ConnectionEvent::Output(bytes) if bytes.windows(5).any(|w| w == b"ready")
        );
        let _ = app.update(Message::Connection {
            tab,
            attempt,
            event,
        });
        if ready {
            break Instant::now();
        }
    };
    // What closing the tab does to its attempt.
    cancel.cancel();
    let mut last = None;
    while let Some(event) = tokio::time::timeout(WAIT, events.next())
        .await
        .expect("in time")
    {
        last = Some(event);
    }
    assert!(
        matches!(last, Some(ConnectionEvent::Closed { exit_status: None })),
        "hung up, so no exit code: {last:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
}
