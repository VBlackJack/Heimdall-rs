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

//! Windows Hello asked before connecting, as the C# gate: a saved session waits for the
//! answer, a success counts for the grace, anything else refuses, said once; a batch is
//! asked once. The answers are given here: no prompt is ever raised.

use std::path::Path;
use std::sync::Arc;

use heimdall_app::local_driver::LocalShell;
use heimdall_app::windows_hello::HelloRefusal;
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, Notice, QuickResult,
    SelectionMessage, SettingsMessage, TabId, UiError,
};
use heimdall_core::profile::{
    LocalApproval, LocalArguments, LocalCommand, LocalProfile, ProfileId, SshProfile,
};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn ssh(id: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
        ssh_mode: heimdall_core::profile::SshMode::Embedded,
        x11_forwarding: false,
    }
}

/// A program found on every machine the tests run on, by its full path.
#[cfg(unix)]
const PROGRAM: &str = "/bin/sh";
#[cfg(windows)]
const PROGRAM: &str = r"C:\Windows\System32\cmd.exe";

/// A saved local program, already approved: it runs once opened.
fn tool() -> LocalProfile {
    let command = LocalCommand {
        program: Some(PROGRAM.to_owned()),
        arguments: LocalArguments::List(Vec::new()),
        working_directory: None,
        run_as_administrator: false,
    };
    LocalProfile {
        id: ProfileId::new("tool"),
        name: "Tool".to_owned(),
        group: None,
        command: command.clone(),
        approved: Some(LocalApproval {
            command,
            program_path: PROGRAM.into(),
        }),
        session_logging: None,
    }
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge(["a", "b"].map(ssh));
    let tool = tool();
    let approval = tool.approved.clone().expect("approved");
    store.merge_local([tool]);
    store.approve_local(&ProfileId::new("tool"), approval);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

/// The application with Windows Hello required, verifications counting `grace` minutes.
fn gated(dir: &Path, grace: u32) -> App {
    let mut app = app(dir);
    app.update(Message::Settings(SettingsMessage::RequireWindowsHello(
        true,
    )));
    app.update(Message::Settings(
        SettingsMessage::WindowsHelloGraceMinutes(grace),
    ));
    app
}

fn open(app: &mut App, id: &str) -> Vec<Effect> {
    app.update(Message::OpenProfile(ProfileId::new(id)))
}

fn asked(effects: &[Effect]) -> bool {
    matches!(effects, [Effect::VerifyWindowsHello])
}

fn connects(effects: &[Effect]) -> Vec<(TabId, AttemptId)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Connect { tab, attempt, .. } => Some((*tab, *attempt)),
            _ => None,
        })
        .collect()
}

#[test]
fn off_by_default_a_connection_opens_without_asking() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert!(!app.settings().windows_hello.require_on_connect);
    assert_eq!(connects(&open(&mut app, "a")).len(), 1);
}

#[test]
fn a_success_lets_every_waiting_connection_through_and_counts_for_the_grace() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = gated(dir.path(), 5);
    assert!(asked(&open(&mut app, "a")));
    assert!(app.tabs.is_empty(), "nothing opens before the answer");
    // Asked once: the second waits for the same answer.
    assert!(open(&mut app, "b").is_empty());
    let effects = app.update(Message::WindowsHello(Ok(())));
    assert_eq!(connects(&effects).len(), 2, "{effects:?}");
    assert_eq!(app.tabs.len(), 2);
    // Within the grace, the next one opens at once.
    assert_eq!(connects(&open(&mut app, "a")).len(), 1);
}

#[test]
fn no_grace_asks_every_time() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = gated(dir.path(), 0);
    assert!(asked(&open(&mut app, "a")));
    assert_eq!(
        connects(&app.update(Message::WindowsHello(Ok(())))).len(),
        1
    );
    assert!(asked(&open(&mut app, "a")), "0 always asks again");
}

#[test]
fn unavailable_failed_or_cancelled_refuses_says_why_and_gives_no_grace() {
    for refusal in [
        HelloRefusal::Unavailable,
        HelloRefusal::NotVerified,
        HelloRefusal::Cancelled,
    ] {
        let dir = tempfile::tempdir().expect("dir");
        let mut app = gated(dir.path(), 5);
        assert!(asked(&open(&mut app, "a")));
        assert!(open(&mut app, "b").is_empty());
        let effects = app.update(Message::WindowsHello(Err(refusal)));
        assert!(effects.is_empty(), "{refusal:?}: {effects:?}");
        assert!(app.tabs.is_empty(), "{refusal:?}: fail closed");
        assert_eq!(
            app.notice(),
            Some(&Notice::WindowsHelloRefused(refusal)),
            "{refusal:?}"
        );
        assert!(asked(&open(&mut app, "a")), "{refusal:?}: asked again");
    }
}

#[test]
fn connect_selected_is_asked_once_and_refused_at_once() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = gated(dir.path(), 5);
    app.update(Message::SelectProfile(ProfileId::new("a")));
    app.update(Message::Selection(SelectionMessage::Toggle(
        ProfileId::new("b"),
    )));
    assert!(asked(
        &app.update(Message::Selection(SelectionMessage::Connect))
    ));
    app.update(Message::WindowsHello(Err(HelloRefusal::NotVerified)));
    assert!(app.tabs.is_empty());

    assert!(asked(
        &app.update(Message::Selection(SelectionMessage::Connect))
    ));
    let effects = app.update(Message::WindowsHello(Ok(())));
    assert_eq!(connects(&effects).len(), 2, "{effects:?}");
}

