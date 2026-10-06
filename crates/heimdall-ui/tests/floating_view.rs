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

//! A tab's own window drawn headless, as the C# floating window: its header naming the
//! session, its state and Reattach, the session's question drawn under it, its title, the
//! veil behind the lock screen; and the tab menu's "Detach to Window".
//!
//! Strings are the fallback language's (English): the tests never select a language.

mod common;

use std::path::Path;

use std::sync::Arc;

use heimdall_app::split::{Axis, Placement, SplitMessage};
use heimdall_app::{
    App, AppConfig, AttemptId, BroadcastMessage, ConnectionEvent, Dialog, FloatMessage, KeyInput,
    Message as AppMessage, PointerInput, QuestionId, QuestionKind, SpecialKeys, TabGroup, TabId,
    TabMenuMessage, VncQuality,
};
use heimdall_core::profile::{ProfileId, RdpProfile, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, PasswordQuestion};
use heimdall_term::{CellPixels, CellPoint, GridSize, Key, KeyLocation, Modifiers, MouseAction};
use heimdall_ui::floating_view::floating_message_allowed;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::terminal_view::keys::WindowShortcut;
use heimdall_ui::tree_view::TreeMenu;
use iced::{Element, Settings, Size, window};

const GRID: GridSize = GridSize { cols: 80, rows: 24 };

/// Size of the simulated window: the C# floating window's.
const WINDOW: Size = heimdall_ui::floating_view::WINDOW_SIZE;

/// A main window tall enough for a tab's whole menu.
const TALL_WINDOW: Size = Size::new(1100.0, 1200.0);

/// The C# owner line of the desktop's certificate question, in its own window.
const OWNER: &str = "This question belongs to the tab \"DC\".";

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
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
    }
}

/// A remote desktop, `dc`.
fn desktop_profile() -> RdpProfile {
    RdpProfile {
        extras: heimdall_core::profile::RdpExtras::default(),
        id: ProfileId::new("dc"),
        name: "DC".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
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
        anti_idle: false,
        auto_reconnect: true,
    }
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([profile("a"), profile("b")]);
    store.merge_rdp([desktop_profile()]);
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

fn open_as(app: &mut App, message: AppMessage) -> (TabId, AttemptId) {
    match app.update(message).as_slice() {
        [heimdall_app::Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one Connect, got {other:?}"),
    }
}

fn open(app: &mut App, id: &str) -> (TabId, AttemptId) {
    open_as(app, AppMessage::OpenProfile(ProfileId::new(id)))
}

fn settings() -> Settings {
    Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    }
}

fn simulator<'a>(view: impl Into<Element<'a, Message>>) -> common::Drawn<'a> {
    common::simulator(settings(), WINDOW, view)
}

/// The window with tab `a` on the strip and tab `b`, still connecting, detached; `b`'s
/// window and attempt.
fn detached(dir: &Path) -> (Shell, TabId, AttemptId, window::Id) {
    let mut core = app(dir);
    open(&mut core, "a");
    let (b, attempt) = open(&mut core, "b");
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::App(AppMessage::Float(FloatMessage::Detach(b))));
    let window = shell.floating_window(b).expect("its own window");
    (shell, b, attempt, window)
}

#[test]
fn the_header_names_the_session_its_state_and_reattach_brings_it_back() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, b, _, window) = detached(dir.path());
    let key = shell.app().floating_of(b).expect("detached");
    {
        let mut ui = simulator(shell.window_view(window));
        for label in [
            "SSH",
            "server b",
            " - ",
            "Connecting...",
            "Connecting to admin@b.lab:22...",
        ] {
            ui.find(label).expect(label);
        }
        ui.click("Reattach to Main Window").expect("reattach");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::Float(FloatMessage::Reattach(pressed))) if pressed == key
        )));
    }
    // The session's own buttons are the window's: marked as such.
    {
        let mut ui = simulator(shell.window_view(window));
        ui.click("Cancel").expect("the connecting card's cancel");
        assert!(ui.into_messages().any(|message| matches!(
            &message,
            Message::InFloating(from, inner) if *from == window
                && matches!(**inner, Message::App(AppMessage::RequestCloseTab(tab)) if tab == b)
        )));
    }
    let _ = shell.update(Message::App(AppMessage::Float(FloatMessage::Reattach(key))));
    assert_eq!(shell.floating_window(b), None, "its window closed");
    assert_eq!(shell.app().active, Some(b), "shown in the main window");
    assert!(shell.app().strip().iter().any(|tab| tab.id == b));
}

