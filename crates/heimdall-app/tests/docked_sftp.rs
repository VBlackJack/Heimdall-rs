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

//! The SFTP pane an SSH shell gets beside it once connected, as the C# `AutoOpenSftpAsync`:
//! docked side by side as the second pane, the keyboard left on the shell; not when the
//! settings say no, the tab is split already or the session is saved nowhere; never twice
//! after a reconnect; closed with the C# notice when it fails; beyond the session limit.

use std::path::Path;
use std::sync::Arc;

use heimdall_app::files::{FilesKey, Side};
use heimdall_app::split::{Axis, DEFAULT_RATIO, Placement, SplitMessage};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, FilesMessage, InputSink, Message, Notice,
    Phase, Purpose, QuickResult, SettingsMessage, TabId, UiError,
};
use heimdall_core::profile::{ProfileId, SshGateway, SshProfile};
use heimdall_core::settings::SftpBrowser;
use heimdall_core::store::ProfileStore;
use heimdall_files::RemoteSession;
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION};
use heimdall_sftp::{ClientConfig, SftpClient};
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::GridSize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// The grid every tab opens at until the window says otherwise.
const GRID: GridSize = GridSize { cols: 80, rows: 24 };

#[derive(Debug, Default)]
struct Sink;

impl InputSink for Sink {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

fn profile(id: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: Some("admin".to_owned()),
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
    }
}

/// Profiles `a` and `b`; `anonymous`, with no account; `inner`, behind gateway `jump`.
fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([
        profile("a"),
        profile("b"),
        SshProfile {
            username: None,
            ..profile("anonymous")
        },
        SshProfile {
            gateway: Some(ProfileId::new("jump")),
            ..profile("inner")
        },
    ]);
    store.merge_gateways(vec![SshGateway {
        id: ProfileId::new("jump"),
        name: "JUMP".to_owned(),
        host: "jump.lab".to_owned(),
        port: 22,
        username: Some("ops".to_owned()),
        key_path: None,
        parent: None,
    }]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GRID,
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn open_as(app: &mut App, message: Message) -> (TabId, AttemptId) {
    match app.update(message).as_slice() {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one Connect, got {other:?}"),
    }
}

/// A shell tab of profile `id`, still connecting.
fn open(app: &mut App, id: &str) -> (TabId, AttemptId) {
    open_as(app, Message::OpenProfile(ProfileId::new(id)))
}

/// The shell connected, and what that set off.
fn connect(app: &mut App, (tab, attempt): (TabId, AttemptId)) -> Vec<Effect> {
    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(Sink),
        },
    });
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connected);
    effects
}

/// The Files tab and attempt the effects of a shell's connection opened.
fn docked_connect(effects: &[Effect]) -> (TabId, AttemptId) {
    match effects {
        [
            Effect::Connect {
                tab,
                attempt,
                request,
            },
        ] => {
            assert_eq!(request.purpose, Purpose::Files);
            (*tab, *attempt)
        }
        other => panic!("expected the pane's Connect, got {other:?}"),
    }
}

fn settings(app: &mut App, sftp: SftpBrowser) {
    app.update(Message::Settings(SettingsMessage::SftpBrowser(sftp)));
    assert_eq!(app.settings().sftp_browser, sftp);
}

fn strip(app: &App) -> Vec<TabId> {
    app.strip().iter().map(|tab| tab.id).collect()
}

fn leaves(app: &App, host: TabId) -> Vec<TabId> {
    app.tab(host)
        .and_then(|tab| tab.layout.as_ref())
        .map(heimdall_app::split::Layout::leaves)
        .unwrap_or_default()
}

#[test]
fn an_ssh_shell_connected_docks_its_files_beside_it_the_keyboard_on_the_shell() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let shell = open(&mut app, "a");
    let (pane, _) = docked_connect(&connect(&mut app, shell));
    let layout = app
        .tab(shell.0)
        .and_then(|tab| tab.layout.clone())
        .expect("split");
    assert_eq!(layout.leaves(), [shell.0, pane], "second, as the C# pane");
    assert_eq!(layout.axis(), Some(Axis::SideBySide), "the C# Vertical");
    assert_eq!(layout.ratio(), Some(DEFAULT_RATIO));
    assert_eq!(layout.focus, shell.0);
    assert_eq!(app.active, Some(shell.0), "the keyboard stays on the shell");
    assert_eq!(strip(&app), [shell.0]);
    let docked = app.tab(pane).expect("pane");
    assert_eq!(docked.purpose, Purpose::Files);
    assert_eq!(docked.phase, Phase::Connecting, "connects in its pane");
    let files = docked.files.as_deref().expect("files");
    assert!(
        files.local_hidden,
        "the server's files alone, as the C# pane"
    );
    assert_eq!(files.focus, Side::Remote);
}

