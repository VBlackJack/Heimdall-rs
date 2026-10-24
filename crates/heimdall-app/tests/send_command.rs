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

//! The command library's Send: a command is written into a terminal only once the question
//! is agreed to, and only into a terminal that takes input.

use std::path::Path;
use std::sync::{Arc, Mutex};

use heimdall_app::tools::{ToolContext, ToolId};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Delivery, Dialog, Effect, InputSink, Message,
    Phase, TabId, ToolsMessage,
};
use heimdall_core::profile::{ProfileId, SshProfile, VncProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::GridSize;

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
    }]);
    store.merge_vnc([VncProfile {
        id: ProfileId::new("screen"),
        name: "Screen".to_owned(),
        group: None,
        host: "screen.lab".to_owned(),
        port: 5900,
        view_only: false,
        allow_no_password: true,
        require_tls: false,
        username: None,
        vault_entry: None,
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

/// A session of profile "web" opened; connected when `connect`.
fn session(app: &mut App, connect: bool) -> (TabId, Arc<RecordingSink>) {
    let (tab, attempt): (TabId, AttemptId) = match app
        .update(Message::OpenProfile(ProfileId::new("web")))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    let sink = Arc::new(RecordingSink::default());
    if connect {
        app.update(Message::Connection {
            tab,
            attempt,
            event: ConnectionEvent::Connected {
                input: sink.clone(),
            },
        });
    }
    (tab, sink)
}

fn send(app: &mut App, target: TabId, command: &str) {
    app.update(Message::Tools(ToolsMessage::SendCommand {
        target,
        command: command.to_owned(),
        dangerous: false,
        typed_as_is: false,
    }));
}

#[test]
fn only_connected_terminals_are_offered_as_targets() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (connecting, _) = session(&mut app, false);
    let (first, _) = session(&mut app, true);
    let (second, _) = session(&mut app, true);
    app.update(Message::Tools(ToolsMessage::Open {
        tool: ToolId::CmdLib,
        title: "CMDLIB".to_owned(),
        context: ToolContext::default(),
    }));
    let offered: Vec<TabId> = app.send_targets().iter().map(|each| each.tab).collect();
    assert_eq!(offered, [first, second], "in the order of the strip");
    assert!(!offered.contains(&connecting));
}

#[test]
fn nothing_is_written_before_the_question_is_agreed_to() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, sink) = session(&mut app, true);
    send(&mut app, tab, "uptime");
    let Some(Dialog::ConfirmSendCommand(question)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!((question.tab, question.command.as_str()), (tab, "uptime"));
    assert_eq!(sink.taken(), "", "asked, not written");
    // Refused: still nothing.
    app.update(Message::DismissDialog);
    assert_eq!(app.dialog, None);
    assert_eq!(sink.taken(), "", "declined");
    // Agreed to: the command, then Enter, once.
    send(&mut app, tab, "uptime");
    app.update(Message::ConfirmDialog);
    assert_eq!(sink.taken(), "uptime\r");
    assert_eq!(app.dialog, None);
    app.update(Message::ConfirmDialog);
    assert_eq!(sink.taken(), "", "not twice");
}

#[test]
fn a_command_with_a_line_break_or_nothing_is_refused_without_a_question() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, sink) = session(&mut app, true);
    for command in ["ls\nrm -rf x", "ls\r", "  ", "", "a\u{1b}[2Jb"] {
        send(&mut app, tab, command);
        assert_eq!(app.dialog, None, "{command:?}");
    }
    assert_eq!(sink.taken(), "");
}

#[test]
fn the_command_goes_to_the_terminal_chosen_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (first, first_sink) = session(&mut app, true);
    let (second, second_sink) = session(&mut app, true);
    send(&mut app, second, "df -h");
    app.update(Message::ConfirmDialog);
    assert_eq!(
        [first_sink.taken(), second_sink.taken()],
        ["", "df -h\r"],
        "{first:?}"
    );
}

