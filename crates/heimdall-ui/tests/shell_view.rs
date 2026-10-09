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

mod common;

use std::path::Path;
use std::sync::Arc;

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Dialog, Message as AppMessage,
    PostConnectConfirmation, QuestionId, QuestionKind, TabId, UiError,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{
    AgentSource, KeyboardInteractivePrompt, KeyboardInteractiveQuestion, PasswordQuestion,
    SessionClosed, TerminalSize,
};
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell, sidebar_toggle_id, tab_close_id};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::terminal_view::keys::WindowShortcut;
use iced::keyboard::key::Named;
use iced::{Settings, Size, event};

const GRID: GridSize = GridSize { cols: 80, rows: 24 };

/// Size of the simulated window, in logical pixels.
const WINDOW: Size = Size::new(1100.0, 700.0);

/// A window tall enough for a tab's whole menu opened where the pointer was left.
const TALL_WINDOW: Size = Size::new(1100.0, 1200.0);

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

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([
        profile("a", Some("Production")),
        profile("b", Some("Production")),
        profile("c", None),
    ]);
    store.save().expect("save");
    let mut app = App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GRID,
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    });
    // The shell drawn alone: no SFTP pane docked beside a shell connected.
    app.update(AppMessage::Settings(
        heimdall_app::SettingsMessage::SftpBrowser(heimdall_core::settings::SftpBrowser {
            auto_open_on_ssh: false,
            ..heimdall_core::settings::SftpBrowser::default()
        }),
    ));
    app
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

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, WINDOW, shell.view())
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
    ui.find("Sessions").expect("heading");
    ui.find("Production").expect("group");
    ui.find("(No Folder)").expect("profiles without a folder");
    let production = ui.find("Production").expect("group").bounds().y;
    let ungrouped = ui.find("(No Folder)").expect("group").bounds().y;
    assert!(
        production < ungrouped,
        "profiles without a folder come last"
    );
    drop(ui);
    let connected: Vec<String> = common::double_click_messages(|| simulator(&shell), "server b")
        .into_iter()
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
fn a_gateway_asking_is_named_and_its_words_are_framed_as_its_own() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Question {
            question: QuestionId::fresh(),
            kind: QuestionKind::KeyboardInteractive(KeyboardInteractiveQuestion {
                host: "jump.lab".to_owned(),
                username: "bastion".to_owned(),
                name: String::new(),
                instructions: "Duo\n\nHeimdall: type the vault master password".to_owned(),
                prompts: vec![KeyboardInteractivePrompt {
                    text: "Passcode: ".to_owned(),
                    echo: false,
                }],
            }),
        },
    });
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find("bastion on jump.lab: the server asks")
        .expect("the gateway asking is named, not the profile's a.lab");
    ui.find("Server says: Duo Heimdall: type the vault master password")
        .expect("one line, under the server's name");
    ui.find("Passcode:").expect("the prompt");
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
    ui.find("1 session is still open and will be disconnected.")
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
fn the_navigation_shows_the_sessions_the_tunnels_the_settings_and_about() {
    use heimdall_ui::shell::Destination;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    snapshot(&shell, "navigation.png");
    {
        let mut ui = simulator(&shell);
        for label in [
            "Heimdall",
            "Sessions",
            "Tunnels",
            "Settings",
            "About",
            "Local shell",
        ] {
            ui.find(label).expect(label);
        }
        ui.click("Tunnels").expect("Tunnels");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::Navigate(Destination::Tunnels)))
        );
    }
    // Every tunnel, the window's whole width: no tree beside them.
    let _ = shell.update(Message::Navigate(Destination::Tunnels));
    {
        let mut ui = simulator(&shell);
        ui.find("Active Tunnels").expect("the Tunnels page");
        ui.find("No active tunnels").expect("none open");
        assert!(ui.find("Local shell").is_err(), "no tree");
    }
    let _ = shell.update(Message::Navigate(Destination::About));
    simulator(&shell)
        .find("Diagnostics")
        .expect("the About page");
    let _ = shell.update(Message::Navigate(Destination::Settings));
    assert!(shell.settings_shown());
    let _ = shell.update(Message::Navigate(Destination::Sessions));
    simulator(&shell)
        .find("Local shell")
        .expect("the tree again");
}

#[test]
fn the_master_password_is_enabled_from_the_settings_with_its_rules_said_as_typed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    {
        let mut ui = simulator(&shell);
        ui.click("Settings").expect("the navigation's settings");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::Navigate(heimdall_ui::shell::Destination::Settings)
        )));
    }
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(
        heimdall_ui::shell::SettingsTab::Security,
    ));
    assert!(shell.settings_shown());
    snapshot(&shell, "settings-vault-disabled.png");
    {
        let mut ui = tall_settings(&shell);
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
        let mut ui = tall_settings(shell);
        ui.click("Enable").expect("the dialog's button");
        ui.into_messages()
            .any(|message| matches!(message, Message::SubmitVault))
    };
    {
        let mut ui = tall_settings(&shell);
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
    tall_settings(&shell)
        .find("Too short: use at least 12 characters.")
        .expect("too short");
    type_new(&mut shell, "alllowercase", "alllowercase");
    tall_settings(&shell)
        .find("Use at least 3 character types (lower, upper, digit, symbol), or 20 characters or more.")
        .expect("too simple");
    assert!(!enable_clicked(&shell), "refused by the rules");
    type_new(&mut shell, "Mixed-case 12", "Mixed-case 13");
    tall_settings(&shell)
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
        assert!(ui.find("Sessions").is_err(), "the window is hidden");
        assert!(ui.find("Cancel").is_err(), "no way around it");
        // Found is not enough: a dialog laid out in no room is found and cannot be typed in.
        ui.click(iced::widget::Id::from("vault-field-0".to_owned()))
            .expect("the master password's field takes a click");
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
    // The anonymized one, as the C# RDP overlay's: no server, account nor message.
    let anonymous = shell
        .anonymous_report(tab, std::time::UNIX_EPOCH)
        .expect("a report");
    let lines: Vec<&str> = anonymous.lines().collect();
    assert_eq!(lines[0], "SSH diagnostic report (anonymized)");
    assert_eq!(lines[1], "Time: 1970-01-01 00:00:00Z");
    assert!(lines[3].starts_with("Failure: "), "{anonymous}");
    assert_eq!(lines[3], "Failure: Timeout");
    for named in ["a.lab", "Server:", "did not answer"] {
        assert!(!anonymous.contains(named), "{named} in {anonymous}");
    }

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
fn the_session_selected_is_shown_in_detail_until_one_is_open() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    assert!(simulator(&shell).find("Connect").is_err(), "none selected");
    let _ = shell.update(Message::TreeClick(ProfileId::new("a")));
    snapshot(&shell, "detail.png");
    let mut ui = simulator(&shell);
    for label in ["a.lab:22", "Folder:", "Username:", "admin"] {
        ui.find(label).expect(label);
    }
    ui.click("Connect").expect("Connect");
    assert!(ui.into_messages().any(|message| matches!(
        &message,
        Message::App(AppMessage::ConnectProfile(id)) if id.as_str() == "a"
    )));
}

#[test]
fn a_session_menu_writes_a_note_about_it_from_a_csharp_template() {
    use heimdall_app::notes::NoteTemplate;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let id = ProfileId::new("a");
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Profile(id.clone())));
    {
        let mut ui = simulator(&shell);
        ui.click("Notes").expect("Notes");
        assert!(ui.into_messages().any(|message| matches!(
            &message,
            Message::OpenTreeMenu(TreeMenu::Notes(about)) if *about == id
        )));
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Notes(id.clone())));
    let mut ui = simulator(&shell);
    for label in ["New", "Daily", "Incident"] {
        ui.find(label).expect(label);
    }
    ui.click("Procedure").expect("Procedure");
    assert!(ui.into_messages().any(|message| matches!(
        &message,
        Message::NewNote { id: about, template: NoteTemplate::Procedure } if *about == id
    )));
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
        let mut ui = common::simulator(
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
    ui.find("Are you sure you want to delete 2 selected items?\n- server a\n- server b")
        .expect("listed");
}

#[test]
fn the_tree_takes_the_keyboard_from_a_terminal_until_a_click_gives_it_back() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _) = connected_shell(dir.path());
    let typed = |shell: &Shell| {
        let mut ui = simulator(shell);
        ui.typewrite("x");
        ui.into_messages()
            .filter(|message| matches!(message, Message::App(AppMessage::Key { tab: t, .. }) if *t == tab))
            .count()
    };
    assert_eq!(typed(&shell), 1, "the terminal has it");
    let _ = shell.update(Message::TreeClick(ProfileId::new("a")));
    assert_eq!(typed(&shell), 0, "the tree took it");
    {
        let mut ui = simulator(&shell);
        ui.point_at(iced::Point::new(700.0, 400.0));
        ui.simulate([
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                iced::mouse::Button::Left,
            )),
        ]);
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::ContentFocus)),
            "a click in the session"
        );
    }
    let _ = shell.update(Message::ContentFocus);
    assert_eq!(typed(&shell), 1, "given back");
}

#[test]
fn the_tree_keys_move_connect_rename_delete_and_edit_as_the_csharp_ones() {
    use heimdall_app::Dialog;
    use heimdall_app::files::FilesKey;
    use heimdall_ui::shell::TreeShortcut;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let selected = |shell: &Shell| {
        shell
            .app()
            .selected_profile
            .clone()
            .map(|id| id.to_string())
    };

    // Not before the tree has the keyboard: Ctrl+E is nobody's.
    let _ = shell.update(Message::App(AppMessage::SelectProfile(ProfileId::new("a"))));
    let _ = shell.update(Message::TreeShortcut(TreeShortcut::Edit));
    assert!(shell.app().dialog.is_none());
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    assert_eq!(
        selected(&shell).as_deref(),
        Some("a"),
        "nor are the arrows the tree's"
    );

    let _ = shell.update(Message::TreeClick(ProfileId::new("a")));
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    assert_eq!(selected(&shell).as_deref(), Some("b"));
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    assert_eq!(
        (selected(&shell), shell.app().selected_folder.as_deref()),
        (None, Some(heimdall_app::NO_FOLDER)),
        "a folder is a row too, as in the C# tree"
    );
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    assert_eq!(
        selected(&shell).as_deref(),
        Some("c"),
        "into the next folder"
    );
    assert_eq!(shell.app().selected_folder, None);
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    assert_eq!(selected(&shell).as_deref(), Some("c"), "the last stays");
    let _ = shell.update(Message::FilesKey(FilesKey::Previous));
    let _ = shell.update(Message::FilesKey(FilesKey::Previous));
    assert_eq!(selected(&shell).as_deref(), Some("b"));

    let _ = shell.update(Message::FilesKey(FilesKey::Rename));
    assert!(matches!(
        shell.app().dialog,
        Some(Dialog::RenameProfile { .. })
    ));
    let _ = shell.update(Message::App(AppMessage::DismissDialog));
    let _ = shell.update(Message::FilesKey(FilesKey::Delete));
    assert!(matches!(
        shell.app().dialog,
        Some(Dialog::ConfirmDeleteProfile { .. })
    ));
    let _ = shell.update(Message::App(AppMessage::DismissDialog));
    let _ = shell.update(Message::TreeShortcut(TreeShortcut::Edit));
    assert!(matches!(
        shell.app().dialog,
        Some(Dialog::EditProfile { .. })
    ));
    let _ = shell.update(Message::App(AppMessage::DismissDialog));
    let _ = shell.update(Message::TreeShortcut(TreeShortcut::New));
    assert!(matches!(
        &shell.app().dialog,
        Some(Dialog::EditProfile { draft, .. }) if draft.editing.is_none()
    ));
    let _ = shell.update(Message::App(AppMessage::DismissDialog));

    // Enter connects the one selected.
    let _ = shell.update(Message::DialogKey { confirm: true });
    assert_eq!(shell.app().tabs.len(), 1);
    assert_eq!(shell.app().tabs[0].title, "server b");
}

#[test]
fn ctrl_k_opens_quick_connect_which_finds_a_session_or_a_host_and_opens_it() {
    use heimdall_app::files::FilesKey;
    use heimdall_ui::shell::TreeShortcut;
    use heimdall_ui::tree_view::TreeMenu;

    const FIELD: &str = "Search host or IP... (Ctrl+K)";
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    assert!(simulator(&shell).find(FIELD).is_err(), "closed at first");

    // Over a menu open, it closes the menu.
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Add));
    simulator(&shell).find("New folder").expect("the menu");
    let _ = shell.update(Message::TreeShortcut(TreeShortcut::QuickConnect));
    assert!(
        simulator(&shell).find("New folder").is_err(),
        "the menu closed"
    );
    snapshot(&shell, "quick-connect.png");
    {
        let mut ui = simulator(&shell);
        for name in ["SSH  server a", "SSH  server b", "SSH  server c"] {
            ui.find(name).expect(name);
        }
        ui.click(FIELD).expect("its field");
        ui.typewrite("b");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::PaletteQuery(query) if query == "b"))
        );
    }
    let _ = shell.update(Message::PaletteQuery("B.LAB".to_owned()));
    {
        let mut ui = simulator(&shell);
        ui.find("SSH  server b").expect("found by its host");
        assert!(ui.find("SSH  server a").is_err(), "filtered out");
    }
    // Enter opens the one chosen, and closes the palette.
    let _ = shell.update(Message::DialogKey { confirm: true });
    assert_eq!(shell.app().tabs.len(), 1);
    assert_eq!(shell.app().tabs[0].title, "server b");
    assert!(
        simulator(&shell).find("SSH  server b").is_err(),
        "closed: the tree names it alone"
    );

    // Escape closes it, and nothing more.
    let _ = shell.update(Message::TreeShortcut(TreeShortcut::QuickConnect));
    let _ = shell.update(Message::DialogKey { confirm: false });
    assert!(simulator(&shell).find(FIELD).is_err());
    assert_eq!(shell.app().tabs.len(), 1);

    // A host no session matches: SSH or RDP to it, the arrows choosing.
    let _ = shell.update(Message::TreeShortcut(TreeShortcut::QuickConnect));
    let _ = shell.update(Message::PaletteQuery("jump.lab".to_owned()));
    {
        let mut ui = simulator(&shell);
        ui.find("[SSH] Connect to jump.lab").expect("SSH");
        ui.click("[RDP] Connect to jump.lab").expect("RDP");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::PaletteChoose(1)))
        );
    }
    // Down twice stays on the last; up comes back to the first.
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    let _ = shell.update(Message::FilesKey(FilesKey::Previous));
    let _ = shell.update(Message::DialogKey { confirm: true });
    assert_eq!(shell.app().tabs.len(), 2);
    assert_eq!(
        shell.app().tab_kind(&shell.app().tabs[1]),
        heimdall_app::ProfileKind::Ssh
    );

    // Neither a session nor a host: said, and a choice there opens nothing.
    let _ = shell.update(Message::TreeShortcut(TreeShortcut::QuickConnect));
    let _ = shell.update(Message::PaletteQuery("jump.lab".to_owned()));
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    let _ = shell.update(Message::PaletteQuery("no such thing".to_owned()));
    simulator(&shell)
        .find("No session matches, and this is no host to connect to.")
        .expect("says so");
    let _ = shell.update(Message::PaletteChoose(0));
    assert_eq!(shell.app().tabs.len(), 2);
    simulator(&shell)
        .find("No session matches, and this is no host to connect to.")
        .expect("still open");
    // A new search chooses its first again.
    let _ = shell.update(Message::PaletteQuery("jump.lab".to_owned()));
    let _ = shell.update(Message::DialogKey { confirm: true });
    assert_eq!(shell.app().tabs.len(), 3);
    assert_eq!(
        shell.app().tab_kind(&shell.app().tabs[2]),
        heimdall_app::ProfileKind::Ssh
    );
}

