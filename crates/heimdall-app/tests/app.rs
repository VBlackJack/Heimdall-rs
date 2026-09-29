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

//! The application state machine, driven by messages as the UI and the driver send them.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use heimdall_app::{
    Answer, App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, InputSink, KeyInput,
    Message, Phase, PointerInput, QuestionId, QuestionKind, TabId, UiError,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{
    AgentSource, KeyboardInteractivePrompt, KeyboardInteractiveQuestion, KnownHosts,
    PasswordQuestion, PublicKey, Secret, SessionClosed, TerminalSize,
};
use heimdall_term::{
    CellPixels, CellPoint, GridSize, Key, KeyLocation, Modifiers, MouseAction, MouseButton,
    NamedKey,
};

const GRID: GridSize = GridSize { cols: 80, rows: 24 };
const CELL: CellPixels = CellPixels {
    width: 9,
    height: 18,
};
const HOST_KEY: &str = include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519.pub");
const OTHER_HOST_KEY: &str =
    include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519-other.pub");

/// Records what the application sends to a session.
#[derive(Debug, Default)]
struct RecordingSink {
    written: Mutex<Vec<u8>>,
    resizes: Mutex<Vec<TerminalSize>>,
    closed: Mutex<bool>,
}

impl RecordingSink {
    fn written(&self) -> Vec<u8> {
        self.written.lock().expect("written").clone()
    }
    fn resizes(&self) -> Vec<TerminalSize> {
        self.resizes.lock().expect("resizes").clone()
    }
    fn closed(&self) -> bool {
        *self.closed.lock().expect("closed")
    }
}

impl InputSink for RecordingSink {
    fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        self.written.lock().expect("written").extend(bytes);
        Ok(())
    }
    fn resize(&self, size: TerminalSize) -> Result<(), SessionClosed> {
        self.resizes.lock().expect("resizes").push(size);
        Ok(())
    }
    fn close(&self) {
        *self.closed.lock().expect("closed") = true;
    }
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
        vault_entry: None,
    }
}

fn config(dir: &Path) -> AppConfig {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([profile("a"), profile("b")]);
    store.save().expect("save");
    AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GRID,
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    }
}

/// Opens a tab and returns its identifiers.
fn open(app: &mut App, id: &str) -> (TabId, AttemptId) {
    let effects = app.update(Message::OpenProfile(ProfileId::new(id)));
    match effects.as_slice() {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one Connect, got {other:?}"),
    }
}

fn event(app: &mut App, tab: TabId, attempt: AttemptId, event: ConnectionEvent) -> Vec<Effect> {
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    })
}

/// Opens a tab and connects it to a recording sink.
fn connected(app: &mut App, id: &str) -> (TabId, AttemptId, Arc<RecordingSink>) {
    let (tab, attempt) = open(app, id);
    let sink = Arc::new(RecordingSink::default());
    event(
        app,
        tab,
        attempt,
        ConnectionEvent::Connected {
            input: sink.clone(),
        },
    );
    (tab, attempt, sink)
}

fn output(app: &mut App, tab: TabId, attempt: AttemptId, bytes: &[u8]) -> Vec<Effect> {
    event(app, tab, attempt, ConnectionEvent::Output(bytes.to_vec()))
}

fn key(text: &str) -> KeyInput {
    KeyInput {
        key: Key::Character(text.chars().next().expect("a character")),
        text: Some(text.to_owned()),
        physical_digit: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::default(),
    }
}

fn password_question(host: &str) -> QuestionKind {
    QuestionKind::Password(PasswordQuestion {
        host: host.to_owned(),
        port: 22,
        username: "admin".to_owned(),
        attempt: 1,
    })
}

fn question(app: &mut App, tab: TabId, attempt: AttemptId) -> QuestionId {
    let question = QuestionId::fresh();
    event(
        app,
        tab,
        attempt,
        ConnectionEvent::Question {
            question,
            kind: password_question("a.lab"),
        },
    );
    question
}

// ---- opening and identity ---------------------------------------------------------------