#[test]
fn the_window_is_titled_as_the_csharp_one() {
    let dir = tempfile::tempdir().expect("dir");
    let (shell, _, _, window) = detached(dir.path());
    assert_eq!(shell.window_title(window), "server b - Detached");
    assert_eq!(
        shell.window_title(window::Id::unique()),
        shell.title(),
        "another window, in tests the main one, keeps its title"
    );
    assert!(
        !shell.title().contains("server b"),
        "the main window's tab is a"
    );
}

#[test]
fn a_question_of_the_detached_tab_is_asked_in_its_window() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, b, attempt, window) = detached(dir.path());
    let question = QuestionId::fresh();
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab: b,
        attempt,
        event: ConnectionEvent::Question {
            question,
            kind: QuestionKind::Password(PasswordQuestion {
                host: "b.lab".to_owned(),
                port: 22,
                username: "admin".to_owned(),
                attempt: 1,
            }),
        },
    }));
    let mut ui = simulator(shell.window_view(window));
    ui.find("Password for admin on b.lab:22")
        .expect("asked here");
    ui.click("Continue").expect("submit button");
    assert!(ui.into_messages().any(|message| matches!(
        &message,
        Message::InFloating(from, inner) if *from == window
            && matches!(**inner, Message::Submit(tab) if tab == b)
    )));
}

#[test]
fn a_dialog_raised_in_the_window_is_the_main_windows() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, b, attempt, window) = detached(dir.path());
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab: b,
        attempt,
        event: ConnectionEvent::Connected {
            input: std::sync::Arc::new(NullSink),
        },
    }));
    let _ = shell.update(Message::InFloating(
        window,
        Box::new(Message::App(AppMessage::RequestCloseTab(b))),
    ));
    assert_eq!(shell.app().dialog, Some(Dialog::ConfirmCloseTab(b)));
    let mut main = simulator(shell.view());
    main.find("Close this session?")
        .expect("asked in the main window");
}

#[test]
fn a_certificate_question_in_the_window_names_its_tab_as_the_csharp_one() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    open(&mut core, "a");
    let (tab, attempt) = match core
        .update(AppMessage::OpenRdp(ProfileId::new("dc")))
        .as_slice()
    {
        [heimdall_app::Effect::ConnectRdp { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::App(AppMessage::Float(FloatMessage::Detach(tab))));
    let window = shell.floating_window(tab).expect("its own window");
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::UnknownRdpCertificate {
            subject: None,
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
                .parse()
                .expect("fingerprint"),
        },
    }));
    {
        let mut ui = simulator(shell.window_view(window));
        ui.find("Unrecognised Server Certificate")
            .expect("asked here");
        ui.find(OWNER).expect("its tab named");
    }
    let key = shell.app().floating_of(tab).expect("detached");
    let _ = shell.update(Message::App(AppMessage::Float(FloatMessage::Reattach(key))));
    let mut main = common::simulator(settings(), TALL_WINDOW, shell.view());
    main.find("Unrecognised Server Certificate")
        .expect("asked in its tab");
    assert!(main.find(OWNER).is_err(), "in its tab, nothing to name");
}

#[derive(Debug, Default)]
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
fn behind_the_lock_the_window_shows_a_veil_and_takes_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    create_vault(&mut core);
    open(&mut core, "a");
    let (b, _) = open(&mut core, "b");
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::App(AppMessage::Float(FloatMessage::Detach(b))));
    let window = shell.floating_window(b).expect("its own window");
    let _ = shell.update(Message::LockKey);
    assert!(shell.app().is_locked());
    {
        let mut ui = simulator(shell.window_view(window));
        ui.find("Workspace locked").expect("the veil");
        assert!(ui.find("server b").is_err(), "nothing of the session");
        assert!(ui.find("Reattach to Main Window").is_err());
    }
    let _ = shell.update(Message::InFloating(
        window,
        Box::new(Message::App(AppMessage::RequestCloseTab(b))),
    ));
    assert!(shell.app().tab(b).is_some(), "the window's input dropped");
    assert!(shell.app().is_locked(), "the lock screen stays");
}

