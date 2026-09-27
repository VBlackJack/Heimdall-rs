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
        system_credentials: heimdall_app::SystemCredentials::memory(),
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
    ui.find("(No Folder)").expect("profiles without a folder");
    let production = ui.find("Production").expect("group").bounds().y;
    let ungrouped = ui.find("(No Folder)").expect("group").bounds().y;
    assert!(
        production < ungrouped,
        "profiles without a folder come last"
    );
    ui.click("server b").expect("profile");
    ui.click("server b").expect("profile");
    let connected: Vec<String> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::ConnectProfile(id)) => Some(id.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(connected, vec!["b".to_owned()], "a double click connects");
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
        new: None,
        confirm: Some(Secret::new(MASTER.to_owned())),
    });
    let Ok(
        [
            Effect::OpenVault {
                path,
                password,
                job,
            },
        ],
    ) = <[Effect; 1]>::try_from(effects)
    else {
        panic!("expected OpenVault");
    };
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let result = runtime.block_on(open_vault(path, password, job));
    core.update(AppMessage::VaultOpened(result));
}

#[test]
fn the_master_password_is_enabled_from_the_settings_with_its_rules_said_as_typed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    {
        let mut ui = simulator(&shell);
        ui.click("Settings").expect("the sidebar's settings");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::ShowSettings))
        );
    }
    let _ = shell.update(Message::ShowSettings);
    assert!(shell.settings_shown());
    snapshot(&shell, "settings-vault-disabled.png");
    {
        let mut ui = simulator(&shell);
        for label in [
            "Security",
            "Master password",
            "Encrypt your stored credentials under a master password. You will be asked for it each time the app starts.",
            "Disabled",
        ] {
            ui.find(label).expect(label);
        }
        assert!(ui.find("Lock").is_err(), "nothing to lock yet");
        ui.click("Enable master password").expect("enable");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::ShowVault)))
        );
    }
    let _ = shell.update(Message::App(AppMessage::ShowVault));
    let enable_clicked = |shell: &Shell| {
        let mut ui = simulator(shell);
        ui.click("Enable").expect("the dialog's button");
        ui.into_messages()
            .any(|message| matches!(message, Message::SubmitVault))
    };
    {
        let mut ui = simulator(&shell);
        for label in [
            "Set a master password",
            "New master password",
            "Confirm master password",
            "Use at least 12 characters.",
        ] {
            ui.find(label).expect(label);
        }
    }
    assert!(!enable_clicked(&shell), "nothing typed");
    let type_new = |shell: &mut Shell, new: &str, confirm: &str| {
        for (index, value) in [(0, new), (1, confirm)] {
            let _ = shell.update(Message::VaultField {
                index,
                value: value.to_owned(),
            });
        }
    };
    type_new(&mut shell, "short", "short");
    simulator(&shell)
        .find("Too short: use at least 12 characters.")
        .expect("too short");
    type_new(&mut shell, "alllowercase", "alllowercase");
    simulator(&shell)
        .find("Use at least 3 character types (lower, upper, digit, symbol), or 20 characters or more.")
        .expect("too simple");
    assert!(!enable_clicked(&shell), "refused by the rules");
    type_new(&mut shell, "Mixed-case 12", "Mixed-case 13");
    simulator(&shell)
        .find("Password strength is sufficient.")
        .expect("strong enough");
    assert!(!enable_clicked(&shell), "not typed twice alike");
    type_new(&mut shell, "Mixed-case 12", "Mixed-case 12");
    snapshot(&shell, "vault-enable.png");
    assert!(enable_clicked(&shell));
}

