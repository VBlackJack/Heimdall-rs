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

mod common;

use std::path::Path;

use heimdall_app::profile_draft::{DraftProtocol, ProfileField, ProfileToggle};
use heimdall_app::{App, AppConfig, Message as AppMessage};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, ProfileTab, Shell, tree_add_id};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::tree_view::TreeMenu;
use iced::{Settings, Size};

const WINDOW: Size = Size::new(1200.0, 720.0);
/// Height of a window showing a whole form: RDP with its C# groups, or SSH with its
/// post-connect steps.
const TALL_HEIGHT: f32 = 3000.0;

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

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, WINDOW, shell.view())
}

/// A window tall enough for the whole RDP form: in [`WINDOW`] its last options scroll.
fn tall_simulator(shell: &Shell) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, Size::new(WINDOW.width, TALL_HEIGHT), shell.view())
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

/// Shows a tab of the profile form, as a click on its header does.
fn show(shell: &mut Shell, tab: ProfileTab) {
    let _ = shell.update(Message::ProfileTab(tab));
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
    }]);
    store.save().expect("save");
    let mut shell = shell(dir.path());
    {
        let mut ui = simulator(&shell);
        // A glyph alone, as the C#'s Add button: found by its identifier.
        ui.click(tree_add_id()).expect("add button");
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
        ui.find("Secure file transfer over SSH").expect("SFTP card");
        ui.find("Local terminal session").expect("Local card");
        ui.find("Classic file transfer").expect("FTP card");
        ui.click("Secure shell terminal").expect("SSH card");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::ChooseProtocol(DraftProtocol::Ssh))
        )));
    }
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
    snapshot(&shell, "profile-new.png");
    {
        let mut ui = simulator(&shell);
        for label in [
            "Add Session",
            "General",
            "Options",
            "Network",
            "Info",
            "Connection basics",
            "Display name *",
            "Server *",
            "Remote SSH port",
            "SSH credentials",
            "Username",
            "SSH key",
            "Password",
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
}

#[test]
fn the_info_tab_holds_the_folder_and_the_metadata_as_the_csharp_dialog() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
    {
        let mut ui = simulator(&shell);
        assert!(ui.find("Folder").is_err(), "not on the General tab");
        ui.click("Info").expect("the Info tab");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::ProfileTab(ProfileTab::Info)))
        );
    }
    show(&mut shell, ProfileTab::Info);
    let mut ui = simulator(&shell);
    for label in [
        "Organization",
        "Use grouping metadata to keep the session list organized.",
        "Folder",
        "Environment",
        "Metadata",
        "Tags",
        "MAC address",
    ] {
        ui.find(label).expect(label);
    }
    assert!(ui.find("Server *").is_err(), "the General tab hidden");
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
    // As in the C# tree: right click, then Edit. The Settings page behind, so that the
    // menu's Edit is the only one: the profile selected shows its own in the detail panel.
    let _ = shell.update(Message::ShowSettings);
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
        let mut ui = tall_simulator(&shell);
        for label in [
            "Remote Desktop",
            "Remote RDP port",
            "RDP credentials",
            "Windows domain",
            "Password",
        ] {
            ui.find(label).expect(label);
        }
        assert!(ui.find("SSH key").is_err(), "an SSH field");
    }
    show(&mut shell, ProfileTab::Options);
    {
        let mut ui = tall_simulator(&shell);
        for label in [
            "RDP session options",
            "Redirect clipboard",
            "Enable Network Level Authentication",
        ] {
            ui.find(label).expect(label);
        }
    }
    // The box is the global defaults' while the new form follows them.
    let _ = shell.update(app(AppMessage::ProfileToggle {
        toggle: ProfileToggle::FollowDefaults,
        on: false,
    }));
    {
        let mut ui = tall_simulator(&shell);
        common::reveal(&mut ui, "Enable Network Level Authentication");
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
        assert!(ui.find("Audio mode").is_err(), "an RDP list");
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
    {
        let mut ui = simulator(&shell);
        ui.find("Username")
            .expect("a stored credential names its account");
        ui.find("Uses WinRM over HTTPS, normally port 5986. HTTP normally uses port 5985.")
            .expect("the C# hint under Use SSL");
    }
    // TLS moved back by hand to the plaintext port: said, not corrected.
    let _ = shell.update(app(AppMessage::ProfileToggle {
        toggle: ProfileToggle::UseSsl,
        on: true,
    }));
    let _ = shell.update(app(AppMessage::ProfileField {
        field: ProfileField::Port,
        value: "5985".to_owned(),
    }));
    let mut ui = tall_simulator(&shell);
    ui.find(
        "TLS is enabled but the port is the plaintext default 5985; WinRM over TLS listens on 5986.",
    )
    .expect("says TLS is on the HTTP port");
}

