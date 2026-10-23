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

//! "Copy output, secrets masked", as the C# `MaskedCopyDialogViewModelTests` and
//! `MaskedTerminalCopyServiceTests` decide it: nothing reaches the clipboard until Copy is
//! pressed on a masked text, an engine that gave up copies nothing, switching the host option
//! masks again, and the entry is offered for a session of text only.

use std::path::Path;
use std::sync::Arc;

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, MaskedCopy, MaskedCopyMessage,
    MaskedView, Message, Notice, TabId,
};
use heimdall_core::profile::{ProfileId, RdpProfile, SshProfile};
use heimdall_core::redaction::{self, RedactionError, RedactionOptions};
use heimdall_core::store::ProfileStore;
use heimdall_rdp::Framebuffer;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio::sync::mpsc;

/// A secret no rule of the engine is needed to find by its shape: the key names it.
const SECRET: &str = "hunter2-Zq9";

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

fn event(app: &mut App, tab: TabId, attempt: AttemptId, event: ConnectionEvent) {
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    });
}

/// A connected SSH shell that printed `output`.
fn shell(app: &mut App, output: &str) -> TabId {
    let (tab, attempt) = match app
        .update(Message::OpenProfile(ProfileId::new("web")))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    event(
        app,
        tab,
        attempt,
        ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    );
    event(
        app,
        tab,
        attempt,
        ConnectionEvent::Output(output.as_bytes().to_vec()),
    );
    tab
}

type Source = Arc<zeroize::Zeroizing<String>>;

fn masking(app: &mut App, tab: TabId) -> (u64, Source, RedactionOptions) {
    let effects = app.update(Message::MaskedCopy(MaskedCopyMessage::Open(tab)));
    redact_effect(&effects)
}

fn redact_effect(effects: &[Effect]) -> (u64, Source, RedactionOptions) {
    match effects {
        [Effect::Redact { run, text, options }] => (*run, Arc::clone(text), *options),
        other => panic!("one masking expected, got {other:?}"),
    }
}

/// What the window does with a masking: runs the engine and reports.
fn finish(app: &mut App, run: u64, text: &str, options: RedactionOptions) -> Vec<Effect> {
    app.update(Message::MaskedCopy(MaskedCopyMessage::Done {
        run,
        outcome: redaction::redact(text, options),
    }))
}

fn dialog(app: &App) -> &MaskedCopy {
    match &app.dialog {
        Some(Dialog::MaskedCopy(state)) => state,
        other => panic!("the masked copy dialog expected, got {other:?}"),
    }
}

fn is_copy(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::WriteClipboard(_)))
}

#[test]
fn open_reads_the_whole_text_and_masks_it_before_anything_is_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tab = shell(&mut app, &format!("$ uptime\r\nDB_PASSWORD={SECRET}\r\n"));
    let (run, text, options) = masking(&mut app, tab);
    assert!(text.contains(SECRET), "the engine is given the real text");
    assert!(!options.pseudonymize_hosts, "off until asked for");
    assert_eq!(dialog(&app).view, MaskedView::Working);
    assert_eq!(dialog(&app).masked_text(), None, "nothing to copy yet");

    assert!(!is_copy(&finish(&mut app, run, &text, options)));
    let shown = dialog(&app).masked_text().expect("masked");
    assert!(!shown.contains(SECRET), "{shown}");
    assert!(shown.contains("DB_PASSWORD=[masked:"), "{shown}");
}

#[test]
fn rows_the_terminal_wrapped_are_one_line_when_masked() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    // 80 columns: the value runs over two rows, which only a joined line can mask whole.
    let value = "Ab3".repeat(40);
    let tab = shell(&mut app, &format!("export API_TOKEN={value}\r\n"));
    let (run, text, options) = masking(&mut app, tab);
    assert!(
        text.contains(&format!("API_TOKEN={value}")),
        "the wrapped rows reach the engine as one line"
    );
    finish(&mut app, run, &text, options);
    let shown = dialog(&app).masked_text().expect("masked");
    assert!(
        !shown.contains("Ab3Ab3"),
        "no part of the value is left: {shown}"
    );
}

#[test]
fn nothing_is_copied_until_copy_is_pressed_on_a_masked_text() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tab = shell(&mut app, &format!("password={SECRET}\r\n"));
    let (run, text, options) = masking(&mut app, tab);
    // Still masking: Copy does nothing and the dialog stays.
    assert!(app.update(Message::ConfirmDialog).is_empty());
    assert!(app.dialog.is_some());

    let effects = finish(&mut app, run, &text, options);
    assert!(!is_copy(&effects), "showing the text copies nothing");
    let masked = dialog(&app).masked_text().expect("masked").to_owned();

    let effects = app.update(Message::ConfirmDialog);
    match effects.as_slice() {
        [Effect::WriteClipboard(copied)] => {
            assert_eq!(*copied, masked, "exactly what the screen shows");
            assert!(!copied.contains(SECRET));
        }
        other => panic!("{other:?}"),
    }
    assert!(app.dialog.is_none());
    assert_eq!(app.notice(), Some(&Notice::MaskedCopyCopied));
}

#[test]
fn cancel_copies_nothing_and_a_late_answer_is_dropped() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tab = shell(&mut app, &format!("password={SECRET}\r\n"));
    let (run, text, options) = masking(&mut app, tab);
    assert!(app.update(Message::DismissDialog).is_empty());
    assert!(app.dialog.is_none());
    assert!(!is_copy(&finish(&mut app, run, &text, options)));
    assert!(app.dialog.is_none(), "the answer opens nothing");
    assert!(!is_copy(&app.update(Message::ConfirmDialog)));
}