#[test]
fn with_no_session_saved_the_window_welcomes_and_offers_to_add_one() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = Shell::with_app(App::new(AppConfig {
        profiles_file: dir.path().join("profiles.toml"),
        known_hosts: dir.path().join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GRID,
        files_start: dir.path().to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    }));
    snapshot(&shell, "home-empty.png");
    let mut ui = simulator(&shell);
    ui.find("Welcome to Heimdall-rs").expect("welcomes");
    ui.find("Ctrl+N to add a session, Ctrl+K to quick connect")
        .expect("the shortcuts");
    assert!(
        ui.find("Select a session or press Ctrl+K to connect")
            .is_err(),
        "no session to select"
    );
    ui.click("Import Connections").expect("shown");
    ui.click("Add Session").expect("its button");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        matches!(messages.as_slice(), [Message::App(AppMessage::NewProfile)]),
        "nothing to import from here: {messages:?}"
    );

    // With the C# Heimdall's sessions beside it, they are offered.
    let legacy = dir.path().join("legacy");
    std::fs::create_dir(&legacy).expect("dir");
    std::fs::write(
        legacy.join(heimdall_core::paths::LEGACY_SERVERS_FILE_NAME),
        "[]",
    )
    .expect("written");
    let shell = Shell::with_app(App::new(AppConfig {
        profiles_file: dir.path().join("profiles.toml"),
        known_hosts: dir.path().join("known_hosts"),
        legacy_dir: Some(legacy),
        agent: AgentSource::Disabled,
        initial_grid: GRID,
        files_start: dir.path().to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    }));
    let mut ui = simulator(&shell);
    ui.click("Import Connections").expect("its button");
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::App(AppMessage::ImportLegacy)))
    );
}

#[test]
fn with_sessions_saved_the_window_says_how_to_open_one() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = Shell::with_app(app(dir.path()));
    let mut ui = simulator(&shell);
    ui.find("Select a session or press Ctrl+K to connect")
        .expect("how to open one");
    assert!(ui.find("Welcome to Heimdall-rs").is_err());
    assert!(ui.find("Add Session").is_err());
}

#[test]
fn the_status_bar_says_the_session_shown_and_counts_the_sessions() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    {
        let mut ui = simulator(&shell);
        ui.find("Ready. Select a session to get started.")
            .expect("ready");
        ui.find("3 sessions").expect("counted");
    }
    let _ = shell.update(Message::Search("b.lab".to_owned()));
    simulator(&shell)
        .find("1 of 3 sessions")
        .expect("those the search shows");
    let _ = shell.update(Message::Search("  ".to_owned()));
    simulator(&shell)
        .find("3 sessions")
        .expect("blank: no search");
    let _ = shell.update(Message::Search(String::new()));

    let _ = shell.update(Message::App(AppMessage::OpenProfile(ProfileId::new("a"))));
    snapshot(&shell, "status-bar.png");
    simulator(&shell)
        .find("server a: Connecting...")
        .expect("the session shown");
    let _ = shell.update(Message::App(AppMessage::CopyProfile {
        id: ProfileId::new("b"),
        what: heimdall_app::ProfileCopy::Hostname,
    }));
    simulator(&shell)
        .find("Copied to clipboard: b.lab")
        .expect("what was just done");
}

#[test]
fn ctrl_plus_and_minus_zoom_the_terminal_shown_within_the_csharp_bounds() {
    use heimdall_term::MouseAction;
    use heimdall_ui::terminal_view::keys::Zoom;
    use iced::{Point, keyboard, mouse};

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _) = connected_shell(dir.path());
    let columns = |shell: &Shell| {
        let mut ui = simulator(shell);
        // Any event: the terminal reports its size on the first it gets.
        let _ = ui.simulate([iced::Event::Keyboard(keyboard::Event::ModifiersChanged(
            keyboard::Modifiers::empty(),
        ))]);
        ui.into_messages()
            .find_map(|message| match message {
                Message::App(AppMessage::Resize { grid, .. }) => Some(grid.cols),
                _ => None,
            })
            .expect("a size reported")
    };
    let same = |a: f32, b: f32| (a - b).abs() < f32::EPSILON;
    let normal = columns(&shell);
    assert!(same(shell.font_size(tab), 15.0));

    let _ = shell.update(Message::Shortcut(WindowShortcut::Zoom(Zoom::In)));
    assert!(same(shell.font_size(tab), 16.0));
    assert!(columns(&shell) < normal, "larger text, fewer columns");
    for _ in 0..20 {
        let _ = shell.update(Message::Shortcut(WindowShortcut::Zoom(Zoom::In)));
    }
    assert!(same(shell.font_size(tab), 28.0), "no larger than 28");
    for _ in 0..30 {
        let _ = shell.update(Message::Shortcut(WindowShortcut::Zoom(Zoom::Out)));
    }
    assert!(same(shell.font_size(tab), 8.0), "no smaller than 8");
    assert!(columns(&shell) > normal);
    let _ = shell.update(Message::Shortcut(WindowShortcut::Zoom(Zoom::Reset)));
    assert!(same(shell.font_size(tab), 15.0));
    assert_eq!(columns(&shell), normal);

    // The wheel with Ctrl held zooms instead of scrolling; without it, it scrolls.
    let wheel = |ctrl: bool| {
        let mut ui = simulator(&shell);
        ui.point_at(Point::new(700.0, 400.0));
        let modifiers = if ctrl {
            keyboard::Modifiers::CTRL
        } else {
            keyboard::Modifiers::empty()
        };
        let _ = ui.simulate([
            iced::Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(700.0, 400.0),
            }),
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)),
            iced::Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
            }),
        ]);
        ui.into_messages().collect::<Vec<_>>()
    };
    let zoomed = wheel(true);
    assert!(
        zoomed
            .iter()
            .any(|message| matches!(message, Message::Shortcut(WindowShortcut::Zoom(Zoom::Out)))),
        "{zoomed:?}"
    );
    assert!(
        !zoomed.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::Pointer { input, .. })
                if matches!(input.action, MouseAction::WheelUp | MouseAction::WheelDown)
        )),
        "and does not scroll"
    );
    let scrolled = wheel(false);
    assert!(
        !scrolled
            .iter()
            .any(|message| matches!(message, Message::Shortcut(WindowShortcut::Zoom(_)))),
        "{scrolled:?}"
    );
    assert!(
        scrolled.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::Pointer { input, .. })
                if matches!(input.action, MouseAction::WheelUp | MouseAction::WheelDown)
        )),
        "it scrolls: {scrolled:?}"
    );
}

#[test]
fn ctrl_shift_f_searches_the_terminal_history_as_the_csharp_bar() {
    use heimdall_term::FindDirection;
    use iced::keyboard;

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = connected_shell(dir.path());
    let output: String = (0..60)
        .map(|n| {
            if n == 10 {
                "an error here\r\n".to_owned()
            } else {
                format!("line {n}\r\n")
            }
        })
        .collect();
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Output(output.into_bytes()),
    }));
    let typed = |shell: &mut Shell, query: &str| {
        let _ = shell.update(Message::FinderQuery {
            tab,
            query: query.to_owned(),
        });
    };
    let searched = |shell: &mut Shell, direction: FindDirection| {
        let _ = shell.update(Message::FinderFind { tab, direction });
    };
    let bar_shown = |shell: &Shell| simulator(shell).find("\u{25b2}").is_ok();
    assert!(!bar_shown(&shell));

    let _ = shell.update(Message::Shortcut(WindowShortcut::Find));
    assert!(bar_shown(&shell));
    snapshot(&shell, "terminal-find.png");
    {
        let mut ui = simulator(&shell);
        ui.typewrite("x");
        assert!(
            !ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::Key { .. }))),
            "under the bar, the terminal takes no keys"
        );
    }
    {
        let mut ui = simulator(&shell);
        ui.click("Search...").expect("its field");
        ui.typewrite("e");
        assert!(
            ui.into_messages().any(
                |message| matches!(message, Message::FinderQuery { query, .. } if query == "e")
            )
        );
    }
    typed(&mut shell, "ERROR");
    searched(&mut shell, FindDirection::Up);
    assert_eq!(
        shell.app().tabs[0].terminal.selected_text().as_deref(),
        Some("error"),
        "found, whatever the case"
    );
    assert!(simulator(&shell).find("No match").is_err());

    typed(&mut shell, "absent");
    searched(&mut shell, FindDirection::Down);
    simulator(&shell).find("No match").expect("said");
    typed(&mut shell, "absen");
    assert!(
        simulator(&shell).find("No match").is_err(),
        "not for a text not yet looked for"
    );

    // Enter looks down; with Shift, up.
    typed(&mut shell, "");
    for (modifiers, direction) in [
        (keyboard::Modifiers::empty(), FindDirection::Down),
        (keyboard::Modifiers::SHIFT, FindDirection::Up),
    ] {
        let _ = shell.update(Message::Modifiers(modifiers));
        let mut ui = simulator(&shell);
        ui.click("Search...").expect("its field");
        let _ = ui.tap_key(keyboard_named(Named::Enter));
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::FinderFind { direction: d, .. } if d == direction)),
            "{direction:?}"
        );
    }
    let _ = shell.update(Message::Modifiers(keyboard::Modifiers::empty()));

    // Escape closes it, then the terminal takes keys again; Ctrl+Shift+F toggles it.
    let _ = shell.update(Message::DialogKey { confirm: false });
    assert!(!bar_shown(&shell));
    {
        let mut ui = simulator(&shell);
        ui.typewrite("x");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::Key { .. })))
        );
    }
    let _ = shell.update(Message::Shortcut(WindowShortcut::Find));
    let _ = shell.update(Message::Shortcut(WindowShortcut::Find));
    assert!(!bar_shown(&shell), "the shortcut again closes it");
    let _ = shell.update(Message::Shortcut(WindowShortcut::Find));
    let _ = shell.update(Message::FinderClose(tab));
    assert!(!bar_shown(&shell));
}

#[test]
fn a_session_still_connecting_has_no_search_bar() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::Shortcut(WindowShortcut::Find));
    assert!(simulator(&shell).find("\u{25b2}").is_err());
    // Nor once it is connected: the shortcut asked for none then.
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    }));
    assert!(simulator(&shell).find("\u{25b2}").is_err());
}

#[test]
fn the_settings_page_has_the_terminal_appearance_section() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(
        heimdall_ui::shell::SettingsTab::Terminal,
    ));
    snapshot(&shell, "settings-terminal.png");
    let mut ui = simulator(&shell);
    ui.find("Terminal Appearance").expect("its section");
    ui.find("Color scheme").expect("its label");
}

#[test]
fn a_transcript_starts_from_the_tab_menu_and_its_tab_says_rec() {
    use heimdall_app::TabMenuMessage;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _) = connected_shell(dir.path());
    let chosen = |shell: &Shell, entry: &str| {
        let settings = Settings {
            fonts: FONTS.iter().map(|face| (*face).into()).collect(),
            ..Settings::default()
        };
        let mut ui = common::simulator(settings, TALL_WINDOW, shell.view());
        ui.click(entry).expect(entry);
        ui.into_messages()
            .filter_map(|message| match message {
                Message::MenuChoice(chosen) => Some(format!("{chosen:?}")),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(simulator(&shell).find("REC").is_err());
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(tab)));
    assert_eq!(
        chosen(&shell, "Start Transcript"),
        [format!(
            "{:?}",
            AppMessage::TabMenu(TabMenuMessage::StartTranscript(tab))
        )]
    );
    let _ = shell.update(Message::MenuChoice(AppMessage::TabMenu(
        TabMenuMessage::StartTranscript(tab),
    )));
    snapshot(&shell, "tab-recording.png");
    let path = shell.app().tabs[0]
        .transcript
        .as_ref()
        .map(|transcript| transcript.path().to_owned())
        .expect("kept");
    assert!(path.starts_with(dir.path()), "beside the profiles");
    {
        let mut ui = simulator(&shell);
        ui.find("REC").expect("the badge");
        ui.find(format!("Transcript started: {}", path.display()).as_str())
            .expect("the status says where");
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(tab)));
    assert!(simulator(&shell).find("Start Transcript").is_err());
    assert_eq!(
        chosen(&shell, "Stop Transcript"),
        [format!(
            "{:?}",
            AppMessage::TabMenu(TabMenuMessage::StopTranscript(tab))
        )]
    );
    let _ = shell.update(Message::MenuChoice(AppMessage::TabMenu(
        TabMenuMessage::StopTranscript(tab),
    )));
    let mut ui = simulator(&shell);
    assert!(ui.find("REC").is_err());
    ui.find("Transcript stopped").expect("said");
}