#[test]
fn the_rdp_form_offers_the_sound_colours_and_administrative_session() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Rdp)));
    show(&mut shell, ProfileTab::Options);
    let mut ui = tall_simulator(&shell);
    // The lists themselves are tested in `rdp_options`: their value is not a text to find.
    for label in [
        "Audio mode",
        "Color depth",
        "Run as administrator session (/admin)",
    ] {
        ui.find(label).expect(label);
    }
    common::reveal(&mut ui, "Run as administrator session (/admin)");
    ui.click("Run as administrator session (/admin)")
        .expect("admin box");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::ProfileToggle {
            toggle: ProfileToggle::AdminSession,
            on: true
        })
    )));
}

#[test]
fn the_rdp_form_shows_the_resolution_card_and_the_fixed_size_fields_in_its_mode() {
    use heimdall_app::profile_draft::ProfileChoice;
    use heimdall_core::profile::Resolution;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Rdp)));
    show(&mut shell, ProfileTab::Options);
    {
        let mut ui = tall_simulator(&shell);
        for label in [
            "Resolution profile",
            "Resolution mode",
            "Allow dynamic resolution updates",
        ] {
            ui.find(label).expect(label);
        }
        assert!(ui.find("Width").is_err(), "fitting the window");
    }
    let _ = shell.update(app(AppMessage::ProfileChoice(ProfileChoice::Resolution(
        Resolution::Fixed,
    ))));
    let mut ui = tall_simulator(&shell);
    for label in ["Common resolutions", "Width", "Height"] {
        ui.find(label).expect(label);
    }
}

