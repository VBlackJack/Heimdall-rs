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
//! veil behind the lock screen; a Files tab in it, its fields its own, its keys, drops and
//! menus its own; and the tab menu's "Detach to Window" and "Detach Secondary Pane".
//!
//! Strings are the fallback language's (English): the tests never select a language.

mod common;

use std::path::Path;

use std::sync::Arc;

use heimdall_app::files::{Direction, EntryKind, FilesKey, RemoteEntry, Side, SortColumn};
use heimdall_app::split::{Axis, Placement, SplitMessage};
use heimdall_app::{
    App, AppConfig, AttemptId, BroadcastMessage, ConnectionEvent, Dialog, EditorId, FilesMessage,
    FloatMessage, KeyInput, Message as AppMessage, PointerInput, QuestionId, QuestionKind,
    SpecialKeys, TabGroup, TabId, TabMenuMessage, VncQuality,
};
use heimdall_core::profile::{ProfileId, RdpProfile, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_files::RemoteSession;
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use heimdall_ssh::{AgentSource, PasswordQuestion};
use heimdall_term::{CellPixels, CellPoint, GridSize, Key, KeyLocation, Modifiers, MouseAction};
use heimdall_ui::files_drag::Spot;
use heimdall_ui::floating_view::{
    ColumnWidths, EditorKey, EditorMessage, FloatEvent, PaneField, field_id,
    floating_message_allowed, list_id,
};
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::terminal_view::keys::WindowShortcut;
use heimdall_ui::tree_view::TreeMenu;
use iced::{Element, Settings, Size, window};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

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
        ssh_mode: heimdall_core::profile::SshMode::Embedded,
        x11_forwarding: false,
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
            details: None,
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
                ticket,
            },
        ],
    ) = <[Effect; 1]>::try_from(effects)
    else {
        panic!("expected OpenVault");
    };
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let result = runtime.block_on(open_vault(path, password, job));
    core.update(AppMessage::VaultOpened(ticket, result));
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
fn the_tab_menu_offers_detach_to_a_tab_not_split_and_detach_secondary_to_a_split_one() {
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
    {
        let mut ui = common::simulator(settings(), TALL_WINDOW, shell.view());
        assert!(
            ui.find("Detach Secondary Pane").is_err(),
            "not for a tab not split"
        );
    }
    // A Files tab too, as the C# offers it to any tab not split.
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(files)));
    let detach = chosen(&shell, "Detach to Window");
    assert!(
        matches!(
            detach.as_slice(),
            [Message::MenuChoice(AppMessage::Float(FloatMessage::Detach(tab)))] if *tab == files
        ),
        "{detach:?}"
    );
    // A split tab: Detach Secondary Pane in its place, as the C# `AppendDetachItem`.
    let _ = shell.update(Message::App(AppMessage::Split(SplitMessage::Merge {
        host: a,
        tab: b,
        axis: Axis::SideBySide,
        placement: Placement::Second,
    })));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(a)));
    {
        let mut ui = common::simulator(settings(), TALL_WINDOW, shell.view());
        ui.find("Unsplit").expect("the split's menu");
        assert!(ui.find("Detach to Window").is_err(), "not for a split tab");
    }
    let detach = chosen(&shell, "Detach Secondary Pane");
    assert!(
        matches!(
            detach.as_slice(),
            [Message::MenuChoice(AppMessage::Float(FloatMessage::DetachSecondary(host)))]
                if *host == a
        ),
        "{detach:?}"
    );
    // Chosen: the secondary pane in its own window, the host alone on the strip.
    let _ = shell.update(detach.into_iter().next().expect("chosen"));
    assert!(shell.app().is_floating(b));
    assert!(shell.floating_window(b).is_some(), "its window asked for");
    assert!(!shell.app().in_split(a));
    assert_eq!(shell.app().active, Some(a), "the host keeps the keyboard");
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
fn ctrl_k_in_the_window_opens_quick_connect_in_the_main_window_and_sends_nothing() {
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Event, Key, Location, Modifiers as Held};

    const FIELD: &str = "Search host or IP... (Ctrl+K)";
    let dir = tempfile::tempdir().expect("dir");
    let Connected {
        mut shell,
        window,
        sink_b,
        ..
    } = connected_pair(dir.path());
    let ctrl_k = iced::Event::Keyboard(Event::KeyPressed {
        key: Key::Character("k".into()),
        modified_key: Key::Character("k".into()),
        physical_key: Physical::Code(Code::KeyK),
        location: Location::Standard,
        modifiers: Held::CTRL,
        text: None,
        repeat: false,
    });
    {
        let mut ui = simulator(shell.window_view(window));
        let statuses = ui.simulate([ctrl_k]);
        assert_eq!(statuses, [iced::event::Status::Ignored], "the window's");
        assert!(
            !ui.into_messages().any(|message| matches!(
                &message,
                Message::InFloating(_, inner)
                    if matches!(**inner, Message::App(AppMessage::Key { .. }))
            )),
            "nothing for its session"
        );
    }
    // Its window reports it: Quick Connect opens in the main window.
    let _ = shell.update(Message::Float(window, FloatEvent::QuickConnect));
    simulator(shell.view())
        .find(FIELD)
        .expect("Quick Connect open");
    assert!(sink_b.taken().is_empty(), "its session got nothing");
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
        // The search bar of the main window's tab.
        Message::FinderClose(a),
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