#[test]
fn an_engine_that_gives_up_leaves_nothing_to_copy() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tab = shell(&mut app, &format!("password={SECRET}\r\n"));
    let (run, ..) = masking(&mut app, tab);
    app.update(Message::MaskedCopy(MaskedCopyMessage::Done {
        run,
        outcome: Err(RedactionError::Timeout {
            rule: redaction::rule_id::KEY_VALUE_SECRET,
        }),
    }));
    assert_eq!(
        dialog(&app).view,
        MaskedView::Refused {
            rule: redaction::rule_id::KEY_VALUE_SECRET
        }
    );
    assert_eq!(dialog(&app).masked_text(), None);
    assert!(
        app.update(Message::ConfirmDialog).is_empty(),
        "Copy is refused"
    );
    assert!(app.dialog.is_some(), "the refusal stays on screen");
    assert_ne!(app.notice(), Some(&Notice::MaskedCopyCopied));

    // A masking that could not run at all is the same.
    let tab = shell(&mut app, "x\r\n");
    app.update(Message::DismissDialog);
    let (run, ..) = masking(&mut app, tab);
    app.update(Message::MaskedCopy(MaskedCopyMessage::Failed { run }));
    assert_eq!(dialog(&app).view, MaskedView::Failed);
    assert!(app.update(Message::ConfirmDialog).is_empty());
}

#[test]
fn switching_the_host_option_masks_again_and_only_the_last_answer_counts() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tab = shell(&mut app, "ping 10.1.2.3\r\nreply from 10.1.2.3\r\n");
    let (first, text, options) = masking(&mut app, tab);
    finish(&mut app, first, &text, options);
    assert!(
        dialog(&app)
            .masked_text()
            .expect("masked")
            .contains("10.1.2.3")
    );

    let (second, again, options) =
        redact_effect(&app.update(Message::MaskedCopy(MaskedCopyMessage::HostsChanged(true))));
    assert!(options.pseudonymize_hosts);
    assert_eq!(**again, **text, "the same text is masked again");
    assert_eq!(dialog(&app).view, MaskedView::Working);
    assert!(
        app.update(Message::ConfirmDialog).is_empty(),
        "Copy waits for the text the screen is going to show"
    );

    // The older masking answers late: dropped.
    finish(&mut app, first, &text, RedactionOptions::default());
    assert_eq!(dialog(&app).view, MaskedView::Working);

    finish(&mut app, second, &again, options);
    let shown = dialog(&app).masked_text().expect("masked");
    assert!(!shown.contains("10.1.2.3"), "{shown}");
    assert!(shown.contains("ip-1"), "{shown}");

    // Switched back, the addresses are back.
    let (third, text, options) =
        redact_effect(&app.update(Message::MaskedCopy(MaskedCopyMessage::HostsChanged(false))));
    assert!(!options.pseudonymize_hosts);
    finish(&mut app, third, &text, options);
    assert!(
        dialog(&app)
            .masked_text()
            .expect("masked")
            .contains("10.1.2.3")
    );
    // Asking for what is already so masks nothing again.
    assert!(
        app.update(Message::MaskedCopy(MaskedCopyMessage::HostsChanged(false)))
            .is_empty()
    );
}

#[test]
fn it_is_offered_for_a_session_of_text_only() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let ssh = shell(&mut app, "ls\r\n");
    assert!(app.can_copy_masked(app.tab(ssh).expect("tab")));

    // A session still connecting has no text yet.
    let connecting = match app
        .update(Message::OpenProfile(ProfileId::new("web")))
        .as_slice()
    {
        [Effect::Connect { tab, .. }] => *tab,
        other => panic!("{other:?}"),
    };
    assert!(!app.can_copy_masked(app.tab(connecting).expect("tab")));

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
    assert!(!app.can_copy_masked(app.tab(rdp).expect("tab")));
    let effects = app.update(Message::MaskedCopy(MaskedCopyMessage::Open(rdp)));
    assert!(effects.is_empty());
    assert!(app.dialog.is_none());
    assert_eq!(app.notice(), Some(&Notice::MaskedCopyNoTerminal));
}

#[test]
fn another_dialog_keeps_the_keyboard_and_the_dialog_does_not_stack() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tab = shell(&mut app, "ls\r\n");
    app.update(Message::MaskedCopy(MaskedCopyMessage::Open(tab)));
    assert!(
        app.update(Message::MaskedCopy(MaskedCopyMessage::Open(tab)))
            .is_empty(),
        "a second key press does not open a second review"
    );
    assert!(matches!(app.dialog, Some(Dialog::MaskedCopy(_))));
}

#[test]
fn neither_the_dialog_nor_the_messages_print_the_text() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tab = shell(&mut app, &format!("password={SECRET}\r\n"));
    let effects = app.update(Message::MaskedCopy(MaskedCopyMessage::Open(tab)));
    let (run, text, options) = redact_effect(&effects);
    assert!(!format!("{effects:?}").contains(SECRET));
    assert!(!format!("{:?}", app.dialog).contains(SECRET));
    let outcome = redaction::redact(&text, options);
    let message = MaskedCopyMessage::Done { run, outcome };
    assert!(!format!("{message:?}").contains(SECRET));
    app.update(Message::MaskedCopy(message));
    assert!(!format!("{:?}", app.dialog).contains(SECRET));
}

#[test]
fn the_masked_text_is_wiped_when_the_dialog_is_wiped() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tab = shell(&mut app, &format!("password={SECRET}\r\n"));
    let (run, text, options) = masking(&mut app, tab);
    finish(&mut app, run, &text, options);
    let mut copy = dialog(&app).clone();
    assert!(copy.masked_text().is_some_and(|shown| !shown.is_empty()));
    copy.wipe();
    assert_eq!(
        copy.masked_text(),
        Some(""),
        "overwritten, not only dropped"
    );
}
