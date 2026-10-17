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

//! The properties panel under the tree, drawn headless, as the C# `SessionPropertiesView`
//! docked there: off until asked for, what it shows of a session, a folder and a selection,
//! never a secret, and Ctrl+Shift+B.
//!
//! Strings are the fallback language's (English): the tests never select a language.

mod common;

use std::path::Path;

use heimdall_app::tools::SidebarTab;
use heimdall_app::{
    App, AppConfig, FilterMessage, Message as AppMessage, SelectionMessage, SystemCredentials,
    ToolsMessage,
};
use heimdall_core::credentials::{
    CredentialProtocol, Endpoint, SavedPassword, encode, password_entry,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell, TreeShortcut};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};

/// Size of the simulated window, in logical pixels.
const WINDOW: Size = Size::new(1100.0, 700.0);

/// A window as short as a laptop's split screen: the tree keeps its room.
const SHORT_WINDOW: Size = Size::new(800.0, 420.0);

/// The password saved for session "a", which the panel must never show.
const SECRET: &str = "hunter2-never-shown";

fn profile(id: &str, group: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: group.map(str::to_owned),
        host: format!("{id}.lab"),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
        local_tunnel_port: None,
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

/// Sessions "a" and "b" in Production, "c" in no folder; a password saved for "a".
fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([
        profile("a", Some("Production")),
        profile("b", Some("Production")),
        profile("c", None),
    ]);
    store.save().expect("save");
    let system = SystemCredentials::memory();
    let SystemCredentials::Memory(entries) = &system else {
        unreachable!()
    };
    let saved = SavedPassword {
        endpoint: Endpoint {
            protocol: CredentialProtocol::Ssh,
            host: "a.lab".to_owned(),
            port: 22,
            username: Some("admin".to_owned()),
        },
        password: zeroize::Zeroizing::new(SECRET.to_owned()),
    };
    entries.lock().expect("entries").insert(
        password_entry(&ProfileId::new("a")),
        zeroize::Zeroizing::new(encode(&saved).to_vec()),
    );
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: system,
    })
}

fn simulator(shell: &Shell, size: Size) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, size, shell.view())
}

fn select(shell: &mut Shell, id: &str) {
    let _ = shell.update(Message::App(AppMessage::SelectProfile(ProfileId::new(id))));
}

fn toggle(shell: &mut Shell) {
    let _ = shell.update(Message::App(AppMessage::Filter(
        FilterMessage::PropertiesPanel,
    )));
}

#[test]
fn the_panel_is_off_until_asked_for_and_says_when_nothing_is_selected() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let mut ui = simulator(&shell, WINDOW);
    assert!(ui.find("Properties").is_err(), "off, as the C# default");
    drop(ui);
    toggle(&mut shell);
    let mut ui = simulator(&shell, WINDOW);
    ui.find("Properties").expect("its title");
    ui.find("Select a session to see its properties.")
        .expect("nothing selected");
    ui.find("server a").expect("the tree still drawn above it");
}

#[test]
fn a_session_shows_how_it_signs_in_never_its_password() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    toggle(&mut shell);
    select(&mut shell, "a");
    // A session tab open over the detail pane: the panel still says it.
    let mut ui = simulator(&shell, WINDOW);
    ui.find("Authentication:").expect("the sign-in line");
    ui.find("Username + Password")
        .expect("a password saved, said so");
    ui.find("a.lab:22").expect("its address");
    ui.find("Production").expect("its folder");
    assert!(ui.find(SECRET).is_err(), "the password itself never");

    drop(ui);
    select(&mut shell, "b");
    let mut ui = simulator(&shell, WINDOW);
    ui.find("Username + SSH agent")
        .expect("nothing saved: the agent signs");
    assert!(ui.find(SECRET).is_err());
}

#[test]
fn a_value_copies_itself_at_a_click() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    toggle(&mut shell);
    select(&mut shell, "a");
    let mut ui = simulator(&shell, WINDOW);
    ui.click("a.lab:22").expect("the address");
    let copied: Vec<String> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::CopyValue(value)) => Some(value),
            _ => None,
        })
        .collect();
    assert_eq!(copied, ["a.lab:22"]);
}

#[test]
fn a_folder_and_a_selection_are_summed_up() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    toggle(&mut shell);
    let _ = shell.update(Message::App(AppMessage::SelectFolder(
        "Production".to_owned(),
    )));
    let mut ui = simulator(&shell, WINDOW);
    ui.find("Sessions:").expect("its count");
    ui.find("Connected:").expect("those connected");
    drop(ui);

    select(&mut shell, "a");
    let _ = shell.update(Message::App(AppMessage::Selection(
        SelectionMessage::Toggle(ProfileId::new("c")),
    )));
    let mut ui = simulator(&shell, WINDOW);
    ui.find("Sessions:").expect("the selection's count");
    assert!(
        ui.find("Authentication:").is_err(),
        "a summary, not one session"
    );
}

#[test]
fn ctrl_shift_b_toggles_the_panel_and_shows_it_when_it_cannot_be_seen() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let key = |shell: &mut Shell| {
        let _ = shell.update(Message::TreeShortcut(TreeShortcut::PropertiesPanel));
    };
    key(&mut shell);
    assert!(shell.app().shows_properties_panel(), "shown");
    key(&mut shell);
    assert!(!shell.app().shows_properties_panel(), "hidden again");

    // Behind the Tools tab, the key shows it with the sessions, never hides it unseen.
    let _ = shell.update(Message::App(AppMessage::Tools(
        ToolsMessage::ShowSidebarTab(SidebarTab::Tools),
    )));
    key(&mut shell);
    assert!(shell.app().shows_properties_panel());
    assert_eq!(shell.app().sidebar_tab(), SidebarTab::Sessions);
    key(&mut shell);
    assert!(!shell.app().shows_properties_panel());
    let _ = shell.update(Message::App(AppMessage::Tools(
        ToolsMessage::ShowSidebarTab(SidebarTab::Tools),
    )));
    let _ = shell.update(Message::App(AppMessage::Filter(
        FilterMessage::PropertiesPanel,
    )));
    let mut ui = simulator(&shell, WINDOW);
    assert!(
        ui.find("Select a session to see its properties.").is_err(),
        "the Tools tab has no session to describe"
    );
    drop(ui);

    // The sidebar hidden: shown again with it.
    let _ = shell.update(Message::App(AppMessage::Tools(
        ToolsMessage::ShowSidebarTab(SidebarTab::Sessions),
    )));
    let _ = shell.update(Message::TreeShortcut(TreeShortcut::ToggleSidebar));
    key(&mut shell);
    assert!(shell.app().shows_properties_panel(), "kept on");
    let mut ui = simulator(&shell, WINDOW);
    ui.find("Properties")
        .expect("the sidebar back, the panel in it");
}

#[test]
fn in_a_short_window_the_tree_keeps_its_room_above_the_panel() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    toggle(&mut shell);
    select(&mut shell, "a");
    let mut ui = simulator(&shell, SHORT_WINDOW);
    let tree = ui.find("server a").expect("the tree").bounds();
    let title = ui.find("Properties").expect("the panel").bounds();
    assert!(tree.y < title.y, "the tree above the panel");
    assert!(
        title.y + title.height <= SHORT_WINDOW.height,
        "the panel within the window"
    );
}