/// Tab `a` connected and shown in the main window, `b` connected and detached, each with
/// `output` on its screen; `b`'s window.
fn searchable_pair(dir: &Path, output: [&str; 2]) -> (Shell, TabId, TabId, window::Id) {
    let mut core = app(dir);
    let mut tabs = Vec::new();
    for (id, shown) in ["a", "b"].into_iter().zip(output) {
        let (tab, attempt) = open(&mut core, id);
        core.update(AppMessage::Connection {
            tab,
            attempt,
            event: ConnectionEvent::Connected {
                input: Arc::new(RecordingSink::default()),
            },
        });
        core.update(AppMessage::Connection {
            tab,
            attempt,
            event: ConnectionEvent::Output(shown.as_bytes().to_vec()),
        });
        tabs.push(tab);
    }
    let (a, b) = (tabs[0], tabs[1]);
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::App(AppMessage::Float(FloatMessage::Detach(b))));
    let window = shell.floating_window(b).expect("its own window");
    (shell, a, b, window)
}

/// Whether `view` draws a terminal's search bar: its "previous" button.
fn bar_shown(view: Element<'_, Message>) -> bool {
    simulator(view).find("\u{25b2}").is_ok()
}

/// What is selected on `tab`'s screen.
fn selected(shell: &Shell, tab: TabId) -> Option<String> {
    shell.app().tab(tab).expect("tab").terminal.selected_text()
}

#[test]
fn ctrl_shift_f_searches_a_detached_terminal_in_its_own_window_and_leaves_the_main_bar_alone() {
    use heimdall_term::FindDirection;

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, a, b, window) =
        searchable_pair(dir.path(), ["main line\r\nneedle\r\n", "a needle here\r\n"]);
    let in_window = |message: Message| Message::InFloating(window, Box::new(message));
    // The main window's bar, over `a`, its text typed.
    let _ = shell.update(Message::Shortcut(WindowShortcut::Find));
    let _ = shell.update(Message::FinderQuery {
        tab: a,
        query: "main".to_owned(),
    });
    assert!(
        !bar_shown(shell.window_view(window)),
        "none in the window yet"
    );

    let _ = shell.update(Message::Float(window, FloatEvent::TerminalFind));
    assert!(bar_shown(shell.window_view(window)), "drawn in the window");
    {
        let mut ui = simulator(shell.window_view(window));
        ui.click("Search...").expect("its field");
        ui.typewrite("n");
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages.iter().any(|message| matches!(
                message,
                Message::InFloating(from, inner) if *from == window
                    && matches!(&**inner, Message::FinderQuery { tab, query } if *tab == b && query == "n")
            )),
            "{messages:?}"
        );
    }
    let _ = shell.update(in_window(Message::FinderQuery {
        tab: b,
        query: "needle".to_owned(),
    }));
    let _ = shell.update(in_window(Message::FinderFind {
        tab: b,
        direction: FindDirection::Up,
    }));
    assert_eq!(selected(&shell, b).as_deref(), Some("needle"), "found in b");
    assert_eq!(
        selected(&shell, a),
        None,
        "a, on the main window, untouched"
    );

    // The main window's bar kept its own text, and searches its own tab.
    assert!(bar_shown(shell.view()));
    let _ = shell.update(Message::FinderFind {
        tab: a,
        direction: FindDirection::Up,
    });
    assert_eq!(selected(&shell, a).as_deref(), Some("main"));

    // Escape in the window closes its bar alone; Ctrl+Shift+F again opens it, then closes it.
    let _ = shell.update(Message::Float(window, FloatEvent::Escape));
    assert!(!bar_shown(shell.window_view(window)));
    assert!(bar_shown(shell.view()), "the main window's still open");
    let _ = shell.update(Message::Float(window, FloatEvent::TerminalFind));
    let _ = shell.update(Message::Float(window, FloatEvent::TerminalFind));
    assert!(!bar_shown(shell.window_view(window)));
    let _ = shell.update(Message::Float(window, FloatEvent::TerminalFind));
    let _ = shell.update(in_window(Message::FinderClose(b)));
    assert!(!bar_shown(shell.window_view(window)));
    assert_ne!(
        heimdall_ui::finder::field_id(a),
        heimdall_ui::finder::field_id(b),
        "each bar's field its own"
    );
}