#[test]
fn the_lock_hides_the_window_until_the_master_password_is_typed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    create_vault(&mut core);
    // Two sessions: behind the lock, Ctrl+Tab would move between them.
    open(&mut core, "a");
    open(&mut core, "a");
    let mut shell = Shell::with_app(core);
    {
        let mut ui = simulator(&shell);
        ui.click("Lock")
            .expect("the sidebar's lock, once a master password is set");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::LockVault)))
        );
    }
    let _ = shell.update(Message::LockKey);
    assert!(shell.app().is_locked());
    snapshot(&shell, "vault-locked.png");
    {
        let mut ui = simulator(&shell);
        ui.find("Workspace locked").expect("the lock screen");
        ui.find("Enter your master password to unlock.")
            .expect("says how");
        assert!(ui.find("Profiles").is_err(), "the window is hidden");
        assert!(ui.find("Cancel").is_err(), "no way around it");
    }
    let shown = shell.app().active;
    let _ = shell.update(Message::Shortcut(WindowShortcut::NextTab));
    assert_eq!(
        shell.app().active,
        shown,
        "the window's keys do nothing behind the lock"
    );
}

#[test]
fn showing_a_tab_leaves_the_settings() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, _) = open(&mut core, "a");
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::ShowSettings);
    assert!(shell.settings_shown());
    let _ = shell.update(Message::App(AppMessage::SelectTab(tab)));
    assert!(!shell.settings_shown(), "the tab clicked is shown");
}

/// The password a new connection to profile `a` is answered with by itself, if any.
fn saved_answer(core: &mut App) -> Option<String> {
    let (tab, attempt) = open(core, "a");
    let question = QuestionId::fresh();
    let effects = core.update(AppMessage::Connection {
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
    match effects.as_slice() {
        [] => None,
        [
            heimdall_app::Effect::Answer {
                question: answered,
                answer: Some(heimdall_app::Answer::Secret(secret)),
            },
        ] if *answered == question => Some(secret.expose().to_owned()),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn the_profile_form_saves_a_password_and_then_says_it_is_saved() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    core.update(AppMessage::EditProfile(ProfileId::new("a")));
    let mut shell = Shell::with_app(core);
    {
        let mut ui = simulator(&shell);
        ui.find("Password").expect("a password field");
        assert!(ui.find("Password saved").is_err(), "none yet");
    }
    let _ = shell.update(Message::ProfilePassword("hunter2".to_owned()));
    let _ = shell.update(Message::SaveProfileForm);
    let mut core = shell.into_app();
    assert!(core.dialog.is_none(), "{:?}", core.dialog);
    assert_eq!(saved_answer(&mut core).as_deref(), Some("hunter2"));

    core.update(AppMessage::EditProfile(ProfileId::new("a")));
    let shell = Shell::with_app(core);
    snapshot(&shell, "profile-password-saved.png");
    let mut ui = simulator(&shell);
    ui.find("Password saved").expect("says so");
    ui.click("Clear").expect("a clear button");
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::App(AppMessage::ClearPassword)))
    );
}

#[test]
fn enter_in_the_profile_form_saves_the_password_typed_too() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    core.update(AppMessage::EditProfile(ProfileId::new("a")));
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::ProfilePassword("hunter2".to_owned()));
    let _ = shell.update(Message::DialogKey { confirm: true });
    let mut core = shell.into_app();
    assert!(core.dialog.is_none(), "{:?}", core.dialog);
    assert_eq!(saved_answer(&mut core).as_deref(), Some("hunter2"));
}

#[test]
fn a_password_typed_then_dismissed_is_neither_saved_nor_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    core.update(AppMessage::EditProfile(ProfileId::new("a")));
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::ProfilePassword("hunter2".to_owned()));
    let _ = shell.update(Message::App(AppMessage::DismissDialog));
    // Opened again and saved without typing: what was typed before is gone.
    let _ = shell.update(Message::App(AppMessage::EditProfile(ProfileId::new("a"))));
    let _ = shell.update(Message::SaveProfileForm);
    let mut core = shell.into_app();
    assert_eq!(saved_answer(&mut core), None);
}

