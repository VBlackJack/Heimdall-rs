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

//! The profile form drawn headless: reached from the sidebar, typed into, refused with a
//! reason. Setting `HEIMDALL_SNAPSHOT_DIR` writes PNGs, for a visual pass.

use std::path::Path;

use heimdall_app::profile_draft::ProfileField;
use heimdall_app::{App, AppConfig, Message as AppMessage};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};
use iced_test::simulator::Simulator;

const WINDOW: Size = Size::new(1200.0, 720.0);

const SNAPSHOT_VARIABLE: &str = "HEIMDALL_SNAPSHOT_DIR";

fn shell(dir: &Path) -> Shell {
    Shell::with_app(App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    }))
}

fn simulator(shell: &Shell) -> Simulator<'_, Message> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    Simulator::with_size(settings, WINDOW, shell.view())
}

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

fn app(message: AppMessage) -> Message {
    Message::App(message)
}

#[test]
fn the_sidebar_opens_an_empty_form_and_typing_reaches_its_field() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    {
        let mut ui = simulator(&shell);
        ui.click("New profile").expect("button");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::NewProfile)))
        );
    }
    let _ = shell.update(app(AppMessage::NewProfile));
    snapshot(&shell, "profile-new.png");
    let mut ui = simulator(&shell);
    for label in [
        "Name",
        "Group",
        "Server address",
        "Port",
        "User name",
        "Private key file",
        "Save",
    ] {
        ui.find(label).expect(label);
    }
    assert!(
        ui.find("Delete this profile").is_err(),
        "a new profile has nothing to delete"
    );
    ui.click("server.example.org").expect("host field");
    ui.typewrite("w");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::ProfileField { field: ProfileField::Host, value }) if value == "w"
    )));
}

#[test]
fn a_saved_profile_has_an_edit_button_and_a_refused_form_says_why() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    for (field, value) in [(ProfileField::Name, "web"), (ProfileField::Host, "web")] {
        let _ = shell.update(app(AppMessage::ProfileField {
            field,
            value: value.to_owned(),
        }));
    }
    let _ = shell.update(app(AppMessage::ConfirmDialog));
    let id = shell.app().profiles()[0].id.clone();
    {
        let mut ui = simulator(&shell);
        ui.click("Edit").expect("edit button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::EditProfile(edited)) if edited == id
        )));
    }
    let _ = shell.update(app(AppMessage::EditProfile(id)));
    let _ = shell.update(app(AppMessage::ProfileField {
        field: ProfileField::Port,
        value: "99999".to_owned(),
    }));
    let _ = shell.update(app(AppMessage::ConfirmDialog));
    snapshot(&shell, "profile-refused.png");
    let mut ui = simulator(&shell);
    ui.find("Edit profile").expect("title");
    ui.find("The port is a number from 1 to 65535.")
        .expect("reason");
    ui.find("Delete this profile").expect("delete button");
}