#[test]
fn opening_a_profile_connects_at_the_current_grid_size() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let effects = app.update(Message::OpenProfile(ProfileId::new("b")));
    let [Effect::Connect { tab, request, .. }] = effects.as_slice() else {
        panic!("expected Connect, got {effects:?}");
    };
    assert_eq!(request.profile.host, "b.lab");
    assert_eq!(
        (
            request.options.initial_size.cols,
            request.options.initial_size.rows
        ),
        (80, 24)
    );
    assert_eq!(app.active, Some(*tab));
    assert_eq!(app.tab(*tab).expect("tab").phase, Phase::Connecting);
}

#[test]
fn events_of_an_abandoned_attempt_are_ignored_and_its_session_closed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, old) = open(&mut app, "a");
    let key = Arc::new(PublicKey::from_openssh(HOST_KEY.trim()).expect("key"));
    event(
        &mut app,
        tab,
        old,
        ConnectionEvent::UnknownHostKey {
            host: "a.lab".to_owned(),
            port: 22,
            fingerprint: "SHA256:x".to_owned(),
            key,
        },
    );
    let effects = app.update(Message::HostKeyDecision { tab, accept: true });
    let [Effect::Connect { attempt: new, .. }] = effects.as_slice() else {
        panic!("expected a new attempt, got {effects:?}");
    };
    assert_ne!(*new, old);

    output(&mut app, tab, old, b"from the old attempt");
    assert!(
        !app.tab(tab)
            .expect("tab")
            .terminal
            .snapshot()
            .row_text(0)
            .contains("old")
    );
    let stale = Arc::new(RecordingSink::default());
    event(
        &mut app,
        tab,
        old,
        ConnectionEvent::Connected {
            input: stale.clone(),
        },
    );
    assert!(stale.closed(), "a late session is closed, not kept");
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connecting);
}

#[test]
fn a_question_for_a_closed_tab_is_cancelled() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt) = open(&mut app, "a");
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::Timeout),
    );
    app.update(Message::RequestCloseTab(tab));
    assert!(app.tab(tab).is_none(), "a failed tab closes without asking");
    let question = QuestionId::fresh();
    let effects = event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Question {
            question,
            kind: password_question("a.lab"),
        },
    );
    assert!(
        matches!(effects.as_slice(), [Effect::Answer { answer: None, .. }]),
        "{effects:?}"
    );
}

// ---- host keys --------------------------------------------------------------------------

fn unknown_key_event(key: &str) -> ConnectionEvent {
    ConnectionEvent::UnknownHostKey {
        host: "a.lab".to_owned(),
        port: 22,
        fingerprint: "SHA256:x".to_owned(),
        key: Arc::new(PublicKey::from_openssh(key.trim()).expect("key")),
    }
}

#[test]
fn accepting_an_unknown_key_records_it_and_reconnects() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt) = open(&mut app, "a");
    event(&mut app, tab, attempt, unknown_key_event(HOST_KEY));
    assert!(matches!(
        app.tab(tab).expect("tab").phase,
        Phase::HostKey { .. }
    ));

    let effects = app.update(Message::HostKeyDecision { tab, accept: true });
    assert!(
        matches!(effects.as_slice(), [Effect::Connect { .. }]),
        "{effects:?}"
    );
    let recorded = KnownHosts::new(dir.path().join("known_hosts"))
        .recorded("a.lab", 22)
        .expect("read");
    assert_eq!(recorded.len(), 1);
}

#[test]
fn a_key_trusted_this_once_connects_serves_later_tabs_and_is_never_recorded() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt) = open(&mut app, "a");
    event(&mut app, tab, attempt, unknown_key_event(HOST_KEY));
    let key = PublicKey::from_openssh(HOST_KEY.trim()).expect("key");

    let effects = app.update(Message::HostKeyTrustOnce(tab));
    let [Effect::Connect { request, .. }] = effects.as_slice() else {
        panic!("expected a new attempt, got {effects:?}");
    };
    assert_eq!(
        request.options.run_trust.keys("a.lab", 22),
        std::slice::from_ref(&key)
    );
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connecting);
    assert!(
        !dir.path().join("known_hosts").exists(),
        "for this run only: nothing written"
    );

    // A later tab to the same server is not asked again.
    let effects = app.update(Message::OpenProfile(ProfileId::new("a")));
    let [Effect::Connect { request, .. }] = effects.as_slice() else {
        panic!("expected one Connect, got {effects:?}");
    };
    assert_eq!(request.options.run_trust.keys("a.lab", 22), [key]);
    assert!(request.options.run_trust.keys("b.lab", 22).is_empty());

    // Nothing is asked about: nothing to trust.
    assert!(app.update(Message::HostKeyTrustOnce(tab)).is_empty());
}

