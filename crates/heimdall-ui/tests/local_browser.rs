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

//! The file browser docked beside a local shell, drawn headless: this computer's files
//! alone, with no server pane, no toggle and nothing to send; its entries' menu offers the
//! C# "Open in Explorer" and "Open in terminal", a new shell, and nothing that would reach
//! a server or the shell beside it; a file that would run is asked about first; and the
//! Settings page docks it or not.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use heimdall_app::files::{EntryKind, LocalEntry, Side};
use heimdall_app::local_driver::LocalShell;
use heimdall_app::{
    App, AppConfig, ConnectionEvent, Effect, FilesMessage, InputSink, Message as AppMessage,
    SettingsMessage, TabId,
};
use heimdall_core::settings::SftpBrowser;
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::GridSize;
use heimdall_term::local::LocalArguments;
use heimdall_ui::shell::{Message, SettingsTab, Shell};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::tree_view::{FilesEntryFacts, FilesTabFacts};
use iced::{Settings, Size};

/// Size of the simulated window, in logical pixels: wide enough for two panes.
const WINDOW: Size = Size::new(1400.0, 720.0);

/// Size of the simulated Settings page: tall enough for the whole SSH tab.
const SETTINGS_WINDOW: Size = Size::new(1100.0, 2400.0);

#[derive(Debug, Default)]
struct Sink;

impl InputSink for Sink {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

fn app(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

/// A local shell started, its browser docked beside it and listed.
fn docked(dir: &Path) -> (App, TabId) {
    let mut core = app(dir);
    let (shell, attempt) = match core
        .update(AppMessage::OpenLocal(LocalShell {
            name: "Shell".to_owned(),
            program: None,
            arguments: LocalArguments::List(Vec::new()),
            working_directory: None,
            environment: Vec::new(),
        }))
        .as_slice()
    {
        [Effect::ConnectLocal { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    let effects = core.update(AppMessage::Connection {
        tab: shell,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(Sink),
        },
    });
    let [Effect::ListLocal { tab: pane, .. }] = effects.as_slice() else {
        panic!("the browser's listing: {effects:?}");
    };
    let pane = *pane;
    core.update(AppMessage::Files(FilesMessage::LocalListed {
        tab: pane,
        result: Ok((
            PathBuf::from(dir),
            vec![LocalEntry {
                name: "notes.md".into(),
                label: "notes.md".to_owned(),
                kind: EntryKind::File,
                size: Some(2048),
                modified: None,
            }],
        )),
    }));
    assert_eq!(core.active, Some(shell), "the keyboard on the shell");
    (core, pane)
}

fn settings() -> Settings {
    Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    }
}

#[test]
fn a_docked_local_browser_shows_this_computers_files_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, _) = docked(dir.path());
    let shell = Shell::with_app(core);
    let mut ui = common::simulator(settings(), WINDOW, shell.view());
    ui.find("This computer").expect("this computer's pane");
    ui.find("notes.md").expect("its files");
    for absent in ["Local files", "Upload", "Download"] {
        assert!(
            ui.find(absent).is_err(),
            "{absent}: no server pane beside it"
        );
    }
}

#[test]
fn the_browsers_entry_menu_opens_in_explorer_and_sends_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let (_core, pane) = docked(dir.path());
    let facts = FilesTabFacts {
        can_paste: false,
        can_copy: false,
        connected: false,
        sftp: false,
        over_ssh: false,
        local_only: true,
    };
    let file = FilesEntryFacts {
        index: 0,
        single: true,
        one_file: true,
        link: false,
    };
    let menu = || {
        common::simulator(
            settings(),
            WINDOW,
            heimdall_ui::tree_view::files_entry_menu((pane, Side::Local), Some(file), facts),
        )
    };
    let mut ui = menu();
    for label in [
        "Open",
        "Rename",
        "Delete",
        "Copy path",
        "New Folder",
        "Refresh",
    ] {
        ui.find(label).expect(label);
    }
    assert!(ui.find("Upload").is_err(), "nowhere to send it");
    for (label, message) in [
        (
            "Open in Explorer",
            FilesMessage::OpenInExplorer { tab: pane },
        ),
        (
            "Open in terminal",
            FilesMessage::OpenInTerminal { tab: pane },
        ),
    ] {
        let mut ui = menu();
        ui.click(label).expect("the C# entry");
        let chosen: Vec<String> = ui
            .into_messages()
            .filter_map(|message| match message {
                Message::MenuChoice(AppMessage::Files(files)) => Some(format!("{files:?}")),
                _ => None,
            })
            .collect();
        assert_eq!(chosen, [format!("{message:?}")], "{label}");
    }
    // On the folder shown too, nothing selected.
    let mut ui = common::simulator(
        settings(),
        WINDOW,
        heimdall_ui::tree_view::files_entry_menu((pane, Side::Local), None, facts),
    );
    ui.find("Open in terminal").expect("the folder shown");
}