#[test]
fn a_target_that_takes_no_input_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (connecting, _) = session(&mut app, false);
    send(&mut app, connecting, "uptime");
    assert_eq!(app.dialog, None, "not connected");
    app.update(Message::Tools(ToolsMessage::Open {
        tool: ToolId::CmdLib,
        title: "CMDLIB".to_owned(),
        context: ToolContext::default(),
    }));
    let tool = app.tabs.last().expect("the tool's tab").id;
    send(&mut app, tool, "uptime");
    assert_eq!(app.dialog, None, "a tool is no terminal");
}

#[test]
fn a_terminal_closed_before_the_answer_receives_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, sink) = session(&mut app, true);
    send(&mut app, tab, "uptime");
    app.tabs.retain(|each| each.id != tab);
    app.update(Message::ConfirmDialog);
    assert_eq!(sink.taken(), "");
    send(&mut app, tab, "uptime");
    assert_eq!(app.dialog, None, "no such tab");
}

fn broadcast(app: &mut App, targets: &[TabId], command: &str) {
    app.update(Message::Tools(ToolsMessage::BroadcastCommand {
        targets: targets.to_vec(),
        command: command.to_owned(),
        dangerous: false,
    }));
}

#[test]
fn a_broadcast_asks_one_question_listing_every_terminal_and_writes_nothing_before_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (first, first_sink) = session(&mut app, true);
    let (_second, second_sink) = session(&mut app, true);
    let (third, third_sink) = session(&mut app, true);
    broadcast(&mut app, &[third, first], "uptime");
    let Some(Dialog::ConfirmBroadcastCommand(question)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    let asked: Vec<TabId> = question.targets.iter().map(|each| each.tab).collect();
    assert_eq!(
        asked,
        [first, third],
        "every terminal, in the order of the strip"
    );
    assert_eq!(question.command, "uptime");
    assert_eq!(
        [first_sink.taken(), second_sink.taken(), third_sink.taken()],
        ["", "", ""],
        "asked, not written"
    );
    // Declined: still nothing, and nothing to record.
    app.update(Message::DismissDialog);
    assert_eq!(app.dialog, None);
    assert!(app.take_sent_commands().is_empty());
    assert_eq!(first_sink.taken(), "");

    // Agreed to: the command and Enter in each terminal asked, none other, once.
    broadcast(&mut app, &[third, first], "uptime");
    app.update(Message::ConfirmDialog);
    assert_eq!(
        [first_sink.taken(), second_sink.taken(), third_sink.taken()],
        ["uptime\r", "", "uptime\r"]
    );
    let receipts = app.take_sent_commands();
    let reached: Vec<TabId> = receipts.iter().map(|each| each.tab).collect();
    assert_eq!(reached, [first, third]);
    assert!(receipts.iter().all(|each| each.command == "uptime"));
    assert!(app.take_sent_commands().is_empty(), "taken once");
    app.update(Message::ConfirmDialog);
    assert_eq!(first_sink.taken(), "", "not twice");
}

#[test]
fn a_broadcast_leaves_out_what_takes_no_input_and_refuses_what_a_send_refuses() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (connecting, _) = session(&mut app, false);
    let (ready, sink) = session(&mut app, true);
    broadcast(&mut app, &[connecting, ready], "df -h");
    let Some(Dialog::ConfirmBroadcastCommand(question)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    let asked: Vec<TabId> = question.targets.iter().map(|each| each.tab).collect();
    assert_eq!(asked, [ready], "the terminal not connected is left out");
    app.update(Message::DismissDialog);

    broadcast(&mut app, &[connecting], "df -h");
    assert_eq!(app.dialog, None, "no terminal left: no question");
    broadcast(&mut app, &[], "df -h");
    assert_eq!(app.dialog, None);
    for command in ["ls\nrm -rf x", "ls\r", "  ", "", "a\u{1b}[2Jb"] {
        broadcast(&mut app, &[ready], command);
        assert_eq!(app.dialog, None, "{command:?}");
    }
    assert_eq!(sink.taken(), "");
}