#[test]
fn a_gateway_is_added_from_the_form_and_the_tree_says_via_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
    show(&mut shell, ProfileTab::Network);
    snapshot(&shell, "profile-network.png");
    {
        let mut ui = tall_simulator(&shell);
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
    {
        let mut ui = simulator(&shell);
        ui.find("via bastion").expect("the tree's badge");
    }
    // A view choice of the filter menu, as the C# one: the row stays, its badge goes.
    let _ = shell.update(app(AppMessage::Filter(
        heimdall_app::FilterMessage::GatewayBadge,
    )));
    let mut ui = simulator(&shell);
    ui.find("web").expect("still listed");
    assert!(ui.find("via bastion").is_err(), "the badge hidden");
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
    let mut ui = common::simulator(settings, SHORT_WINDOW, shell.view());
    let password = ui.find("Password").expect("the General tab's last field");
    assert!(
        password.bounds().y + password.bounds().height > SHORT_WINDOW.height,
        "the form is taller than the window: {:?}",
        password.bounds()
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

#[test]
fn the_socks_card_shows_only_through_a_gateway_and_says_where_it_listens() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
    show(&mut shell, ProfileTab::Network);
    assert!(
        tall_simulator(&shell).find("SOCKS5 Proxy").is_err(),
        "no gateway"
    );
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
    {
        let mut ui = tall_simulator(&shell);
        for label in ["SOCKS5 Proxy", "Local port", "Disabled"] {
            ui.find(label).expect(label);
        }
    }
    let _ = shell.update(app(AppMessage::ProfileField {
        field: ProfileField::SocksPort,
        value: "1080".to_owned(),
    }));
    {
        let mut ui = tall_simulator(&shell);
        ui.find("127.0.0.1:1080").expect("where it listens");
        for label in [
            "Remote Port Forwarding",
            "Remote port (server)",
            "0 = same as remote port",
        ] {
            ui.find(label).expect(label);
        }
        // The remote forward is still off.
        ui.find("Disabled").expect("no remote forward");
    }
    let _ = shell.update(app(AppMessage::ProfileField {
        field: ProfileField::RemoteBindPort,
        value: "8080".to_owned(),
    }));
    tall_simulator(&shell)
        .find("server:8080 -> local:8080")
        .expect("the same port by default");
    let _ = shell.update(app(AppMessage::ProfileField {
        field: ProfileField::RemoteLocalPort,
        value: "3000".to_owned(),
    }));
    let mut ui = tall_simulator(&shell);
    ui.find("server:8080 -> local:3000").expect("where it goes");
    assert!(ui.find("Disabled").is_err());
}

#[test]
fn an_ssh_form_lists_its_post_connect_steps_as_the_csharp_card() {
    use heimdall_app::steps_draft::StepEdit;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
    show(&mut shell, ProfileTab::Options);
    {
        let mut ui = tall_simulator(&shell);
        for label in [
            "Post-connect sequence",
            "No steps yet. Add a step to send commands automatically once this session is connected.",
        ] {
            ui.find(label).expect(label);
        }
        // Nothing selected: Remove does nothing.
        common::reveal(&mut ui, "Remove");
        ui.click("Remove").expect("remove");
        ui.click("Add").expect("add");
        let messages: Vec<_> = ui.into_messages().collect();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::App(AppMessage::PostConnectEdit(StepEdit::Add))))
        );
        assert!(
            !messages.iter().any(|m| matches!(
                m,
                Message::App(AppMessage::PostConnectEdit(StepEdit::Remove))
            )),
            "{messages:?}"
        );
    }
    let _ = shell.update(app(AppMessage::PostConnectEdit(StepEdit::Add)));
    snapshot(&shell, "post-connect-steps.png");
    let mut ui = tall_simulator(&shell);
    // A pick list's choice is not found by its text: the snapshot shows it.
    for label in ["Command", "Delay (ms)", "On failure", "150"] {
        ui.find(label).expect(label);
    }
    assert!(
        ui.find("No steps yet. Add a step to send commands automatically once this session is connected.")
            .is_err()
    );
    common::reveal(&mut ui, "Remove");
    ui.click("Remove").expect("remove");
    assert!(ui.into_messages().any(|m| matches!(
        m,
        Message::App(AppMessage::PostConnectEdit(StepEdit::Remove))
    )));
}

#[test]
fn only_an_ssh_form_has_post_connect_steps() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    for protocol in [DraftProtocol::Rdp, DraftProtocol::Telnet] {
        let _ = shell.update(app(AppMessage::NewProfile));
        let _ = shell.update(app(AppMessage::ChooseProtocol(protocol)));
        show(&mut shell, ProfileTab::Options);
        assert!(
            tall_simulator(&shell)
                .find("Post-connect sequence")
                .is_err(),
            "{protocol:?}"
        );
    }
}

#[test]
fn an_ssh_form_offers_to_forward_the_agent_as_the_csharp_box() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
    show(&mut shell, ProfileTab::Options);
    {
        let mut ui = tall_simulator(&shell);
        for label in ["SSH options", "Enable compression", "Forward SSH agent"] {
            ui.find(label).expect(label);
        }
    }
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Rdp)));
    show(&mut shell, ProfileTab::Options);
    let mut ui = tall_simulator(&shell);
    assert!(ui.find("Forward SSH agent").is_err());
    assert!(ui.find("Enable compression").is_err());
}