#[test]
fn the_settings_page_turns_session_logging_on_and_applies_its_folder_with_enter() {
    use heimdall_app::SettingsMessage;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(
        heimdall_ui::shell::SettingsTab::Terminal,
    ));
    {
        let mut ui = tall_settings(&shell);
        ui.find("Session Logging").expect("its section");
        ui.find(
            "Transcripts keep what you type as well as what is shown, including passwords or \
             tokens echoed to the terminal. Keep the log folder private.",
        )
        .expect("what a transcript keeps is said");
        ui.click("Record session transcripts (what each terminal shows, typed input included)")
            .expect("its box");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::Settings(SettingsMessage::SessionLogging(true)))
        )));
    }
    let _ = shell.update(Message::App(AppMessage::Settings(
        SettingsMessage::SessionLogging(true),
    )));
    {
        let mut ui = tall_settings(&shell);
        ui.find("Record session transcripts?").expect("asked first");
        ui.click("Turn on").expect("its answer");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog)))
        );
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    assert!(shell.app().settings().session_logging);
    {
        let mut ui = tall_settings(&shell);
        ui.click("logs/sessions").expect("its folder");
        ui.typewrite("x");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::LogDirectoryEdited(_)))
        );
    }
    let _ = shell.update(Message::LogDirectoryEdited("records".to_owned()));
    assert_eq!(
        shell.app().settings().session_log_directory,
        "logs/sessions",
        "typed, not yet applied"
    );
    let _ = shell.update(Message::LogDirectoryApply);
    assert_eq!(shell.app().settings().session_log_directory, "records");
    let _ = shell.update(Message::LogDirectoryApply);
    assert_eq!(
        shell.app().settings().session_log_directory,
        "records",
        "nothing typed since: nothing changes"
    );
}

#[test]
fn the_status_bar_turns_broadcast_on_and_marks_tabs_in_the_selected_scope() {
    use heimdall_app::{BroadcastMessage, Dialog};

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _) = connected_shell(dir.path());
    let broadcast = |message| Message::App(AppMessage::Broadcast(message));
    let clicked = |shell: &Shell, label: &str| {
        let mut ui = simulator(shell);
        ui.click(label).expect(label);
        ui.into_messages()
            .filter_map(|message| match message {
                Message::App(AppMessage::Broadcast(message)) => Some(message),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(clicked(&shell, "BROADCAST"), [BroadcastMessage::Toggle]);
    assert_eq!(clicked(&shell, "Current tab"), [BroadcastMessage::Scope]);
    assert!(
        simulator(&shell).find("\u{25cb}").is_err(),
        "no marks while off"
    );
    let _ = shell.update(broadcast(BroadcastMessage::Scope));
    simulator(&shell).find("All tabs").expect("the next scope");

    let _ = shell.update(broadcast(BroadcastMessage::Toggle));
    assert_eq!(shell.app().dialog, Some(Dialog::ConfirmBroadcast));
    snapshot(&shell, "broadcast-confirm.png");
    simulator(&shell)
        .find("Broadcast to all tabs?")
        .expect("asked");
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    {
        let mut ui = simulator(&shell);
        ui.find("Broadcast mode ON - All tabs").expect("said");
        assert!(
            ui.find("\u{25cb}").is_err(),
            "every tab is reached: no marks"
        );
    }

    let _ = shell.update(broadcast(BroadcastMessage::Scope));
    snapshot(&shell, "broadcast-selected.png");
    simulator(&shell)
        .find("Selected tabs (0)")
        .expect("its scope");
    assert_eq!(
        clicked(&shell, "\u{25cb}"),
        [BroadcastMessage::Target(tab)],
        "the tab's mark"
    );
    let _ = shell.update(broadcast(BroadcastMessage::Target(tab)));
    {
        let mut ui = simulator(&shell);
        ui.find("\u{25c9}").expect("marked");
        ui.find("Selected tabs (1)").expect("counted");
    }
    let _ = shell.update(broadcast(BroadcastMessage::Toggle));
    assert!(
        simulator(&shell).find("\u{25c9}").is_err(),
        "off: no marks, whatever the scope"
    );
}

#[test]
fn ctrl_alt_b_typed_in_a_terminal_is_left_to_the_window() {
    use iced::keyboard::{self, Location, key};

    let dir = tempfile::tempdir().expect("dir");
    let (shell, _, _) = connected_shell(dir.path());
    let pressed = |c: &str| {
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Character(c.into()),
            modified_key: keyboard::Key::Character(c.into()),
            physical_key: key::Physical::Code(key::Code::KeyB),
            location: Location::Standard,
            modifiers: keyboard::Modifiers::CTRL | keyboard::Modifiers::ALT,
            text: None,
            repeat: false,
        })
    };
    let mut ui = simulator(&shell);
    let statuses = ui.simulate([pressed("b")]);
    assert_eq!(statuses, [event::Status::Ignored], "the window's");
    let statuses = ui.simulate([pressed("{")]);
    assert_eq!(
        statuses,
        [event::Status::Captured],
        "AltGr+B typing a brace is the session's"
    );
    let keys = ui
        .into_messages()
        .filter(|message| matches!(message, Message::App(AppMessage::Key { .. })))
        .count();
    assert_eq!(keys, 1, "only the brace reached the terminal");
}

#[test]
fn a_tab_menu_leaves_the_keyboard_to_the_session() {
    use heimdall_app::TabMenuMessage;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _) = connected_shell(dir.path());
    // A profile selected in the tree: Enter there would open it.
    let _ = shell.update(Message::App(AppMessage::SelectProfile(ProfileId::new("a"))));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(tab)));
    let _ = shell.update(Message::MenuChoice(AppMessage::TabMenu(
        TabMenuMessage::StartTranscript(tab),
    )));
    let mut ui = simulator(&shell);
    ui.typewrite("ls");
    assert_eq!(
        ui.into_messages()
            .filter(|message| matches!(message, Message::App(AppMessage::Key { .. })))
            .count(),
        2,
        "typed into the session"
    );
    // Enter is the session's too: it opens nothing from the tree.
    let _ = shell.update(Message::DialogKey { confirm: true });
    assert_eq!(shell.app().tabs.len(), 1);
}

#[test]
fn the_font_size_set_starts_new_terminals_and_ctrl_0_comes_back_to_it() {
    use heimdall_ui::terminal_view::keys::Zoom;

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _) = connected_shell(dir.path());
    let same = |a: f32, b: f32| (a - b).abs() < f32::EPSILON;
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(
        heimdall_ui::shell::SettingsTab::Terminal,
    ));
    for (typed, refused) in [("7", true), ("big", true), ("73", true), (" 20 ", false)] {
        let _ = shell.update(Message::FontSizeEdited(typed.to_owned()));
        let _ = shell.update(Message::FontSizeApply);
        let mut ui = simulator(&shell);
        assert_eq!(
            ui.find("Terminal font size must be between 8 and 72.")
                .is_ok(),
            refused,
            "{typed}"
        );
    }
    assert_eq!(shell.app().settings().terminal_font_size, 20);
    assert!(same(shell.font_size(tab), 20.0), "a terminal not zoomed");
    // Applied, the text typed is gone: Enter on the field later never brings it back over a
    // size set since.
    let _ = shell.update(Message::App(AppMessage::Settings(
        heimdall_app::SettingsMessage::TerminalFontSize(12),
    )));
    let _ = shell.update(Message::FontSizeApply);
    assert_eq!(shell.app().settings().terminal_font_size, 12);
    let _ = shell.update(Message::App(AppMessage::Settings(
        heimdall_app::SettingsMessage::TerminalFontSize(20),
    )));
    let _ = shell.update(Message::Shortcut(WindowShortcut::Zoom(Zoom::In)));
    assert!(same(shell.font_size(tab), 21.0));
    let _ = shell.update(Message::Shortcut(WindowShortcut::Zoom(Zoom::Reset)));
    assert!(same(shell.font_size(tab), 20.0), "back to the size set");
    // Kept as set, drawn no larger than the C# terminal draws.
    let _ = shell.update(Message::FontSizeEdited("72".to_owned()));
    let _ = shell.update(Message::FontSizeApply);
    assert_eq!(shell.app().settings().terminal_font_size, 72);
    assert!(same(shell.font_size(tab), 28.0));
}

#[test]
fn the_font_family_chosen_draws_every_terminal_and_one_missing_falls_back_visibly() {
    use heimdall_ui::terminal_view::font::{FONT_FAMILY, TerminalFont};

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _) = connected_shell(dir.path());
    let drawn = |shell: &Shell| {
        let tab = shell.app().tab(tab).expect("tab");
        let view = shell.terminal_view(tab, true);
        (view.family(), view.metrics())
    };
    assert_eq!(shell.terminal_font(), TerminalFont::EMBEDDED);
    assert_eq!(
        drawn(&shell),
        (
            FONT_FAMILY,
            heimdall_ui::terminal_view::metrics::CellMetrics::default()
        )
    );
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(
        heimdall_ui::shell::SettingsTab::Terminal,
    ));
    {
        let mut ui = simulator(&shell);
        ui.find("Font family")
            .expect("beside the size, as the C# label");
        assert!(
            ui.find(
                "Source Code Pro is not installed on this computer: the terminal uses Source                  Code Pro."
            )
            .is_err(),
            "the embedded font is always there"
        );
    }
    // A family no computer has, as a settings file written by hand may name: kept as named,
    // drawn in the embedded font, and said so.
    let _ = shell.update(Message::App(AppMessage::Settings(
        heimdall_app::SettingsMessage::TerminalFontFamily(" Not A Font ".to_owned()),
    )));
    assert_eq!(shell.app().settings().terminal_font_family, "Not A Font");
    assert_eq!(shell.terminal_font(), TerminalFont::EMBEDDED);
    assert_eq!(
        drawn(&shell).0,
        FONT_FAMILY,
        "never a proportional fallback"
    );
    let mut ui = simulator(&shell);
    ui.find("Not A Font is not installed on this computer: the terminal uses Source Code Pro.")
        .expect("the fallback said");
    drop(ui);
    let _ = shell.update(Message::App(AppMessage::Settings(
        heimdall_app::SettingsMessage::TerminalFontFamily(String::new()),
    )));
    assert_eq!(shell.app().settings().terminal_font_family, FONT_FAMILY);
}

#[test]
fn a_terminal_lays_its_grid_out_in_the_cells_of_its_font() {
    use heimdall_ui::terminal_view::font::{FaceMetrics, TerminalFont};
    use iced::keyboard;

    let dir = tempfile::tempdir().expect("dir");
    let (shell, tab, _) = connected_shell(dir.path());
    let grid = |font: TerminalFont| {
        let shown = shell.app().tab(tab).expect("tab");
        let view = shell.terminal_view(shown, true).font(font).font_size(20.0);
        let mut ui = common::simulator(
            Settings {
                fonts: FONTS.iter().map(|face| (*face).into()).collect(),
                ..Settings::default()
            },
            Size::new(800.0, 440.0),
            view,
        );
        // Any event: the terminal reports its size on the first it gets.
        let _ = ui.simulate([iced::Event::Keyboard(keyboard::Event::ModifiersChanged(
            keyboard::Modifiers::empty(),
        ))]);
        ui.into_messages()
            .find_map(|message| match message {
                Message::App(AppMessage::Resize { grid, cell, .. }) => {
                    Some((grid.cols, grid.rows, cell.width, cell.height))
                }
                _ => None,
            })
            .expect("a size reported")
    };
    let narrow = TerminalFont {
        family: "Narrow Test Face",
        metrics: FaceMetrics {
            advance: 0.5,
            line: 1.1,
        },
    };
    // 800 by 440 in cells of 10 by 22, and of 12 by 26 (20 x 1.257, rounded up).
    assert_eq!(grid(narrow), (80, 20, 10, 22));
    assert_eq!(grid(TerminalFont::EMBEDDED), (66, 16, 12, 26));
}

/// A field of the session card, the rule said under a value out of its range, and values
/// typed with whether they are refused.
type SessionCase<'a> = (
    heimdall_ui::shell::SessionField,
    &'a str,
    &'a [(&'a str, bool)],
);

#[test]
fn the_session_numbers_are_typed_in_the_ssh_settings_each_within_its_csharp_range() {
    use heimdall_ui::shell::SessionField;

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, _, _) = connected_shell(dir.path());
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(heimdall_ui::shell::SettingsTab::Ssh));
    {
        let mut ui = simulator(&shell);
        for label in [
            "SSH keep-alive interval",
            "TMOUT reset interval (0 = off)",
            "Anti-idle interval (0 = off)",
        ] {
            ui.find(label).expect(label);
        }
    }
    let cases: [SessionCase<'_>; 3] = [
        (
            SessionField::KeepAlive,
            "SSH keep-alive interval must be between 5 and 600 seconds.",
            &[("4", true), ("601", true), ("later", true), (" 45 ", false)],
        ),
        (
            SessionField::TmoutReset,
            "SSH TMOUT reset interval must be between 0 and 3600 seconds.",
            &[("3601", true), ("-1", true), (" 0 ", false)],
        ),
        (
            SessionField::AntiIdle,
            "Anti-idle interval must be 0, or between 10 and 3600 seconds.",
            &[("9", true), ("soon", true), ("3601", true), (" 0 ", false)],
        ),
    ];
    for (field, refusal, typed) in cases {
        for (typed, refused) in typed {
            let _ = shell.update(Message::SessionFieldEdited(field, (*typed).to_owned()));
            let _ = shell.update(Message::SessionFieldApply(field));
            let mut ui = simulator(&shell);
            assert_eq!(ui.find(refusal).is_ok(), *refused, "{field:?} {typed}");
        }
    }
    let settings = shell.app().settings();
    assert_eq!(
        (
            settings.ssh_keep_alive_interval,
            settings.ssh_tmout_reset_interval,
            settings.anti_idle_interval
        ),
        (45, 0, 0)
    );
}

