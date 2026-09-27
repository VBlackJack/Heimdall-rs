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

//! Reconnect, as the C# Heimdall's: a failed or ended session opens again in its tab's
//! place, from its profile as now saved; and a changed SSH key forgotten, then asked about.

use std::path::Path;
use std::sync::Arc;

use heimdall_app::local_driver::LocalShell;
use heimdall_app::profile_draft::ProfileField;
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, Phase, Purpose, ServerAddress,
    TabId, UiError,
};
use heimdall_core::profile::{ProfileId, SshProfile, WinRmProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, KnownHosts, PublicKey};
use heimdall_term::GridSize;
use heimdall_term::local::LocalArguments;

const HOST_KEY: &str = include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519.pub");

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("a"),
        name: "a".to_owned(),
        group: None,
        host: "a.lab".to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
    }]);
    store.merge_winrm([WinRmProfile {
        id: ProfileId::new("w"),
        name: "w".to_owned(),
        group: None,
        // Refused by the command Heimdall writes: an option, not a host.
        host: "-bad".to_owned(),
        port: 5985,
        use_ssl: false,
        skip_certificate_check: false,
        username: None,
    }]);
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

fn connect(effects: &[Effect]) -> (TabId, AttemptId, String) {
    match effects {
        [
            Effect::Connect {
                tab,
                attempt,
                request,
            },
        ] => (*tab, *attempt, request.profile.host.clone()),
        other => panic!("expected one Connect, got {other:?}"),
    }
}

fn fail(app: &mut App, tab: TabId, attempt: AttemptId, error: UiError) {
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(error),
    });
}

fn position(app: &App, tab: TabId) -> Option<usize> {
    app.tabs.iter().position(|found| found.id == tab)
}

#[test]
fn a_failed_session_opens_again_in_its_place_from_its_profile_as_now_saved() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (first, _, _) = connect(&app.update(Message::OpenProfile(ProfileId::new("a"))));
    let (tab, attempt, _) = connect(&app.update(Message::OpenProfile(ProfileId::new("a"))));
    let (last, _, _) = connect(&app.update(Message::OpenProfile(ProfileId::new("a"))));
    assert!(
        !app.can_reconnect(app.tab(tab).expect("tab")),
        "still connecting: nothing to reconnect"
    );
    assert!(app.update(Message::ReconnectTab(tab)).is_empty());

    fail(&mut app, tab, attempt, UiError::Timeout);
    assert!(app.can_reconnect(app.tab(tab).expect("tab")));
    // Fixed after the failure: the fix is what connects.
    app.update(Message::EditProfile(ProfileId::new("a")));
    app.update(Message::ProfileField {
        field: ProfileField::Host,
        value: "a2.lab".to_owned(),
    });
    app.update(Message::SaveProfile { password: None });

    let (again, _, host) = connect(&app.update(Message::ReconnectTab(tab)));
    assert_eq!(host, "a2.lab");
    assert!(app.tab(tab).is_none(), "the failed tab is replaced");
    assert_eq!(position(&app, again), Some(1), "in its place");
    assert_eq!(position(&app, first), Some(0));
    assert_eq!(position(&app, last), Some(2));
    assert_eq!(app.active, Some(again));
    assert_eq!(app.tab(again).expect("again").phase, Phase::Connecting);

    // An old attempt's late event reaches no tab.
    fail(&mut app, tab, attempt, UiError::Timeout);
    assert_eq!(app.tab(again).expect("again").phase, Phase::Connecting);
}

#[test]
fn an_ended_session_reconnects_and_a_files_tab_stays_a_files_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = app.update(Message::OpenFiles(ProfileId::new("a")));
    let (tab, attempt, _) = connect(&effects);
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Closed { exit_status: None },
    });
    assert!(app.can_reconnect(app.tab(tab).expect("tab")));
    let effects = app.update(Message::ReconnectTab(tab));
    let [
        Effect::Connect {
            tab: again,
            request,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    assert_eq!(request.purpose, Purpose::Files);
    assert_eq!(app.tab(*again).expect("again").purpose, Purpose::Files);
}

#[test]
fn a_deleted_profile_offers_no_reconnect() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt, _) = connect(&app.update(Message::OpenProfile(ProfileId::new("a"))));
    fail(&mut app, tab, attempt, UiError::Timeout);
    app.update(Message::RequestDeleteProfile(ProfileId::new("a")));
    app.update(Message::ConfirmDialog);
    assert!(!app.can_reconnect(app.tab(tab).expect("tab")));
    assert!(app.update(Message::ReconnectTab(tab)).is_empty());
    assert!(app.tab(tab).is_some(), "left as it was");
}