#[test]
fn the_window_lets_through_the_search_of_its_own_tab_only() {
    use heimdall_term::FindDirection;

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, a, b, window) = searchable_pair(dir.path(), ["needle\r\n", "needle\r\n"]);
    let finder_messages = |tab: TabId| {
        [
            Message::FinderQuery {
                tab,
                query: "needle".to_owned(),
            },
            Message::FinderFind {
                tab,
                direction: FindDirection::Up,
            },
            Message::FinderClose(tab),
        ]
    };
    {
        let floating = shell.app().tab(b).expect("detached");
        for message in finder_messages(b) {
            assert!(floating_message_allowed(&message, floating), "{message:?}");
        }
        for message in finder_messages(a) {
            assert!(!floating_message_allowed(&message, floating), "{message:?}");
        }
    }
    // The main window's bar open over `a`: a message of the window naming `a` reaches it not.
    let _ = shell.update(Message::Shortcut(WindowShortcut::Find));
    for message in finder_messages(a) {
        let _ = shell.update(Message::InFloating(window, Box::new(message)));
    }
    assert_eq!(selected(&shell, a), None, "nothing searched in a");
    assert!(bar_shown(shell.view()), "the main window's bar still open");
    assert!(
        !bar_shown(shell.window_view(window)),
        "none opened in the window"
    );
}

/// An SFTP server that answers the start, then nothing.
async fn idle_client() -> RemoteSession {
    let (client_end, mut server) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let mut length = [0; 4];
        server.read_exact(&mut length).await.expect("init length");
        let mut body = vec![0; u32::from_be_bytes(length) as usize];
        server.read_exact(&mut body).await.expect("init");
        assert!(matches!(Request::decode(&body), Ok(Request::Init { .. })));
        let version = Response::Version {
            version: SFTP_VERSION,
            extensions: Vec::new(),
        };
        server.write_all(&version.encode()).await.expect("version");
        std::future::pending::<()>().await;
    });
    RemoteSession::Sftp(
        SftpClient::start(client_end, ClientConfig::default())
            .await
            .expect("started"),
    )
}

/// A file of the server's folder.
fn remote_file(name: &str) -> RemoteEntry {
    RemoteEntry {
        name: name.as_bytes().to_vec(),
        label: name.to_owned(),
        kind: EntryKind::File,
        size: Some(4),
        modified: None,
        permissions: None,
        owner: None,
        group: None,
        inode: None,
    }
}

/// Files tab `floating`, detached to `window`, and Files tab `shown`, shown in the main
/// window; both connected, their server's folder listing two files, its pane with the
/// keyboard.
struct TwoFiles {
    shell: Shell,
    floating: TabId,
    shown: TabId,
    window: window::Id,
}

async fn two_files(dir: &Path) -> TwoFiles {
    let mut core = app(dir);
    let mut tabs = Vec::new();
    for id in ["a", "b"] {
        let (tab, attempt) = open_as(&mut core, AppMessage::OpenFiles(ProfileId::new(id)));
        core.update(AppMessage::Connection {
            tab,
            attempt,
            event: ConnectionEvent::FilesReady {
                client: idle_client().await,
                shell: None,
            },
        });
        core.update(AppMessage::Files(FilesMessage::RemoteListed {
            tab,
            result: Ok((
                RemotePath::from("/srv"),
                vec![remote_file("notes.txt"), remote_file("old.txt")],
            )),
        }));
        core.update(AppMessage::Files(FilesMessage::Key {
            tab,
            key: FilesKey::Focus(Side::Remote),
        }));
        tabs.push(tab);
    }
    let (floating, shown) = (tabs[0], tabs[1]);
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::App(AppMessage::Float(FloatMessage::Detach(
        floating,
    ))));
    let window = shell.floating_window(floating).expect("its own window");
    assert_eq!(shell.app().active, Some(shown));
    TwoFiles {
        shell,
        floating,
        shown,
        window,
    }
}

