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

//! What the status bar says, as the C# Heimdall's: the state of the session shown, and
//! what was just done until that state changes.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, FolderMessage, Message, NetworkFailure,
    Notice, ProfileCopy, SessionState, SessionStatus, TabId, UiError,
};
use heimdall_core::profile::{ProfileId, RdpProfile, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_rdp::Framebuffer;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio::sync::mpsc;

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_rdp([RdpProfile {
        id: ProfileId::new("dc"),
        name: "Domain controller".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: None,
        domain: None,
        allow_tls_only: false,
        gateway: None,
        redirect_clipboard: false,
        redirect_drives: false,
        options: heimdall_core::profile::RdpOptions::default(),
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        follow_defaults: false,
        several_servers: false,
    }]);
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "web".to_owned(),
        group: Some("Prod".to_owned()),
        host: "web.lab".to_owned(),
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

fn event(app: &mut App, tab: TabId, attempt: AttemptId, event: ConnectionEvent) -> Vec<Effect> {
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    })
}

fn open_ssh(app: &mut App) -> (TabId, AttemptId) {
    match app
        .update(Message::OpenProfile(ProfileId::new("web")))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    }
}

fn copy_host(app: &mut App) {
    app.update(Message::CopyProfile {
        id: ProfileId::new("web"),
        what: ProfileCopy::Hostname,
    });
}

fn named(value: &str) -> String {
    value.to_owned()
}

#[test]
fn the_status_follows_the_session_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert_eq!(app.session_status(), SessionStatus::Ready);

    let (tab, attempt) = open_ssh(&mut app);
    assert_eq!(
        app.session_status(),
        SessionStatus::Connecting(named("web"))
    );
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Connected {
            input: std::sync::Arc::new(NullSink),
        },
    );
    assert_eq!(app.session_status(), SessionStatus::Connected(named("web")));
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Closed { exit_status: None },
    );
    assert_eq!(
        app.session_status(),
        SessionStatus::Disconnected(named("web"))
    );

    let (tab, attempt) = open_ssh(&mut app);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::Timeout),
    );
    assert_eq!(app.session_status(), SessionStatus::Error(named("web")));
}

#[test]
fn a_dropped_desktop_coming_back_is_reconnecting() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = match app
        .update(Message::OpenRdp(ProfileId::new("dc")))
        .as_slice()
    {
        [Effect::ConnectRdp { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    let (input, _received) = mpsc::unbounded_channel();
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(64, 48),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    assert_eq!(
        app.session_status(),
        SessionStatus::Connected(named("Domain controller"))
    );
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::Network {
            failure: NetworkFailure::Reset,
            detail: "reset".to_owned(),
        }),
    );
    assert_eq!(
        app.session_status(),
        SessionStatus::Reconnecting(named("Domain controller"))
    );
}

#[test]
fn a_copy_is_said_until_the_session_shown_changes_and_not_again_after() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert_eq!(app.notice(), None);
    copy_host(&mut app);
    assert_eq!(app.notice(), Some(&Notice::Copied(named("web.lab"))));
    app.update(Message::SelectProfile(ProfileId::new("dc")));
    assert_eq!(
        app.notice(),
        Some(&Notice::Copied(named("web.lab"))),
        "the session shown is as it was"
    );

    let (first, _) = open_ssh(&mut app);
    assert_eq!(app.notice(), None, "a session shown now");
    copy_host(&mut app);
    let (second, _) = open_ssh(&mut app);
    assert_eq!(app.notice(), None);
    // Back to the first, as it was when the copy was said: said no more.
    app.update(Message::SelectTab(first));
    assert_eq!(app.notice(), None);
    assert_ne!(first, second);

    // Nothing copied, nothing said.
    app.update(Message::CopyProfile {
        id: ProfileId::new("gone"),
        what: ProfileCopy::Hostname,
    });
    assert_eq!(app.notice(), None);
}

#[test]
fn a_folder_created_is_said_by_its_path_and_a_refused_one_is_not() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let create = |app: &mut App, name: &str| {
        app.update(Message::Folder(FolderMessage::New {
            parent: "Prod".to_owned(),
        }));
        app.update(Message::Folder(FolderMessage::NameEdited(name.to_owned())));
        app.update(Message::ConfirmDialog);
    };
    create(&mut app, "Archive");
    assert_eq!(
        app.notice(),
        Some(&Notice::FolderCreated(named("Prod/Archive")))
    );
    app.update(Message::DismissDialog);
    copy_host(&mut app);
    create(&mut app, "archive");
    assert_eq!(
        app.notice(),
        Some(&Notice::Copied(named("web.lab"))),
        "refused: the last notice stays"
    );
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
fn a_profile_row_shows_the_most_alive_of_its_sessions() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let web = ProfileId::new("web");
    assert_eq!(app.profile_state(&web), None, "no session open");

    let (first, first_attempt) = open_ssh(&mut app);
    assert_eq!(app.profile_state(&web), Some(SessionState::Connecting));
    event(
        &mut app,
        first,
        first_attempt,
        ConnectionEvent::Failed(UiError::Timeout),
    );
    assert_eq!(app.profile_state(&web), Some(SessionState::Failed));

    let (second, second_attempt) = open_ssh(&mut app);
    assert_eq!(
        app.profile_state(&web),
        Some(SessionState::Connecting),
        "one on its way says more than one that failed"
    );
    let (third, third_attempt) = open_ssh(&mut app);
    event(
        &mut app,
        second,
        second_attempt,
        ConnectionEvent::Connected {
            input: std::sync::Arc::new(NullSink),
        },
    );
    assert_eq!(
        app.profile_state(&web),
        Some(SessionState::Connected),
        "an open one says more than one on its way"
    );
    assert_eq!(
        app.profile_state(&ProfileId::new("dc")),
        None,
        "another profile's sessions are not its own"
    );
    event(
        &mut app,
        third,
        third_attempt,
        ConnectionEvent::Failed(UiError::Timeout),
    );

    event(
        &mut app,
        second,
        second_attempt,
        ConnectionEvent::Closed { exit_status: None },
    );
    assert_eq!(
        app.profile_state(&web),
        Some(SessionState::Failed),
        "an ended session says nothing"
    );
    app.update(Message::RequestCloseTab(first));
    app.update(Message::RequestCloseTab(third));
    assert_eq!(app.profile_state(&web), None, "only an ended session left");
}
