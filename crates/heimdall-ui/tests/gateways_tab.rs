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

//! The Settings page's Gateways tab.

mod common;

use std::path::Path;

use heimdall_app::{
    App, AppConfig, Dialog, GatewaysMessage, Message as AppMessage, SystemCredentials,
};
use heimdall_core::profile::{ProfileId, SshGateway, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, SettingsTab, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};

/// A window tall enough for the whole tab.
const WINDOW: Size = Size::new(1100.0, 1400.0);

/// The folder a picture of the tab is written to, when set.
const SNAPSHOT_VARIABLE: &str = "HEIMDALL_SNAPSHOT_DIR";

fn shell(dir: &Path) -> Shell {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_gateways([SshGateway {
        id: ProfileId::new("edge"),
        name: "Edge".to_owned(),
        host: "edge.example".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        parent: None,
    }]);
    let ssh = |id: &str, gateway: &str| SshProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: None,
        key_path: None,
        gateway: Some(ProfileId::new(gateway)),
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
    };
    store.merge([ssh("web", "edge"), ssh("old", "gone")]);
    store.save().expect("save");
    let mut shell = Shell::with_app(App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    }));
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(SettingsTab::Gateways));
    shell
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, WINDOW, shell.view())
}

/// A picture of the window, written as `name` to the folder [`SNAPSHOT_VARIABLE`] names.
fn snapshot(shell: &Shell, name: &str) {
    let Some(dir) = std::env::var_os(SNAPSHOT_VARIABLE) else {
        return;
    };
    let path = Path::new(&dir).join(name);
    // iced names the picture after its renderer: clear every variant, or an old picture is
    // only compared against and never replaced.
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

fn gateways_messages(ui: common::Drawn<'_>) -> Vec<GatewaysMessage> {
    ui.into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::Gateways(message)) => Some(message),
            _ => None,
        })
        .collect()
}

#[test]
fn the_tab_lists_each_gateway_with_its_sessions_and_the_missing_ones() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = shell(dir.path());
    snapshot(&shell, "gateways.png");
    let mut ui = simulator(&shell);
    for label in [
        "SSH Gateways",
        "1 gateway, 2 routed sessions, 1 unresolved reference",
        "Edge",
        "edge.example:22",
        "Configured gateways",
        "Unresolved references",
        "Missing gateway id: gone",
    ] {
        ui.find(label).expect(label);
    }
    ui.click("Delete").expect("Delete");
    assert_eq!(
        gateways_messages(ui),
        [GatewaysMessage::AskDelete(ProfileId::new("edge"))]
    );
}

#[test]
fn a_missing_gateway_is_reassigned_once_one_is_picked_or_cleared() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let mut ui = simulator(&shell);
    ui.click("Reassign").expect("Reassign");
    assert!(gateways_messages(ui).is_empty(), "nothing picked yet");
    let _ = shell.update(Message::GatewayReassignPicked {
        missing: ProfileId::new("gone"),
        to: ProfileId::new("edge"),
    });
    let mut ui = simulator(&shell);
    ui.click("Reassign").expect("Reassign");
    assert_eq!(
        gateways_messages(ui),
        [GatewaysMessage::Reassign {
            missing: ProfileId::new("gone"),
            to: ProfileId::new("edge"),
        }]
    );
    let mut ui = simulator(&shell);
    ui.click("Clear").expect("Clear");
    assert_eq!(
        gateways_messages(ui),
        [GatewaysMessage::Clear(ProfileId::new("gone"))]
    );

    // Deleting asks first, with what it clears.
    let _ = shell.update(Message::App(AppMessage::Gateways(
        GatewaysMessage::AskDelete(ProfileId::new("edge")),
    )));
    assert!(matches!(
        shell.app().dialog,
        Some(Dialog::ConfirmDeleteGateway { servers: 1, .. })
    ));
    simulator(&shell)
        .find("Delete gateway")
        .expect("the question");
}