/// The entries of the server's pane of `tab` chosen, and the one selected.
fn chosen_files(shell: &Shell, tab: TabId) -> (Vec<usize>, Option<usize>) {
    shell
        .app()
        .tab(tab)
        .and_then(|found| found.files.as_deref())
        .map(|files| (files.remote.chosen(), files.remote.selected))
        .unwrap_or_default()
}

#[tokio::test]
async fn a_detached_files_tab_draws_its_fields_by_ids_of_its_own() {
    let dir = tempfile::tempdir().expect("dir");
    let TwoFiles {
        mut shell,
        floating,
        shown,
        window,
    } = two_files(dir.path()).await;
    let path = |tab| field_id(tab, Side::Remote, PaneField::Path);
    let filter = |tab| field_id(tab, Side::Remote, PaneField::Filter);
    // Alt+D in its window: its own path bar typed in, the one the focus is given to.
    let _ = shell.update(Message::Float(
        window,
        FloatEvent::FilesKey(FilesKey::FocusPath),
    ));
    {
        let mut ui = simulator(shell.window_view(window));
        ui.find(path(floating)).expect("its path bar, to type in");
        ui.find(filter(floating)).expect("its filter");
        ui.find(list_id(floating, Side::Remote)).expect("its list");
        for id in [path(shown), filter(shown), list_id(shown, Side::Remote)] {
            assert!(ui.find(id).is_err(), "nothing answers for the main tab");
        }
    }
    {
        let mut main = common::simulator(settings(), TALL_WINDOW, shell.view());
        main.find(filter(shown)).expect("the main tab's filter");
        main.find(list_id(shown, Side::Remote))
            .expect("the main tab's list");
        for id in [
            path(floating),
            filter(floating),
            list_id(floating, Side::Remote),
            path(shown),
        ] {
            assert!(
                main.find(id).is_err(),
                "nothing answers for the detached tab"
            );
        }
    }
    // Escape in the main window is its own tab's: the window's path bar stays typed in.
    let _ = shell.update(Message::DialogKey { confirm: false });
    simulator(shell.window_view(window))
        .find(path(floating))
        .expect("still typed in");
    // Escape in the window gives its folders back.
    let _ = shell.update(Message::Float(window, FloatEvent::Escape));
    assert!(
        simulator(shell.window_view(window))
            .find(path(floating))
            .is_err()
    );
}

#[tokio::test]
async fn the_windows_files_keys_drops_and_clicks_reach_its_tab_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let TwoFiles {
        mut shell,
        floating,
        shown,
        window,
    } = two_files(dir.path()).await;
    let before = chosen_files(&shell, shown);
    let _ = shell.update(Message::Float(window, FloatEvent::FilesKey(FilesKey::Last)));
    assert_eq!(chosen_files(&shell, floating), (vec![1], Some(1)));
    assert_eq!(
        chosen_files(&shell, shown),
        before,
        "the main tab untouched"
    );
    assert_eq!(shell.app().active, Some(shown));
    // The main window's keys stay its own tab's.
    let _ = shell.update(Message::FilesKey(FilesKey::First));
    assert_eq!(chosen_files(&shell, shown).1, Some(0));
    assert_eq!(chosen_files(&shell, floating).1, Some(1));

    // Ctrl held over the window: a click adds to its selection, as in the C# list.
    let _ = shell.update(Message::Float(
        window,
        FloatEvent::Modifiers(iced::keyboard::Modifiers::CTRL),
    ));
    let _ = shell.update(Message::InFloating(
        window,
        Box::new(Message::App(AppMessage::Files(FilesMessage::Select {
            tab: floating,
            side: Side::Remote,
            index: 0,
        }))),
    ));
    assert_eq!(chosen_files(&shell, floating).0, [0, 1]);
    let _ = shell.update(Message::Float(
        window,
        FloatEvent::Modifiers(iced::keyboard::Modifiers::empty()),
    ));

    // Files dragged from Explorer over the window: said there alone, and dropped there.
    let _ = shell.update(Message::Float(window, FloatEvent::FilesHovered(true)));
    simulator(shell.window_view(window))
        .find("Drop files to upload")
        .expect("said over its tab");
    assert!(
        common::simulator(settings(), TALL_WINDOW, shell.view())
            .find("Drop files to upload")
            .is_err(),
        "not over the main window"
    );
    let outside = tempfile::tempdir().expect("dir");
    let file = outside.path().join("report.pdf");
    std::fs::write(&file, b"12345").expect("written");
    let _ = shell.update(Message::Float(window, FloatEvent::FileDropped(file)));
    let transfers = |shell: &Shell, tab| {
        shell
            .app()
            .tab(tab)
            .and_then(|found| found.files.as_deref())
            .map_or(0, |files| files.transfers.len())
    };
    assert_eq!(transfers(&shell, floating), 1, "sent to its server");
    assert_eq!(transfers(&shell, shown), 0);
    assert!(
        simulator(shell.window_view(window))
            .find("Drop files to upload")
            .is_err(),
        "dropped: no longer said"
    );
    assert_eq!(shell.app().active, Some(shown));
    assert!(shell.app().floating_invariant_holds());
}