/// The messages the entry `label` of the menu open in `shell` sends.
fn chosen(shell: &Shell, label: &str) -> Vec<Message> {
    let mut ui = common::simulator(settings(), TALL_WINDOW, shell.view());
    ui.click(label).expect(label);
    ui.into_messages()
        .filter(|message| matches!(message, Message::MenuChoice(_)))
        .collect()
}

#[test]
fn the_tab_menu_offers_detach_to_a_tab_not_split_and_not_files() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let (a, _) = open(&mut core, "a");
    let (b, _) = open(&mut core, "b");
    let (files, _) = open_as(&mut core, AppMessage::OpenFiles(ProfileId::new("a")));
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(a)));
    let detach = chosen(&shell, "Detach to Window");
    assert!(
        matches!(
            detach.as_slice(),
            [Message::MenuChoice(AppMessage::Float(FloatMessage::Detach(tab)))] if *tab == a
        ),
        "{detach:?}"
    );
    // A Files tab: its panes stay in the main window for now.
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(files)));
    {
        let mut ui = common::simulator(settings(), TALL_WINDOW, shell.view());
        ui.find("Duplicate Session").expect("the menu is open");
        assert!(ui.find("Detach to Window").is_err(), "not for a Files tab");
    }
    // A split tab: refused, as the C# offers Detach Secondary Pane there instead.
    let _ = shell.update(Message::App(AppMessage::Split(SplitMessage::Merge {
        host: a,
        tab: b,
        axis: Axis::SideBySide,
        placement: Placement::Second,
    })));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(a)));
    let mut ui = common::simulator(settings(), TALL_WINDOW, shell.view());
    ui.find("Unsplit").expect("the split's menu");
    assert!(ui.find("Detach to Window").is_err(), "not for a split tab");
}

/// Records what the application sends to a session.
#[derive(Debug, Default)]
struct RecordingSink {
    written: std::sync::Mutex<Vec<u8>>,
}

impl RecordingSink {
    fn taken(&self) -> Vec<u8> {
        std::mem::take(&mut *self.written.lock().expect("written"))
    }
}

impl heimdall_app::InputSink for RecordingSink {
    fn write(&self, bytes: Vec<u8>) -> Result<(), heimdall_ssh::SessionClosed> {
        self.written.lock().expect("written").extend(bytes);
        Ok(())
    }

    fn resize(&self, _size: heimdall_ssh::TerminalSize) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }

    fn close(&self) {}
}

/// Tab `a` connected and shown in the main window, tab `b` connected and detached; what
/// each is sent.
struct Connected {
    shell: Shell,
    a: TabId,
    b: TabId,
    window: window::Id,
    sink_a: Arc<RecordingSink>,
    sink_b: Arc<RecordingSink>,
}

fn connected_pair(dir: &Path) -> Connected {
    let mut core = app(dir);
    let (a, attempt_a) = open(&mut core, "a");
    let (b, attempt_b) = open(&mut core, "b");
    let (sink_a, sink_b) = (
        Arc::new(RecordingSink::default()),
        Arc::new(RecordingSink::default()),
    );
    for (tab, attempt, sink) in [(a, attempt_a, &sink_a), (b, attempt_b, &sink_b)] {
        core.update(AppMessage::Connection {
            tab,
            attempt,
            event: ConnectionEvent::Connected {
                input: sink.clone(),
            },
        });
    }
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::App(AppMessage::Float(FloatMessage::Detach(b))));
    let window = shell.floating_window(b).expect("its own window");
    assert_eq!(shell.app().active, Some(a));
    Connected {
        shell,
        a,
        b,
        window,
        sink_a,
        sink_b,
    }
}