#[test]
fn an_sftp_form_is_the_ssh_one_without_what_a_shell_needs() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Sftp)));
    {
        let mut ui = tall_simulator(&shell);
        for label in ["SFTP", "Remote SSH port", "SSH credentials", "SSH key"] {
            ui.find(label).expect(label);
        }
    }
    show(&mut shell, ProfileTab::Options);
    {
        let mut ui = tall_simulator(&shell);
        for label in ["SSH options", "Enable compression"] {
            ui.find(label).expect(label);
        }
        for absent in ["Forward SSH agent", "Post-connect sequence"] {
            assert!(ui.find(absent).is_err(), "{absent}");
        }
    }
    show(&mut shell, ProfileTab::Network);
    tall_simulator(&shell)
        .find("Gateway routing")
        .expect("Gateway routing");
}

#[test]
fn a_local_form_asks_for_a_program_and_its_folder_not_a_server() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Local)));
    snapshot(&shell, "profile-local.png");
    {
        let mut ui = tall_simulator(&shell);
        for label in [
            "Local Shell",
            "Local shell",
            "Executable",
            "The default shell",
            "Arguments",
        ] {
            ui.find(label).expect(label);
        }
        ui.find("Name the session as the tree lists it.")
            .expect("its own basics");
        for absent in [
            "Server *",
            "Password",
            "Network",
            "Set the destination host and the service port Heimdall should open.",
        ] {
            assert!(ui.find(absent).is_err(), "{absent}");
        }
    }
    show(&mut shell, ProfileTab::Options);
    let mut ui = tall_simulator(&shell);
    for label in ["Advanced shell options", "Working directory"] {
        ui.find(label).expect(label);
    }
}

#[test]
fn an_ftp_form_asks_for_an_account_and_its_options_as_the_csharp_cards() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ftp)));
    {
        let mut ui = tall_simulator(&shell);
        for label in [
            "FTP port",
            "FTP Authentication",
            "Enter the FTP username and password. Leave blank for anonymous access.",
        ] {
            ui.find(label).expect(label);
        }
        assert!(ui.find("Network").is_err(), "FTP goes directly");
    }
    show(&mut shell, ProfileTab::Options);
    let mut ui = tall_simulator(&shell);
    for label in [
        "FTP Options",
        "Configure FTP connection behavior.",
        "Passive mode (recommended for firewalled networks)",
        "Enable SSL/TLS (FTPS)",
    ] {
        ui.find(label).expect(label);
    }
}

#[test]
fn the_ssh_key_is_browsed_for_and_the_folder_separator_is_taught_as_the_csharp_form() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
    {
        let mut ui = tall_simulator(&shell);
        ui.click("Browse...").expect("the button beside the key");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::BrowseKeyFile))
        );
    }
    // The path picked fills the field, as any typing does.
    let _ = shell.update(app(AppMessage::ProfileField {
        field: ProfileField::KeyPath,
        value: "/home/me/.ssh/id_ed25519".to_owned(),
    }));
    let mut ui = tall_simulator(&shell);
    ui.find("/home/me/.ssh/id_ed25519")
        .expect("the key path shown");
    drop(ui);
    show(&mut shell, ProfileTab::Info);
    tall_simulator(&shell)
        .find(
            "Use / to nest folders: Production/Databases puts this session in Databases, inside Production.",
        )
        .expect("the folder hint");
    show(&mut shell, ProfileTab::General);

    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Rdp)));
    let mut ui = tall_simulator(&shell);
    assert!(ui.find("Browse...").is_err(), "no SSH key for RDP");
}

