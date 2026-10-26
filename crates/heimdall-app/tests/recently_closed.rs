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

//! "Reopen closed tab", as the C# Ctrl+Shift+R: the last ten closes remembered, a bulk close
//! reopened whole, a Quick Connect session as well as a saved one, never a secret kept, and
//! nothing left once the workspace locks or the application quits.

use std::path::Path;
use std::sync::Arc;

use heimdall_app::local_driver::LocalShell;
use heimdall_app::{
    Answer, App, AppConfig, AttemptId, ClosedTab, ConnectionEvent, Effect, Message, Phase, Purpose,
    QuestionId, QuestionKind, QuickResult, RECENTLY_CLOSED_CAPACITY, SettingsMessage,
    SystemCredentials, TabGroup, TabId, TabMenuMessage, TabProfile, VaultStatus, open_vault,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, PasswordQuestion, Secret};
use heimdall_term::GridSize;
use heimdall_term::local::LocalArguments;

const MASTER: &str = "correct horse battery staple";

/// What the user types at a Quick Connect session's password question.
const TYPED_PASSWORD: &str = "typed-at-the-prompt-7Qx!";

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

/// A profile of server `id`, with no account: its shell gets no SFTP pane docked by itself.
fn profile(id: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
        local_tunnel_port: None,
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
        remote_os: heimdall_core::profile::RemoteOs::Auto,
    }
}

/// Twelve profiles, `p00` to `p11`, more than the history keeps.
const PROFILES: usize = 12;

fn id(n: usize) -> String {
    format!("p{n:02}")
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge((0..PROFILES).map(|n| profile(&id(n))));
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

/// The tab and attempt of the one connection `effects` start.
fn connect(effects: &[Effect]) -> (TabId, AttemptId) {
    match effects {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one Connect, got {other:?}"),
    }
}

fn connected(app: &mut App, tab: TabId, attempt: AttemptId) {
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    });
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connected);
}

/// A shell of profile `n`, connected.
fn live(app: &mut App, n: usize) -> TabId {
    let (tab, attempt) = connect(&app.update(Message::OpenProfile(ProfileId::new(id(n)))));
    connected(app, tab, attempt);
    tab
}

/// Closes `tab` as its close button does, the question about its live session answered.
fn close(app: &mut App, tab: TabId) {
    app.update(Message::RequestCloseTab(tab));
    if app.dialog.is_some() {
        app.update(Message::ConfirmDialog);
    }
    assert!(app.tab(tab).is_none(), "closed");
}

/// The profiles the tabs open show, in the strip's order.
fn open_profiles(app: &App) -> Vec<String> {
    app.tabs
        .iter()
        .map(|tab| match &tab.profile {
            TabProfile::Ssh(profile) => profile.id.as_str().to_owned(),
            other => format!("{other:?}"),
        })
        .collect()
}

/// The profiles of each gesture remembered, oldest first.
fn remembered(app: &App) -> Vec<Vec<String>> {
    app.recently_closed()
        .gestures()
        .map(|gesture| {
            gesture
                .iter()
                .map(|tab| match tab {
                    ClosedTab::Profile { id, .. } => id.as_str().to_owned(),
                    ClosedTab::Transient { .. } => "transient".to_owned(),
                })
                .collect()
        })
        .collect()
}

#[test]
fn a_tab_closed_reopens_from_its_profile_and_the_most_recent_first() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert_eq!(app.reopen_closed_count(), 0);
    assert!(
        app.update(Message::ReopenClosedTab).is_empty(),
        "nothing to reopen"
    );
    let first = live(&mut app, 0);
    let second = live(&mut app, 1);
    close(&mut app, first);
    close(&mut app, second);
    assert_eq!(remembered(&app), [["p00"], ["p01"]]);
    assert_eq!(app.reopen_closed_count(), 1);

    let effects = app.update(Message::ReopenClosedTab);
    let [Effect::Connect { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.profile.id.as_str(), "p01", "the last closed first");
    assert_eq!(request.purpose, Purpose::Shell);
    let effects = app.update(Message::ReopenClosedTab);
    let [Effect::Connect { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.profile.id.as_str(), "p00");
    assert_eq!(open_profiles(&app), ["p01", "p00"]);
    assert!(app.recently_closed().is_empty());
    assert!(app.update(Message::ReopenClosedTab).is_empty());
}

#[test]
fn the_last_ten_closes_are_remembered() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    for n in 0..PROFILES {
        let tab = live(&mut app, n);
        close(&mut app, tab);
    }
    let kept = remembered(&app);
    assert_eq!(kept.len(), RECENTLY_CLOSED_CAPACITY);
    assert_eq!(kept[0], ["p02"], "the two oldest forgotten");
    assert_eq!(kept[RECENTLY_CLOSED_CAPACITY - 1], ["p11"]);
    for _ in 0..PROFILES {
        app.update(Message::ReopenClosedTab);
    }
    assert_eq!(
        app.tabs.len(),
        RECENTLY_CLOSED_CAPACITY,
        "ten reopened, no more"
    );
}