#[test]
fn a_command_sent_leaves_one_receipt_for_the_history() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _sink) = session(&mut app, true);
    send(&mut app, tab, "uptime");
    assert!(app.take_sent_commands().is_empty(), "asked, not sent");
    app.update(Message::ConfirmDialog);
    let receipts = app.take_sent_commands();
    assert_eq!(receipts.len(), 1);
    assert_eq!(
        (receipts[0].tab, receipts[0].command.as_str()),
        (tab, "uptime")
    );
}

fn insert(app: &mut App, target: TabId, command: &str) {
    app.update(Message::Tools(ToolsMessage::InsertCommand {
        target,
        command: command.to_owned(),
    }));
}

#[test]
fn an_inserted_command_is_typed_at_the_prompt_with_no_enter_and_no_question() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, sink) = session(&mut app, true);
    insert(&mut app, tab, "uptime");
    assert_eq!(app.dialog, None, "nothing runs, so nothing is asked");
    assert_eq!(sink.taken(), "uptime", "no Enter after it");
    let receipts = app.take_sent_commands();
    assert_eq!(receipts.len(), 1);
    assert_eq!(
        (
            receipts[0].tab,
            receipts[0].command.as_str(),
            receipts[0].delivery
        ),
        (tab, "uptime", Delivery::Inserted)
    );
    // A send of the same text is told apart: it submits, and its receipt says so.
    send(&mut app, tab, "uptime");
    app.update(Message::ConfirmDialog);
    assert_eq!(sink.taken(), "uptime\r");
    assert_eq!(app.take_sent_commands()[0].delivery, Delivery::Run);
}

#[test]
fn a_destructive_command_is_inserted_without_the_dangerous_question() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, sink) = session(&mut app, true);
    insert(&mut app, tab, "rm -rf /var/tmp/cache");
    assert_eq!(app.dialog, None);
    assert_eq!(sink.taken(), "rm -rf /var/tmp/cache");
}

#[test]
fn an_insert_goes_through_the_same_paste_encoding_as_a_send() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt): (TabId, AttemptId) = match app
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
    // The shell asked for bracketed paste: the command is wrapped, and still has no Enter.
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Output(b"[?2004h".to_vec()),
    });
    insert(&mut app, tab, "ls -l");
    assert_eq!(sink.taken(), "\x1b[200~ls -l\x1b[201~");
}

#[test]
fn an_insert_refuses_what_a_send_refuses() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, sink) = session(&mut app, true);
    for command in [
        "ls\nrm -rf x",
        "ls\r",
        "  ",
        "",
        "a\u{1b}[2Jb",
        "a\tb",
        "a\u{7f}b",
    ] {
        insert(&mut app, tab, command);
        assert_eq!(sink.taken(), "", "{command:?}");
    }
    assert!(app.take_sent_commands().is_empty());
}

#[test]
fn an_insert_reaches_only_a_terminal_that_takes_input() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (connecting, connecting_sink) = session(&mut app, false);
    insert(&mut app, connecting, "uptime");
    assert_eq!(connecting_sink.taken(), "", "not connected");
    app.update(Message::Tools(ToolsMessage::Open {
        tool: ToolId::CmdLib,
        title: "CMDLIB".to_owned(),
        context: ToolContext::default(),
    }));
    let tool = app.tabs.last().expect("the tool's tab").id;
    insert(&mut app, tool, "uptime");
    assert!(app.take_sent_commands().is_empty(), "a tool is no terminal");
    // A closed terminal is not replaced by another.
    let (closed, sink) = session(&mut app, true);
    app.tabs.retain(|each| each.id != closed);
    insert(&mut app, closed, "uptime");
    assert_eq!(sink.taken(), "");
    assert!(app.take_sent_commands().is_empty());
}

#[test]
fn an_insert_never_reaches_a_remote_desktop() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::OpenVnc(ProfileId::new("screen")));
    let id = app.tabs.last().expect("the desktop's tab").id;
    // Even once connected, a desktop is no terminal: it is not offered, and refused.
    app.tabs.last_mut().expect("tab").phase = Phase::Connected;
    assert!(app.send_targets().iter().all(|each| each.tab != id));
    insert(&mut app, id, "uptime");
    assert!(app.take_sent_commands().is_empty());
    send(&mut app, id, "uptime");
    assert_eq!(app.dialog, None);
}