#[test]
fn a_new_rdp_form_follows_the_global_defaults_and_says_its_own_options_are_not_in_effect() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Rdp)));
    show(&mut shell, ProfileTab::Options);
    {
        let mut ui = tall_simulator(&shell);
        ui.find("This server is using your global RDP defaults. Uncheck \"Use global RDP defaults\" to set per-server options.")
            .expect("the banner");
        ui.find("The greyed options below come from the global defaults: the values shown for them are this server's own, not the ones in effect.")
            .expect("what the options below are");
        ui.click("Use global RDP defaults").expect("its box");
        common::reveal(&mut ui, "Redirect printers");
        ui.click("Redirect printers").expect("shown");
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::ProfileToggle {
                toggle: ProfileToggle::FollowDefaults,
                on: false
            })
        )));
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, Message::App(AppMessage::ProfileChoice(_)))),
            "the printers are the defaults' while followed: greyed"
        );
    }
    let _ = shell.update(app(AppMessage::ProfileToggle {
        toggle: ProfileToggle::FollowDefaults,
        on: false,
    }));
    {
        let mut ui = tall_simulator(&shell);
        common::reveal(&mut ui, "Redirect printers");
        ui.click("Redirect printers").expect("shown");
        assert!(
            ui.into_messages().any(|message| matches!(
                message,
                Message::App(AppMessage::ProfileChoice(
                    heimdall_app::profile_draft::ProfileChoice::Extra(
                        heimdall_core::profile::RdpSwitch::Printers,
                        true
                    )
                ))
            )),
            "the profile's own once left"
        );
    }
    let mut ui = tall_simulator(&shell);
    assert!(
        ui.find("The greyed options below come from the global defaults: the values shown for them are this server's own, not the ones in effect.")
            .is_err(),
        "its own options are the ones in effect"
    );
}

#[test]
fn the_rdp_form_groups_its_options_as_the_csharp_tabs_and_asks_the_rd_gateway_last() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Rdp)));
    let in_order = |shell: &Shell, labels: &[&str]| {
        let mut ui = tall_simulator(shell);
        let mut above = f32::MIN;
        for label in labels {
            let top = ui.find(*label).expect(label).bounds().y;
            assert!(
                top > above,
                "{label} below the one before, as in the C# dialog"
            );
            above = top;
        }
    };
    show(&mut shell, ProfileTab::Options);
    in_order(
        &shell,
        &[
            "Use global RDP defaults",
            "Display & Audio",
            "Audio mode",
            "Resolution profile",
            "Session mode",
            "Enable multi-monitor mode",
            "Display",
            "Allow dynamic resolution updates",
            "Audio",
            "Capture local microphone",
            "Devices",
            "Redirect clipboard",
            "Redirect drives",
            "Redirect printers",
            "Redirect COM ports",
            "Redirect smart cards",
            "Redirect webcam",
            "Redirect USB devices",
            "Performance",
            "Connection",
            "Enable anti-idle keepalive",
            "Keep bitmap cache on disk between sessions",
            "Enable RDP compression",
            "Use hardware-accelerated rendering",
            "Automatically reconnect",
            "Visual experience",
            "Avoid UDP transport probing",
            "Behavior",
            "Security",
            "Enable Network Level Authentication",
            "Require server identity validation",
            "Run as administrator session (/admin)",
            "Open in fullscreen",
        ],
    );
    // The RD Gateway after the SSH gateway, on the Network tab.
    show(&mut shell, ProfileTab::Network);
    in_order(
        &shell,
        &[
            "Gateway routing",
            "RD Gateway server",
            "Microsoft Remote Desktop Gateway used to reach this host over HTTPS. Not the same as the SSH jump host configured above.",
        ],
    );
}

