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

//! The window drawn headless: what it shows for each state of the application core, and
//! the messages a click or a key produces.
//!
//! Strings are the fallback language's (English): the tests never select a language.
//! Setting `HEIMDALL_SNAPSHOT_DIR` writes a PNG of each state there, for a visual pass.

use std::path::Path;
use std::sync::Arc;

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Message as AppMessage, QuestionId, QuestionKind,
    TabId, UiError,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, PasswordQuestion, SessionClosed, TerminalSize};
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::terminal_view::keys::WindowShortcut;
use iced::keyboard::key::Named;
use iced::{Settings, Size, event};
use iced_test::simulator::Simulator;

const GRID: GridSize = GridSize { cols: 80, rows: 24 };

/// Size of the simulated window, in logical pixels.
const WINDOW: Size = Size::new(1100.0, 700.0);

/// Environment variable naming a directory for PNG snapshots.
const SNAPSHOT_VARIABLE: &str = "HEIMDALL_SNAPSHOT_DIR";

#[derive(Debug, Default)]
struct NullSink;

impl heimdall_app::InputSink for NullSink {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), SessionClosed> {
        Ok(())
    }

    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }

    fn close(&self) {}
}

fn profile(id: &str, group: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: group.map(str::to_owned),
        host: format!("{id}.lab"),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
    }
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([
        profile("a", Some("Production")),
        profile("b", Some("Production")),
        profile("c", None),
    ]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GRID,
        files_start: dir.to_owned(),
    })
}

fn open(app: &mut App, id: &str) -> (TabId, AttemptId) {
    match app
        .update(AppMessage::OpenProfile(ProfileId::new(id)))
        .as_slice()
    {
        [heimdall_app::Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one Connect, got {other:?}"),
    }
}

fn simulator(shell: &Shell) -> Simulator<'_, Message> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    Simulator::with_size(settings, WINDOW, shell.view())
}

/// Writes a PNG of the window when `HEIMDALL_SNAPSHOT_DIR` is set.
fn snapshot(shell: &Shell, name: &str) {
    let Some(dir) = std::env::var_os(SNAPSHOT_VARIABLE) else {
        return;
    };
    let path = Path::new(&dir).join(name);
    // iced names the picture after its renderer (`name-wgpu.png`): clear every variant, or
    // an old picture is only compared against and never replaced.
    let stem = name.trim_end_matches(".png");
    if let Ok(entries) = std::fs::read_dir(Path::new(&dir)) {
        for entry in entries.flatten() {
            let file = entry.file_name();
            let file = file.to_string_lossy();
            if file == name || file.starts_with(&format!("{stem}-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    simulator(shell)
        .snapshot(&shell.theme())
        .expect("drawn")
        .matches_image(path)
        .expect("written");
}

#[test]
fn the_sidebar_lists_profiles_by_group_and_a_click_opens_one() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = Shell::with_app(app(dir.path()));
    snapshot(&shell, "home.png");
    let mut ui = simulator(&shell);
    ui.find("Profiles").expect("heading");
    ui.find("Production").expect("group");
    ui.find("Ungrouped").expect("profiles without a group");
    ui.find("admin@a.lab:22").expect("target line");
    let production = ui.find("Production").expect("group").bounds().y;
    let ungrouped = ui.find("Ungrouped").expect("group").bounds().y;
    assert!(production < ungrouped, "profiles without a group come last");
    ui.click("server b").expect("profile button");
    let opened: Vec<String> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::OpenProfile(id)) => Some(id.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(opened, vec!["b".to_owned()]);
}

#[test]
fn a_password_question_is_answered_from_the_field_and_its_draft_is_wiped() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    let question = QuestionId::fresh();
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Question {
            question,
            kind: QuestionKind::Password(PasswordQuestion {
                host: "a.lab".to_owned(),
                port: 22,
                username: "admin".to_owned(),
                attempt: 2,
            }),
        },
    });
    let mut shell = Shell::with_app(core);
    snapshot(&shell, "password.png");
    {
        let mut ui = simulator(&shell);
        ui.find("Password for admin on a.lab:22").expect("title");
        ui.find("The password was refused. Try again.")
            .expect("second attempt says so");
        ui.click("Continue").expect("submit button");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::Submit(t) if t == tab))
        );
    }
    let _ = shell.update(Message::Field {
        question,
        index: 0,
        value: "hunter2".to_owned(),
    });
    assert!(shell.holds_draft(question));
    let _ = shell.update(Message::Submit(tab));
    let shown = shell.app().tab(tab).expect("tab");
    assert!(shown.prompts.is_empty(), "the question was answered");
    assert!(
        !shell.holds_draft(question),
        "the typed password is dropped"
    );
}