#[test]
fn a_local_shell_runs_the_same_program_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let shell = LocalShell {
        name: "tools".to_owned(),
        program: Some("tool".to_owned()),
        arguments: LocalArguments::List(vec!["--flag".to_owned()]),
        working_directory: None,
    };
    let effects = app.update(Message::OpenLocal(shell));
    let [Effect::ConnectLocal { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    fail(&mut app, tab, attempt, UiError::Timeout);
    let effects = app.update(Message::ReconnectTab(tab));
    let [Effect::ConnectLocal { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.shell.program.as_deref(), Some("tool"));
    assert_eq!(
        request.shell.arguments,
        LocalArguments::List(vec!["--flag".to_owned()])
    );
}

#[test]
fn a_refused_winrm_tab_reopens_from_its_profile_never_as_an_empty_shell() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert!(
        app.update(Message::OpenWinRm(ProfileId::new("w")))
            .is_empty()
    );
    let tab = app.active.expect("a tab saying why");
    assert!(matches!(app.tab(tab).expect("tab").phase, Phase::Failed(_)));
    assert!(app.can_reconnect(app.tab(tab).expect("tab")));

    // Still refused: a tab saying so again, and nothing run.
    assert!(app.update(Message::ReconnectTab(tab)).is_empty());
    let refused = app.active.expect("tab");
    assert!(matches!(
        app.tab(refused).expect("tab").phase,
        Phase::Failed(_)
    ));
    assert_eq!(app.tabs.len(), 1, "in its place");

    app.update(Message::EditProfile(ProfileId::new("w")));
    app.update(Message::ProfileField {
        field: ProfileField::Host,
        value: "dc.lab".to_owned(),
    });
    app.update(Message::SaveProfile { password: None });
    let effects = app.update(Message::ReconnectTab(refused));
    let [Effect::ConnectLocal { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert!(
        request.shell.program.is_some(),
        "PowerShell, not the default shell"
    );
    let LocalArguments::List(arguments) = &request.shell.arguments else {
        panic!("{:?}", request.shell.arguments);
    };
    assert!(
        arguments
            .iter()
            .any(|argument| argument.contains("'dc.lab'")),
        "{arguments:?}"
    );
}

#[test]
fn a_changed_ssh_key_is_forgotten_then_the_new_one_is_asked_about() {
    let dir = tempfile::tempdir().expect("dir");
    let known_hosts = KnownHosts::new(dir.path().join("known_hosts"));
    known_hosts
        .learn(
            "a.lab",
            22,
            &PublicKey::from_openssh(HOST_KEY.trim()).expect("key"),
        )
        .expect("learnt");
    let mut app = app(dir.path());
    let (tab, attempt, _) = connect(&app.update(Message::OpenProfile(ProfileId::new("a"))));
    fail(
        &mut app,
        tab,
        attempt,
        UiError::HostKeyChanged {
            target: Some(ServerAddress {
                host: "a.lab".to_owned(),
                port: 22,
            }),
            recorded: "SHA256:old".to_owned(),
            offered: "SHA256:new".to_owned(),
        },
    );
    let (again, attempt, _) = connect(&app.update(Message::ForgetServer(tab)));
    assert!(
        known_hosts.recorded("a.lab", 22).expect("read").is_empty(),
        "the old key is forgotten"
    );
    // The new key is not trusted by itself: it is asked about.
    let key = Arc::new(PublicKey::from_openssh(HOST_KEY.trim()).expect("key"));
    app.update(Message::Connection {
        tab: again,
        attempt,
        event: ConnectionEvent::UnknownHostKey {
            host: "a.lab".to_owned(),
            port: 22,
            fingerprint: "SHA256:new".to_owned(),
            key,
        },
    });
    assert!(matches!(
        app.tab(again).expect("again").phase,
        Phase::HostKey { .. }
    ));
}

#[test]
fn reconnecting_stops_whatever_the_old_attempt_still_holds() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = app.update(Message::OpenProfile(ProfileId::new("a")));
    let [
        Effect::Connect {
            tab,
            attempt,
            request,
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    let (tab, attempt, cancel) = (*tab, *attempt, request.cancel.clone());
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Closed { exit_status: None },
    });
    assert!(!app.update(Message::ReconnectTab(tab)).is_empty());
    assert!(cancel.is_cancelled(), "the old session is hung up");
}