#[test]
fn a_shell_connecting_behind_another_tab_leaves_that_tab_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let shell = open(&mut app, "a");
    let (other, _) = open(&mut app, "b");
    let (pane, _) = docked_connect(&connect(&mut app, shell));
    assert_eq!(app.active, Some(other), "the tab shown stays shown");
    assert_eq!(leaves(&app, shell.0), [shell.0, pane]);
    app.update(Message::SelectTab(shell.0));
    assert_eq!(app.active, Some(shell.0), "back on the shell, not its pane");
}

#[test]
fn either_setting_off_docks_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    for sftp in [
        SftpBrowser {
            auto_open_on_ssh: false,
            ..SftpBrowser::default()
        },
        SftpBrowser {
            enabled: false,
            ..SftpBrowser::default()
        },
    ] {
        settings(&mut app, sftp);
        let shell = open(&mut app, "a");
        assert!(connect(&mut app, shell).is_empty(), "{sftp:?}");
        assert!(app.tab(shell.0).expect("shell").layout.is_none());
    }
    assert_eq!(app.tabs.len(), 2, "the two shells alone");
}

#[test]
fn the_sftp_browser_off_refuses_a_files_tab_as_the_csharp_handler() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    settings(
        &mut app,
        SftpBrowser {
            enabled: false,
            ..SftpBrowser::default()
        },
    );
    assert!(
        app.update(Message::OpenFiles(ProfileId::new("a")))
            .is_empty()
    );
    assert!(app.tabs.is_empty());
    assert_eq!(app.notice(), Some(&Notice::SftpBrowserDisabled));
}

#[test]
fn a_tab_split_already_or_docked_docks_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let host = open(&mut app, "a");
    let docked = open(&mut app, "b");
    app.update(Message::Split(SplitMessage::Merge {
        host: host.0,
        tab: docked.0,
        axis: Axis::Stacked,
        placement: Placement::Second,
    }));
    assert!(connect(&mut app, host).is_empty(), "split already");
    assert!(
        connect(&mut app, docked).is_empty(),
        "docked in another tab"
    );
    assert_eq!(leaves(&app, host.0), [host.0, docked.0]);
    assert_eq!(app.tabs.len(), 2);
}

#[test]
fn a_reconnect_keeps_the_pane_and_docks_no_second_one() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let shell = open(&mut app, "a");
    let (pane, _) = docked_connect(&connect(&mut app, shell));
    let again = open_as(&mut app, Message::ReconnectTab(shell.0));
    assert_eq!(leaves(&app, again.0), [again.0, pane], "the split carried");
    assert!(connect(&mut app, again).is_empty(), "no second pane");
    assert_eq!(leaves(&app, again.0), [again.0, pane]);
    assert_eq!(app.tabs.len(), 2);

    // Closed by the user, the pane comes back with the next connection, as the C# opens it
    // with every session made ready.
    app.update(Message::Split(SplitMessage::ClosePane(pane)));
    assert!(app.tab(pane).is_none());
    let third = open_as(&mut app, Message::ReconnectTab(again.0));
    let (back, _) = docked_connect(&connect(&mut app, third));
    assert_eq!(leaves(&app, third.0), [third.0, back]);
}

#[test]
fn an_sftp_pane_failing_closes_with_the_csharp_notice_and_leaves_the_shell() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let shell = open(&mut app, "a");
    let (pane, attempt) = docked_connect(&connect(&mut app, shell));
    let effects = app.update(Message::Connection {
        tab: pane,
        attempt,
        event: ConnectionEvent::Failed(UiError::Timeout),
    });
    assert!(effects.is_empty(), "{effects:?}");
    assert!(app.tab(pane).is_none(), "closed, as the C# never shows it");
    let host = app.tab(shell.0).expect("the shell stays");
    assert!(host.layout.is_none());
    assert_eq!(host.phase, Phase::Connected);
    assert_eq!(app.active, Some(shell.0));
    assert_eq!(
        app.notice(),
        Some(&Notice::SftpAutoOpenFailed(UiError::Timeout))
    );
}