#[test]
fn rejecting_an_unknown_key_records_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt) = open(&mut app, "a");
    event(&mut app, tab, attempt, unknown_key_event(HOST_KEY));
    let effects = app.update(Message::HostKeyDecision { tab, accept: false });
    assert!(effects.is_empty());
    assert_eq!(
        app.tab(tab).expect("tab").phase,
        Phase::Failed(UiError::Cancelled)
    );
    assert!(!dir.path().join("known_hosts").exists());
}

#[test]
fn a_key_recorded_meanwhile_by_another_tab_turns_acceptance_into_a_refusal() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt) = open(&mut app, "a");
    event(&mut app, tab, attempt, unknown_key_event(HOST_KEY));
    KnownHosts::new(dir.path().join("known_hosts"))
        .learn(
            "a.lab",
            22,
            &PublicKey::from_openssh(OTHER_HOST_KEY.trim()).expect("key"),
        )
        .expect("learn");

    let effects = app.update(Message::HostKeyDecision { tab, accept: true });
    assert!(effects.is_empty(), "no reconnection: {effects:?}");
    assert!(matches!(
        app.tab(tab).expect("tab").phase,
        Phase::Failed(UiError::HostKeyChanged { .. })
    ));
}

// ---- prompts ----------------------------------------------------------------------------

#[test]
fn questions_stay_in_their_tab_and_block_its_keyboard_only() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab_a, attempt_a, sink_a) = connected(&mut app, "a");
    let (tab_b, attempt_b, sink_b) = connected(&mut app, "b");

    let question = question(&mut app, tab_b, attempt_b);
    assert!(app.tab(tab_a).expect("a").prompts.is_empty());

    app.update(Message::Key {
        tab: tab_a,
        input: key("x"),
    });
    app.update(Message::Key {
        tab: tab_b,
        input: key("y"),
    });
    assert_eq!(sink_a.written(), b"x");
    assert!(sink_b.written().is_empty(), "b is waiting for its answer");

    let effects = app.update(Message::Answer {
        tab: tab_b,
        question,
        answer: Some(Answer::Secret(Secret::new("pw".to_owned()))),
    });
    assert!(matches!(effects.as_slice(), [Effect::Answer { .. }]));
    assert!(app.tab(tab_b).expect("b").prompts.is_empty());
    let _ = attempt_a;
}

#[test]
fn server_texts_in_a_keyboard_interactive_question_are_made_safe() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt) = open(&mut app, "a");
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Question {
            question: QuestionId::fresh(),
            kind: QuestionKind::KeyboardInteractive(KeyboardInteractiveQuestion {
                host: "a.lab".to_owned(),
                username: "admin".to_owned(),
                name: "Bank\u{202E}lanigiro".to_owned(),
                instructions: "\x1b[2J".to_owned(),
                prompts: vec![KeyboardInteractivePrompt {
                    text: "Code\u{7}: ".to_owned(),
                    echo: false,
                }],
            }),
        },
    );
    let QuestionKind::KeyboardInteractive(shown) = &app.tab(tab).expect("tab").prompts[0].kind
    else {
        panic!("keyboard-interactive expected");
    };
    assert_eq!(shown.name, "Banklanigiro");
    assert_eq!(shown.instructions, "[2J");
    assert_eq!(shown.prompts[0].text, "Code: ");
}

// ---- output, input, resize --------------------------------------------------------------

#[test]
fn output_is_drawn_and_queries_are_answered_through_the_session() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt, sink) = connected(&mut app, "a");
    output(&mut app, tab, attempt, b"ok\x1b[6n");
    assert!(
        app.tab(tab)
            .expect("tab")
            .terminal
            .snapshot()
            .row_text(0)
            .starts_with("ok")
    );
    assert_eq!(sink.written(), b"\x1b[1;3R");
}