#[test]
fn closing_a_tab_drops_what_was_typed_into_its_question() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    let question = QuestionId::fresh();
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Question {
            question,
            kind: QuestionKind::Password(PasswordQuestion {
                host: "a.lab".to_owned(),
                port: 22,
                username: "admin".to_owned(),
                attempt: 1,
            }),
        },
    });
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::Field {
        question,
        index: 0,
        value: "hunter2".to_owned(),
    });
    let _ = shell.update(Message::App(AppMessage::RequestCloseTab(tab)));
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    assert!(shell.app().tab(tab).is_none(), "the tab closed");
    assert!(
        !shell.holds_draft(question),
        "its unanswered password is dropped"
    );
}

#[test]
fn a_cancelled_attempt_is_not_shown_as_a_failure() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::Cancelled),
    });
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find("The connection was cancelled.")
        .expect("said plainly");
    assert!(ui.find("The connection failed").is_err());
}

#[test]
fn a_field_message_never_shows_what_was_typed() {
    let message = Message::Field {
        question: QuestionId::fresh(),
        index: 0,
        value: "hunter2".to_owned(),
    };
    assert!(!format!("{message:?}").contains("hunter2"));
}

#[test]
fn a_field_beyond_the_question_is_ignored() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    let question = QuestionId::fresh();
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Question {
            question,
            kind: QuestionKind::Password(PasswordQuestion {
                host: "a.lab".to_owned(),
                port: 22,
                username: "admin".to_owned(),
                attempt: 1,
            }),
        },
    });
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::Field {
        question,
        index: 1,
        value: "x".to_owned(),
    });
    let unknown = QuestionId::fresh();
    let _ = shell.update(Message::Field {
        question: unknown,
        index: 0,
        value: "x".to_owned(),
    });
    assert!(
        !shell.holds_draft(question),
        "no field 5 in a password question"
    );
    assert!(
        !shell.holds_draft(unknown),
        "no draft for a question nobody asked"
    );
}

#[test]
fn quitting_with_a_live_session_asks_in_a_dialog() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    });
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Output(
            b"admin@a:~$ ls --color\r\n\x1b[1;34mdocs\x1b[0m  notes.txt  \xe4\xb8\xad\xe6\x96\x87\r\nadmin@a:~$ "
                .to_vec(),
        ),
    });
    snapshot(&Shell::with_app(core), "terminal.png");
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    });
    core.update(AppMessage::WindowCloseRequested);
    let shell = Shell::with_app(core);
    snapshot(&shell, "quit.png");
    let mut ui = simulator(&shell);
    ui.find("Quit Heimdall?").expect("dialog title");
    ui.find("One session is still open and will be disconnected.")
        .expect("plural form for one");
    ui.click("Quit").expect("confirm button");
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog)))
    );
}

#[test]
fn tab_shortcuts_cycle_through_the_tabs() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (first, _) = open(&mut core, "a");
    let (second, _) = open(&mut core, "b");
    let (third, _) = open(&mut core, "c");
    let mut shell = Shell::with_app(core);
    assert_eq!(shell.app().active, Some(third));
    let _ = shell.update(Message::Shortcut(WindowShortcut::NextTab));
    assert_eq!(shell.app().active, Some(first), "wraps to the first");
    let _ = shell.update(Message::Shortcut(WindowShortcut::PreviousTab));
    assert_eq!(shell.app().active, Some(third), "wraps to the last");
    let _ = shell.update(Message::Shortcut(WindowShortcut::PreviousTab));
    assert_eq!(shell.app().active, Some(second));
}

fn connected_shell(dir: &Path) -> (Shell, TabId, AttemptId) {
    let mut core = app(dir);
    let (tab, attempt) = open(&mut core, "a");
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    });
    (Shell::with_app(core), tab, attempt)
}

#[test]
fn the_terminal_reports_its_size_and_takes_typing() {
    let dir = tempfile::tempdir().expect("dir");
    let (shell, tab, _) = connected_shell(dir.path());
    let mut ui = simulator(&shell);
    ui.typewrite("ls");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::Resize { tab: t, grid, .. }) if *t == tab && grid.cols > 80
        )),
        "the grid of the window, not the initial 80 columns"
    );
    let keys = messages
        .iter()
        .filter(
            |message| matches!(message, Message::App(AppMessage::Key { tab: t, .. }) if *t == tab),
        )
        .count();
    assert_eq!(keys, 2);
}

#[test]
fn under_a_dialog_the_terminal_takes_nothing_and_leaves_enter_to_the_window() {
    let dir = tempfile::tempdir().expect("dir");
    let (shell, _, _) = connected_shell(dir.path());
    let mut shell = shell;
    let _ = shell.update(Message::App(AppMessage::WindowCloseRequested));
    assert!(shell.app().dialog.is_some());
    {
        let mut ui = simulator(&shell);
        ui.typewrite("y");
        let status = ui.tap_key(keyboard_named(Named::Enter));
        assert_eq!(status, event::Status::Ignored, "Enter reaches the window");
        assert!(
            !ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::Key { .. }))),
            "no key reached the terminal"
        );
    }
    let _ = shell.update(Message::DialogKey { confirm: false });
    assert!(shell.app().dialog.is_none(), "Escape dismisses");
}

