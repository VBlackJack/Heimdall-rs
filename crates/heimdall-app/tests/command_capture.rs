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

//! "Capture a command from the terminal", as the C# `CommandCapture` (PR #659): the selection
//! of a session of text becomes a draft for the command library, joined to one line for the
//! platform of the session; it is offered for a session of text only, never run, and nothing
//! is kept when nothing is selected.

use std::path::Path;
use std::sync::{Arc, Mutex};

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, Message, Notice, PointerInput,
    TabId,
};
use heimdall_core::profile::{ProfileId, RdpProfile, RemoteOs, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_core::tools::user_commands::TemplatePlatform;
use heimdall_rdp::Framebuffer;
use heimdall_ssh::AgentSource;
use heimdall_term::{CellPoint, GridSize, Modifiers, MouseAction, MouseButton};
use tokio::sync::mpsc;

fn ssh(id: &str, host: &str, user: Option<&str>, remote_os: RemoteOs) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: host.to_owned(),
        port: 22,
        username: user.map(str::to_owned),
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
        remote_os,
    }
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([
        ssh("web", "web.lab", Some("ops"), RemoteOs::Auto),
        ssh("win", "win.lab", None, RemoteOs::Windows),
    ]);
    store.merge_rdp([RdpProfile {
        extras: heimdall_core::profile::RdpExtras::default(),
        id: ProfileId::new("dc"),
        name: "DC".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: None,
        domain: None,
        allow_tls_only: false,
        gateway: None,
        local_tunnel_port: None,
        redirect_clipboard: false,
        redirect_drives: false,
        options: heimdall_core::profile::RdpOptions::default(),
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: false,
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

/// A sink that keeps what the session was written, to show that a capture writes nothing.
#[derive(Debug, Default)]
struct Sink {
    written: Mutex<Vec<u8>>,
}

impl heimdall_app::InputSink for Sink {
    fn write(&self, bytes: Vec<u8>) -> Result<(), heimdall_ssh::SessionClosed> {
        self.written.lock().expect("lock").extend(bytes);
        Ok(())
    }
    fn resize(&self, _size: heimdall_ssh::TerminalSize) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

fn event(app: &mut App, tab: TabId, attempt: AttemptId, event: ConnectionEvent) {
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    });
}

/// A connected SSH shell of profile `id` that printed `output`.
fn shell(app: &mut App, id: &str, output: &str) -> (TabId, Arc<Sink>) {
    let (tab, attempt) = match app
        .update(Message::OpenProfile(ProfileId::new(id)))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    let sink = Arc::new(Sink::default());
    event(
        app,
        tab,
        attempt,
        ConnectionEvent::Connected {
            input: sink.clone(),
        },
    );
    event(
        app,
        tab,
        attempt,
        ConnectionEvent::Output(output.as_bytes().to_vec()),
    );
    (tab, sink)
}

fn pointer(action: MouseAction, row: usize, col: usize, right_half: bool) -> PointerInput {
    PointerInput {
        at: CellPoint {
            row,
            col,
            right_half,
        },
        action,
        modifiers: Modifiers::default(),
        clicks: 1,
    }
}

/// Selects from the start of `from_row` to the end of column `to_col` of `to_row`, with the
/// mouse.
fn select(app: &mut App, tab: TabId, from_row: usize, to_row: usize, to_col: usize) {
    let steps = [
        pointer(MouseAction::Press(MouseButton::Left), from_row, 0, false),
        pointer(
            MouseAction::Motion {
                held: Some(MouseButton::Left),
            },
            to_row,
            to_col,
            true,
        ),
        pointer(
            MouseAction::Release(MouseButton::Left),
            to_row,
            to_col,
            true,
        ),
    ];
    for input in steps {
        app.update(Message::Pointer { tab, input });
    }
}

#[test]
fn a_selection_on_a_linux_session_becomes_one_line_joined_with_double_ampersands() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, sink) = shell(&mut app, "web", "cd /var/log\r\nls -la\r\n");
    select(&mut app, tab, 0, 1, 5);

    assert!(app.update(Message::CaptureCommand(tab)).is_empty());
    let context = app.take_capture().expect("a capture");
    let capture = context.capture.expect("the command");
    assert_eq!(capture.text, "cd /var/log && ls -la");
    assert_eq!(capture.platform, TemplatePlatform::Linux);
    assert_eq!(
        capture.tag.as_deref(),
        Some("ssh"),
        "the protocol is the tag"
    );
    assert_eq!(context.host.as_deref(), Some("web.lab"));
    assert_eq!(context.user.as_deref(), Some("ops"));
    assert_eq!(context.session_tab, Some(tab));
    assert!(app.take_capture().is_none(), "taken once");
    assert!(
        sink.written.lock().expect("lock").is_empty(),
        "nothing is sent to the session"
    );
}