#[test]
fn with_the_vault_locked_the_form_says_to_unlock_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    create_vault(&mut core);
    core.update(AppMessage::LockVault);
    core.update(AppMessage::EditProfile(ProfileId::new("a")));
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find("Unlock the vault to save or change a password.")
        .expect("says why the field is closed");
}

#[test]
fn a_password_typed_in_the_form_survives_a_visit_to_the_gateway_dialog() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    core.update(AppMessage::EditProfile(ProfileId::new("a")));
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::ProfilePassword("hunter2".to_owned()));
    let _ = shell.update(Message::App(AppMessage::NewGateway));
    let _ = shell.update(Message::App(AppMessage::DismissDialog));
    let _ = shell.update(Message::SaveProfileForm);
    let mut core = shell.into_app();
    assert!(core.dialog.is_none(), "{:?}", core.dialog);
    assert_eq!(saved_answer(&mut core).as_deref(), Some("hunter2"));
}

#[test]
fn the_tree_search_filters_the_profiles_and_says_when_none_match() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    {
        let mut ui = simulator(&shell);
        ui.click("Search").expect("the search box");
        ui.typewrite("b");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::Search(term) if term == "b"))
        );
    }
    let _ = shell.update(Message::Search("B.LAB".to_owned()));
    {
        let mut ui = simulator(&shell);
        ui.find("server b").expect("found by its host");
        assert!(ui.find("server a").is_err(), "filtered out");
        assert!(ui.find("server c").is_err(), "filtered out");
    }
    let _ = shell.update(Message::Search("nowhere".to_owned()));
    snapshot(&shell, "tree-search-empty.png");
    {
        let mut ui = simulator(&shell);
        ui.find("No sessions match your search.").expect("says so");
        ui.click("Clear search").expect("a way back");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::Search(term) if term.is_empty()))
        );
    }
    let _ = shell.update(Message::Search(String::new()));
    let mut ui = simulator(&shell);
    for name in ["server a", "server b", "server c"] {
        ui.find(name).expect(name);
    }
    assert!(ui.find("No sessions match your search.").is_err());
}

#[test]
fn a_failed_session_offers_reconnect_and_a_changed_ssh_key_its_way_past() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    {
        let shell = Shell::with_app(core);
        assert!(
            simulator(&shell).find("Reconnect").is_err(),
            "nothing to reconnect while connecting"
        );
        core = shell.into_app();
    }
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::HostKeyChanged {
            target: Some(heimdall_app::ServerAddress {
                host: "a.lab".to_owned(),
                port: 22,
            }),
            recorded: "SHA256:old".to_owned(),
            offered: "SHA256:new".to_owned(),
        }),
    });
    let shell = Shell::with_app(core);
    snapshot(&shell, "session-hostkey-changed.png");
    {
        let mut ui = simulator(&shell);
        ui.click("Reconnect").expect("reconnect");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::ReconnectTab(reconnected)) if reconnected == tab
        )));
    }
    let mut ui = simulator(&shell);
    ui.click("Accept new key (destructive)")
        .expect("the way past");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::ForgetServer(forgotten)) if forgotten == tab
    )));
}

#[test]
fn a_session_whose_profile_is_gone_offers_close_only() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::Timeout),
    });
    core.update(AppMessage::RequestDeleteProfile(ProfileId::new("a")));
    core.update(AppMessage::ConfirmDialog);
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find("Close the tab").expect("close");
    assert!(
        ui.find("Reconnect").is_err(),
        "nothing left to reconnect to"
    );
}