#[test]
fn the_reachability_check_is_set_in_the_general_settings_within_the_csharp_ranges() {
    use heimdall_ui::shell::SessionField;

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, _, _) = connected_shell(dir.path());
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(
        heimdall_ui::shell::SettingsTab::General,
    ));
    {
        let mut ui = simulator(&shell);
        for label in [
            "Session Health Monitor",
            "Enable background reachability probes",
            "Check interval",
            "Probe timeout",
            "Max concurrent probes",
        ] {
            ui.find(label).expect(label);
        }
    }
    let cases: [SessionCase<'_>; 3] = [
        (
            SessionField::ReachabilityInterval,
            "Check interval must be between 15 and 3600 seconds.",
            &[("14", true), ("3601", true), (" 120 ", false)],
        ),
        (
            SessionField::ReachabilityTimeout,
            "Probe timeout must be between 250 and 30000 ms.",
            &[("249", true), ("30001", true), ("750", false)],
        ),
        (
            SessionField::ReachabilityProbes,
            "Max concurrent probes must be between 1 and 50.",
            &[("0", true), ("51", true), ("5", false)],
        ),
    ];
    for (field, refusal, typed) in cases {
        for (typed, refused) in typed {
            let _ = shell.update(Message::SessionFieldEdited(field, (*typed).to_owned()));
            let _ = shell.update(Message::SessionFieldApply(field));
            let mut ui = simulator(&shell);
            assert_eq!(ui.find(refusal).is_ok(), *refused, "{field:?} {typed}");
        }
    }
    let reachability = shell.app().settings().reachability;
    assert_eq!(
        (
            reachability.interval,
            reachability.timeout,
            reachability.probes
        ),
        (120, 750, 5)
    );
}

#[test]
fn the_tab_shows_the_post_connect_count_and_a_click_on_it_stops_the_steps() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = connected_shell(dir.path());
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::PostConnect(heimdall_app::PostConnectProgress {
            step: 1,
            total: 2,
            command: "sudo -i".to_owned(),
            status: heimdall_app::StepStatus::Running,
            stop: tokio_util::sync::CancellationToken::new(),
        }),
    }));
    let mut ui = simulator(&shell);
    ui.click("1/2").expect("the count");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::StopPostConnect(stopped)) if stopped == tab
    )));
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::PostConnectDone,
    }));
    assert!(simulator(&shell).find("1/2").is_err(), "gone once over");
}

#[test]
fn imported_post_connect_steps_are_shown_whole_and_enter_never_runs_them() {
    use heimdall_core::post_connect::{PostConnect, PostConnectStep};

    let dir = tempfile::tempdir().expect("dir");
    let mut imported = profile("d", None);
    imported.post_connect = PostConnect {
        steps: vec![
            PostConnectStep::new("sudo -i"),
            PostConnectStep::new("cd /srv"),
        ],
        approved: None,
    };
    // In the file before the application reads it, as an import leaves it.
    let mut store = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    store.merge([imported]);
    store.save().expect("save");
    let mut core = app(dir.path());
    let _ = core.update(AppMessage::OpenProfile(ProfileId::new("d")));
    let mut shell = Shell::with_app(core);
    snapshot(&shell, "post-connect-question.png");
    {
        let mut ui = simulator(&shell);
        for label in [
            "Run post-connect commands?",
            "sudo -i\ncd /srv",
            "Run and remember",
            "Connect without them",
        ] {
            ui.find(label).expect(label);
        }
        ui.click("Connect without them").expect("no");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::SkipPostConnect)))
        );
    }
    // A key pressed as the dialog appears is not agreement.
    let _ = shell.update(Message::DialogKey { confirm: true });
    assert!(shell.app().dialog.is_some());
    assert!(shell.app().tabs.is_empty());
}

#[test]
fn the_close_button_inside_a_tab_closes_it_without_selecting_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, _) = open(&mut core, "a");
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    // A circled cross, as the C# tab's: found by its identifier.
    ui.click(tab_close_id(tab)).expect("the tab's close button");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::RequestCloseTab(closed)) if *closed == tab
        )),
        "{messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, Message::App(AppMessage::SelectTab(_)))),
        "the press is the close button's alone: {messages:?}"
    );
}

#[test]
fn a_profile_found_by_a_search_says_its_folder_and_host_under_its_name() {
    let dir = tempfile::tempdir().expect("dir");
    let core = app(dir.path());
    let mut shell = Shell::with_app(core);
    {
        let mut ui = simulator(&shell);
        assert!(
            ui.find("Production  a.lab").is_err(),
            "not while the tree is not searched"
        );
    }
    let _ = shell.update(Message::Search("server a".to_owned()));
    {
        let mut ui = simulator(&shell);
        ui.find("Production  a.lab")
            .expect("its folder and host, as the C# tree");
    }
    let _ = shell.update(Message::Search("server c".to_owned()));
    let mut ui = simulator(&shell);
    ui.find("c.lab").expect("in no folder: its host alone");
}

#[test]
fn the_more_menu_exports_the_sessions_as_the_csharp_one() {
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::More));
    let mut ui = simulator(&shell);
    ui.click("Export Sessions").expect("the entry");
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::MenuChoice(AppMessage::ExportSessions)))
    );
}

#[test]
fn the_export_says_how_many_sessions_and_that_no_credential_went() {
    use heimdall_app::ExportOutcome;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    for (count, said) in [
        (1, "1 session exported successfully."),
        (3, "3 sessions exported successfully."),
    ] {
        let _ = shell.update(Message::App(AppMessage::ExportFinished(
            ExportOutcome::Saved(count),
        )));
        let mut ui = simulator(&shell);
        ui.find(said).expect(said);
        ui.find("Credentials were not included in the export file.")
            .expect("as the C# says");
        drop(ui);
        let _ = shell.update(Message::App(AppMessage::DismissDialog));
    }
    let _ = shell.update(Message::App(AppMessage::ExportFinished(
        ExportOutcome::Failed("disk full".to_owned()),
    )));
    let mut ui = simulator(&shell);
    ui.find("Export failed: disk full").expect("the reason");
}

#[test]
fn the_filter_menu_ticks_a_protocol_and_stays_open_as_the_csharp_one() {
    use heimdall_app::{FilterMessage, ProfileKind};
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Filter));
    let mut ui = simulator(&shell);
    for label in [
        "Protocols",
        "Connected",
        "Via gateway",
        "Show gateway badge",
    ] {
        ui.find(label).expect(label);
    }
    ui.click("RDP").expect("the protocol");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::Filter(FilterMessage::Protocol(
                ProfileKind::Rdp
            )))
        )),
        "{messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, Message::MenuChoice(_) | Message::CloseTreeMenu)),
        "the menu stays open for the next choice: {messages:?}"
    );
}

#[test]
fn nothing_passing_the_filters_offers_to_reset_them_and_the_search() {
    use heimdall_app::{FilterMessage, ProfileKind};

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::Search("server".to_owned()));
    let _ = shell.update(Message::App(AppMessage::Filter(FilterMessage::Protocol(
        ProfileKind::Rdp,
    ))));
    {
        let mut ui = simulator(&shell);
        ui.find("No sessions match your search and filters.")
            .expect("as the C# says it");
        ui.click("Reset all filters").expect("the way back");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::ResetTreeFilters))
        );
    }
    let _ = shell.update(Message::ResetTreeFilters);
    assert!(!shell.app().tree_filter().is_active());
    let mut ui = simulator(&shell);
    ui.find("server a").expect("every profile back");
    assert!(
        ui.find("Production  a.lab").is_err(),
        "the search emptied too"
    );
}

#[test]
fn the_more_menu_imports_an_openssh_config_as_the_csharp_one() {
    use heimdall_app::SessionsMessage;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::More));
    let mut ui = simulator(&shell);
    ui.click("Import OpenSSH config...").expect("the entry");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::MenuChoice(AppMessage::Sessions(SessionsMessage::Start))
    )));
}

#[test]
fn the_openssh_preview_lists_the_servers_what_was_left_out_and_imports() {
    use heimdall_app::SessionsMessage;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let text = "Host web\n    HostName web.lab\n    ProxyJump alice@edge.lab:2200\nHost \"server a\" spare\n    HostName other.lab\n    Frobnicate yes\n";
    let _ = shell.update(Message::App(AppMessage::Sessions(SessionsMessage::Read(
        Ok(text.to_owned()),
    ))));
    {
        let mut ui = simulator(&shell);
        for said in [
            "Import OpenSSH config",
            "3 candidates - 2 new, 1 duplicate",
            "ProxyJump entries are imported as SSH gateway chains.",
            "web.lab",
            "alice@edge.lab:2200",
            "New",
            "Duplicate",
            "Diagnostics (1)",
            "Line 6: Unknown directive ignored: Frobnicate",
        ] {
            ui.find(said).expect(said);
        }
        ui.click("Import").expect("the button");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog)))
        );
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    let mut ui = simulator(&shell);
    ui.find("2 imported, 0 skipped (duplicates), 0 warnings")
        .expect("as the C# says it");
    ui.find("1 SSH gateway created for the ProxyJump chains.")
        .expect("the chain");
}

#[tokio::test]
async fn an_unreadable_file_is_said_with_its_path() {
    let dir = tempfile::tempdir().expect("dir");
    let missing = dir.path().join("config");
    let error = heimdall_ui::sessions_view::read_file(&missing)
        .await
        .expect_err("no such file");
    assert!(error.starts_with(&missing.display().to_string()), "{error}");
    std::fs::write(&missing, "Host a\n").expect("write");
    assert_eq!(
        heimdall_ui::sessions_view::read_file(&missing).await,
        Ok("Host a\n".to_owned())
    );
}

#[test]
fn nothing_chosen_in_the_openssh_preview_leaves_import_disabled() {
    use heimdall_app::SessionsMessage;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::App(AppMessage::Sessions(SessionsMessage::Read(
        Ok("Host web\n".to_owned()),
    ))));
    let _ = shell.update(Message::App(AppMessage::Sessions(
        SessionsMessage::ChooseAll(false),
    )));
    let mut ui = simulator(&shell);
    ui.click("Import").expect("the button");
    assert!(
        !ui.into_messages()
            .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog))),
        "nothing to import"
    );
}

#[test]
fn the_more_menu_imports_rdp_files_as_the_csharp_one() {
    use heimdall_app::RdpMessage;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::More));
    let mut ui = simulator(&shell);
    ui.click("Import RDP files...").expect("the entry");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::MenuChoice(AppMessage::Rdp(RdpMessage::Start))
    )));
}

#[test]
fn the_rdp_preview_says_each_files_state_and_imports() {
    use heimdall_app::RdpMessage;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::App(AppMessage::Rdp(RdpMessage::Read {
        files: vec![
            (
                "server a.rdp".into(),
                Ok("full address:s:dc.lab:3390\npassword 51:b:01\ncompression:i:1\n".to_owned()),
            ),
            (
                "gw.rdp".into(),
                Ok("full address:s:x\ngatewayhostname:s:g\ngatewayusagemethod:i:1\n".to_owned()),
            ),
            ("gone.rdp".into(), Err("gone.rdp: missing".to_owned())),
        ],
        names: heimdall_ui::rdp_view::names(),
    })));
    {
        let mut ui = simulator(&shell);
        for said in [
            "Import .rdp files",
            "1 selected / 2 files, 1 conflict, 1 password warning.",
            "1 file could not be read.",
            "dc.lab:3390",
            "Conflict with server a, Password not imported, Partial mapping",
            "Goes through a Remote Desktop Gateway, not supported yet",
            "Apply to all conflicts:",
            "Auto-rename",
        ] {
            ui.find(said).expect(said);
        }
        ui.click("Import selected").expect("the button");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog)))
        );
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    let mut ui = simulator(&shell);
    ui.find("1 imported, 0 replaced, 1 auto-renamed, 0 skipped, 1 password ignored.")
        .expect("as the C# counts it");
    drop(ui);
    let _ = shell.update(Message::App(AppMessage::DismissDialog));
    let mut ui = simulator(&shell);
    ui.find("server a (Imported 2)")
        .expect("renamed in the tree");
}

#[tokio::test]
async fn rdp_files_are_read_or_said_unreadable_with_their_path() {
    let dir = tempfile::tempdir().expect("dir");
    let here = dir.path().join("a.rdp");
    std::fs::write(&here, "full address:s:a\n").expect("write");
    let missing = dir.path().join("b.rdp");
    let read = heimdall_ui::rdp_view::read_all(vec![here.clone(), missing.clone()]).await;
    assert_eq!(read[0], (here, Ok("full address:s:a\n".to_owned())));
    assert!(matches!(&read[1].1, Err(why) if why.starts_with(&missing.display().to_string())));
}

#[test]
fn the_more_menu_imports_putty_sessions_as_the_csharp_one() {
    use heimdall_app::SessionsMessage;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::More));
    let mut ui = simulator(&shell);
    ui.click("Import PuTTY sessions...").expect("the entry");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::MenuChoice(AppMessage::Sessions(SessionsMessage::Putty))
    )));
}