#[test]
fn a_key_is_encoded_with_the_terminals_current_mode() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt, sink) = connected(&mut app, "a");
    let up = KeyInput {
        key: Key::Named(NamedKey::ArrowUp),
        text: None,
        physical_digit: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::default(),
    };
    app.update(Message::Key {
        tab,
        input: up.clone(),
    });
    output(&mut app, tab, attempt, b"\x1b[?1h");
    app.update(Message::Key { tab, input: up });
    assert_eq!(sink.written(), b"\x1b[A\x1bOA");
}

#[test]
fn a_resize_reaches_the_terminal_at_once_and_the_session_once() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, _, sink) = connected(&mut app, "a");
    let grid = GridSize {
        cols: 132,
        rows: 43,
    };
    app.update(Message::Resize {
        tab,
        grid,
        cell: CELL,
    });
    app.update(Message::Resize {
        tab,
        grid,
        cell: CELL,
    });
    assert_eq!(app.tab(tab).expect("tab").terminal.size(), grid);
    assert_eq!(
        sink.resizes(),
        vec![TerminalSize {
            cols: 132,
            rows: 43,
            pixel_width: 132 * 9,
            pixel_height: 43 * 18,
        }]
    );
}

#[test]
fn a_session_opened_after_the_view_changed_size_is_told_the_real_size() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt) = open(&mut app, "a");
    let grid = GridSize {
        cols: 100,
        rows: 30,
    };
    app.update(Message::Resize {
        tab,
        grid,
        cell: CELL,
    });
    let sink = Arc::new(RecordingSink::default());
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Connected {
            input: sink.clone(),
        },
    );
    assert_eq!(
        sink.resizes()
            .iter()
            .map(|s| (s.cols, s.rows))
            .collect::<Vec<_>>(),
        vec![(100, 30)]
    );
}

// ---- pointer ----------------------------------------------------------------------------

fn pointer(action: MouseAction, row: usize, col: usize) -> PointerInput {
    PointerInput {
        at: CellPoint {
            row,
            col,
            right_half: false,
        },
        action,
        modifiers: Modifiers::default(),
        clicks: 1,
    }
}

#[test]
fn a_tracked_mouse_goes_to_the_server_and_an_untracked_one_selects() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt, sink) = connected(&mut app, "a");
    output(&mut app, tab, attempt, b"hello world");
    app.update(Message::Pointer {
        tab,
        input: pointer(MouseAction::Press(MouseButton::Left), 0, 0),
    });
    let mut drag = pointer(
        MouseAction::Motion {
            held: Some(MouseButton::Left),
        },
        0,
        4,
    );
    drag.at.right_half = true;
    app.update(Message::Pointer { tab, input: drag });
    assert_eq!(
        app.tab(tab)
            .expect("tab")
            .terminal
            .selected_text()
            .as_deref(),
        Some("hello")
    );
    assert!(sink.written().is_empty());

    output(&mut app, tab, attempt, b"\x1b[?1000h\x1b[?1006h");
    app.update(Message::Pointer {
        tab,
        input: pointer(MouseAction::Press(MouseButton::Left), 2, 3),
    });
    assert_eq!(sink.written(), b"\x1b[<0;4;3M");
}

#[test]
fn the_wheel_scrolls_history_when_the_screen_does_not_want_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt, sink) = connected(&mut app, "a");
    for line in 0..60 {
        output(
            &mut app,
            tab,
            attempt,
            format!("line {line}\r\n").as_bytes(),
        );
    }
    app.update(Message::Pointer {
        tab,
        input: pointer(MouseAction::WheelUp, 5, 5),
    });
    assert_eq!(app.tab(tab).expect("tab").terminal.display_offset(), 3);
    output(&mut app, tab, attempt, b"\x1b[?1049h");
    app.update(Message::Pointer {
        tab,
        input: pointer(MouseAction::WheelUp, 5, 5),
    });
    assert_eq!(
        sink.written(),
        b"\x1b[A",
        "a pager on the alternate screen gets arrows"
    );
}

// ---- clipboard --------------------------------------------------------------------------