#[test]
fn a_right_click_on_a_tab_opens_its_menu_as_the_csharp_one() {
    use heimdall_app::{TabGroup, TabMenuMessage};
    use heimdall_ui::tree_view::TreeMenu;
    use iced::mouse;

    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, _) = open(&mut core, "a");
    let (second, _) = open(&mut core, "b");
    // A name of its own: the tree shows the profile's, so the click can only be the tab's.
    core.update(AppMessage::TabMenu(TabMenuMessage::Rename(tab)));
    core.update(AppMessage::TabMenu(TabMenuMessage::NameEdited(
        "first tab".to_owned(),
    )));
    core.update(AppMessage::ConfirmDialog);
    let mut shell = Shell::with_app(core);
    {
        let mut ui = simulator(&shell);
        let title = ui.find("first tab").expect("tab title");
        ui.point_at(title.bounds().center());
        ui.simulate([
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
        ]);
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::OpenTreeMenu(TreeMenu::Tab(opened)) if opened == tab
        )));
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(tab)));
    snapshot(&shell, "tab-menu.png");
    for (entry, expected) in [
        ("Disconnect", Some(AppMessage::RequestCloseTab(tab))),
        (
            "Rename tab",
            Some(AppMessage::TabMenu(TabMenuMessage::Rename(tab))),
        ),
        (
            "Reset title",
            Some(AppMessage::TabMenu(TabMenuMessage::ResetTitle(tab))),
        ),
        // Still connecting: nothing to reconnect yet.
        ("Reconnect Session", None),
        (
            "Duplicate Session",
            Some(AppMessage::TabMenu(TabMenuMessage::Duplicate(tab))),
        ),
        ("Edit", Some(AppMessage::EditProfile(ProfileId::new("a")))),
        (
            "Close others",
            Some(AppMessage::TabMenu(TabMenuMessage::Close {
                tab,
                group: TabGroup::Others,
            })),
        ),
        (
            "Close to the right",
            Some(AppMessage::TabMenu(TabMenuMessage::Close {
                tab,
                group: TabGroup::Right,
            })),
        ),
    ] {
        let mut ui = simulator(&shell);
        ui.click(entry).expect(entry);
        let chosen: Vec<Message> = ui
            .into_messages()
            .filter(|message| matches!(message, Message::MenuChoice(_)))
            .collect();
        match expected {
            Some(expected) => assert!(
                matches!(chosen.as_slice(), [Message::MenuChoice(got)]
                    if format!("{got:?}") == format!("{expected:?}")),
                "{entry}: {chosen:?}"
            ),
            None => assert!(chosen.is_empty(), "{entry} is greyed out: {chosen:?}"),
        }
    }
    {
        let mut ui = simulator(&shell);
        ui.click("Fullscreen (F11)").expect("fullscreen");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::MenuFullscreen(shown) if shown == tab
        )));
    }
    // The last tab has none to its right, and no name of its own to reset.
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(second)));
    let mut ui = simulator(&shell);
    assert!(ui.find("Reset title").is_err());
    ui.click("Close to the right").expect("entry");
    assert!(
        !ui.into_messages()
            .any(|message| matches!(message, Message::MenuChoice(_))),
        "greyed out on the last tab"
    );
}

#[test]
fn a_failed_session_offers_its_error_and_its_profile_as_the_csharp_card() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    let (cancelled, cancelled_attempt) = open(&mut core, "b");
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::Timeout),
    });
    core.update(AppMessage::Connection {
        tab: cancelled,
        attempt: cancelled_attempt,
        event: ConnectionEvent::Failed(UiError::Cancelled),
    });
    core.update(AppMessage::SelectTab(tab));
    let shell = Shell::with_app(core);
    snapshot(&shell, "session-failed-card.png");
    {
        let mut ui = simulator(&shell);
        ui.click("Copy error").expect("copy error");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::CopyError(copied) if copied == tab
        )));
    }
    {
        let mut ui = simulator(&shell);
        ui.click("Edit profile").expect("edit profile");
        assert!(ui.into_messages().any(|message| matches!(
            &message,
            Message::App(AppMessage::EditProfile(id)) if *id == ProfileId::new("a")
        )));
    }
    let report = shell
        .failure_report(tab, std::time::UNIX_EPOCH)
        .expect("a report");
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(lines[0], "Heimdall SSH error report");
    assert_eq!(lines[1], "Time: 1970-01-01 00:00:00Z");
    assert_eq!(lines[2], "Server: server a (a.lab:22)");
    assert!(lines[3].starts_with("App: Heimdall-rs v"), "{report}");
    assert_eq!(
        lines.last(),
        Some(&"The server did not answer in time."),
        "{report}"
    );
    assert!(
        shell
            .failure_report(cancelled, std::time::UNIX_EPOCH)
            .is_some(),
        "a report exists, the card just offers none"
    );

    let mut core = shell.into_app();
    core.update(AppMessage::SelectTab(cancelled));
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    assert!(ui.find("Copy error").is_err(), "a cancel is no error");
    ui.find("Edit profile")
        .expect("still the way to the profile");
}