#[test]
fn a_bulk_close_reopens_whole_in_its_order() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tabs: Vec<TabId> = (0..4).map(|n| live(&mut app, n)).collect();
    app.update(Message::TabMenu(TabMenuMessage::Close {
        tab: tabs[1],
        group: TabGroup::Right,
    }));
    app.update(Message::ConfirmDialog);
    assert_eq!(open_profiles(&app), ["p00", "p01"]);
    app.update(Message::TabMenu(TabMenuMessage::Close {
        tab: tabs[1],
        group: TabGroup::Others,
    }));
    app.update(Message::ConfirmDialog);
    assert_eq!(open_profiles(&app), ["p01"]);
    assert_eq!(remembered(&app), vec![vec!["p02", "p03"], vec!["p00"]]);

    app.update(Message::ReopenClosedTab);
    assert_eq!(open_profiles(&app), ["p01", "p00"]);
    assert_eq!(app.reopen_closed_count(), 2, "the two to the right, as one");
    let effects = app.update(Message::ReopenClosedTab);
    assert_eq!(effects.len(), 2, "both reconnect: {effects:?}");
    assert_eq!(open_profiles(&app), ["p01", "p00", "p02", "p03"]);

    // "Close all tabs": every one, one gesture.
    let shown = app.tabs[0].id;
    app.update(Message::TabMenu(TabMenuMessage::Close {
        tab: shown,
        group: TabGroup::All,
    }));
    app.update(Message::ConfirmDialog);
    assert!(app.tabs.is_empty());
    assert_eq!(remembered(&app), [["p01", "p00", "p02", "p03"]]);
    app.update(Message::ReopenClosedTab);
    assert_eq!(open_profiles(&app), ["p01", "p00", "p02", "p03"]);
}

#[test]
fn a_profile_deleted_since_is_left_out() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tabs: Vec<TabId> = (0..3).map(|n| live(&mut app, n)).collect();
    app.update(Message::TabMenu(TabMenuMessage::Close {
        tab: tabs[0],
        group: TabGroup::Right,
    }));
    app.update(Message::ConfirmDialog);
    app.update(Message::RequestDeleteProfile(ProfileId::new(id(1))));
    app.update(Message::ConfirmDialog);
    assert_eq!(app.reopen_closed_count(), 1, "p01 is gone");
    app.update(Message::ReopenClosedTab);
    assert_eq!(open_profiles(&app), ["p00", "p02"]);
}

