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

use heimdall_app::profile_draft::{DraftProtocol, ProfileField, ProfileToggle};
use heimdall_app::{App, AppConfig, Message as AppMessage};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::tree_view::TreeMenu;
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
fn the_add_menu_opens_an_empty_form_and_typing_reaches_its_field() {
    let dir = tempfile::tempdir().expect("dir");
    // One session saved: the window's welcome, and its own Add Session, are gone.
    let mut store =
        heimdall_core::store::ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    store.merge([heimdall_core::profile::SshProfile {
        id: heimdall_core::profile::ProfileId::new("saved"),
        name: "saved".to_owned(),
        group: None,
        host: "saved.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
    }]);
    store.save().expect("save");
    let mut shell = shell(dir.path());
    {
        let mut ui = simulator(&shell);
        ui.click("+").expect("add button");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::OpenTreeMenu(TreeMenu::Add)))
        );
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Add));
    {
        let mut ui = simulator(&shell);
        ui.click("Add Session").expect("menu entry");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::MenuChoice(AppMessage::NewProfile)))
        );
    }
    let _ = shell.update(Message::MenuChoice(AppMessage::NewProfile));
    // As in C#: the protocol first.
    {
        let mut ui = simulator(&shell);
        snapshot(&shell, "profile-protocols.png");
        ui.find("Choose a protocol").expect("picker");
        ui.find("Windows remote desktop session").expect("RDP card");
        ui.click("Secure shell terminal").expect("SSH card");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::ChooseProtocol(DraftProtocol::Ssh))
        )));
    }
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
    snapshot(&shell, "profile-new.png");
    let mut ui = simulator(&shell);
    for label in [
        "Add Session",
        "Connection basics",
        "Display name *",
        "Server *",
        "Remote SSH port",
        "SSH credentials",
        "Username",
        "SSH key",
        "Password",
        "Folder",
        "Save",
    ] {
        ui.find(label).expect(label);
    }
    assert!(ui.find("22").is_ok(), "the default port is written in");
    ui.click("server.example.org").expect("host field");
    ui.typewrite("w");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::ProfileField { field: ProfileField::Host, value }) if value == "w"
    )));
}

#[test]
fn a_saved_profile_is_edited_from_its_menu_and_a_refused_form_says_why() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
    for (field, value) in [(ProfileField::Name, "web"), (ProfileField::Host, "web")] {
        let _ = shell.update(app(AppMessage::ProfileField {
            field,
            value: value.to_owned(),
        }));
    }
    let _ = shell.update(app(AppMessage::ConfirmDialog));
    let id = shell.app().profiles()[0].id.clone();
    // As in the C# tree: right click, then Edit.
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Profile(id.clone())));
    {
        let mut ui = simulator(&shell);
        ui.click("Edit").expect("edit entry");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::MenuChoice(AppMessage::EditProfile(edited)) if edited == id
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
    ui.find("Edit Session").expect("title");
    ui.find("The port is a number from 1 to 65535.")
        .expect("reason");
    assert!(
        ui.find("Delete this profile").is_err(),
        "as in C#, a profile is deleted from its menu"
    );
}