#[test]
fn an_unknown_ssh_host_is_asked_about_as_the_csharp_one_with_trust_this_session() {
    use heimdall_ssh::PublicKey;

    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    let key = include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519.pub");
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::UnknownHostKey {
            host: "a.lab".to_owned(),
            port: 22,
            fingerprint: "SHA256:x".to_owned(),
            key: Arc::new(PublicKey::from_openssh(key.trim()).expect("key")),
        },
    });
    let shell = Shell::with_app(core);
    snapshot(&shell, "hostkey-unknown.png");
    let mut ui = simulator(&shell);
    ui.find("Unknown SSH host").expect("title");
    assert!(ui.find("Unrecognised Server Certificate").is_err());
    ui.click("Trust this session").expect("once");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::HostKeyTrustOnce(trusted)) if trusted == tab
    )));
    let mut ui = simulator(&shell);
    ui.click("Accept").expect("accept");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::HostKeyDecision { accept: true, .. })
    )));
}

#[test]
fn a_click_on_a_folder_folds_it_and_hides_its_profiles() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    {
        let mut ui = simulator(&shell);
        ui.find("server a").expect("shown");
        ui.click("Production").expect("folder");
        assert!(ui.into_messages().any(|message| matches!(
            &message,
            Message::App(AppMessage::ToggleFolder(path)) if path == "Production"
        )));
    }
    let _ = shell.update(Message::App(AppMessage::ToggleFolder(
        "Production".to_owned(),
    )));
    let mut ui = simulator(&shell);
    ui.find("Production").expect("the folder stays");
    assert!(ui.find("server a").is_err(), "its profiles are folded away");
    ui.find("server c").expect("another folder's profile stays");
}

#[test]
fn a_folder_has_the_csharp_menu_and_its_name_dialog_says_why_a_name_is_refused() {
    use heimdall_app::FolderMessage;
    use heimdall_ui::tree_view::TreeMenu;
    use iced::mouse;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    {
        let mut ui = simulator(&shell);
        let folder = ui.find("Production").expect("folder");
        ui.point_at(folder.bounds().center());
        ui.simulate([
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
        ]);
        assert!(ui.into_messages().any(|message| matches!(
            &message,
            Message::OpenTreeMenu(TreeMenu::Folder(path)) if path == "Production"
        )));
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Folder(
        "Production".to_owned(),
    )));
    snapshot(&shell, "folder-menu.png");
    {
        let mut ui = simulator(&shell);
        for entry in [
            "Connect all (2)",
            "Add Session",
            "New folder",
            "Rename",
            "Move to",
        ] {
            ui.find(entry).expect(entry);
        }
        ui.click("Delete folder").expect("delete");
        assert!(ui.into_messages().any(|message| matches!(
            &message,
            Message::MenuChoice(AppMessage::Folder(FolderMessage::RequestDelete(path)))
                if path == "Production"
        )));
    }
    {
        let mut ui = simulator(&shell);
        ui.click("Move to").expect("move to");
        assert!(ui.into_messages().any(|message| matches!(
            &message,
            Message::OpenTreeMenu(TreeMenu::MoveFolder(path)) if path == "Production"
        )));
    }
    // At the top already, with no other folder: nowhere to go.
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::MoveFolder(
        "Production".to_owned(),
    )));
    assert!(simulator(&shell).find("Top level").is_err());

    let _ = shell.update(Message::MenuChoice(AppMessage::Folder(
        FolderMessage::New {
            parent: String::new(),
        },
    )));
    let _ = shell.update(Message::App(AppMessage::Folder(FolderMessage::NameEdited(
        "production".to_owned(),
    ))));
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    let mut ui = simulator(&shell);
    ui.find("New Folder").expect("the dialog stays");
    ui.find("A folder with this name already exists at the same level.")
        .expect("and says why");
}

