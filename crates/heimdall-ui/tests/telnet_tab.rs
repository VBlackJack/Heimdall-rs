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

//! Telnet in the window, drawn headless: the profile in the sidebar opens a Telnet tab.

use std::path::Path;

use heimdall_app::{App, AppConfig, Message as AppMessage};
use heimdall_core::profile::{ProfileId, TelnetProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};
use iced_test::simulator::Simulator;

const WINDOW: Size = Size::new(1200.0, 720.0);

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_telnet([TelnetProfile {
        id: ProfileId::new("sw"),
        name: "Core switch".to_owned(),
        group: Some("Network".to_owned()),
        host: "sw1.lab".to_owned(),
        port: 2323,
    }]);
    store.save().expect("save");
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

#[test]
fn a_telnet_profile_is_listed_and_opens_a_telnet_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = Shell::with_app(app(dir.path()));
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    let mut ui = Simulator::with_size(settings, WINDOW, shell.view());
    ui.find("Telnet").expect("protocol");
    // As in the C# tree: a click selects, a double click connects.
    ui.click("Core switch").expect("profile");
    ui.click("Core switch").expect("profile");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::TreeClick(id) if id.as_str() == "sw"
    )));
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::ConnectProfile(id)) if id.as_str() == "sw"
        )),
        "a double click connects"
    );
}