#[test]
fn every_rdp_extra_is_saved_from_the_form_and_read_back_from_the_store() {
    use heimdall_app::profile_draft::ProfileChoice;
    use heimdall_core::profile::{RdpExtras, RdpSwitch, Resolution};

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Rdp)));
    let _ = shell.update(app(AppMessage::ProfileToggle {
        toggle: ProfileToggle::FollowDefaults,
        on: false,
    }));
    for (field, value) in [
        (ProfileField::Name, "dc"),
        (ProfileField::Host, "dc.lab"),
        (ProfileField::RdGateway, "rdg.lab.example"),
    ] {
        let _ = shell.update(app(AppMessage::ProfileField {
            field,
            value: value.to_owned(),
        }));
    }
    let defaults = RdpExtras::default();
    let switches = [
        RdpSwitch::Printers,
        RdpSwitch::ComPorts,
        RdpSwitch::SmartCards,
        RdpSwitch::Webcam,
        RdpSwitch::Usb,
        RdpSwitch::Microphone,
        RdpSwitch::BitmapCaching,
        RdpSwitch::Compression,
        RdpSwitch::HardwareAcceleration,
        RdpSwitch::DisableUdp,
        RdpSwitch::FullScreen,
    ];
    // Each the other way from a new profile's.
    for switch in switches {
        let _ = shell.update(app(AppMessage::ProfileChoice(ProfileChoice::Extra(
            switch,
            !switch.is_on(&defaults),
        ))));
    }
    let _ = shell.update(app(AppMessage::ProfileChoice(ProfileChoice::MultiMonitor(
        true,
    ))));
    for index in [2, 0] {
        let _ = shell.update(app(AppMessage::ProfileChoice(ProfileChoice::Monitor(
            index, true,
        ))));
    }
    let _ = shell.update(Message::SaveProfileForm);
    let store =
        heimdall_core::store::ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    let saved = store
        .rdp_profiles()
        .iter()
        .find(|profile| profile.name == "dc")
        .cloned()
        .expect("saved");
    for switch in switches {
        assert_eq!(
            switch.is_on(&saved.extras),
            !switch.is_on(&defaults),
            "{switch:?}"
        );
    }
    assert_eq!(saved.extras.rd_gateway.as_deref(), Some("rdg.lab.example"));
    assert!(saved.extras.multi_monitor);
    assert_eq!(saved.extras.monitors, [0, 2]);
    assert_eq!(saved.options.resolution, Resolution::MultiMonitor);
    // Edited again: the form reads every one back.
    let _ = shell.update(app(AppMessage::EditProfile(saved.id.clone())));
    show(&mut shell, ProfileTab::Network);
    let mut ui = tall_simulator(&shell);
    ui.find("rdg.lab.example").expect("the gateway typed");
}

#[test]
fn the_multi_monitor_picker_shows_the_screens_as_listed_not_as_drawn() {
    use heimdall_app::profile_draft::ProfileChoice;
    use heimdall_ui::rdp_options::Monitor;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Rdp)));
    let _ = shell.update(app(AppMessage::ProfileToggle {
        toggle: ProfileToggle::FollowDefaults,
        on: false,
    }));
    // Screens this test machine does not have: shown only if the form reads the list kept.
    shell.set_monitors(vec![
        Monitor {
            index: 0,
            width: 2560,
            height: 1440,
            primary: true,
        },
        Monitor {
            index: 1,
            width: 1200,
            height: 1920,
            primary: false,
        },
    ]);
    let _ = shell.update(app(AppMessage::ProfileChoice(ProfileChoice::MultiMonitor(
        true,
    ))));
    show(&mut shell, ProfileTab::Options);
    {
        let mut ui = tall_simulator(&shell);
        ui.find("Monitor 1: 2560x1440 (primary)")
            .expect("the first screen kept");
        common::reveal(&mut ui, "Monitor 2: 1200x1920 (vertical)");
        ui.click("Monitor 2: 1200x1920 (vertical)")
            .expect("the second screen kept");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::ProfileChoice(ProfileChoice::Monitor(1, true)))
        )));
    }
    // The form closed, the list goes; opened again, the screens there are are listed.
    let _ = shell.update(app(AppMessage::DismissDialog));
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Rdp)));
    let _ = shell.update(app(AppMessage::ProfileChoice(ProfileChoice::MultiMonitor(
        true,
    ))));
    let mut ui = tall_simulator(&shell);
    assert!(
        ui.find("Monitor 1: 2560x1440 (primary)").is_err(),
        "listed again on opening"
    );
}

