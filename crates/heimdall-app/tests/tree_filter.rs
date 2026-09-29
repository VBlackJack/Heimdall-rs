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

//! The tree's filters, as the C# sidebar's: protocols, connected, through a gateway, each
//! one chosen holding; the gateway badge a view choice beside them.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, ConnectionEvent, Effect, FilterMessage, Message, ProfileKind, TreeRow,
};
use heimdall_core::profile::{Forwards, ProfileId, RdpOptions, RdpProfile, SshGateway, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn ssh(id: &str, group: &str, gateway: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: Some(group.to_owned()),
        host: format!("{id}.lab"),
        port: 22,
        username: None,
        key_path: None,
        gateway: gateway.map(ProfileId::new),
        vault_entry: None,
        forwards: Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
    }
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_gateways([SshGateway {
        id: ProfileId::new("edge"),
        name: "edge".to_owned(),
        host: "edge.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        parent: None,
    }]);
    store.merge([
        ssh("web", "Prod", Some("edge")),
        ssh("db", "Prod", None),
        ssh("dev", "Dev", None),
    ]);
    store.merge_rdp([RdpProfile {
        id: ProfileId::new("dc"),
        name: "dc".to_owned(),
        group: Some("Windows".to_owned()),
        host: "dc.lab".to_owned(),
        port: 3389,
        username: None,
        domain: None,
        allow_tls_only: false,
        gateway: None,
        redirect_clipboard: true,
        redirect_drives: false,
        options: RdpOptions::default(),
        vault_entry: None,
        forwards: Forwards::default(),
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

/// The rows, short: `+folder` for an open folder, `-folder` a closed one, then profiles.
fn outline(app: &App) -> Vec<String> {
    app.tree_rows("")
        .iter()
        .map(|row| match row {
            TreeRow::Folder { path, open, .. } => {
                format!("{}{path}", if *open { '+' } else { '-' })
            }
            TreeRow::Profile { profile, .. } => profile.id.as_str().to_owned(),
        })
        .collect()
}

fn filter(app: &mut App, message: FilterMessage) {
    app.update(Message::Filter(message));
}

#[test]
fn chosen_protocols_list_their_profiles_in_open_folders_only() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::ToggleFolder("Windows".to_owned()));
    assert_eq!(
        outline(&app),
        ["+Dev", "dev", "+Prod", "db", "web", "-Windows"]
    );
    assert!(!app.tree_filter().is_active());

    filter(&mut app, FilterMessage::Protocol(ProfileKind::Rdp));
    assert!(app.tree_filter().is_active());
    assert_eq!(
        outline(&app),
        ["+Windows", "dc"],
        "a closed folder shows what was found, as a search does"
    );
    filter(&mut app, FilterMessage::Protocol(ProfileKind::Ssh));
    assert_eq!(
        outline(&app),
        ["+Dev", "dev", "+Prod", "db", "web", "+Windows", "dc"],
        "any protocol chosen"
    );
    filter(&mut app, FilterMessage::Protocol(ProfileKind::Rdp));
    filter(&mut app, FilterMessage::Protocol(ProfileKind::Ssh));
    assert!(!app.tree_filter().is_active(), "chosen twice is not chosen");
}

#[test]
fn every_filter_chosen_must_hold_and_reset_keeps_the_badge_choice() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    filter(&mut app, FilterMessage::Gateway);
    assert_eq!(outline(&app), ["+Prod", "web"]);
    filter(&mut app, FilterMessage::Protocol(ProfileKind::Rdp));
    assert!(
        app.tree_rows("").is_empty(),
        "no RDP profile goes through a gateway"
    );

    filter(&mut app, FilterMessage::GatewayBadge);
    assert!(!app.tree_filter().shows_gateway_badge());
    filter(&mut app, FilterMessage::Reset);
    assert!(!app.tree_filter().is_active());
    assert!(
        !app.tree_filter().shows_gateway_badge(),
        "a view choice, not a filter"
    );
    assert_eq!(outline(&app).len(), 7);
}

#[test]
fn connected_lists_the_profiles_with_a_session_connected() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    filter(&mut app, FilterMessage::Connected);
    assert!(app.tree_rows("").is_empty(), "nothing open yet");

    let (tab, attempt) = match app
        .update(Message::OpenProfile(ProfileId::new("db")))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    assert!(app.tree_rows("").is_empty(), "still connecting");
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: std::sync::Arc::new(NullSink),
        },
    });
    assert_eq!(outline(&app), ["+Prod", "db"]);
}

#[derive(Debug)]
struct NullSink;

impl heimdall_app::InputSink for NullSink {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }
    fn resize(&self, _size: heimdall_ssh::TerminalSize) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}
