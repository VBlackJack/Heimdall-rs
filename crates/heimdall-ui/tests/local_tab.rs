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

//! Local shells from the window, drawn headless: the sidebar button opens the user's own
//! shell, and a saved local profile shows what it runs before it runs it.

mod common;

use std::path::Path;

use heimdall_app::{App, AppConfig, Dialog, Message as AppMessage};
use heimdall_core::profile::{LocalArguments, LocalCommand, LocalProfile, ProfileId};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_term::local::LocalArguments as TermArguments;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};

const WINDOW: Size = Size::new(1200.0, 720.0);

/// A program found on every machine the tests run on, by its full path.
#[cfg(unix)]
const PROGRAM: &str = "/bin/sh";
#[cfg(windows)]
const PROGRAM: &str = r"C:\Windows\System32\cmd.exe";

fn app(dir: &Path, local: Option<LocalProfile>) -> App {
    let profiles_file = dir.join("profiles.toml");
    if let Some(profile) = local {
        let mut store = ProfileStore::open(&profiles_file).expect("store");
        store.merge_local([profile]);
        store.save().expect("save");
    }
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

fn settings() -> Settings {
    Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    }
}

/// A saved profile running [`PROGRAM`] with an argument holding a right-to-left override.
fn tool() -> LocalProfile {
    LocalProfile {
        id: ProfileId::new("tool"),
        name: "Build tool".to_owned(),
        group: Some("Tools".to_owned()),
        command: LocalCommand {
            program: Some(PROGRAM.to_owned()),
            arguments: LocalArguments::List(vec!["run\u{202E}txt.exe".to_owned()]),
            working_directory: None,
            run_as_administrator: false,
        },
        approved: None,
        session_logging: None,
    }
}

#[test]
fn the_sidebar_opens_the_default_local_shell() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = Shell::with_app(app(dir.path(), None));
    let mut ui = common::simulator(settings(), WINDOW, shell.view());
    ui.click("Local shell").expect("button");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::OpenLocal(shell))
            if shell.program.is_none()
                && shell.arguments == TermArguments::default()
                && shell.name == "Local shell"
    )));
}

#[test]
fn a_saved_local_profile_is_listed_and_opens_by_its_id() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path(), Some(tool())));
    // As in the C# tree: a click selects, a double click connects.
    let messages = common::double_click_messages(
        || common::simulator(settings(), WINDOW, shell.view()),
        "Build tool",
    );
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::TreeClick(id) if id.as_str() == "tool"
    )));
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::ConnectProfile(id)) if id.as_str() == "tool"
        )),
        "a double click connects"
    );
    // The tree shows the protocol as its icon; the details of the session selected name it.
    let _ = shell.update(Message::TreeClick(ProfileId::new("tool")));
    common::simulator(settings(), WINDOW, shell.view())
        .find("Local")
        .expect("protocol");
}

#[test]
fn an_unapproved_command_is_shown_whole_and_enter_does_not_run_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path(), Some(tool())));
    let _ = shell.update(Message::App(AppMessage::OpenLocalProfile(ProfileId::new(
        "tool",
    ))));
    assert!(
        matches!(shell.app().dialog, Some(Dialog::ConfirmLocalCommand(_))),
        "{:?}",
        shell.app().dialog
    );
    assert!(shell.app().tabs.is_empty(), "nothing runs before agreeing");
    {
        let mut ui = common::simulator(settings(), WINDOW, shell.view());
        ui.find("Run this program?").expect("title");
        // The override is written out, so the argument reads as what it is.
        #[cfg(unix)]
        let command = r"/bin/sh 'run\u{202E}txt.exe'";
        #[cfg(windows)]
        let command = r#""C:\Windows\System32\cmd.exe" run\u{202E}txt.exe"#;
        ui.find(command).expect("argument written out");
    }
    // Enter, as someone typing when the dialog appears would press it.
    let _ = shell.update(Message::DialogKey { confirm: true });
    assert!(
        matches!(shell.app().dialog, Some(Dialog::ConfirmLocalCommand(_))),
        "Enter does not agree"
    );
    assert!(shell.app().tabs.is_empty());
    let _ = shell.update(Message::DialogKey { confirm: false });
    assert!(shell.app().dialog.is_none(), "Escape still dismisses");
    assert!(shell.app().tabs.is_empty());
}