#[test]
fn a_file_that_would_run_is_asked_about_with_its_full_path() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut core, pane) = docked(dir.path());
    let file = dir.path().join("setup.exe");
    std::fs::write(&file, b"MZ").expect("file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).expect("mode");
    }
    core.update(AppMessage::Files(FilesMessage::LocalListed {
        tab: pane,
        result: Ok((
            PathBuf::from(dir.path()),
            vec![LocalEntry {
                name: "setup.exe".into(),
                label: "setup.exe".to_owned(),
                kind: EntryKind::File,
                size: Some(2),
                modified: None,
            }],
        )),
    }));
    assert!(
        core.update(AppMessage::Files(FilesMessage::Open {
            tab: pane,
            side: Side::Local,
            index: 0,
        }))
        .is_empty(),
        "nothing opened before agreeing"
    );
    let shell = Shell::with_app(core);
    let mut ui = common::simulator(settings(), WINDOW, shell.view());
    ui.find("Open a program").expect("the question");
    ui.click("Open").expect("its button");
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog))),
        "agreed by a click"
    );
}

#[test]
fn a_file_or_folder_that_did_not_open_says_so_not_that_an_editor_failed() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut core, pane) = docked(dir.path());
    core.update(AppMessage::Files(FilesMessage::EditorLaunched {
        tab: pane,
        result: Err(heimdall_app::files::FilesError::OpenFailed {
            detail: "no handler".to_owned(),
        }),
    }));
    let shell = Shell::with_app(core);
    let mut ui = common::simulator(settings(), WINDOW, shell.view());
    ui.find("Could not open it on this computer: no handler")
        .expect("its own wording");
    assert!(
        ui.find("The external editor could not be started: no handler. Check the editor path in Settings.")
            .is_err(),
        "no editor involved"
    );
}

/// The settings a click on the card's `label` asks for.
fn clicked(shell: &Shell, label: &str) -> Vec<SftpBrowser> {
    let mut ui = common::simulator(settings(), SETTINGS_WINDOW, shell.view());
    ui.click(label).expect(label);
    ui.into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::Settings(SettingsMessage::SftpBrowser(sftp))) => Some(sftp),
            _ => None,
        })
        .collect()
}

#[test]
fn the_settings_card_docks_the_local_browser_or_not_whatever_the_sftp_browser() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(SettingsTab::Ssh));
    let label = "Dock a file browser beside local shells";
    let defaults = SftpBrowser::default();
    let off = SftpBrowser {
        dock_local_browser: false,
        ..defaults
    };
    let asked = clicked(&shell, label);
    assert_eq!(asked, [off]);
    for sftp in asked {
        let _ = shell.update(Message::App(AppMessage::Settings(
            SettingsMessage::SftpBrowser(sftp),
        )));
    }
    assert!(!shell.app().settings().sftp_browser.dock_local_browser);
    assert_eq!(clicked(&shell, label), [defaults], "on again");
    // The SFTP browser off leaves it: no server is involved.
    let sftp_off = SftpBrowser {
        enabled: false,
        ..defaults
    };
    let _ = shell.update(Message::App(AppMessage::Settings(
        SettingsMessage::SftpBrowser(sftp_off),
    )));
    assert_eq!(
        clicked(&shell, label),
        [SftpBrowser {
            dock_local_browser: false,
            ..sftp_off
        }]
    );
}