#[test]
fn the_putty_preview_names_invalid_sessions_and_says_what_is_not_imported() {
    use heimdall_app::SessionsMessage;
    use heimdall_core::import::putty::{RawSession, Value};

    let text = |value: &str| Value::Text(value.to_owned());
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let sessions = vec![
        RawSession::new(
            "db".to_owned(),
            [
                ("Protocol".to_owned(), text("ssh")),
                ("HostName".to_owned(), text("db.lab")),
                ("PortForwardings".to_owned(), text("L1=a:1,L2=b:2")),
            ],
        ),
        RawSession::new("empty".to_owned(), [("Protocol".to_owned(), text("ssh"))]),
    ];
    let _ = shell.update(Message::App(AppMessage::Sessions(
        SessionsMessage::PuttyRead(Ok(sessions)),
    )));
    {
        let mut ui = simulator(&shell);
        for said in [
            "Import PuTTY sessions",
            "2 candidates - 1 new, 0 duplicates, 1 invalid",
            "(no host)",
            "Invalid",
            "Session \"db\" defines 2 tunnels captured but not mapped",
            "Session \"empty\" has no host name and will be marked invalid",
        ] {
            ui.find(said).expect(said);
        }
        assert!(
            ui.find("ProxyJump entries are imported as SSH gateway chains.")
                .is_err(),
            "an OpenSSH hint"
        );
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    let mut ui = simulator(&shell);
    ui.find("1 imported, 0 skipped (duplicates), 0 invalid, 0 warnings")
        .expect("as the C# counts it");
}

#[test]
fn the_more_menu_imports_trusted_ssh_hosts_as_the_csharp_one() {
    use heimdall_app::{HostKeysMessage, SettingsMessage, TrustedKeysMessage};
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::More));
    let mut ui = simulator(&shell);
    ui.click("Import trusted SSH hosts...").expect("the entry");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::MenuChoice(AppMessage::Settings(SettingsMessage::TrustedKeys(
            TrustedKeysMessage::Import(HostKeysMessage::Start)
        )))
    )));
}

#[test]
fn the_known_hosts_preview_says_each_key_and_what_was_left_out_then_imports() {
    use heimdall_app::HostKeysMessage;

    const ED25519: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEdv/0kqpfKUkuXCpQIlyU34zlRbf2MM2wBP+uTTnDTR";
    let dir = tempfile::tempdir().expect("dir");
    let key = heimdall_ssh::PublicKey::from_openssh(ED25519).expect("key");
    heimdall_ssh::KnownHosts::new(dir.path().join("known_hosts"))
        .learn("web.lab", 22, &key)
        .expect("learn");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(heimdall_ui::hostkeys_view::app(HostKeysMessage::Read(Ok(
        format!("web.lab {ED25519}\n[new.lab]:2222 {ED25519}\n|1|c2FsdA==|aGFzaA== {ED25519}\nshort-line\n"),
    ))));
    {
        let mut ui = simulator(&shell);
        for said in [
            "Import trusted SSH hosts",
            "2 entries: 1 new, 1 already trusted, 0 conflict",
            "web.lab:22",
            "new.lab:2222",
            "Already trusted",
            "Same fingerprint already trusted",
            "Hashed known_hosts entry is not supported (line 3).",
            "Malformed line 4: 1 field instead of 3",
            heimdall_ssh::fingerprint(&key).as_str(),
        ] {
            ui.find(said).expect(said);
        }
        ui.click("Import").expect("the button");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog)))
        );
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    let mut ui = simulator(&shell);
    ui.find("1 imported, 1 skipped (already trusted), 0 skipped (conflict), 1 warning")
        .expect("as the C# counts it");
}

#[tokio::test]
async fn a_known_hosts_file_larger_than_the_csharp_limit_is_refused_unread() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("known_hosts");
    let file = std::fs::File::create(&path).expect("create");
    file.set_len(heimdall_ssh::known_hosts_import::MAX_FILE_BYTES + 1)
        .expect("grow");
    let read = heimdall_ui::hostkeys_view::read_file(&path).await;
    assert_eq!(
        read,
        Err(format!(
            "The file is too large to import ({} bytes).",
            heimdall_ssh::known_hosts_import::MAX_FILE_BYTES + 1
        ))
    );
    file.set_len(heimdall_ssh::known_hosts_import::MAX_FILE_BYTES)
        .expect("shrink");
    assert!(
        heimdall_ui::hostkeys_view::read_file(&path).await.is_ok(),
        "the limit itself is read"
    );
}

#[test]
fn a_known_hosts_conflict_says_whether_the_file_or_the_trusted_key_it_contradicts() {
    use heimdall_app::HostKeysMessage;

    let fixture = |name: &str| {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../heimdall-ssh/tests/fixtures/hostkeys")
                .join(format!("{name}.pub")),
        )
        .expect("fixture");
        text.split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (ed, other) = (fixture("host-ed25519"), fixture("host-ed25519-other"));
    let dir = tempfile::tempdir().expect("dir");
    heimdall_ssh::KnownHosts::new(dir.path().join("known_hosts"))
        .learn(
            "changed.lab",
            22,
            &heimdall_ssh::PublicKey::from_openssh(&ed).expect("key"),
        )
        .expect("learn");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(heimdall_ui::hostkeys_view::app(HostKeysMessage::Read(Ok(
        format!("changed.lab {other}\ntwice.lab {ed}\ntwice.lab {other}\n"),
    ))));
    let mut ui = simulator(&shell);
    ui.find("Conflict with the fingerprint already trusted")
        .expect("the store");
    ui.find("Multiple different fingerprints for this host in the source file")
        .expect("the file");
}

#[test]
fn the_more_menu_imports_sessions_from_a_file_as_the_csharp_one() {
    use heimdall_app::SessionsMessage;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::More));
    let mut ui = simulator(&shell);
    ui.click("Import Sessions")
        .expect("the entry, without a C# folder");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::MenuChoice(AppMessage::Sessions(SessionsMessage::File))
    )));
}

#[test]
fn a_mobaxterm_file_is_asked_about_then_its_passwords_are_said() {
    use heimdall_app::{ImportFile, SessionsMessage};

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::App(AppMessage::Sessions(
        SessionsMessage::FileRead(Ok(ImportFile {
            name: "lab.mxtsessions".to_owned(),
            text: "[Bookmarks]\nweb= #109#0%web.lab%22%root\n[Passwords]\na=x\nb=y\n".to_owned(),
            settings: None,
        })),
    )));
    {
        let mut ui = simulator(&shell);
        ui.find(
            "Import 1 session from MobaXterm? Passwords cannot be imported and must be re-entered.",
        )
        .expect("the C# question");
        ui.click("Import").expect("the button");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog)))
        );
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    let mut ui = simulator(&shell);
    ui.find("Added: 1. Updated: 0. Unchanged: 0.")
        .expect("the counts");
    ui.find(
        "Detected 2 stored passwords in the MobaXterm file. MobaXterm encrypts them with a proprietary algorithm, so they were not imported - please re-enter credentials for the affected sessions.",
    )
    .expect("the C# notice");
}

#[test]
fn a_file_giving_nothing_says_so_and_why() {
    use heimdall_app::{ImportFile, SessionsMessage};

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::App(AppMessage::Sessions(
        SessionsMessage::FileRead(Ok(ImportFile {
            name: "confCons.xml".to_owned(),
            text: r#"<Connections FullFileEncryption="true"/>"#.to_owned(),
            settings: None,
        })),
    )));
    let mut ui = simulator(&shell);
    for said in [
        "Import Sessions",
        "No sessions found in the selected file.",
        "The file is fully encrypted. Decrypt it in mRemoteNG first (File > Save As with no encryption).",
    ] {
        ui.find(said).expect(said);
    }
}

#[tokio::test]
async fn a_servers_json_is_read_with_its_settings_and_a_large_file_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let servers = dir.path().join("servers.json");
    std::fs::write(&servers, "[]").expect("write");
    std::fs::write(dir.path().join("settings.json"), "{}").expect("write");
    let read = heimdall_ui::file_import_view::read(servers)
        .await
        .expect("read");
    assert_eq!(
        (read.name.as_str(), read.settings.as_deref()),
        ("servers.json", Some("{}"))
    );
    let other = dir.path().join("export.json");
    std::fs::write(&other, "[]").expect("write");
    let read = heimdall_ui::file_import_view::read(other)
        .await
        .expect("read");
    assert_eq!(
        read.settings, None,
        "only a servers.json takes its settings"
    );

    let large = dir.path().join("big.xml");
    let file = std::fs::File::create(&large).expect("create");
    file.set_len(heimdall_ui::file_import_view::MAX_FILE_BYTES + 1)
        .expect("grow");
    assert!(
        heimdall_ui::file_import_view::read(large.clone())
            .await
            .is_err_and(|why| why.starts_with("The file is too large to import"))
    );
    file.set_len(heimdall_ui::file_import_view::MAX_FILE_BYTES)
        .expect("shrink");
    assert!(
        !heimdall_ui::file_import_view::read(large)
            .await
            .is_err_and(|why| why.starts_with("The file is too large")),
        "the limit itself is read"
    );
    assert!(heimdall_ui::file_import_view::is_rdp(Path::new("a/B.RDP")));
    assert!(!heimdall_ui::file_import_view::is_rdp(Path::new("a/b.rdg")));
}

#[test]
fn the_settings_page_turns_ssh_auto_reconnect_on_with_its_attempts_as_the_csharp_card() {
    use heimdall_app::SettingsMessage;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(heimdall_ui::shell::SettingsTab::Ssh));
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    // Tall enough for the whole page.
    let mut ui = common::simulator(settings, Size::new(1100.0, 2400.0), shell.view());
    for said in [
        "SSH auto-reconnect",
        "Automatically retry an SSH session that disconnects unexpectedly. Disabled by default.",
        "Max attempts before falling back to manual reconnect",
    ] {
        ui.find(said).expect(said);
    }
    ui.click("Enable bounded auto-reconnect").expect("its box");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::Settings(SettingsMessage::SshAutoReconnect(
            true
        )))
    )));
}

#[test]
fn the_settings_page_is_in_the_csharp_tabs_and_keeps_the_one_chosen() {
    use heimdall_ui::shell::SettingsTab;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::ShowSettings);
    {
        let mut ui = simulator(&shell);
        for tab in ["General", "Terminal", "SSH", "RDP", "Security"] {
            ui.find(tab).expect(tab);
        }
        ui.find("Language").expect("General first");
        assert!(ui.find("Master password").is_err(), "the Security tab's");
        ui.click("Security").expect("its tab");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::SettingsTab(SettingsTab::Security)))
        );
    }
    let _ = shell.update(Message::SettingsTab(SettingsTab::Security));
    let _ = shell.update(Message::ShowSettings);
    let mut ui = simulator(&shell);
    ui.find("Master password").expect("the tab chosen, kept");
    assert!(ui.find("Language").is_err());
}

#[test]
fn the_rdp_tab_sets_the_global_rdp_defaults_as_the_csharp_settings() {
    use heimdall_app::SettingsMessage;
    use heimdall_core::profile::RdpDefaults;
    use heimdall_ui::shell::SettingsTab;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(SettingsTab::Rdp));
    let mut ui = simulator(&shell);
    for said in [
        "RDP Defaults",
        "The options of every RDP server that uses the global defaults.",
        "Redirect clipboard",
        "Enable Network Level Authentication",
        "Allow dynamic resolution updates",
        "Trusted RDP certificates",
    ] {
        ui.find(said).expect(said);
    }
    ui.click("Redirect drives").expect("its box");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::Settings(SettingsMessage::RdpDefaults(defaults)))
            if defaults == RdpDefaults { redirect_drives: true, ..RdpDefaults::default() }
    )));
}

#[test]
fn ctrl_comma_shows_the_settings_with_or_without_a_tab_but_not_over_a_dialog() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::Shortcut(WindowShortcut::Settings));
    assert!(shell.settings_shown(), "no tab open");

    let mut core = app(dir.path());
    let _ = open(&mut core, "a");
    core.dialog = Some(heimdall_app::Dialog::ConfirmBroadcast);
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::Shortcut(WindowShortcut::Settings));
    assert!(!shell.settings_shown(), "the dialog has the keyboard");
    let _ = shell.update(Message::App(AppMessage::DismissDialog));
    let _ = shell.update(Message::Shortcut(WindowShortcut::Settings));
    assert!(shell.settings_shown(), "over the tab shown");
}

#[test]
fn enter_in_the_search_opens_the_one_profile_found_and_never_guesses_among_several() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::Search("server".to_owned()));
    let _ = shell.update(Message::SearchSubmit);
    assert!(shell.app().tabs.is_empty(), "three found: none opened");
    let _ = shell.update(Message::Search("b.lab".to_owned()));
    let _ = shell.update(Message::SearchSubmit);
    assert_eq!(shell.app().tabs.len(), 1, "the only one found is opened");
    assert_eq!(shell.app().tabs[0].profile.name(), "server b");
    let _ = shell.update(Message::Search(String::new()));
    let _ = shell.update(Message::SearchSubmit);
    assert_eq!(shell.app().tabs.len(), 1, "an empty search opens nothing");
}

#[test]
fn escape_and_down_in_the_search_only_while_it_has_the_keyboard() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::Search("production".to_owned()));
    {
        // Nobody has the keyboard: the keys are the window's.
        let mut ui = simulator(&shell);
        let _ = ui.tap_key(Named::ArrowDown);
        let _ = ui.tap_key(Named::Escape);
        assert!(
            !ui.into_messages()
                .any(|message| matches!(message, Message::SearchDown | Message::Search(_))),
            "not the search's"
        );
    }
    {
        let mut ui = simulator(&shell);
        ui.click("production").expect("the search field");
        let _ = ui.tap_key(Named::ArrowDown);
        let _ = ui.tap_key(Named::Escape);
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, Message::SearchDown)),
            "{messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, Message::Search(text) if text.is_empty())),
            "{messages:?}"
        );
    }
    let _ = shell.update(Message::SearchDown);
    assert_eq!(
        shell.app().selected_profile,
        Some(ProfileId::new("a")),
        "the first found, in the order shown"
    );
    let _ = shell.update(Message::Search(String::new()));
    let mut ui = simulator(&shell);
    ui.click("Search")
        .expect("the empty search field, by its placeholder");
    let _ = ui.tap_key(Named::Escape);
    assert!(
        !ui.into_messages()
            .any(|message| matches!(message, Message::Search(_))),
        "an empty search leaves Escape to the rest of the window"
    );
}

#[test]
fn one_post_connect_command_is_said_in_the_singular() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    core.dialog = Some(Dialog::ConfirmPostConnect(Box::new(
        PostConnectConfirmation {
            profile: profile("a", None),
            name: "server a".to_owned(),
            commands: vec!["uptime".to_owned()],
        },
    )));
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find(
        "\"server a\" was imported and will automatically run 1 command in this session. Only \
         continue if you trust this profile. Run it and remember this choice?",
    )
    .expect("the singular, chosen by the number");
}