#[tokio::test]
async fn a_files_menu_opened_in_the_window_is_drawn_and_chosen_there() {
    let dir = tempfile::tempdir().expect("dir");
    let TwoFiles {
        mut shell,
        floating,
        window,
        ..
    } = two_files(dir.path()).await;
    let _ = shell.update(Message::InFloating(
        window,
        Box::new(Message::OpenTreeMenu(TreeMenu::FilesEntry {
            tab: floating,
            side: Side::Remote,
            index: Some(0),
        })),
    ));
    assert!(
        common::simulator(settings(), TALL_WINDOW, shell.view())
            .find("Copy path")
            .is_err(),
        "not in the main window"
    );
    {
        let mut ui = simulator(shell.window_view(window));
        ui.click("Copy path").expect("in its window");
        assert!(ui.into_messages().any(|message| matches!(
            &message,
            Message::InFloating(from, inner) if *from == window
                && matches!(
                    **inner,
                    Message::MenuChoice(AppMessage::Files(FilesMessage::CopyPath {
                        tab,
                        side: Side::Remote,
                    })) if tab == floating
                )
        )));
    }
    // A click beside it, in its window, closes it; Escape there too.
    let _ = shell.update(Message::InFloating(
        window,
        Box::new(Message::CloseTreeMenu),
    ));
    assert!(
        simulator(shell.window_view(window))
            .find("Copy path")
            .is_err()
    );
    let _ = shell.update(Message::InFloating(
        window,
        Box::new(Message::OpenTreeMenu(TreeMenu::FilesEntry {
            tab: floating,
            side: Side::Remote,
            index: Some(0),
        })),
    ));
    let _ = shell.update(Message::Float(window, FloatEvent::Escape));
    assert!(
        simulator(shell.window_view(window))
            .find("Copy path")
            .is_err()
    );
}

/// Every message a Files pane, its menus and its integrated editor send for `tab`, drawn
/// in a tab's own window, as the view code sends them.
fn files_messages(tab: TabId, editor: EditorId) -> Vec<Message> {
    let mut messages = pane_messages(tab);
    messages.extend(menu_messages(tab));
    messages.extend(window_messages(tab, editor));
    messages
}