#[test]
fn a_profile_renames_and_moves_to_another_folder_from_its_menu() {
    use heimdall_app::ProfileMenuMessage;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let id = ProfileId::new("a");
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Profile(id.clone())));
    {
        let mut ui = simulator(&shell);
        ui.click("Rename").expect("rename");
        assert!(ui.into_messages().any(|message| matches!(
            &message,
            Message::MenuChoice(AppMessage::ProfileMenu(ProfileMenuMessage::Rename(renamed)))
                if *renamed == id
        )));
    }
    {
        let mut ui = simulator(&shell);
        ui.click("Move to folder").expect("move");
        assert!(ui.into_messages().any(|message| matches!(
            &message,
            Message::OpenTreeMenu(TreeMenu::MoveProfile(moved)) if *moved == id
        )));
    }
    // The list alone: every label is unique there, the tree's own "(No Folder)" aside.
    let targets = vec![
        (None, true),
        (Some("Production".to_owned()), false),
        (Some("Lab".to_owned()), true),
    ];
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    for (label, expected) in [
        ("(No Folder)", Some(None)),
        ("Production", None),
        ("Lab", Some(Some("Lab".to_owned()))),
    ] {
        let mut ui = Simulator::with_size(
            settings.clone(),
            WINDOW,
            heimdall_ui::tree_view::move_profile_entries(&id, &targets),
        );
        ui.click(label).expect(label);
        let moved: Vec<Option<String>> = ui
            .into_messages()
            .filter_map(|message| match message {
                Message::MenuChoice(AppMessage::ProfileMenu(ProfileMenuMessage::Move {
                    to,
                    ..
                })) => Some(to),
                _ => None,
            })
            .collect();
        match expected {
            Some(to) => assert_eq!(moved, [to], "{label}"),
            None => assert!(moved.is_empty(), "{label} is its own folder: greyed"),
        }
    }
}

#[test]
fn ctrl_click_selects_several_and_their_right_click_is_the_bulk_menu() {
    use heimdall_app::SelectionMessage;
    use heimdall_ui::tree_view::TreeMenu;
    use iced::keyboard::Modifiers;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::TreeClick(ProfileId::new("b")));
    let _ = shell.update(Message::Modifiers(Modifiers::CTRL));
    let _ = shell.update(Message::TreeClick(ProfileId::new("a")));
    let _ = shell.update(Message::Modifiers(Modifiers::empty()));
    let core = shell.into_app();
    assert_eq!(
        core.selected_profiles(),
        [ProfileId::new("a"), ProfileId::new("b")]
    );
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Profile(ProfileId::new(
        "a",
    ))));
    snapshot(&shell, "selection-menu.png");
    {
        let mut ui = simulator(&shell);
        ui.find("2 items selected").expect("the bulk menu");
        ui.find("Connect selected (2)").expect("connect");
        ui.click("Delete selected (2)").expect("delete");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::MenuChoice(AppMessage::Selection(SelectionMessage::RequestDelete))
        )));
    }
    let _ = shell.update(Message::MenuChoice(AppMessage::Selection(
        SelectionMessage::RequestDelete,
    )));
    let mut ui = simulator(&shell);
    ui.find("Delete Selected Items").expect("asked");
    ui.find("Are you sure you want to delete 2 selected item(s)?\n- server a\n- server b")
        .expect("listed");
}