fn keyboard_named(named: Named) -> iced::keyboard::Key {
    iced::keyboard::Key::Named(named)
}

#[test]
fn a_long_server_title_is_cut_in_its_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let (shell, tab, attempt) = connected_shell(dir.path());
    let mut core_title = format!("\x1b]2;{}\x07", "w".repeat(100));
    let mut shell = shell;
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Output(std::mem::take(&mut core_title).into_bytes()),
    }));
    let mut ui = simulator(&shell);
    let shown = format!("{}...", "w".repeat(29));
    ui.find(shown.as_str()).expect("cut to 32 characters");
}

#[test]
fn enter_confirms_the_dialog_and_escape_cancels_it() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _) = connected_shell(dir.path());
    let _ = shell.update(Message::App(AppMessage::RequestCloseTab(tab)));
    let _ = shell.update(Message::DialogKey { confirm: false });
    assert!(shell.app().dialog.is_none());
    assert!(shell.app().tab(tab).is_some(), "Escape keeps the session");
    let _ = shell.update(Message::App(AppMessage::RequestCloseTab(tab)));
    let _ = shell.update(Message::DialogKey { confirm: true });
    assert!(shell.app().tab(tab).is_none(), "Enter closes it");
    let _ = shell.update(Message::DialogKey { confirm: true });
    assert!(
        shell.app().dialog.is_none(),
        "without a dialog, Enter does nothing"
    );
}

/// Creates the vault of `core` as its dialog would, the key derivation run to its end.
fn create_vault(core: &mut App) {
    use heimdall_app::{Effect, open_vault};
    use heimdall_ssh::Secret;

    const MASTER: &str = "correct horse battery staple";
    core.update(AppMessage::ShowVault);
    let effects = core.update(AppMessage::SubmitVault {
        password: Secret::new(MASTER.to_owned()),
        confirm: Some(Secret::new(MASTER.to_owned())),
    });
    let [
        Effect::OpenVault {
            path,
            password,
            create,
        },
    ] = effects.as_slice()
    else {
        panic!("expected OpenVault, got {effects:?}");
    };
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let result = runtime.block_on(open_vault(path.clone(), password.clone(), *create));
    core.update(AppMessage::VaultOpened(result));
}

fn ask_password(core: &mut App, tab: TabId, attempt: AttemptId) -> QuestionId {
    let question = QuestionId::fresh();
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Question {
            question,
            kind: QuestionKind::Password(PasswordQuestion {
                host: "a.lab".to_owned(),
                port: 22,
                username: "admin".to_owned(),
                attempt: 1,
            }),
        },
    });
    question
}

#[test]
fn a_password_ticked_to_be_remembered_answers_the_next_connection() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    create_vault(&mut core);
    let (tab, attempt) = open(&mut core, "a");
    let question = ask_password(&mut core, tab, attempt);
    let mut shell = Shell::with_app(core);
    snapshot(&shell, "password-remember.png");
    {
        let mut ui = simulator(&shell);
        ui.find("Lock vault").expect("the vault is open");
        ui.click("Remember in the vault")
            .expect("the box is offered");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::Remember { question: q, remember: true } if q == question
        )));
    }
    let _ = shell.update(Message::Remember {
        question,
        remember: true,
    });
    let _ = shell.update(Message::Field {
        question,
        index: 0,
        value: "hunter2".to_owned(),
    });
    let _ = shell.update(Message::Submit(tab));
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    }));

    // Another connection to the same server: answered without asking.
    let mut core = shell_core(shell);
    let (tab, attempt) = open(&mut core, "a");
    let question = ask_password(&mut core, tab, attempt);
    assert!(
        core.tab(tab)
            .expect("tab")
            .prompts
            .iter()
            .all(|prompt| prompt.question != question),
        "answered from the vault"
    );
}

#[test]
fn a_password_left_unticked_is_not_remembered() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    create_vault(&mut core);
    let (tab, attempt) = open(&mut core, "a");
    let question = ask_password(&mut core, tab, attempt);
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::Field {
        question,
        index: 0,
        value: "hunter2".to_owned(),
    });
    let _ = shell.update(Message::Submit(tab));
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    }));
    let mut core = shell_core(shell);
    let (tab, attempt) = open(&mut core, "a");
    let question = ask_password(&mut core, tab, attempt);
    assert!(
        core.tab(tab)
            .expect("tab")
            .prompts
            .iter()
            .any(|prompt| prompt.question == question),
        "asked"
    );
}

/// The core of `shell`, to drive it further by messages.
fn shell_core(shell: Shell) -> App {
    shell.into_app()
}