#[test]
fn a_multi_line_paste_into_a_shell_without_bracketed_paste_asks_first() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt, sink) = connected(&mut app, "a");
    app.update(Message::ClipboardText {
        tab,
        text: Some("ls\nrm -rf /tmp/x\n".to_owned()),
    });
    assert!(matches!(
        app.dialog,
        Some(Dialog::ConfirmPaste { lines: 2, .. })
    ));
    assert!(sink.written().is_empty());
    app.update(Message::ConfirmDialog);
    assert_eq!(sink.written(), b"ls\rrm -rf /tmp/x\r");

    output(&mut app, tab, attempt, b"\x1b[?2004h");
    app.update(Message::ClipboardText {
        tab,
        text: Some("a\nb".to_owned()),
    });
    assert!(
        app.dialog.is_none(),
        "bracketed paste runs nothing by itself"
    );
    assert!(sink.written().ends_with(b"\x1b[200~a\rb\x1b[201~"));
}

#[test]
fn a_paste_counts_every_line_the_shell_would_run() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, _, _) = connected(&mut app, "a");
    for (text, expected) in [("a\rb\rc", 3), ("\nrm -rf x", 1), ("a\r\nb\r\n", 2)] {
        app.update(Message::ClipboardText {
            tab,
            text: Some(text.to_owned()),
        });
        assert!(
            matches!(app.dialog, Some(Dialog::ConfirmPaste { lines, .. }) if lines == expected),
            "{text:?}: {:?}",
            app.dialog
        );
        app.update(Message::DismissDialog);
    }
}

#[test]
fn while_a_dialog_is_open_keys_and_pointer_never_reach_the_session() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt, sink) = connected(&mut app, "a");
    output(&mut app, tab, attempt, b"\x1b[?1000h\x1b[?1006h");
    app.update(Message::WindowCloseRequested);
    assert!(app.dialog.is_some());
    app.update(Message::Key {
        tab,
        input: key("y"),
    });
    app.update(Message::Pointer {
        tab,
        input: pointer(MouseAction::Press(MouseButton::Left), 1, 1),
    });
    assert!(sink.written().is_empty(), "{:?}", sink.written());
    app.update(Message::DismissDialog);
    app.update(Message::Key {
        tab,
        input: key("x"),
    });
    assert_eq!(sink.written(), b"x");
}

#[test]
fn closing_a_tab_still_connecting_does_not_ask() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, _) = open(&mut app, "a");
    app.update(Message::RequestCloseTab(tab));
    assert!(app.dialog.is_none());
    assert!(app.tab(tab).is_none());
    assert!(matches!(
        app.update(Message::WindowCloseRequested).as_slice(),
        [Effect::Exit]
    ));
}

#[test]
fn effects_never_show_clipboard_text() {
    let effect = Effect::WriteClipboard("hunter2".to_owned());
    assert!(!format!("{effect:?}").contains("hunter2"));
}

#[test]
fn copy_puts_the_selection_on_the_clipboard() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt, _) = connected(&mut app, "a");
    // A hyphen does not end a word: file names and options such as --dry-run stay whole.
    output(&mut app, tab, attempt, b"dry-run mode");
    let mut start = pointer(MouseAction::Press(MouseButton::Left), 0, 0);
    start.clicks = 2;
    app.update(Message::Pointer { tab, input: start });
    let effects = app.update(Message::Copy(tab));
    assert!(
        matches!(effects.as_slice(), [Effect::WriteClipboard(text)] if text == "dry-run"),
        "{effects:?}"
    );
}

// ---- closing ----------------------------------------------------------------------------

#[test]
fn closing_a_live_tab_asks_then_closes_its_session() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab_a, _, sink_a) = connected(&mut app, "a");
    let (tab_b, _, _) = connected(&mut app, "b");
    app.update(Message::RequestCloseTab(tab_b));
    assert_eq!(app.dialog, Some(Dialog::ConfirmCloseTab(tab_b)));
    app.update(Message::DismissDialog);
    assert!(app.tab(tab_b).is_some());

    app.update(Message::RequestCloseTab(tab_a));
    app.update(Message::ConfirmDialog);
    assert!(app.tab(tab_a).is_none());
    assert!(sink_a.closed());
    assert_eq!(app.active, Some(tab_b));
}