#[test]
fn a_dangerous_paste_names_the_command_before_it_reaches_the_shell() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _) = connected_shell(dir.path());
    let _ = shell.update(Message::App(AppMessage::ClipboardText {
        tab,
        text: Some("dd if=/dev/zero of=/dev/sda".to_owned()),
    }));
    let mut ui = simulator(&shell);
    ui.find("Paste a dangerous command?").expect("title");
    ui.find(
        "The text holds dd if=, a command that can destroy data or stop the machine. Check it \
         before it reaches the shell.",
    )
    .expect("the command named");
    ui.find("dd if=/dev/zero of=/dev/sda")
        .expect("the text itself");
    assert!(ui.find(TRUNCATED).is_err(), "all of it shown");
    ui.click("Paste anyway").expect("the way on");
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog)))
    );
}

/// What the C# says under a paste preview cut short.
const TRUNCATED: &str =
    "Preview is truncated. The full clipboard content will be pasted if you continue.";

#[test]
fn a_paste_of_several_lines_shows_them_written_out_and_says_when_some_are_out_of_sight() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _) = connected_shell(dir.path());
    let _ = shell.update(Message::App(AppMessage::ClipboardText {
        tab,
        text: Some("ls -l\r\nprintf '\u{1b}[2J'\techo\n".to_owned()),
    }));
    {
        let mut ui = simulator(&shell);
        ui.find("Paste several lines?").expect("title");
        ui.find(
            r"ls -l
printf '\u{001B}[2J'\u{0009}echo",
        )
        .expect("its lines, the escape and the tab written out");
        assert!(ui.find(TRUNCATED).is_err(), "all of it shown");
        ui.click("Cancel").expect("cancel");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::DismissDialog)))
        );
    }
    {
        let mut ui = simulator(&shell);
        ui.click("Paste").expect("paste");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog)))
        );
    }
    let _ = shell.update(Message::App(AppMessage::DismissDialog));

    let long: Vec<String> = (0..=heimdall_app::PastePreview::MAX_LINES)
        .map(|n| format!("echo {n}"))
        .collect();
    let _ = shell.update(Message::App(AppMessage::ClipboardText {
        tab,
        text: Some(long.join("\n")),
    }));
    let mut ui = simulator(&shell);
    ui.find(
        long[..heimdall_app::PastePreview::MAX_LINES]
            .join("\n")
            .as_str(),
    )
    .expect("the first lines");
    ui.find(TRUNCATED).expect("said cut short");
}

#[test]
fn an_import_says_which_settings_it_left_out_of_which_profile() {
    use heimdall_core::import::csharp::Dropped;
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    core.dialog = Some(Dialog::ImportDone(heimdall_app::ImportSummary {
        merged: heimdall_core::store::MergeReport {
            added: 1,
            updated: 0,
            unchanged: 0,
        },
        skipped: Vec::new(),
        warnings: Vec::new(),
        stored_credentials: None,
        host_keys: None,
        gateways: heimdall_core::import::gateways::Reconciliation::default(),
        dropped: vec![(
            "desk".to_owned(),
            vec![Dropped::RdpPrinters, Dropped::RdpSmartCards],
        )],
    }));
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find("Imported with settings the built-in client does not use yet:")
        .expect("said");
    ui.find("desk: printers, smart cards")
        .expect("the profile and what it came without");
    assert!(
        ui.find("SSH gateways: 0 created, 0 merged.").is_err(),
        "no gateway line for a file without gateways"
    );
}

#[test]
fn an_import_counts_the_gateways_it_created_and_merged_as_the_csharp_summary() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    core.dialog = Some(Dialog::ImportDone(heimdall_app::ImportSummary {
        merged: heimdall_core::store::MergeReport {
            added: 4,
            updated: 0,
            unchanged: 0,
        },
        skipped: Vec::new(),
        warnings: Vec::new(),
        stored_credentials: None,
        host_keys: None,
        gateways: heimdall_core::import::gateways::Reconciliation {
            created: 2,
            merged: 1,
        },
        dropped: Vec::new(),
    }));
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find("SSH gateways: 2 created, 1 merged.")
        .expect("the gateway line");
}

/// What the dialog of a profile file unreadable at start says, never said of a save.
const UNREADABLE_BODY: &str = "Heimdall started with no profile; changes are saved beside the unreadable file, which is left untouched.";

#[test]
fn a_save_over_a_profile_file_changed_outside_says_so_and_not_that_it_was_unreadable() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    // Another instance on the same file records a profile of its own.
    let mut other = ProfileStore::open(dir.path().join("profiles.toml")).expect("opens");
    other
        .apply(|store| store.merge([profile("other", None)]))
        .expect("saves");
    core.update(AppMessage::ProfileMenu(
        heimdall_app::ProfileMenuMessage::Move {
            id: ProfileId::new("c"),
            to: Some("Production".to_owned()),
        },
    ));
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find("Profiles not saved").expect("titled");
    ui.find(
        "The profile file was changed by another program or another Heimdall since it was \
         read, so this change was not saved and the file was left as it is. Reopen Heimdall \
         to load the current file, then make the change again.",
    )
    .expect("what happened and what to do");
    assert!(ui.find(UNREADABLE_BODY).is_err());
}

#[test]
fn a_save_that_fails_says_why_and_not_that_the_file_was_unreadable() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    core.dialog = Some(Dialog::StoreError {
        detail: "disk full".to_owned(),
    });
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find("Save Error").expect("titled as the C#");
    ui.find("Failed to save: disk full").expect("the reason");
    assert!(ui.find(UNREADABLE_BODY).is_err());
}

#[test]
fn the_profile_form_saves_a_key_passphrase_and_then_says_it_is_saved() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    core.update(AppMessage::EditProfile(ProfileId::new("a")));
    core.update(AppMessage::ProfileField {
        field: heimdall_app::profile_draft::ProfileField::KeyPath,
        value: "/keys/id_ed25519".to_owned(),
    });
    let mut shell = Shell::with_app(core);
    {
        let mut ui = simulator(&shell);
        ui.find("Key passphrase").expect("a passphrase field");
        ui.find(
            "Used only to decrypt the selected SSH key. Leave blank if the key has no \
             passphrase or is unlocked by an SSH agent.",
        )
        .expect("the C# hint");
        assert!(ui.find("Passphrase saved").is_err(), "none yet");
    }
    let _ = shell.update(Message::ProfilePassphrase("unlock me".to_owned()));
    let _ = shell.update(Message::SaveProfileForm);
    let mut core = shell.into_app();
    assert!(core.dialog.is_none(), "{:?}", core.dialog);

    core.update(AppMessage::EditProfile(ProfileId::new("a")));
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find("Passphrase saved").expect("says so");
    ui.click("Clear").expect("a clear button");
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::App(AppMessage::ClearPassphrase)))
    );
}

#[test]
fn a_session_pressed_and_dragged_onto_a_folder_moves_into_it() {
    use iced::Point;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let group = |shell: &Shell| {
        shell
            .app()
            .profile_summary(&ProfileId::new("c"))
            .and_then(|profile| profile.group)
    };
    // A click is no drag: nothing moves.
    let _ = shell.update(Message::TreeClick(ProfileId::new("c")));
    let _ = shell.update(Message::TreeHover(heimdall_app::DropTarget::Folder(
        "Production".to_owned(),
    )));
    let _ = shell.update(Message::TreeDragEnd);
    assert_eq!(group(&shell), None);

    // Pressed, moved past the threshold, let go over the folder: in it.
    let _ = shell.update(Message::TreeClick(ProfileId::new("c")));
    let _ = shell.update(Message::TreeDragMoved(Point::new(40.0, 60.0)));
    let _ = shell.update(Message::TreeHover(heimdall_app::DropTarget::Folder(
        "Production".to_owned(),
    )));
    let _ = shell.update(Message::TreeDragEnd);
    assert_eq!(group(&shell).as_deref(), Some("Production"));
}

#[test]
fn home_end_select_all_and_letters_move_in_the_tree_and_ctrl_b_hides_it() {
    use heimdall_app::files::FilesKey;
    use heimdall_ui::shell::TreeShortcut;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let selected = |shell: &Shell| {
        shell
            .app()
            .selected_profile
            .clone()
            .map(|id| id.to_string())
    };
    // Not before the tree has the keyboard.
    let _ = shell.update(Message::TypeAhead("s".to_owned()));
    assert_eq!(selected(&shell), None);

    let _ = shell.update(Message::TreeClick(ProfileId::new("a")));
    let _ = shell.update(Message::FilesKey(FilesKey::Last));
    assert_eq!(selected(&shell).as_deref(), Some("c"));
    let _ = shell.update(Message::FilesKey(FilesKey::First));
    assert_eq!(
        shell.app().selected_folder.as_deref(),
        Some("Production"),
        "Home: the first row, a folder, as in the C# tree"
    );
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    assert_eq!(selected(&shell).as_deref(), Some("a"));

    // The same letter again goes on to the next name starting with it.
    let _ = shell.update(Message::TypeAhead("S".to_owned()));
    assert_eq!(selected(&shell).as_deref(), Some("b"));
    let _ = shell.update(Message::TypeAhead("s".to_owned()));
    assert_eq!(selected(&shell).as_deref(), Some("c"));
    let _ = shell.update(Message::TypeAhead("z".to_owned()));
    assert_eq!(
        selected(&shell).as_deref(),
        Some("c"),
        "none: left where it is"
    );

    let _ = shell.update(Message::FilesKey(FilesKey::SelectAll));
    assert_eq!(shell.app().selected_profiles().len(), 3, "every one shown");

    // The sidebar's own button: "Sessions" is the navigation's too.
    let title = |shell: &Shell| simulator(shell).find("Local shell").is_ok();
    assert!(title(&shell));
    let _ = shell.update(Message::TreeShortcut(TreeShortcut::ToggleSidebar));
    assert!(!title(&shell), "hidden");
    let _ = shell.update(Message::TreeShortcut(TreeShortcut::ToggleSidebar));
    assert!(title(&shell), "shown again");
}

#[test]
fn the_tree_keys_reach_folders_fold_them_and_extend_the_selection() {
    use heimdall_app::Dialog;
    use heimdall_app::files::{FilesKey, Side};

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let open = |shell: &Shell, path: &str| {
        shell.app().tree_rows("").into_iter().any(|row| {
            matches!(row, heimdall_app::TreeRow::Folder { path: found, open: true, .. }
                if found == path)
        })
    };
    let _ = shell.update(Message::TreeClick(ProfileId::new("a")));
    // Left from a profile: up to its folder.
    let _ = shell.update(Message::FilesKey(FilesKey::Focus(Side::Local)));
    assert_eq!(shell.app().selected_folder.as_deref(), Some("Production"));
    assert_eq!(shell.app().selected_profile, None);
    // Left again folds it; Right unfolds it, then goes down to its first row.
    let _ = shell.update(Message::FilesKey(FilesKey::Focus(Side::Local)));
    assert!(!open(&shell, "Production"), "folded");
    let _ = shell.update(Message::FilesKey(FilesKey::Focus(Side::Remote)));
    assert!(open(&shell, "Production"), "unfolded");
    assert_eq!(shell.app().selected_folder.as_deref(), Some("Production"));
    let _ = shell.update(Message::FilesKey(FilesKey::Focus(Side::Remote)));
    assert_eq!(
        shell.app().selected_profile.as_ref().map(ProfileId::as_str),
        Some("a")
    );
    // Enter on a folder folds it, and connects nothing.
    let _ = shell.update(Message::FilesKey(FilesKey::Focus(Side::Local)));
    let _ = shell.update(Message::DialogKey { confirm: true });
    assert!(!open(&shell, "Production"));
    assert!(shell.app().tabs.is_empty());
    let _ = shell.update(Message::DialogKey { confirm: true });
    // F2 and Delete on a folder are the folder's.
    let _ = shell.update(Message::FilesKey(FilesKey::Rename));
    assert!(
        matches!(shell.app().dialog, Some(Dialog::FolderName { .. })),
        "{:?}",
        shell.app().dialog
    );
    let _ = shell.update(Message::App(AppMessage::DismissDialog));
    let _ = shell.update(Message::FilesKey(FilesKey::Delete));
    assert!(shell.app().dialog.is_some(), "the folder's deletion asked");
    let _ = shell.update(Message::App(AppMessage::DismissDialog));

    // Shift+Down from a: a and b; Ctrl+Space takes b out again.
    let _ = shell.update(Message::TreeClick(ProfileId::new("a")));
    let _ = shell.update(Message::FilesKey(FilesKey::ExtendNext));
    assert_eq!(
        shell.app().selected_profiles(),
        [ProfileId::new("a"), ProfileId::new("b")]
    );
    let _ = shell.update(Message::FilesKey(FilesKey::ToggleMark));
    assert!(
        shell.app().selected_profiles().is_empty(),
        "b taken out: a alone again"
    );
}

#[test]
fn escape_no_widget_took_leaves_full_screen_once_nothing_else_is_open() {
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::ToggleFullscreen);
    {
        let mut ui = simulator(&shell);
        assert!(ui.find("Local shell").is_err(), "no tree in full screen");
        ui.click("Exit fullscreen")
            .expect("a way out for the mouse");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::ToggleFullscreen))
        );
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Profile(ProfileId::new(
        "a",
    ))));
    let _ = shell.update(Message::EscapeUntaken);
    assert!(
        simulator(&shell).find("Exit fullscreen").is_ok(),
        "the menu closes first"
    );
    let _ = shell.update(Message::EscapeUntaken);
    let mut ui = simulator(&shell);
    ui.find("Local shell").expect("the tree again");
    assert!(ui.find("Exit fullscreen").is_err());
}