#[test]
fn the_rdp_and_winrm_forms_show_the_csharp_cards() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Rdp)));
    snapshot(&shell, "profile-rdp.png");
    {
        let mut ui = simulator(&shell);
        for label in [
            "Remote Desktop",
            "Remote RDP port",
            "RDP credentials",
            "Windows domain",
            "Password",
            "RDP session options",
            "Redirect clipboard",
            "Enable Network Level Authentication",
        ] {
            ui.find(label).expect(label);
        }
        assert!(ui.find("SSH key").is_err(), "an SSH field");
        ui.click("Enable Network Level Authentication")
            .expect("nla box");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::ProfileToggle {
                toggle: ProfileToggle::Nla,
                on: false
            })
        )));
    }
    let _ = shell.update(app(AppMessage::ProfileToggle {
        toggle: ProfileToggle::Nla,
        on: false,
    }));
    {
        let mut ui = simulator(&shell);
        ui.find(
            "Without Network Level Authentication, a saved password is not sent: Heimdall asks for it.",
        )
        .expect("says what clearing NLA costs");
    }

    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::WinRm)));
    {
        let mut ui = simulator(&shell);
        for label in ["WinRM port", "WinRM credentials", "Identity", "Use SSL"] {
            ui.find(label).expect(label);
        }
        assert!(
            ui.find("Username").is_err(),
            "the current identity names no account"
        );
        assert!(ui.find("Password").is_err(), "PowerShell asks for it");
    }
    let _ = shell.update(app(AppMessage::ProfileToggle {
        toggle: ProfileToggle::StoredCredential,
        on: true,
    }));
    let mut ui = simulator(&shell);
    ui.find("Username")
        .expect("a stored credential names its account");
}

#[test]
fn a_gateway_is_added_from_the_form_and_the_tree_says_via_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
    snapshot(&shell, "profile-network.png");
    {
        let mut ui = simulator(&shell);
        for label in [
            "Gateway routing",
            "Connect directly without an SSH gateway",
            "No gateways configured",
        ] {
            ui.find(label).expect(label);
        }
        ui.click("Add Gateway").expect("add gateway");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::NewGateway)))
        );
    }
    let _ = shell.update(app(AppMessage::NewGateway));
    snapshot(&shell, "gateway-new.png");
    {
        let mut ui = simulator(&shell);
        for label in [
            "Add SSH Gateway",
            "Name",
            "Host",
            "Port",
            "Username",
            "Key Path",
            "Password",
            "Parent Gateway",
        ] {
            ui.find(label).expect(label);
        }
    }
    for (field, value) in [
        (ProfileField::Name, "bastion"),
        (ProfileField::Host, "bastion.lab"),
        (ProfileField::Username, "jump"),
    ] {
        let _ = shell.update(app(AppMessage::GatewayField {
            field,
            value: value.to_owned(),
        }));
    }
    let _ = shell.update(Message::SaveGatewayForm);
    for (field, value) in [(ProfileField::Name, "web"), (ProfileField::Host, "web.lab")] {
        let _ = shell.update(app(AppMessage::ProfileField {
            field,
            value: value.to_owned(),
        }));
    }
    let _ = shell.update(Message::SaveProfileForm);
    assert!(shell.app().dialog.is_none(), "{:?}", shell.app().dialog);
    let mut ui = simulator(&shell);
    ui.find("via bastion").expect("the tree's badge");
}

#[test]
fn a_form_taller_than_the_window_scrolls_above_buttons_that_stay_in_view() {
    // Shorter than an SSH form routed through a gateway.
    const SHORT_WINDOW: Size = Size::new(1200.0, 560.0);
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
    let _ = shell.update(app(AppMessage::NewGateway));
    for (field, value) in [
        (ProfileField::Name, "bastion"),
        (ProfileField::Host, "bastion.lab"),
        (ProfileField::Username, "jump"),
    ] {
        let _ = shell.update(app(AppMessage::GatewayField {
            field,
            value: value.to_owned(),
        }));
    }
    let _ = shell.update(Message::SaveGatewayForm);
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    let mut ui = Simulator::with_size(settings, SHORT_WINDOW, shell.view());
    let folder = ui.find("Folder").expect("the last field");
    assert!(
        folder.bounds().y + folder.bounds().height > SHORT_WINDOW.height,
        "the form is taller than the window: {:?}",
        folder.bounds()
    );
    for button in ["Cancel", "Save"] {
        let found = ui.find(button).expect(button);
        assert!(
            found.bounds().y + found.bounds().height <= SHORT_WINDOW.height,
            "{button} is cut off at {:?}",
            found.bounds()
        );
    }
    ui.click("Save").expect("save button");
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::SaveProfileForm))
    );
}