/// What a Files pane's lists, buttons, fields, transfers and external edits send for `tab`.
fn pane_messages(tab: TabId) -> Vec<Message> {
    let pane = |message| Message::App(AppMessage::Files(message));
    let side = Side::Remote;
    let local = std::path::PathBuf::from("edited.txt");
    let transfer = heimdall_app::files::TransferId::fresh();
    vec![
        pane(FilesMessage::Select {
            tab,
            side,
            index: 0,
        }),
        pane(FilesMessage::SortBy {
            tab,
            side,
            column: SortColumn::Name,
        }),
        pane(FilesMessage::Back { tab, side }),
        pane(FilesMessage::Up { tab, side }),
        pane(FilesMessage::Home { tab, side }),
        pane(FilesMessage::Ascend {
            tab,
            side,
            levels: 1,
        }),
        pane(FilesMessage::PathEdited {
            tab,
            side,
            text: "/srv".to_owned(),
        }),
        pane(FilesMessage::GoTo { tab, side }),
        pane(FilesMessage::Refresh { tab, side }),
        pane(FilesMessage::Filter {
            tab,
            side,
            text: "notes".to_owned(),
        }),
        pane(FilesMessage::ToggleHidden { tab, side }),
        pane(FilesMessage::AskNewFolder { tab, side }),
        pane(FilesMessage::AskRename { tab, side }),
        pane(FilesMessage::AskDelete { tab, side }),
        pane(FilesMessage::Bookmark { tab }),
        pane(FilesMessage::ToggleFollow { tab }),
        pane(FilesMessage::ToggleSudo { tab }),
        pane(FilesMessage::ToggleLocal { tab }),
        pane(FilesMessage::Transfer {
            tab,
            direction: Direction::Download,
        }),
        pane(FilesMessage::Cancel { tab, id: transfer }),
        pane(FilesMessage::Retry { tab, id: transfer }),
        pane(FilesMessage::ClearFinished { tab }),
        pane(FilesMessage::StopBatch { tab }),
        pane(FilesMessage::EditSaveWithSudo {
            tab,
            local: local.clone(),
        }),
        pane(FilesMessage::EditSendAnyway {
            tab,
            local: local.clone(),
        }),
        pane(FilesMessage::EditOpenFolder {
            tab,
            local: local.clone(),
        }),
        pane(FilesMessage::EditStop { tab, local }),
    ]
}

/// What the entries of a Files pane's menus send for `tab`.
fn menu_messages(tab: TabId) -> Vec<Message> {
    let menu = |message| Message::MenuChoice(AppMessage::Files(message));
    let side = Side::Remote;
    vec![
        menu(FilesMessage::Open {
            tab,
            side,
            index: 0,
        }),
        menu(FilesMessage::EditIntegrated { tab }),
        menu(FilesMessage::EditExternal { tab }),
        menu(FilesMessage::EditWithSudo { tab }),
        menu(FilesMessage::Transfer {
            tab,
            direction: Direction::Download,
        }),
        menu(FilesMessage::AskRename { tab, side }),
        menu(FilesMessage::AskDelete { tab, side }),
        menu(FilesMessage::AskPermissions { tab, side }),
        menu(FilesMessage::UploadHere { tab }),
        menu(FilesMessage::PasteFromExplorer { tab }),
        menu(FilesMessage::Cut { tab }),
        menu(FilesMessage::Copy { tab }),
        menu(FilesMessage::Paste { tab }),
        menu(FilesMessage::Duplicate { tab }),
        menu(FilesMessage::CopyPath { tab, side }),
        menu(FilesMessage::ShowProperties { tab, side }),
        menu(FilesMessage::AskNewFolder { tab, side }),
        menu(FilesMessage::Refresh { tab, side }),
        menu(FilesMessage::OpenInTerminal { tab }),
        menu(FilesMessage::OpenInExplorer { tab }),
        menu(FilesMessage::OpenBookmark { tab, index: 0 }),
        menu(FilesMessage::RemoveBookmark { tab, index: 0 }),
    ]
}

/// What the window's own messages about a Files pane of `tab` and its editor `editor` are:
/// its editor's, its path bar's, its columns', its menus' and the pointer over it.
fn window_messages(tab: TabId, editor: EditorId) -> Vec<Message> {
    let side = Side::Remote;
    let spot = Spot {
        tab,
        side,
        index: Some(0),
    };
    vec![
        Message::Editor(EditorMessage::Action {
            tab,
            id: editor,
            action: iced::widget::text_editor::Action::SelectAll,
        }),
        Message::Editor(EditorMessage::Key {
            tab,
            id: editor,
            key: EditorKey::Save,
        }),
        Message::EditPath { tab, side },
        Message::FileColumns {
            tab,
            side,
            widths: ColumnWidths::default(),
        },
        Message::OpenTreeMenu(TreeMenu::FilesEntry {
            tab,
            side,
            index: Some(0),
        }),
        Message::OpenTreeMenu(TreeMenu::FilesBookmarks(tab)),
        Message::OpenTreeMenu(TreeMenu::FilesBookmarksRemove(tab)),
        Message::FilesHover(spot),
        Message::FilesHoverLeft(spot),
    ]
}