#[test]
fn an_rd_gateway_that_is_no_host_name_is_refused_with_the_csharp_reason() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Rdp)));
    for (field, value) in [
        (ProfileField::Name, "dc"),
        (ProfileField::Host, "dc.lab"),
        (ProfileField::RdGateway, "https://rdg.lab:443"),
    ] {
        let _ = shell.update(app(AppMessage::ProfileField {
            field,
            value: value.to_owned(),
        }));
    }
    let _ = shell.update(Message::SaveProfileForm);
    {
        let mut ui = tall_simulator(&shell);
        ui.find("The RD Gateway must be a valid host name or IP address.")
            .expect("the reason");
    }
    assert!(shell.app().profiles().is_empty() && shell.app().rdp_profiles().is_empty());
    let _ = shell.update(app(AppMessage::ProfileField {
        field: ProfileField::RdGateway,
        value: "rdg.lab".to_owned(),
    }));
    let _ = shell.update(Message::SaveProfileForm);
    assert_eq!(
        shell.app().rdp_profiles()[0].extras.rd_gateway.as_deref(),
        Some("rdg.lab")
    );
}

#[test]
fn the_form_offers_to_test_its_address_and_says_while_it_runs() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
    {
        let mut ui = simulator(&shell);
        ui.find("Test address").expect("the C# button");
        ui.find(
            "Checks that the address and port answer. Does not check your username or password.",
        )
        .expect("the C# hint");
    }
    let _ = shell.update(app(AppMessage::ProfileField {
        field: ProfileField::Host,
        value: "web.lab".to_owned(),
    }));
    // Applied through the shell: the test's task is built, not run, on the window's thread.
    let _ = shell.update(app(AppMessage::TestAddress));
    let mut ui = simulator(&shell);
    ui.find("Testing the address...").expect("the C# chip");
}

#[test]
fn the_gateway_dialog_tests_its_route_and_says_while_it_runs() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
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
    {
        let mut ui = tall_simulator(&shell);
        ui.find("Test and understand this route")
            .expect("the C# card");
        ui.find("This workstation \u{2192} bastion (bastion.lab:22)")
            .expect("the route line");
        ui.find("Test route").expect("its button");
    }
    // Applied through the shell: the test's task is built, not run, on the window's thread.
    let _ = shell.update(Message::TestRouteForm);
    let mut ui = tall_simulator(&shell);
    ui.find("Testing the route. Results appear after each step.")
        .expect("said while it runs");
    ui.find("Stop test").expect("Stop while it runs");
}

#[test]
fn an_ssh_form_chooses_putty_and_x11_forwarding_with_its_warning() {
    use heimdall_app::profile_draft::ProfileChoice;
    use heimdall_core::profile::SshMode;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
    let warning = "X11 forwarding lets the remote host see";
    show(&mut shell, ProfileTab::Options);
    {
        let mut ui = tall_simulator(&shell);
        for label in ["SSH mode", "Enable X11 forwarding"] {
            ui.find(label).expect(label);
        }
        assert!(ui.find("Opens PuTTY in a separate window").is_err());
        assert!(ui.find(warning).is_err(), "no warning while off");
    }
    let _ = shell.update(app(AppMessage::ProfileChoice(ProfileChoice::SshMode(
        SshMode::External,
    ))));
    let _ = shell.update(app(AppMessage::ProfileToggle {
        toggle: ProfileToggle::X11Forwarding,
        on: true,
    }));
    assert!(matches!(
        &shell.app().dialog,
        Some(heimdall_app::Dialog::EditProfile { draft, .. }) if draft.ssh_mode == SshMode::External
    ));
    {
        let mut ui = tall_simulator(&shell);
        ui.find("Opens PuTTY in a separate window. PuTTY asks for the password itself.")
            .expect("explained");
        ui.find("X11 forwarding lets the remote host see this computer's display and the keys typed in its windows. Turn it on only for a server you trust.")
            .expect("warned");
    }
    // Neither in an SFTP form, which opens its files in a tab.
    let _ = shell.update(app(AppMessage::NewProfile));
    let _ = shell.update(app(AppMessage::ChooseProtocol(DraftProtocol::Sftp)));
    show(&mut shell, ProfileTab::Options);
    let mut ui = tall_simulator(&shell);
    for absent in ["SSH mode", "Enable X11 forwarding"] {
        assert!(ui.find(absent).is_err(), "{absent}");
    }
}