fn letter(tab: TabId, c: char) -> AppMessage {
    AppMessage::Key {
        tab,
        input: KeyInput {
            key: Key::Character(c),
            text: Some(c.to_string()),
            physical_digit: None,
            location: KeyLocation::Standard,
            modifiers: Modifiers::default(),
        },
    }
}

/// Every message the session drawn in a tab's own window sends, for `tab`: its terminal,
/// its desktop and its bar, its cards. The question's fields and buttons are tried apart.
fn session_messages(tab: TabId) -> Vec<Message> {
    let app = Message::App;
    vec![
        app(letter(tab, 'x')),
        app(AppMessage::Pointer {
            tab,
            input: PointerInput {
                at: CellPoint {
                    row: 0,
                    col: 0,
                    right_half: false,
                },
                action: MouseAction::Motion { held: None },
                modifiers: Modifiers::default(),
                clicks: 1,
            },
        }),
        app(AppMessage::Resize {
            tab,
            grid: GRID,
            cell: CellPixels {
                width: 8,
                height: 16,
            },
        }),
        app(AppMessage::Copy(tab)),
        app(AppMessage::ClipboardText { tab, text: None }),
        app(AppMessage::ScrollHistory { tab, lines: 1 }),
        app(AppMessage::DesktopInput {
            tab,
            inputs: Vec::new(),
        }),
        app(AppMessage::DesktopResize {
            tab,
            width: 640,
            height: 480,
        }),
        app(AppMessage::DesktopShown {
            tab,
            width: 640,
            height: 480,
        }),
        app(AppMessage::SendKeys {
            tab,
            keys: SpecialKeys::CtrlAltDel,
        }),
        app(AppMessage::VncQuality {
            tab,
            quality: VncQuality::Balanced,
        }),
        app(AppMessage::SendClipboard(tab)),
        app(AppMessage::SaveRemoteFiles(tab)),
        app(AppMessage::CancelSave(tab)),
        app(AppMessage::StopAntiIdle(tab)),
        app(AppMessage::DisconnectDesktop(tab)),
        app(AppMessage::CancelAutoReconnect(tab)),
        app(AppMessage::CopyHostKeyFingerprint(tab)),
        app(AppMessage::HostKeyDecision { tab, accept: false }),
        app(AppMessage::HostKeyTrustOnce(tab)),
        app(AppMessage::ForgetServer(tab)),
        Message::DesktopFit { tab, fit: true },
        Message::CopyError(tab),
        Message::CopyAnonymousError(tab),
        Message::Submit(tab),
        Message::Decline(tab),
        // Last: they close the tab, or open it again as another.
        app(AppMessage::RequestCloseTab(tab)),
        app(AppMessage::ReconnectTab(tab)),
    ]
}

#[test]
fn what_the_window_lets_through_names_its_tab_and_leaves_the_main_window_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let Connected {
        mut shell,
        a,
        b,
        window,
        sink_a,
        sink_b,
    } = connected_pair(dir.path());
    {
        let floating = shell.app().tab(b).expect("detached");
        let shown = shell.app().tab(a).expect("shown");
        for message in session_messages(b) {
            assert!(
                floating_message_allowed(&message, floating),
                "let through: {message:?}"
            );
            assert!(
                !floating_message_allowed(&message, shown),
                "naming another tab: {message:?}"
            );
        }
        for message in session_messages(a) {
            assert!(
                !floating_message_allowed(&message, floating),
                "naming the main window's tab: {message:?}"
            );
        }
        // Tab-free: dialogs of the main window naming nothing shown.
        for message in [
            AppMessage::ShowShortcuts,
            AppMessage::EditProfile(ProfileId::new("b")),
        ] {
            assert!(floating_message_allowed(&Message::App(message), floating));
        }
    }

    // Each goes through the window, and the main window's tab stays shown.
    for message in session_messages(b) {
        let label = format!("{message:?}");
        let _ = shell.update(Message::InFloating(window, Box::new(message)));
        assert_eq!(shell.app().active, Some(a), "after {label}");
        assert!(shell.app().floating_invariant_holds(), "after {label}");
        match &shell.app().dialog {
            // The question is about the detached tab, asked in the main window.
            Some(Dialog::ConfirmCloseTab(asked)) => {
                assert_eq!(*asked, b, "after {label}");
                let _ = shell.update(Message::App(AppMessage::DismissDialog));
            }
            None => {}
            other => panic!("after {label}: {other:?}"),
        }
        assert!(sink_a.taken().is_empty(), "nothing typed in a: {label}");
    }
    // What was typed reached the detached tab, and it opened again in its window.
    assert_eq!(sink_b.taken(), b"x");
    assert!(shell.app().tab(b).is_none(), "opened again as another tab");
    let reopened = shell.app().tabs.last().expect("reopened").id;
    assert_eq!(shell.floating_window(reopened), Some(window));
}

