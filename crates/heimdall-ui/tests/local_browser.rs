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
//! C# "Open in Explorer" and nothing that would reach a server or the shell.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use heimdall_app::files::{EntryKind, LocalEntry, Side};
use heimdall_app::local_driver::LocalShell;
use heimdall_app::{
    App, AppConfig, ConnectionEvent, Effect, FilesMessage, InputSink, Message as AppMessage, TabId,
};
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::GridSize;
use heimdall_term::local::LocalArguments;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::tree_view::{FilesEntryFacts, FilesTabFacts};
use iced::{Settings, Size};

/// Size of the simulated window, in logical pixels: wide enough for two panes.
const WINDOW: Size = Size::new(1400.0, 720.0);

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
    for label in ["Upload", "Open in terminal"] {
        assert!(ui.find(label).is_err(), "{label}");
    }
    let mut ui = menu();
    ui.click("Open in Explorer").expect("the C# entry");
    let chosen: Vec<String> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::MenuChoice(AppMessage::Files(files)) => Some(format!("{files:?}")),
            _ => None,
        })
        .collect();
    assert_eq!(
        chosen,
        [format!("{:?}", FilesMessage::OpenInExplorer { tab: pane })]
    );
}
