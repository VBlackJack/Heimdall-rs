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

//! Broadcast input, as the C# Heimdall's: what is typed or pasted into one terminal reaches
//! the others of its scope, each encoded for its own session.

use std::path::Path;
use std::sync::{Arc, Mutex};

use heimdall_app::{
    App, AppConfig, AttemptId, BroadcastMessage, ConnectionEvent, Dialog, Effect, InputSink,
    KeyInput, Message, Notice, TabId,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::settings::BroadcastScope;
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::{GridSize, Key, KeyLocation, Modifiers, NamedKey};

/// Records what the application sends to a session.
#[derive(Debug, Default)]
struct RecordingSink {
    written: Mutex<Vec<u8>>,
}

impl RecordingSink {
    fn taken(&self) -> String {
        String::from_utf8(std::mem::take(&mut *self.written.lock().expect("written")))
            .expect("text")
    }
}

impl InputSink for RecordingSink {
    fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        self.written.lock().expect("written").extend(bytes);
        Ok(())
    }
    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "Web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
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

/// A connected session of profile "web", and what it is sent.
fn connected(app: &mut App) -> (TabId, AttemptId, Arc<RecordingSink>) {
    let (tab, attempt) = match app
        .update(Message::OpenProfile(ProfileId::new("web")))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    let sink = Arc::new(RecordingSink::default());
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: sink.clone(),
        },
    });
    (tab, attempt, sink)
}

fn typed(app: &mut App, tab: TabId, text: &str) {
    for c in text.chars() {
        app.update(Message::Key {
            tab,
            input: KeyInput {
                key: Key::Character(c),
                text: Some(c.to_string()),
                physical_digit: None,
                location: KeyLocation::Standard,
                modifiers: Modifiers::default(),
            },
        });
    }
}

fn broadcast(app: &mut App, message: BroadcastMessage) {
    app.update(Message::Broadcast(message));
}

#[test]
fn all_tabs_is_asked_first_then_every_session_gets_what_is_typed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _, sink_a) = connected(&mut app);
    let (b, _, sink_b) = connected(&mut app);
    let (c, _, sink_c) = connected(&mut app);
    typed(&mut app, a, "ls");
    assert_eq!(
        (sink_a.taken(), sink_b.taken()),
        ("ls".to_owned(), String::new())
    );

    broadcast(&mut app, BroadcastMessage::Toggle);
    assert_eq!(app.dialog, Some(Dialog::ConfirmBroadcast));
    assert!(!app.broadcasting(), "not before the answer");
    app.update(Message::DismissDialog);
    assert!(!app.broadcasting(), "no");

    broadcast(&mut app, BroadcastMessage::Toggle);
    app.update(Message::ConfirmDialog);
    assert!(app.broadcasting());
    assert_eq!(
        app.notice(),
        Some(&Notice::BroadcastOn(BroadcastScope::AllTabs))
    );
    typed(&mut app, b, "id");
    assert_eq!(
        [sink_a.taken(), sink_b.taken(), sink_c.taken()],
        ["id", "id", "id"],
        "typed into b, reaching a and c"
    );
    app.update(Message::Key {
        tab: c,
        input: KeyInput {
            key: Key::Named(NamedKey::Enter),
            text: None,
            physical_digit: None,
            location: KeyLocation::Standard,
            modifiers: Modifiers::default(),
        },
    });
    assert_eq!(
        [sink_a.taken(), sink_b.taken(), sink_c.taken()],
        ["\r", "\r", "\r"]
    );

    broadcast(&mut app, BroadcastMessage::Toggle);
    assert!(!app.broadcasting());
    assert_eq!(app.notice(), Some(&Notice::BroadcastOff));
    typed(&mut app, a, "x");
    assert_eq!(
        [sink_a.taken(), sink_b.taken(), sink_c.taken()],
        ["x", "", ""]
    );
}

#[test]
fn a_session_not_ready_for_input_is_left_out() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _, sink_a) = connected(&mut app);
    let (b, attempt_b, sink_b) = connected(&mut app);
    // Still connecting: nothing to send to.
    let connecting = match app
        .update(Message::OpenProfile(ProfileId::new("web")))
        .as_slice()
    {
        [Effect::Connect { tab, .. }] => *tab,
        other => panic!("{other:?}"),
    };
    broadcast(&mut app, BroadcastMessage::Toggle);
    app.update(Message::ConfirmDialog);
    app.update(Message::Connection {
        tab: b,
        attempt: attempt_b,
        event: ConnectionEvent::Closed { exit_status: None },
    });
    typed(&mut app, a, "w");
    assert_eq!(
        (sink_a.taken(), sink_b.taken()),
        ("w".to_owned(), String::new())
    );
    typed(&mut app, connecting, "z");
    assert_eq!(
        sink_a.taken(),
        "",
        "typed into a session not ready: nothing goes"
    );
}