#[test]
fn the_tree_folds_at_once_hides_and_shows_its_sidebar_and_quick_connect_is_a_button() {
    use heimdall_ui::shell::TreeShortcut;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::More));
    {
        let mut ui = simulator(&shell);
        ui.find("Expand all").expect("Expand all");
        ui.click("Collapse all").expect("Collapse all");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::MenuChoice(AppMessage::FoldAll(true))))
        );
    }
    let _ = shell.update(Message::App(AppMessage::FoldAll(true)));
    assert!(
        simulator(&shell).find("server a").is_err(),
        "every folder folded"
    );
    let _ = shell.update(Message::DialogKey { confirm: false });
    {
        let mut ui = simulator(&shell);
        ui.click("Quick connect").expect("the navigation's button");
        assert!(
            ui.into_messages().any(|message| matches!(
                message,
                Message::TreeShortcut(TreeShortcut::QuickConnect)
            ))
        );
    }
    {
        let mut ui = simulator(&shell);
        // Glyphs alone, as the C#'s: found by their identifier.
        ui.click(sidebar_toggle_id())
            .expect("the sidebar's hide button");
        assert!(
            ui.into_messages().any(|message| matches!(
                message,
                Message::TreeShortcut(TreeShortcut::ToggleSidebar)
            ))
        );
    }
    let _ = shell.update(Message::TreeShortcut(TreeShortcut::ToggleSidebar));
    let mut ui = simulator(&shell);
    assert!(ui.find("Local shell").is_err(), "hidden");
    ui.click(sidebar_toggle_id())
        .expect("a way to show it again");
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::TreeShortcut(TreeShortcut::ToggleSidebar)))
    );
}

#[test]
fn a_tab_pressed_and_dragged_onto_another_takes_its_place() {
    use iced::Point;

    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (a, _) = open(&mut core, "a");
    let (b, _) = open(&mut core, "b");
    let mut shell = Shell::with_app(core);
    let order = |shell: &Shell| {
        shell
            .app()
            .tabs
            .iter()
            .map(|tab| tab.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(order(&shell), [a, b]);

    // A click is no drag.
    let _ = shell.update(Message::TabHover(b));
    let _ = shell.update(Message::PointerPressed);
    let _ = shell.update(Message::TabHoverLeft(b));
    let _ = shell.update(Message::TabHover(a));
    let _ = shell.update(Message::TabDragEnd);
    assert_eq!(order(&shell), [a, b]);

    // Pressed on b, moved, let go over a: b first.
    let _ = shell.update(Message::TabHover(b));
    let _ = shell.update(Message::PointerPressed);
    let _ = shell.update(Message::TabDragMoved(Point::new(300.0, 300.0)));
    let _ = shell.update(Message::TabHoverLeft(b));
    let _ = shell.update(Message::TabHover(a));
    let _ = shell.update(Message::TabDragEnd);
    assert_eq!(order(&shell), [b, a]);
}

#[test]
fn alt_down_moves_the_session_and_the_undo_bar_puts_it_back() {
    use heimdall_app::files::FilesKey;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let order = |shell: &Shell| -> Vec<String> {
        shell
            .app()
            .tree_rows("")
            .into_iter()
            .filter_map(|row| match row {
                heimdall_app::TreeRow::Profile { profile, .. } => Some(profile.id.to_string()),
                heimdall_app::TreeRow::Folder { .. } => None,
            })
            .collect()
    };
    assert_eq!(order(&shell), ["a", "b", "c"]);
    {
        let mut ui = simulator(&shell);
        assert!(ui.find("Undo").is_err(), "nothing to undo");
    }
    let _ = shell.update(Message::TreeClick(ProfileId::new("a")));
    let _ = shell.update(Message::FilesKey(FilesKey::Lower));
    assert_eq!(order(&shell), ["b", "a", "c"], "within its folder");
    let mut ui = simulator(&shell);
    ui.find("Sessions reordered.").expect("the C# bar");
    ui.click("Undo").expect("Undo");
    let messages: Vec<Message> = ui.into_messages().collect();
    for message in messages {
        let _ = shell.update(message);
    }
    assert_eq!(order(&shell), ["a", "b", "c"]);
}

#[test]
fn a_drag_shows_the_no_folder_zone_which_takes_a_session_out_of_its_folder() {
    use iced::Point;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let zone = "Drop here to take it out of its folder";
    {
        let mut ui = simulator(&shell);
        assert!(ui.find(zone).is_err(), "no drag: no zone");
    }
    let _ = shell.update(Message::TreeClick(ProfileId::new("a")));
    let _ = shell.update(Message::TreeDragMoved(Point::new(40.0, 60.0)));
    {
        let mut ui = simulator(&shell);
        ui.find(zone).expect("the C# zone, while dragging");
    }
    let _ = shell.update(Message::TreeHover(heimdall_app::DropTarget::Folder(
        heimdall_app::NO_FOLDER.to_owned(),
    )));
    let _ = shell.update(Message::TreeDragEnd);
    assert_eq!(
        shell
            .app()
            .profile_summary(&ProfileId::new("a"))
            .and_then(|profile| profile.group),
        None,
        "out of Production"
    );
    let mut ui = simulator(&shell);
    assert!(ui.find(zone).is_err(), "gone with the drag");
}

#[test]
fn f2_renames_a_session_in_its_row_and_a_click_elsewhere_keeps_the_name() {
    use heimdall_app::ProfileMenuMessage;
    use heimdall_app::files::FilesKey;
    use heimdall_ui::shell::TreeShortcut;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let name = |shell: &Shell, id: &str| {
        shell
            .app()
            .profile_summary(&ProfileId::new(id))
            .expect("profile")
            .name
    };
    let _ = shell.update(Message::TreeClick(ProfileId::new("a")));
    let _ = shell.update(Message::FilesKey(FilesKey::Rename));
    {
        let mut ui = simulator(&shell);
        assert!(
            ui.find("Rename Session").is_err(),
            "typed in its row, no dialog over the window"
        );
        ui.find("server b")
            .expect("the rest of the tree still shown");
    }
    let _ = shell.update(Message::App(AppMessage::ProfileMenu(
        ProfileMenuMessage::NameEdited("Alpha".to_owned()),
    )));
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    assert_eq!(name(&shell, "a"), "Alpha", "Enter keeps it");
    assert!(shell.app().dialog.is_none());

    // Another row clicked while typing: the name is kept, as the C# editor losing focus.
    let _ = shell.update(Message::FilesKey(FilesKey::Rename));
    let _ = shell.update(Message::App(AppMessage::ProfileMenu(
        ProfileMenuMessage::NameEdited("Beta".to_owned()),
    )));
    let _ = shell.update(Message::TreeClick(ProfileId::new("b")));
    assert_eq!(name(&shell, "a"), "Beta");
    assert!(shell.app().dialog.is_none());

    // The tree hidden, the name is asked in the dialog.
    let _ = shell.update(Message::TreeShortcut(TreeShortcut::ToggleSidebar));
    let _ = shell.update(Message::App(AppMessage::ProfileMenu(
        ProfileMenuMessage::Rename(ProfileId::new("b")),
    )));
    let mut ui = simulator(&shell);
    ui.find("Rename Session").expect("the dialog");
}

#[test]
fn a_folder_renamed_in_its_row_says_why_a_name_is_refused_under_it() {
    use heimdall_app::FolderMessage;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::App(AppMessage::Folder(FolderMessage::Rename(
        "Production".to_owned(),
    ))));
    let _ = shell.update(Message::App(AppMessage::Folder(FolderMessage::NameEdited(
        "a/b".to_owned(),
    ))));
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    {
        let mut ui = simulator(&shell);
        assert!(
            ui.find("Rename Folder").is_err(),
            "no dialog over the window"
        );
        ui.find("A folder name cannot be empty or contain \"/\".")
            .expect("why, under the name");
    }
    let _ = shell.update(Message::App(AppMessage::Folder(FolderMessage::NameEdited(
        "Live".to_owned(),
    ))));
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    assert_eq!(
        shell
            .app()
            .profile_summary(&ProfileId::new("a"))
            .and_then(|profile| profile.group)
            .as_deref(),
        Some("Live")
    );
}

#[test]
fn a_search_or_a_filter_shows_as_a_chip_with_the_count_it_leaves() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    {
        let mut ui = simulator(&shell);
        assert!(ui.find("Reset all filters").is_err(), "nothing applied");
    }
    let _ = shell.update(Message::Search("a.lab".to_owned()));
    {
        let mut ui = simulator(&shell);
        ui.find("1 / 3 sessions").expect("the C# count");
        ui.find("Reset all filters").expect("the way back");
        // The search box says it too: the chip is clicked by its cross, the only one with
        // no tab open.
        ui.click("\u{2715}").expect("the search's chip");
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, Message::Search(text) if text.is_empty())),
            "{messages:?}"
        );
    }
    let _ = shell.update(Message::Search(String::new()));
    let _ = shell.update(Message::App(AppMessage::Filter(
        heimdall_app::FilterMessage::Connected,
    )));
    let mut ui = simulator(&shell);
    ui.find("0 / 3 sessions").expect("none connected");
    ui.click("Connected").expect("the filter's chip");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::Filter(heimdall_app::FilterMessage::Connected))
        )),
        "{messages:?}"
    );
}

#[test]
fn several_sessions_selected_get_the_csharp_bulk_bar() {
    use heimdall_app::SelectionMessage;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::App(AppMessage::Selection(
        SelectionMessage::Toggle(ProfileId::new("a")),
    )));
    {
        let mut ui = simulator(&shell);
        assert!(ui.find("More actions").is_err(), "one: no bar");
    }
    let _ = shell.update(Message::App(AppMessage::Selection(
        SelectionMessage::Toggle(ProfileId::new("b")),
    )));
    let mut ui = simulator(&shell);
    ui.find("2 sessions selected").expect("the count");
    ui.find("Connect selected (2)").expect("what Connect opens");
    ui.click("Move").expect("Move");
    ui.click("More actions").expect("the rest");
    let messages: Vec<Message> = ui.into_messages().collect();
    for menu in [TreeMenu::MoveSelection, TreeMenu::Selection] {
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, Message::OpenTreeMenu(open) if *open == menu)),
            "{menu:?} in {messages:?}"
        );
    }
}

#[test]
fn an_imported_profile_carries_its_origin_in_the_tree_and_in_its_form() {
    use heimdall_core::metadata::ProfileOrigin;

    let dir = tempfile::tempdir().expect("dir");
    {
        let shell = Shell::with_app(app(dir.path()));
        let mut ui = simulator(&shell);
        assert!(ui.find("PTY").is_err(), "made here: no badge");
    }
    let mut store = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    assert!(store.set_origin(&ProfileId::new("a"), ProfileOrigin::Putty));
    store.save().expect("save");
    let mut shell = Shell::with_app(app(dir.path()));
    {
        let mut ui = simulator(&shell);
        ui.find("PTY").expect("the C# badge code");
    }
    let _ = shell.update(Message::App(AppMessage::EditProfile(ProfileId::new("a"))));
    let mut ui = simulator(&shell);
    ui.find("Imported from PuTTY registry")
        .expect("the C# sentence under the form");
}

#[test]
fn the_computer_is_kept_awake_while_a_session_is_connected_unless_turned_off() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    let mut shell = Shell::with_app(core);
    let _ = shell.step(Message::Tick);
    assert!(!shell.keeps_awake(), "connecting: not yet");
    let _ = shell.step(Message::App(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    }));
    assert!(shell.keeps_awake(), "as the C# default");
    let _ = shell.step(Message::App(AppMessage::Settings(
        heimdall_app::SettingsMessage::PreventSleep(false),
    )));
    assert!(!shell.keeps_awake(), "turned off");
    let _ = shell.step(Message::App(AppMessage::Settings(
        heimdall_app::SettingsMessage::PreventSleep(true),
    )));
    assert!(shell.keeps_awake());
    let _ = shell.step(Message::App(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Closed { exit_status: None },
    }));
    assert!(!shell.keeps_awake(), "no session left");
}

#[test]
fn ctrl_v_pastes_except_in_a_full_screen_program_as_chosen() {
    use heimdall_core::settings::CtrlVPaste;
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Event, Key, Location, Modifiers};

    let ctrl_v = iced::Event::Keyboard(Event::KeyPressed {
        key: Key::Character("v".into()),
        modified_key: Key::Character("\u{16}".into()),
        physical_key: Physical::Code(Code::KeyV),
        location: Location::Standard,
        modifiers: Modifiers::CTRL,
        text: None,
        repeat: false,
    });
    // What Ctrl+V gave: a paste, or a key for the session.
    let pressed = |shell: &Shell| -> (bool, bool) {
        let mut ui = simulator(shell);
        let _ = ui.simulate([ctrl_v.clone()]);
        let messages: Vec<Message> = ui.into_messages().collect();
        (
            messages
                .iter()
                .any(|message| matches!(message, Message::App(AppMessage::ClipboardText { .. }))),
            messages
                .iter()
                .any(|message| matches!(message, Message::App(AppMessage::Key { .. }))),
        )
    };
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = connected_shell(dir.path());
    assert_eq!(
        shell.app().settings().ctrl_v_paste,
        CtrlVPaste::OutsideFullScreenPrograms
    );
    assert_eq!(pressed(&shell), (true, false), "at the prompt: pasted");

    // vim, less: the alternate screen. ^V is theirs.
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Output(b"\x1b[?1049h".to_vec()),
    }));
    assert_eq!(
        pressed(&shell),
        (false, true),
        "in a full-screen program: ^V"
    );

    let _ = shell.update(Message::App(AppMessage::Settings(
        heimdall_app::SettingsMessage::CtrlVPaste(CtrlVPaste::Always),
    )));
    assert_eq!(pressed(&shell), (true, false), "always");
    let _ = shell.update(Message::App(AppMessage::Settings(
        heimdall_app::SettingsMessage::CtrlVPaste(CtrlVPaste::Never),
    )));
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Output(b"\x1b[?1049l".to_vec()),
    }));
    assert_eq!(pressed(&shell), (false, true), "never, even at the prompt");
}