#[test]
fn a_local_shell_is_not_remembered() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = app.update(Message::OpenLocal(LocalShell {
        remote_os: heimdall_core::profile::RemoteOs::default(),
        name: "tools".to_owned(),
        program: Some("tool".to_owned()),
        arguments: LocalArguments::List(Vec::new()),
        working_directory: None,
        environment: Vec::new(),
    }));
    let [Effect::ConnectLocal { tab, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    close(&mut app, *tab);
    assert!(app.recently_closed().is_empty(), "nothing to reopen");
}

#[test]
fn a_quick_connect_session_reopens_as_typed_without_the_password_typed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = connect(&app.update(Message::QuickConnect(QuickResult::Ssh {
        username: Some("root".to_owned()),
        host: "jump.lab".to_owned(),
        port: 2222,
    })));
    let question = QuestionId::fresh();
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Question {
            question,
            kind: QuestionKind::Password(PasswordQuestion {
                host: "jump.lab".to_owned(),
                port: 2222,
                username: "root".to_owned(),
                attempt: 1,
            }),
        },
    });
    app.update(Message::Answer {
        tab,
        question,
        answer: Some(Answer::Secret(Secret::new(TYPED_PASSWORD.to_owned()))),
    });
    connected(&mut app, tab, attempt);
    close(&mut app, tab);

    // What is kept: the target and its options, and nothing else. A profile has no field
    // for a secret; its whole printed form is searched, whatever field it might be in.
    let gestures: Vec<&[ClosedTab]> = app.recently_closed().gestures().collect();
    let [[ClosedTab::Transient { profile, purpose }]] = gestures.as_slice() else {
        panic!("{gestures:?}");
    };
    let TabProfile::Ssh(kept) = profile.as_ref() else {
        panic!("{profile:?}");
    };
    assert_eq!(
        (kept.host.as_str(), kept.port, kept.username.as_deref()),
        ("jump.lab", 2222, Some("root"))
    );
    assert_eq!(*purpose, Purpose::Shell);
    assert!(kept.vault_entry.is_none() && kept.key_path.is_none());
    let printed = format!("{:?}", app.recently_closed());
    assert!(!printed.contains(TYPED_PASSWORD), "{printed}");

    let effects = app.update(Message::ReopenClosedTab);
    let [Effect::Connect { tab, request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (request.profile.host.as_str(), request.profile.port),
        ("jump.lab", 2222),
        "as it was typed"
    );
    let reopened = app.tab(*tab).expect("tab");
    assert!(app.tab_profile(reopened).is_none(), "saved nowhere still");
    assert!(app.can_save_as_profile(reopened));

    // A remote desktop typed reopens as one too.
    let effects = app.update(Message::QuickConnect(QuickResult::Rdp {
        host: "ts.lab".to_owned(),
    }));
    let [Effect::ConnectRdp { tab, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    close(&mut app, *tab);
    let effects = app.update(Message::ReopenClosedTab);
    let [Effect::ConnectRdp { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (request.profile.host.as_str(), request.profile.port),
        ("ts.lab", 3389)
    );
}

#[test]
fn nothing_is_left_once_the_application_quits() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tab = live(&mut app, 0);
    close(&mut app, tab);
    assert!(!app.recently_closed().is_empty());
    let effects = app.update(Message::WindowCloseRequested);
    assert!(
        effects.iter().any(|effect| matches!(effect, Effect::Exit)),
        "{effects:?}"
    );
    assert!(app.recently_closed().is_empty());
}

/// Types `master` into the vault dialog shown and runs its job, as the UI would.
async fn submit(app: &mut App, master: &str, confirm: bool) {
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(master.to_owned()),
        new: None,
        confirm: confirm.then(|| Secret::new(master.to_owned())),
    });
    let (ticket, result) = match <[Effect; 1]>::try_from(effects) {
        Ok(
            [
                Effect::OpenVault {
                    path,
                    password,
                    job,
                    ticket,
                },
            ],
        ) => (ticket, open_vault(path, password, job).await),
        other => panic!("expected OpenVault, got {other:?}"),
    };
    app.update(Message::VaultOpened(ticket, result));
}

#[tokio::test]
async fn the_history_is_forgotten_when_the_workspace_locks() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::ShowVault);
    submit(&mut app, MASTER, true).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);

    // The sessions left running behind the lock screen, as the settings have it by default.
    let tab = live(&mut app, 0);
    close(&mut app, tab);
    live(&mut app, 1);
    app.update(Message::LockVault);
    assert!(app.is_locked());
    assert!(app.recently_closed().is_empty(), "forgotten on lock");
    assert_eq!(app.reopen_closed_count(), 0);
    assert!(app.update(Message::ReopenClosedTab).is_empty());
    submit(&mut app, MASTER, false).await;
    assert!(!app.is_locked());
    assert!(
        app.update(Message::ReopenClosedTab).is_empty(),
        "nothing back after the unlock"
    );

    // Every session closed by the lock, when the settings ask: none of them remembered,
    // and what was closed before forgotten. The idle lock is the same lock.
    app.update(Message::Settings(SettingsMessage::DisconnectOnLock(true)));
    let tab = live(&mut app, 2);
    close(&mut app, tab);
    live(&mut app, 3);
    app.update(Message::Settings(SettingsMessage::AutoLockIdleMinutes(1)));
    app.update(Message::Idle(std::time::Duration::from_secs(60)));
    assert!(app.is_locked());
    assert!(app.recently_closed().is_empty());
    submit(&mut app, MASTER, false).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    assert!(app.update(Message::ReopenClosedTab).is_empty());
}