#[test]
fn a_question_in_the_window_is_typed_into_and_answered_there() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, b, attempt, window) = detached(dir.path());
    let question = QuestionId::fresh();
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab: b,
        attempt,
        event: ConnectionEvent::Question {
            question,
            kind: QuestionKind::Password(PasswordQuestion {
                host: "b.lab".to_owned(),
                port: 22,
                username: "admin".to_owned(),
                attempt: 1,
            }),
        },
    }));
    let shown = shell.app().active;
    let floating = shell.app().tab(b).expect("detached");
    for message in [
        Message::Field {
            question,
            index: 0,
            value: String::new(),
        },
        Message::FocusField { question, index: 0 },
    ] {
        assert!(floating_message_allowed(&message, floating));
    }
    // Another question's field is no message of this window.
    assert!(!floating_message_allowed(
        &Message::FocusField {
            question: QuestionId::fresh(),
            index: 0
        },
        floating
    ));
    let _ = shell.update(Message::InFloating(
        window,
        Box::new(Message::Field {
            question,
            index: 0,
            value: "hunter2".to_owned(),
        }),
    ));
    assert!(shell.holds_draft(question));
    let _ = shell.update(Message::InFloating(window, Box::new(Message::Submit(b))));
    assert!(
        shell.app().tab(b).expect("tab").prompts.is_empty(),
        "answered"
    );
    assert!(!shell.holds_draft(question));
    assert_eq!(shell.app().active, shown);
}

#[test]
fn a_message_the_window_does_not_let_through_is_dropped() {
    let dir = tempfile::tempdir().expect("dir");
    let Connected {
        mut shell,
        a,
        b,
        window,
        sink_a,
        sink_b,
    } = connected_pair(dir.path());
    let dropped = [
        // Naming the main window's tab.
        Message::App(letter(a, 'y')),
        Message::App(AppMessage::RequestCloseTab(a)),
        Message::App(AppMessage::SelectTab(b)),
        // The main window's, resolved through its tab shown or its widgets.
        Message::App(AppMessage::WindowCloseRequested),
        Message::App(AppMessage::Broadcast(BroadcastMessage::Toggle)),
        Message::App(AppMessage::TabMenu(TabMenuMessage::Close {
            tab: b,
            group: TabGroup::Others,
        })),
        Message::Shortcut(WindowShortcut::NextTab),
        Message::Shortcut(WindowShortcut::CloseTab),
        Message::ToggleFullscreen,
        Message::OpenTreeMenu(TreeMenu::Tab(b)),
        Message::ContentRelease,
        Message::CloseKey,
        Message::FinderClose,
        Message::FilesKey(heimdall_app::files::FilesKey::SwitchPane),
    ];
    let floating = shell.app().tab(b).expect("detached");
    for message in &dropped {
        assert!(!floating_message_allowed(message, floating), "{message:?}");
    }
    for message in dropped {
        let label = format!("{message:?}");
        let _ = shell.update(Message::InFloating(window, Box::new(message)));
        assert_eq!(shell.app().active, Some(a), "after {label}");
        assert_eq!(shell.app().dialog, None, "after {label}");
        assert_eq!(shell.app().tabs.len(), 2, "after {label}");
        assert!(shell.app().is_floating(b), "after {label}");
        assert!(!shell.app().broadcasting(), "after {label}");
    }
    assert!(sink_a.taken().is_empty());
    assert!(sink_b.taken().is_empty());
}