#[test]
fn a_session_saved_nowhere_does_not_wait() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = gated(dir.path(), 5);
    let effects = app.update(Message::QuickConnect(QuickResult::Ssh {
        username: None,
        host: "adhoc.lab".to_owned(),
        port: 22,
    }));
    assert_eq!(connects(&effects).len(), 1, "{effects:?}");
}

fn starts(effects: &[Effect]) -> Vec<(TabId, AttemptId)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ConnectLocal { tab, attempt, .. } => Some((*tab, *attempt)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_saved_local_program_waits_when_opened_and_reconnected() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let open = || Message::OpenLocalProfile(ProfileId::new("tool"));
    // Off: it runs at once, as before.
    let [(tab, attempt)] = starts(&app.update(open()))[..] else {
        panic!("one start");
    };
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::Timeout),
    });
    app.update(Message::Settings(SettingsMessage::RequireWindowsHello(
        true,
    )));
    assert!(asked(&app.update(open())));
    let effects = app.update(Message::WindowsHello(Err(HelloRefusal::NotVerified)));
    assert!(starts(&effects).is_empty(), "{effects:?}");
    assert_eq!(app.tabs.len(), 1, "refused: nothing more runs");

    assert!(asked(&app.update(Message::ReconnectTab(tab))));
    let effects = app.update(Message::WindowsHello(Ok(())));
    let [(again, _)] = starts(&effects)[..] else {
        panic!("{effects:?}");
    };
    assert_eq!(app.tabs.len(), 1, "in its place");
    assert_eq!(app.tabs[0].id, again);
}

#[test]
fn a_saved_local_program_restored_waits_with_the_others_asked_once() {
    let dir = tempfile::tempdir().expect("dir");
    let mut before = app(dir.path());
    assert_eq!(connects(&open(&mut before, "a")).len(), 1);
    let tool = Message::OpenLocalProfile(ProfileId::new("tool"));
    assert_eq!(starts(&before.update(tool)).len(), 1);
    before.update(Message::WindowCloseRequested);

    let mut app = gated(dir.path(), 0);
    app.offer_restore();
    assert!(
        asked(&app.update(Message::ConfirmDialog)),
        "{:?}",
        app.dialog
    );
    assert!(app.tabs.is_empty(), "nothing opens before the answer");
    let effects = app.update(Message::WindowsHello(Ok(())));
    assert_eq!(connects(&effects).len(), 1, "{effects:?}");
    assert_eq!(starts(&effects).len(), 1, "{effects:?}");
}

#[test]
fn a_local_shell_without_a_profile_does_not_wait() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = gated(dir.path(), 5);
    let effects = app.update(Message::OpenLocal(LocalShell {
        name: "shell".to_owned(),
        program: Some(PROGRAM.to_owned()),
        arguments: heimdall_term::local::LocalArguments::List(Vec::new()),
        working_directory: None,
        environment: Vec::new(),
    }));
    assert_eq!(starts(&effects).len(), 1, "{effects:?}");
}

#[test]
fn a_reconnect_waits_then_opens_in_its_tab_s_place() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let [(tab, attempt)] = connects(&open(&mut app, "a"))[..] else {
        panic!("one connect");
    };
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::Timeout),
    });
    app.update(Message::Settings(SettingsMessage::RequireWindowsHello(
        true,
    )));
    assert!(asked(&app.update(Message::ReconnectTab(tab))));
    let effects = app.update(Message::WindowsHello(Ok(())));
    let [(again, _)] = connects(&effects)[..] else {
        panic!("{effects:?}");
    };
    assert_eq!(app.tabs.len(), 1, "in its place");
    assert_eq!(app.tabs[0].id, again);
}

#[derive(Debug)]
struct NullSink;

impl heimdall_app::InputSink for NullSink {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }
    fn resize(&self, _size: heimdall_ssh::TerminalSize) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

#[test]
fn an_auto_reconnect_waits_and_a_refusal_stops_its_chain() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Settings(SettingsMessage::SshAutoReconnect(true)));
    let [(tab, attempt)] = connects(&open(&mut app, "a"))[..] else {
        panic!("one connect");
    };
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    });
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::ConnectionLost),
    });
    assert!(app.tab(tab).expect("tab").retry.is_some(), "waiting");
    app.update(Message::Settings(SettingsMessage::RequireWindowsHello(
        true,
    )));
    assert!(asked(&app.update(Message::AutoReconnect { tab, attempt })));
    let before = app.tabs.len();
    let effects = app.update(Message::WindowsHello(Err(HelloRefusal::Cancelled)));
    assert!(connects(&effects).is_empty(), "{effects:?}");
    let found = app.tab(tab).expect("tab");
    assert_eq!(
        found.retry, None,
        "the chain stops: Reconnect is the user's"
    );
    assert_eq!(app.tabs.len(), before, "nothing opens");
}

#[test]
fn the_grace_out_of_the_csharp_range_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Settings(
        SettingsMessage::WindowsHelloGraceMinutes(1441),
    ));
    assert_eq!(app.settings().windows_hello.grace_minutes, 5);
    app.update(Message::Settings(
        SettingsMessage::WindowsHelloGraceMinutes(1440),
    ));
    assert_eq!(app.settings().windows_hello.grace_minutes, 1440);
}
