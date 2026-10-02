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

//! The tunnels drawn headless, as the C# ones: the "New tunnel" dialog, what it says without a
//! gateway and the check it says while something is missing; the status bar's count, and the
//! panel under the sessions with its rows, their menu and their closing.

mod common;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;

use heimdall_app::tunnel::{TunnelEvent, TunnelField, TunnelId};
use heimdall_app::{App, AppConfig, Message as AppMessage, TunnelMessage};
use heimdall_core::profile::{ProfileId, SshGateway};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::tree_view::TreeMenu;
use iced::{Settings, Size};

const WINDOW: Size = Size::new(1200.0, 720.0);

fn shell(dir: &Path, gateways: Vec<SshGateway>) -> Shell {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_gateways(gateways);
    store.save().expect("save");
    Shell::with_app(App::new(AppConfig {
        profiles_file,
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

fn tunnel(message: TunnelMessage) -> Message {
    Message::App(AppMessage::Tunnel(message))
}

fn bastion() -> SshGateway {
    SshGateway {
        id: ProfileId::new("bastion"),
        name: "Bastion".to_owned(),
        host: "bastion.lab".to_owned(),
        port: 2222,
        username: Some("jump".to_owned()),
        key_path: None,
        parent: None,
    }
}

#[test]
fn without_a_gateway_the_dialog_says_to_add_one_as_the_csharp() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path(), Vec::new());
    let _ = shell.update(tunnel(TunnelMessage::New));
    let mut ui = simulator(&shell);
    ui.find("New tunnel").expect("title");
    ui.find("No SSH gateway configured. Add one in Settings before creating a tunnel.")
        .expect("the C# notice");
    assert!(ui.find("Remote host").is_err(), "no field to fill");
}

#[test]
fn the_dialog_names_its_gateway_and_says_what_is_missing_until_nothing_is() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path(), vec![bastion()]);
    let _ = shell.update(tunnel(TunnelMessage::New));
    {
        let mut ui = simulator(&shell);
        ui.find(
            "Create a session-scoped local port forward through one of your configured SSH \
             gateways.",
        )
        .expect("the C# description");
        // The gateway list's value is not a text to find: its label is tested in the view.
        ui.find("Gateway").expect("its list");
        for label in [
            "Remote host",
            "Remote port",
            "Local port",
            "Label (optional)",
        ] {
            ui.find(label).expect(label);
        }
        ui.find("Remote host is required.").expect("said at once");
    }
    let _ = shell.update(tunnel(TunnelMessage::Field {
        field: TunnelField::RemoteHost,
        value: "wiki.lab".to_owned(),
    }));
    let _ = shell.update(tunnel(TunnelMessage::Field {
        field: TunnelField::LocalPort,
        value: "80".to_owned(),
    }));
    {
        let mut ui = simulator(&shell);
        ui.find("Local port must be between 1024 and 65535.")
            .expect("the C# range");
    }
    let _ = shell.update(tunnel(TunnelMessage::Field {
        field: TunnelField::LocalPort,
        value: "9090".to_owned(),
    }));
    {
        let mut ui = simulator(&shell);
        assert!(
            ui.find("Local port must be between 1024 and 65535.")
                .is_err()
        );
        ui.click("Open tunnel")
            .expect("offered once nothing is missing");
    }
    // Opened from the window's thread: the attempt starts once its task runs, on the
    // runtime, and the dialog has closed.
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    let mut ui = simulator(&shell);
    assert!(ui.find("New tunnel").is_err(), "closed on Open tunnel");
}

/// Opens a tunnel to `wiki.lab:443` on local port 9443, labelled "wiki", through `bastion`,
/// as the dialog then the attempt would: the first tunnel of the run.
fn opened_tunnel(shell: &mut Shell) -> TunnelId {
    let _ = shell.update(tunnel(TunnelMessage::New));
    for (field, value) in [
        (TunnelField::RemoteHost, "wiki.lab"),
        (TunnelField::RemotePort, "443"),
        (TunnelField::LocalPort, "9443"),
        (TunnelField::Label, "wiki"),
    ] {
        let _ = shell.update(tunnel(TunnelMessage::Field {
            field,
            value: value.to_owned(),
        }));
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    let id = TunnelId::default();
    let _ = shell.update(tunnel(TunnelMessage::Event {
        id,
        event: TunnelEvent::Opened(SocketAddr::from((Ipv4Addr::LOCALHOST, 9443))),
    }));
    id
}

#[test]
fn the_status_bar_counts_the_tunnels_and_opens_the_panel_which_says_when_there_is_none() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path(), vec![bastion()]);
    {
        let mut ui = simulator(&shell);
        assert!(ui.find("Tunnels (0)").is_err(), "closed to begin with");
        ui.click("0 tunnels").expect("the bar's button");
    }
    let _ = shell.update(tunnel(TunnelMessage::TogglePanel));
    {
        let mut ui = simulator(&shell);
        ui.find("Tunnels (0)").expect("the C# header");
        ui.find("No active tunnels").expect("the C# empty state");
        ui.click("+ New").expect("New");
    }
    let _ = shell.update(tunnel(TunnelMessage::New));
    {
        let mut ui = simulator(&shell);
        ui.find("New tunnel").expect("the dialog");
    }
    let _ = shell.update(Message::App(AppMessage::DismissDialog));
    let _ = shell.update(tunnel(TunnelMessage::TogglePanel));
    let mut ui = simulator(&shell);
    assert!(ui.find("Tunnels (0)").is_err(), "collapsed");
}

#[test]
fn an_open_tunnel_is_a_row_with_the_csharp_columns_its_menu_and_its_close_button() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path(), vec![bastion()]);
    let id = opened_tunnel(&mut shell);
    let _ = shell.update(tunnel(TunnelMessage::TogglePanel));
    {
        let mut ui = simulator(&shell);
        ui.find("1 tunnel").expect("counted, in the singular");
        ui.find("Tunnels (1)").expect("header");
        for cell in ["Bastion", "wiki", "9443", "wiki.lab", "443"] {
            ui.find(cell).expect(cell);
        }
        for title in ["Gateway", "Label", "Local", "Remote", "Port"] {
            ui.find(title).expect(title);
        }
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tunnel(id)));
    {
        let mut ui = simulator(&shell);
        for entry in ["Close Tunnel", "Copy Local Port", "Close All Tunnels"] {
            ui.find(entry).expect(entry);
        }
    }
    let _ = shell.update(Message::CloseTreeMenu);
    let _ = shell.update(tunnel(TunnelMessage::Close(id)));
    let mut ui = simulator(&shell);
    ui.find("No active tunnels").expect("closed");
    ui.find("Tunnel on port 9443 closed.")
        .expect("the C# status line");
}