#[tokio::test]
async fn what_a_files_tab_in_the_window_sends_is_let_through_for_it_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let TwoFiles {
        mut shell,
        floating,
        shown,
        window,
    } = two_files(dir.path()).await;
    // Its integrated editor opened from its menu, in its window.
    let _ = shell.update(Message::Float(
        window,
        FloatEvent::FilesKey(FilesKey::First),
    ));
    let _ = shell.update(Message::InFloating(
        window,
        Box::new(Message::MenuChoice(AppMessage::Files(
            FilesMessage::EditIntegrated { tab: floating },
        ))),
    ));
    let editor = shell
        .app()
        .tab(floating)
        .and_then(|tab| tab.files.as_deref())
        .and_then(|files| files.editor.as_ref())
        .map(|edit| edit.id)
        .expect("its editor opened");
    let (detached, main) = (
        shell.app().tab(floating).expect("detached"),
        shell.app().tab(shown).expect("shown"),
    );
    for message in files_messages(floating, editor) {
        assert!(
            floating_message_allowed(&message, detached),
            "let through: {message:?}"
        );
        assert!(
            !floating_message_allowed(&message, main),
            "naming another tab: {message:?}"
        );
    }
    for message in files_messages(shown, editor) {
        assert!(
            !floating_message_allowed(&message, detached),
            "naming the main window's tab: {message:?}"
        );
    }
    // Naming it, but not sent by its view: keys and drops come from its window, typed;
    // an editor's save from the window's editors; a pane's message is no menu's.
    for message in [
        Message::App(AppMessage::Files(FilesMessage::Key {
            tab: floating,
            key: FilesKey::Next,
        })),
        Message::App(AppMessage::Files(FilesMessage::Dropped {
            tab: floating,
            path: std::path::PathBuf::from("dropped.txt"),
        })),
        Message::App(AppMessage::Files(FilesMessage::EditorSave {
            tab: floating,
            id: editor,
            text: String::new(),
            overwrite: true,
        })),
        Message::App(AppMessage::Files(FilesMessage::Duplicate { tab: floating })),
        Message::MenuChoice(AppMessage::Files(FilesMessage::ToggleLocal {
            tab: floating,
        })),
        Message::OpenTreeMenu(TreeMenu::Tab(floating)),
        Message::FilesKey(FilesKey::Next),
    ] {
        assert!(
            !floating_message_allowed(&message, detached),
            "dropped: {message:?}"
        );
    }
}

/// Opens the menu of the first entry of `tab`'s server pane in its own window `window`.
fn open_entry_menu(shell: &mut Shell, window: window::Id, tab: TabId) {
    let _ = shell.update(Message::InFloating(
        window,
        Box::new(Message::OpenTreeMenu(TreeMenu::FilesEntry {
            tab,
            side: Side::Remote,
            index: Some(0),
        })),
    ));
    simulator(shell.window_view(window))
        .find("Copy path")
        .expect("open in its window");
}

#[tokio::test]
async fn a_menu_of_the_window_goes_with_it_when_its_tab_comes_back() {
    let dir = tempfile::tempdir().expect("dir");
    let TwoFiles {
        mut shell,
        floating,
        window,
        ..
    } = two_files(dir.path()).await;
    // Reattach: the tab shown in the main window, its menu not drawn there.
    open_entry_menu(&mut shell, window, floating);
    let key = shell.app().floating_of(floating).expect("detached");
    let _ = shell.update(Message::App(AppMessage::Float(FloatMessage::Reattach(key))));
    assert_eq!(
        shell.app().active,
        Some(floating),
        "shown in the main window"
    );
    assert!(
        common::simulator(settings(), TALL_WINDOW, shell.view())
            .find("Copy path")
            .is_err(),
        "no menu of the closed window"
    );

    // Its window's close button: back on the strip first, the menu gone too.
    let _ = shell.update(Message::App(AppMessage::Float(FloatMessage::Detach(
        floating,
    ))));
    let window = shell
        .floating_window(floating)
        .expect("its own window again");
    open_entry_menu(&mut shell, window, floating);
    let _ = shell.update(Message::Float(window, FloatEvent::CloseRequested));
    assert!(!shell.app().is_floating(floating));
    assert!(
        common::simulator(settings(), TALL_WINDOW, shell.view())
            .find("Copy path")
            .is_err(),
        "no menu of the closed window"
    );
}