#[tokio::test]
async fn an_sftp_pane_that_connected_stays_when_it_fails_later() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let shell = open(&mut app, "a");
    let (pane, attempt) = docked_connect(&connect(&mut app, shell));
    for event in [
        ConnectionEvent::FilesReady {
            client: idle_client().await,
            shell: None,
        },
        ConnectionEvent::Failed(UiError::Timeout),
    ] {
        app.update(Message::Connection {
            tab: pane,
            attempt,
            event,
        });
    }
    assert_eq!(
        leaves(&app, shell.0),
        [shell.0, pane],
        "kept, its failure shown in its pane as any Files tab"
    );
    assert_eq!(
        app.tab(pane).expect("pane").phase,
        Phase::Failed(UiError::Timeout)
    );
    assert_eq!(app.notice(), None);
}

#[test]
fn the_docked_pane_shows_this_computers_files_again_with_its_toggle() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let shell = open(&mut app, "a");
    let (pane, _) = docked_connect(&connect(&mut app, shell));
    let focus = |app: &App| {
        app.tab(pane)
            .and_then(|tab| tab.files.as_deref())
            .map(|files| (files.local_hidden, files.focus))
    };
    let key = |key| Message::Files(FilesMessage::Key { tab: pane, key });
    app.update(key(FilesKey::Focus(Side::Local)));
    assert_eq!(
        focus(&app),
        Some((true, Side::Remote)),
        "hidden, this computer's pane takes no keys"
    );
    app.update(key(FilesKey::SwitchPane));
    assert_eq!(focus(&app), Some((true, Side::Remote)));
    app.update(Message::Files(FilesMessage::ToggleLocal { tab: pane }));
    app.update(key(FilesKey::Focus(Side::Local)));
    assert_eq!(focus(&app), Some((false, Side::Local)), "shown again");
    app.update(Message::Files(FilesMessage::ToggleLocal { tab: pane }));
    assert_eq!(focus(&app), Some((true, Side::Remote)), "hidden again");
}

/// An SFTP server that answers the start, then nothing.
async fn idle_client() -> RemoteSession {
    let (client_end, mut server) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let mut length = [0; 4];
        server.read_exact(&mut length).await.expect("init length");
        let mut body = vec![0; u32::from_be_bytes(length) as usize];
        server.read_exact(&mut body).await.expect("init");
        assert!(matches!(Request::decode(&body), Ok(Request::Init { .. })));
        let version = Response::Version {
            version: SFTP_VERSION,
            extensions: Vec::new(),
        };
        server.write_all(&version.encode()).await.expect("version");
        std::future::pending::<()>().await;
    });
    RemoteSession::Sftp(
        SftpClient::start(client_end, ClientConfig::default())
            .await
            .expect("started"),
    )
}

#[test]
fn the_session_limit_does_not_hold_the_docked_pane_back_as_the_csharp() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Settings(SettingsMessage::MaxSessions(1)));
    let shell = open(&mut app, "a");
    let (pane, _) = docked_connect(&connect(&mut app, shell));
    assert_eq!(leaves(&app, shell.0), [shell.0, pane]);
    // Counted once open, as the C# counts every pane.
    assert!(
        app.update(Message::OpenProfile(ProfileId::new("b")))
            .is_empty()
    );
    assert_eq!(app.notice(), Some(&Notice::SessionLimitReached(1)));
}

#[test]
fn a_session_saved_nowhere_or_without_an_account_gets_no_pane() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    // Typed in Quick Connect: not in the C# inventory.
    let typed = open_as(
        &mut app,
        Message::QuickConnect(QuickResult::Ssh {
            username: Some("root".to_owned()),
            host: "typed.lab".to_owned(),
            port: 22,
        }),
    );
    assert!(connect(&mut app, typed).is_empty());
    // No account: the C# skips the server.
    let anonymous = open(&mut app, "anonymous");
    assert!(connect(&mut app, anonymous).is_empty());
    assert_eq!(app.tabs.len(), 2);
}

#[test]
fn a_saved_session_opened_from_quick_connect_or_through_a_gateway_gets_its_pane() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let summary = app.profile_summary(&ProfileId::new("a")).expect("summary");
    let saved = open_as(
        &mut app,
        Message::QuickConnect(QuickResult::Profile(summary)),
    );
    docked_connect(&connect(&mut app, saved));

    let routed = open(&mut app, "inner");
    let effects = connect(&mut app, routed);
    let [Effect::Connect { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let hops: Vec<&str> = request.route.iter().map(|hop| hop.host.as_str()).collect();
    assert_eq!(
        hops,
        ["jump.lab"],
        "the same gateway, as the C# companion profile"
    );
    assert_eq!(request.profile.id, ProfileId::new("inner"));
}