#[test]
fn quitting_with_live_sessions_asks_then_closes_them_all() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    assert!(matches!(
        app.update(Message::WindowCloseRequested).as_slice(),
        [Effect::Exit]
    ));
    let (_, _, sink_a) = connected(&mut app, "a");
    let (_, _, sink_b) = connected(&mut app, "b");
    assert!(app.update(Message::WindowCloseRequested).is_empty());
    assert_eq!(app.dialog, Some(Dialog::ConfirmExit { live: 2 }));
    assert!(matches!(
        app.update(Message::ConfirmDialog).as_slice(),
        [Effect::Exit]
    ));
    assert!(sink_a.closed() && sink_b.closed());
}

// ---- titles, sync, store, import, logs --------------------------------------------------

#[test]
fn the_server_title_is_shown_made_safe_and_can_be_reset() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt, _) = connected(&mut app, "a");
    output(
        &mut app,
        tab,
        attempt,
        "\x1b]2;prod\u{202E}web\x07".as_bytes(),
    );
    assert_eq!(app.tab(tab).expect("tab").title, "prodweb");
    output(&mut app, tab, attempt, b"\x1b]2;\x07");
    assert_eq!(app.tab(tab).expect("tab").title, "server a");
}

#[test]
fn an_unclosed_synchronized_update_is_flushed_when_its_deadline_passes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt, _) = connected(&mut app, "a");
    let effects = output(&mut app, tab, attempt, b"\x1b[?2026hlate text");
    let [
        Effect::WakeAt {
            generation,
            deadline,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("expected WakeAt, got {effects:?}");
    };
    assert!(
        app.update(Message::SyncDeadline {
            tab,
            generation: generation + 7
        })
        .is_empty()
    );
    std::thread::sleep(
        deadline.saturating_duration_since(std::time::Instant::now()) + Duration::from_millis(20),
    );
    app.update(Message::SyncDeadline {
        tab,
        generation: *generation,
    });
    assert!(
        app.tab(tab)
            .expect("tab")
            .terminal
            .snapshot()
            .row_text(0)
            .starts_with("late text")
    );
}

#[test]
fn an_unreadable_profile_file_is_reported_and_never_overwritten() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut config = config(dir.path());
    std::fs::write(&config.profiles_file, "version = [").expect("corrupt");
    let legacy = dir.path().join("legacy");
    std::fs::create_dir(&legacy).expect("legacy");
    std::fs::write(
        legacy.join("servers.json"),
        r#"{"servers":[{"id":"x","remoteServer":"x.lab","connectionType":"SSH"}]}"#,
    )
    .expect("servers");
    config.legacy_dir = Some(legacy);

    let mut app = App::new(config.clone());
    assert!(matches!(app.dialog, Some(Dialog::StoreError { .. })));
    assert!(app.profiles().is_empty());
    app.update(Message::DismissDialog);
    app.update(Message::ImportLegacy);
    assert_eq!(
        std::fs::read_to_string(&config.profiles_file).expect("still there"),
        "version = ["
    );
}

#[test]
fn importing_the_csharp_profiles_merges_and_saves_them() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut config = config(dir.path());
    let legacy = dir.path().join("legacy");
    std::fs::create_dir(&legacy).expect("legacy");
    std::fs::write(
        legacy.join("servers.json"),
        r#"{"servers":[
            {"id":"x","displayName":"X","remoteServer":"x.lab","connectionType":"SSH"},
            {"id":"r","displayName":"R","remoteServer":"r.lab","connectionType":"RDP"},
            {"id":"t","displayName":"T","remoteServer":"t.lab","connectionType":"Telnet"},
            {"id":"v","displayName":"V","remoteServer":"v.lab","connectionType":"VNC"},
            {"id":"w","displayName":"W","remoteServer":"w.lab","connectionType":"WINRM"}]}"#,
    )
    .expect("servers");
    config.legacy_dir = Some(legacy);
    let mut app = App::new(config.clone());
    assert!(app.can_import());
    app.update(Message::ImportLegacy);
    let Some(Dialog::ImportDone(summary)) = &app.dialog else {
        panic!("expected ImportDone, got {:?}", app.dialog);
    };
    assert_eq!(summary.merged.added, 5, "one profile of each protocol");
    assert!(summary.skipped.is_empty(), "{:?}", summary.skipped);
    assert_eq!(app.rdp_profiles().len(), 1);
    assert_eq!(app.rdp_profiles()[0].host, "r.lab");
    assert_eq!(app.profiles().len(), 3);
    assert_eq!(app.telnet_profiles().len(), 1);
    let reopened = ProfileStore::open(&config.profiles_file).expect("saved");
    assert_eq!(reopened.ssh_profiles().len(), 3);
    assert_eq!(reopened.telnet_profiles()[0].host, "t.lab");
    assert_eq!(reopened.vnc_profiles()[0].host, "v.lab");
    assert_eq!(app.winrm_profiles().len(), 1);
    assert_eq!(reopened.winrm_profiles()[0].host, "w.lab");
}