#[test]
fn selected_tabs_reach_only_the_tabs_marked_and_start_without_asking() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _, sink_a) = connected(&mut app);
    let (b, _, sink_b) = connected(&mut app);
    let (_, _, sink_c) = connected(&mut app);
    broadcast(&mut app, BroadcastMessage::Scope);
    assert_eq!(app.settings().broadcast_scope, BroadcastScope::SelectedTabs);
    assert_eq!(
        app.notice(),
        Some(&Notice::BroadcastScope(BroadcastScope::SelectedTabs))
    );
    broadcast(&mut app, BroadcastMessage::Toggle);
    assert_eq!(app.dialog, None, "none asked");
    assert!(app.broadcasting());
    typed(&mut app, a, "u");
    assert_eq!(
        [sink_a.taken(), sink_b.taken(), sink_c.taken()],
        ["u", "", ""],
        "none marked yet"
    );
    broadcast(&mut app, BroadcastMessage::Target(b));
    assert!(app.is_broadcast_target(b));
    assert_eq!(app.broadcast_target_count(), 1);
    typed(&mut app, a, "v");
    assert_eq!(
        [sink_a.taken(), sink_b.taken(), sink_c.taken()],
        ["v", "v", ""]
    );
    broadcast(&mut app, BroadcastMessage::Target(b));
    assert!(!app.is_broadcast_target(b), "unmarked");

    // Back to all tabs while on: asked, as starting it is.
    broadcast(&mut app, BroadcastMessage::Target(b));
    broadcast(&mut app, BroadcastMessage::Scope);
    assert_eq!(app.dialog, Some(Dialog::ConfirmBroadcast));
    app.update(Message::ConfirmDialog);
    assert_eq!(app.settings().broadcast_scope, BroadcastScope::AllTabs);
    typed(&mut app, a, "t");
    assert_eq!(
        [sink_a.taken(), sink_b.taken(), sink_c.taken()],
        ["t", "t", "t"]
    );
    // And into selected again: the marks start over.
    broadcast(&mut app, BroadcastMessage::Scope);
    assert_eq!(app.broadcast_target_count(), 0);
    assert_eq!(
        self::app(dir.path()).settings().broadcast_scope,
        BroadcastScope::SelectedTabs,
        "kept for the next run"
    );
    // Off, all tabs is chosen without a question.
    broadcast(&mut app, BroadcastMessage::Toggle);
    broadcast(&mut app, BroadcastMessage::Scope);
    assert_eq!(app.dialog, None);
    assert_eq!(app.settings().broadcast_scope, BroadcastScope::AllTabs);
}

#[test]
fn a_paste_reaches_every_session_and_is_asked_when_any_would_run_its_lines() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, attempt_a, sink_a) = connected(&mut app);
    let (_, _, sink_b) = connected(&mut app);
    broadcast(&mut app, BroadcastMessage::Toggle);
    app.update(Message::ConfirmDialog);
    app.update(Message::ClipboardText {
        tab: a,
        text: Some("uptime".to_owned()),
    });
    assert_eq!(
        (sink_a.taken(), sink_b.taken()),
        ("uptime".to_owned(), "uptime".to_owned())
    );

    // a has bracketed paste, b not: b would run the lines, so it is asked.
    app.update(Message::Connection {
        tab: a,
        attempt: attempt_a,
        event: ConnectionEvent::Output(b"\x1b[?2004h".to_vec()),
    });
    app.update(Message::ClipboardText {
        tab: a,
        text: Some("ls\nid\n".to_owned()),
    });
    assert!(matches!(
        app.dialog,
        Some(Dialog::ConfirmPaste { lines: 2, .. })
    ));
    assert_eq!(
        (sink_a.taken(), sink_b.taken()),
        (String::new(), String::new())
    );
    app.update(Message::ConfirmDialog);
    assert_eq!(
        sink_a.taken(),
        "\x1b[200~ls\rid\r\x1b[201~",
        "each as its modes ask"
    );
    assert_eq!(sink_b.taken(), "ls\rid\r");
}

#[test]
fn a_session_still_connecting_does_not_make_a_paste_ask() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, attempt_a, sink_a) = connected(&mut app);
    app.update(Message::Connection {
        tab: a,
        attempt: attempt_a,
        event: ConnectionEvent::Output(b"\x1b[?2004h".to_vec()),
    });
    // Its terminal knows no bracketed paste, but it takes nothing yet.
    app.update(Message::OpenProfile(ProfileId::new("web")));
    broadcast(&mut app, BroadcastMessage::Toggle);
    app.update(Message::ConfirmDialog);
    app.update(Message::ClipboardText {
        tab: a,
        text: Some("ls\nid\n".to_owned()),
    });
    assert_eq!(app.dialog, None, "nothing would run the lines");
    assert_eq!(sink_a.taken(), "\x1b[200~ls\rid\r\x1b[201~");
}

#[test]
fn a_mark_is_for_an_open_tab_and_a_closed_one_no_longer_counts() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (_, _, _) = connected(&mut app);
    let (b, _, _) = connected(&mut app);
    broadcast(&mut app, BroadcastMessage::Target(b));
    assert_eq!(app.broadcast_target_count(), 1);
    app.update(Message::RequestCloseTab(b));
    app.update(Message::ConfirmDialog);
    assert_eq!(app.broadcast_target_count(), 0);
    broadcast(&mut app, BroadcastMessage::Target(b));
    broadcast(&mut app, BroadcastMessage::Target(b));
    assert!(
        !app.is_broadcast_target(b),
        "a tab that is gone is not marked"
    );
}