#[test]
fn a_session_declared_windows_joins_with_a_semicolon() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = shell(&mut app, "win", "cd C:\\logs\r\nGet-ChildItem\r\n");
    select(&mut app, tab, 0, 1, 12);

    app.update(Message::CaptureCommand(tab));
    let capture = app
        .take_capture()
        .and_then(|context| context.capture)
        .expect("the command");
    assert_eq!(capture.text, "cd C:\\logs ; Get-ChildItem");
    assert_eq!(capture.platform, TemplatePlatform::Windows);
}

#[test]
fn escape_sequences_in_the_selection_never_reach_the_capture() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = shell(&mut app, "web", "echo ok\u{1b}]0;title\u{7}\r\n");
    select(&mut app, tab, 0, 0, 20);

    app.update(Message::CaptureCommand(tab));
    let text = app
        .take_capture()
        .and_then(|context| context.capture)
        .expect("the command")
        .text;
    assert_eq!(text, "echo ok");
    assert!(!text.chars().any(char::is_control));
}

#[test]
fn nothing_selected_keeps_nothing_and_says_so() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = shell(&mut app, "web", "ls\r\n");

    app.update(Message::CaptureCommand(tab));
    assert!(app.take_capture().is_none());
    assert_eq!(app.notice(), Some(&Notice::CaptureNothingSelected));
}

#[test]
fn a_blank_selection_is_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = shell(&mut app, "web", "\r\n\r\nls\r\n");
    select(&mut app, tab, 0, 1, 5);

    app.update(Message::CaptureCommand(tab));
    assert!(app.take_capture().is_none());
    assert_eq!(app.notice(), Some(&Notice::CaptureNothingSelected));
}

#[test]
fn it_is_offered_for_a_session_of_text_only() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (ssh, _) = shell(&mut app, "web", "ls\r\n");
    assert!(app.can_capture_command(app.tab(ssh).expect("tab")));

    // Still connecting: no text yet.
    let connecting = match app
        .update(Message::OpenProfile(ProfileId::new("web")))
        .as_slice()
    {
        [Effect::Connect { tab, .. }] => *tab,
        other => panic!("{other:?}"),
    };
    assert!(!app.can_capture_command(app.tab(connecting).expect("tab")));

    // A remote desktop, connected, never.
    let (rdp, attempt) = match app
        .update(Message::OpenRdp(ProfileId::new("dc")))
        .as_slice()
    {
        [Effect::ConnectRdp { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    let (input, _received) = mpsc::unbounded_channel();
    event(
        &mut app,
        rdp,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(64, 48),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    assert!(!app.can_capture_command(app.tab(rdp).expect("tab")));
    app.update(Message::CaptureCommand(rdp));
    assert!(app.take_capture().is_none());
    assert!(app.dialog.is_none());
}

#[test]
fn a_dialog_open_keeps_the_keyboard() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = shell(&mut app, "web", "ls\r\n");
    select(&mut app, tab, 0, 0, 1);
    app.dialog = Some(Dialog::Shortcuts);

    app.update(Message::CaptureCommand(tab));
    assert!(app.take_capture().is_none());
}