#[test]
fn messages_never_show_what_the_user_typed_or_read() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt) = open(&mut app, "a");
    let messages = [
        Message::Key {
            tab,
            input: key("hunter2"),
        },
        Message::Answer {
            tab,
            question: QuestionId::fresh(),
            answer: Some(Answer::Secret(Secret::new("hunter2".to_owned()))),
        },
        Message::Connection {
            tab,
            attempt,
            event: ConnectionEvent::Output(b"hunter2".to_vec()),
        },
        Message::ClipboardText {
            tab,
            text: Some("hunter2".to_owned()),
        },
    ];
    for message in messages {
        let shown = format!("{message:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
    }
}

/// What a pointer input asks of the window's clipboard.
#[derive(Debug, PartialEq, Eq)]
enum ClipboardAsk {
    Write(String),
    Read,
}

fn clipboard_asks(effects: &[Effect]) -> Vec<ClipboardAsk> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::WriteClipboard(text) => Some(ClipboardAsk::Write(text.clone())),
            Effect::ReadClipboard { .. } => Some(ClipboardAsk::Read),
            _ => None,
        })
        .collect()
}

#[test]
fn releasing_a_selection_copies_it_and_a_bare_click_copies_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt, _sink) = connected(&mut app, "a");
    output(&mut app, tab, attempt, b"hello world");

    app.update(Message::Pointer {
        tab,
        input: pointer(MouseAction::Press(MouseButton::Left), 0, 2),
    });
    let effects = app.update(Message::Pointer {
        tab,
        input: pointer(MouseAction::Release(MouseButton::Left), 0, 2),
    });
    assert!(clipboard_asks(&effects).is_empty(), "a click is not a copy");

    app.update(Message::Pointer {
        tab,
        input: pointer(MouseAction::Press(MouseButton::Left), 0, 0),
    });
    let mut drag = pointer(
        MouseAction::Motion {
            held: Some(MouseButton::Left),
        },
        0,
        4,
    );
    drag.at.right_half = true;
    app.update(Message::Pointer { tab, input: drag });
    let effects = app.update(Message::Pointer {
        tab,
        input: pointer(MouseAction::Release(MouseButton::Left), 0, 4),
    });
    assert_eq!(
        clipboard_asks(&effects),
        [ClipboardAsk::Write("hello".to_owned())]
    );

    // A release with no selection under way (the press went to a dialog, or elsewhere)
    // copies nothing, even with text still selected.
    let effects = app.update(Message::Pointer {
        tab,
        input: pointer(MouseAction::Release(MouseButton::Left), 0, 4),
    });
    assert!(clipboard_asks(&effects).is_empty());
}

#[test]
fn a_right_click_pastes_unless_the_program_tracks_the_mouse() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = App::new(config(dir.path()));
    let (tab, attempt, sink) = connected(&mut app, "a");
    let effects = app.update(Message::Pointer {
        tab,
        input: pointer(MouseAction::Press(MouseButton::Right), 1, 1),
    });
    assert_eq!(clipboard_asks(&effects), [ClipboardAsk::Read]);
    assert!(
        sink.written().is_empty(),
        "the paste comes back as ClipboardText"
    );

    output(&mut app, tab, attempt, b"\x1b[?1000h\x1b[?1006h");
    let effects = app.update(Message::Pointer {
        tab,
        input: pointer(MouseAction::Press(MouseButton::Right), 1, 1),
    });
    assert!(clipboard_asks(&effects).is_empty());
    assert_eq!(
        sink.written(),
        b"\x1b[<2;2;2M",
        "the program gets the click"
    );
}