#[test]
fn ctrl_k_in_a_terminal_opens_quick_connect_unless_the_settings_send_it_to_the_session() {
    use heimdall_core::settings::CtrlKTerminal;
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Event, Key, Location, Modifiers};

    const FIELD: &str = "Search host or IP... (Ctrl+K)";
    let pressed = |typed: &str, modifiers: Modifiers| {
        iced::Event::Keyboard(Event::KeyPressed {
            key: Key::Character(typed.into()),
            modified_key: Key::Character(typed.into()),
            physical_key: Physical::Code(Code::KeyK),
            location: Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    };
    let ctrl_k = pressed("k", Modifiers::CTRL);
    let ctrl_shift_k = pressed("K", Modifiers::CTRL | Modifiers::SHIFT);
    // Whether the terminal took the key, and how many keys reached the session.
    let typed = |shell: &Shell, event: &iced::Event| -> (Vec<event::Status>, usize) {
        let mut ui = simulator(shell);
        let statuses = ui.simulate([event.clone()]);
        let keys = ui
            .into_messages()
            .filter(|message| matches!(message, Message::App(AppMessage::Key { .. })))
            .count();
        (statuses, keys)
    };
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, _, _) = connected_shell(dir.path());
    assert_eq!(
        shell.app().settings().ctrl_k_terminal,
        CtrlKTerminal::QuickConnect
    );
    assert_eq!(
        typed(&shell, &ctrl_k),
        (vec![event::Status::Ignored], 0),
        "left to the window, nothing sent to the session"
    );
    // The window makes it Quick Connect, over the session.
    let _ = shell.update(Message::Shortcut(WindowShortcut::QuickConnect));
    simulator(&shell).find(FIELD).expect("Quick Connect open");
    let typed_over = {
        let mut ui = simulator(&shell);
        let _ = ui.simulate([pressed("a", Modifiers::empty())]);
        ui.into_messages()
            .filter(|message| matches!(message, Message::App(AppMessage::Key { .. })))
            .count()
    };
    assert_eq!(typed_over, 0, "what is typed then is Quick Connect's");
    let _ = shell.update(Message::PaletteClose);
    assert!(simulator(&shell).find(FIELD).is_err());

    // Sent to the session: ^K, kill to the end of the line.
    let _ = shell.update(Message::App(AppMessage::Settings(
        heimdall_app::SettingsMessage::CtrlKTerminal(CtrlKTerminal::SendToSession),
    )));
    assert_eq!(
        typed(&shell, &ctrl_k),
        (vec![event::Status::Captured], 1),
        "the session's"
    );
    // Ctrl+Shift+K opens Quick Connect whatever the setting.
    for choice in CtrlKTerminal::ALL {
        let _ = shell.update(Message::App(AppMessage::Settings(
            heimdall_app::SettingsMessage::CtrlKTerminal(choice),
        )));
        assert_eq!(
            typed(&shell, &ctrl_shift_k),
            (vec![event::Status::Ignored], 0),
            "{choice:?}"
        );
    }
}

#[test]
fn ctrl_w_closes_the_session_shown_unless_a_field_has_the_keyboard() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _) = connected_shell(dir.path());
    // A field had the keyboard: Ctrl+W was its, nothing closes.
    let _ = shell.update(Message::CloseKeyFocus(true));
    assert!(shell.app().dialog.is_none());
    assert!(shell.app().tab(tab).is_some());
    // None had it: the session shown closes, asked first as a live one is.
    let _ = shell.update(Message::CloseKeyFocus(false));
    assert!(
        matches!(shell.app().dialog, Some(Dialog::ConfirmCloseTab(asked)) if asked == tab),
        "{:?}",
        shell.app().dialog
    );
    // A dialog open, Ctrl+W is not asked about: it is the dialog's.
    let _ = shell.update(Message::CloseKey);
    assert!(matches!(
        shell.app().dialog,
        Some(Dialog::ConfirmCloseTab(_))
    ));
}

#[test]
fn only_the_main_window_draws_the_shell_and_is_titled_as_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let _ = open(&mut core, "a");
    let mut shell = Shell::with_app(core);
    let (main, other) = (iced::window::Id::unique(), iced::window::Id::unique());
    shell.set_main_window(main);
    assert_eq!(shell.window_title(main), shell.title());
    assert!(shell.title().contains("server a"), "{}", shell.title());
    assert!(
        !shell.window_title(other).contains("server a"),
        "another window is not titled after the tab shown"
    );
    let settings = || Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    let mut ui = common::simulator(settings(), WINDOW, shell.window_view(main));
    ui.find("Sessions")
        .expect("the main window draws the shell");
    drop(ui);
    let mut ui = common::simulator(settings(), WINDOW, shell.window_view(other));
    assert!(
        ui.find("Sessions").is_err(),
        "another window does not draw the shell"
    );
}

/// A session's input, kept: what reached it.
#[derive(Debug, Default)]
struct Recorder(std::sync::Mutex<Vec<u8>>);

impl heimdall_app::InputSink for Recorder {
    fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(bytes);
        Ok(())
    }

    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }

    fn close(&self) {}
}

/// A shell that said `said`, then lost its connection; the input that reached it.
fn dropped_shell(dir: &Path, said: &[u8], auto_reconnect: bool) -> (Shell, TabId, Arc<Recorder>) {
    let mut core = app(dir);
    if auto_reconnect {
        core.update(AppMessage::Settings(
            heimdall_app::SettingsMessage::SshAutoReconnect(true),
        ));
    }
    let (tab, attempt) = open(&mut core, "a");
    let recorder = Arc::new(Recorder::default());
    for event in [
        ConnectionEvent::Connected {
            input: recorder.clone(),
        },
        ConnectionEvent::Output(said.to_vec()),
        ConnectionEvent::Failed(UiError::ConnectionLost),
    ] {
        core.update(AppMessage::Connection {
            tab,
            attempt,
            event,
        });
    }
    (Shell::with_app(core), tab, recorder)
}

/// Whether the window draws `tab`'s terminal: it reports its size on the first event.
fn draws_terminal(shell: &Shell, tab: TabId) -> bool {
    let mut ui = simulator(shell);
    let _ = ui.simulate([iced::Event::Keyboard(
        iced::keyboard::Event::ModifiersChanged(iced::keyboard::Modifiers::empty()),
    )]);
    ui.into_messages().any(
        |message| matches!(message, Message::App(AppMessage::Resize { tab: t, .. }) if t == tab),
    )
}

#[test]
fn a_dropped_shell_keeps_its_terminal_in_sight_with_the_failure_and_its_ways_out_under_it() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, recorder) = dropped_shell(dir.path(), b"uptime: 42 days\r\n", false);
    assert!(shell.app().tab(tab).is_some_and(heimdall_app::Tab::dropped));
    snapshot(&shell, "session-dropped.png");
    assert!(draws_terminal(&shell, tab), "the terminal stays drawn");
    assert!(
        shell
            .app()
            .tab(tab)
            .expect("tab")
            .terminal
            .snapshot()
            .row_text(0)
            .contains("uptime: 42 days"),
        "what it showed is still there to read"
    );
    {
        let mut ui = simulator(&shell);
        // As the C# says it on the disconnect marker's overlay.
        ui.find("Session disconnected unexpectedly.")
            .expect("the failure said");
        assert!(
            ui.find("The connection failed").is_err(),
            "no card over the terminal"
        );
        ui.find("Copy error").expect("its error copied");
        ui.find("Close the tab").expect("closed");
        ui.click("Reconnect").expect("reconnect");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::ReconnectTab(reconnected)) if reconnected == tab
        )));
    }
    // Typed into, it sends nothing: the session is gone.
    let typed: Vec<Message> = {
        let mut ui = simulator(&shell);
        ui.typewrite("ls");
        let _ = ui.tap_key(keyboard_named(Named::Enter));
        ui.into_messages().collect()
    };
    for message in typed {
        let _ = shell.update(message);
    }
    assert!(
        recorder
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty(),
        "nothing reached the session"
    );
}

#[test]
fn a_dropped_shell_counting_down_to_its_next_attempt_keeps_its_terminal_in_sight() {
    let dir = tempfile::tempdir().expect("dir");
    let (shell, tab, _) = dropped_shell(dir.path(), b"still here\r\n", true);
    assert!(
        shell.app().tab(tab).and_then(|found| found.retry).is_some(),
        "the attempts started"
    );
    assert!(draws_terminal(&shell, tab), "the terminal stays drawn");
    let mut ui = simulator(&shell);
    ui.find("Reconnecting (attempt 1/3)...")
        .expect("the countdown said");
    ui.click("Cancel").expect("cancel");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::CancelAutoReconnect(cancelled)) if cancelled == tab
    )));
}

#[test]
fn a_session_that_never_connected_shows_its_failure_card_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (tab, attempt) = open(&mut core, "a");
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::ConnectionLost),
    });
    let shell = Shell::with_app(core);
    assert!(shell.app().tab(tab).is_some_and(|found| !found.dropped()));
    assert!(!draws_terminal(&shell, tab), "no terminal: nothing to show");
    let mut ui = simulator(&shell);
    ui.find("The connection failed").expect("the card");
    ui.find("Reconnect").expect("its way out");
}

/// The window tall enough for the whole Settings page.
fn tall_settings(shell: &Shell) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, Size::new(1100.0, 2400.0), shell.view())
}

#[test]
fn transcript_retention_is_typed_under_the_transcripts_folder_within_the_csharp_range() {
    use heimdall_ui::shell::{SessionField, SettingsTab};

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(SettingsTab::Terminal));
    let refusal = "Transcript retention must be 0 (keep all) or between 7 and 3650 days.";
    {
        let mut ui = tall_settings(&shell);
        for said in [
            "Delete transcripts older than",
            "days",
            "0 keeps every transcript. Only session transcripts are deleted; the event and file-operation logs are kept.",
        ] {
            ui.find(said).expect(said);
        }
    }
    let field = SessionField::TranscriptRetention;
    for (typed, refused) in [("6", true), ("3651", true), ("week", true), (" 30 ", false)] {
        let _ = shell.update(Message::SessionFieldEdited(field, typed.to_owned()));
        let _ = shell.update(Message::SessionFieldApply(field));
        let mut ui = tall_settings(&shell);
        assert_eq!(ui.find(refusal).is_ok(), refused, "{typed}");
    }
    assert_eq!(shell.app().settings().session_log_retention_days, 30);
    let _ = shell.update(Message::SessionFieldEdited(field, "0".to_owned()));
    let _ = shell.update(Message::SessionFieldApply(field));
    assert_eq!(
        shell.app().settings().session_log_retention_days,
        0,
        "every transcript kept again"
    );
}

#[test]
fn the_selection_menu_offers_the_password_of_its_profiles_and_says_when_it_cannot() {
    use heimdall_app::SelectionMessage;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    core.update(AppMessage::SelectProfile(ProfileId::new("a")));
    core.update(AppMessage::Selection(SelectionMessage::Toggle(
        ProfileId::new("b"),
    )));
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::EditSelection));
    let mut ui = simulator(&shell);
    ui.click("Password... (2)").expect("the entry");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::MenuChoice(AppMessage::Selection(SelectionMessage::EditPassword))
    )));

    // Without a store to save passwords in, the entry says why it is disabled.
    drop(shell);
    let mut core = App::new(AppConfig {
        profiles_file: dir.path().join("profiles.toml"),
        known_hosts: dir.path().join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GRID,
        files_start: dir.path().to_owned(),
        system_credentials: heimdall_app::SystemCredentials::Unavailable,
    });
    core.update(AppMessage::SelectProfile(ProfileId::new("a")));
    core.update(AppMessage::Selection(SelectionMessage::Toggle(
        ProfileId::new("b"),
    )));
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::EditSelection));
    let mut ui = simulator(&shell);
    ui.find("This system has no credential store: set a master password to save passwords.")
        .expect("why it is disabled");
    ui.click("Password... (2)").expect("the entry");
    assert!(!ui.into_messages().any(|message| matches!(
        message,
        Message::MenuChoice(AppMessage::Selection(SelectionMessage::EditPassword))
    )));
}

#[test]
fn the_bulk_password_dialog_names_its_count_and_saves_the_password_typed_twice() {
    use heimdall_app::{BulkPasswordSkips, SelectionMessage};

    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    core.update(AppMessage::SelectProfile(ProfileId::new("a")));
    core.update(AppMessage::Selection(SelectionMessage::Toggle(
        ProfileId::new("b"),
    )));
    core.update(AppMessage::Selection(SelectionMessage::EditPassword));
    let mut shell = Shell::with_app(core);
    {
        let mut ui = simulator(&shell);
        ui.find("Setting password on 2 servers").expect("the count");
        ui.find("New password:").expect("the password");
        ui.find("Confirm password:").expect("again");
    }
    let field = |index: usize, value: &str| Message::BulkPasswordField {
        index,
        value: value.to_owned(),
    };
    let _ = shell.update(field(0, "hunter2"));
    let _ = shell.update(field(1, "hunter3"));
    let _ = shell.update(Message::SubmitBulkPassword);
    {
        let mut ui = simulator(&shell);
        ui.find("Passwords do not match.").expect("refused");
    }
    let _ = shell.update(field(0, "hunter2"));
    let _ = shell.update(field(1, "hunter2"));
    let _ = shell.update(Message::DialogKey { confirm: true });
    let mut core = shell.into_app();
    assert!(core.dialog.is_none(), "{:?}", core.dialog);
    assert_eq!(saved_answer(&mut core).as_deref(), Some("hunter2"));

    // One server, and the profiles left alone said.
    core.dialog = Some(Dialog::BulkPassword {
        ids: vec![ProfileId::new("a")],
        skipped: BulkPasswordSkips {
            winrm: 1,
            no_account: 2,
            other: 0,
        },
        refused: None,
    });
    let shell = Shell::with_app(core);
    snapshot(&shell, "bulk-password.png");
    let mut ui = simulator(&shell);
    ui.find("Setting password on 1 server").expect("one");
    ui.find("Skipped 1 WinRM profile because no username is configured.")
        .expect("WinRM");
    ui.find("Skipped 2 profiles because no username is configured.")
        .expect("without an account");
    assert!(
        ui.find("whose protocol has no saved password").is_err(),
        "none of that kind"
    );
}
