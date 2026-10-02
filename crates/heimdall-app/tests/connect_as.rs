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

//! "Connect as...", as the C# Heimdall's: a profile's host with another protocol, its
//! default port and the profile's account, as a session never saved.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, ConnectAs, ConnectionEvent, Effect, Message, Purpose, TabId, UiError,
};
use heimdall_core::profile::{
    LocalArguments, LocalCommand, LocalProfile, ProfileId, RdpProfile, SshProfile,
};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "Web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 2222,
        username: Some("deploy".to_owned()),
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
    }]);
    store.merge_rdp([RdpProfile {
        id: ProfileId::new("dc"),
        name: "Domain controller".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3390,
        username: Some("admin".to_owned()),
        domain: Some("LAB".to_owned()),
        allow_tls_only: true,
        gateway: None,
        redirect_clipboard: false,
        redirect_drives: true,
        options: heimdall_core::profile::RdpOptions::default(),
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        follow_defaults: false,
        several_servers: false,
    }]);
    store.merge_local([LocalProfile {
        id: ProfileId::new("sh"),
        name: "shell".to_owned(),
        group: None,
        command: LocalCommand {
            program: Some("sh".to_owned()),
            arguments: LocalArguments::List(Vec::new()),
            working_directory: None,
        },
        approved: None,
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

fn connect_as(app: &mut App, id: &str, protocol: ConnectAs) -> Vec<Effect> {
    app.update(Message::ConnectAs {
        id: ProfileId::new(id),
        protocol,
    })
}

#[test]
fn every_protocol_but_the_profiles_own_is_offered_for_a_host() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path());
    assert_eq!(
        app.connect_as_choices(&ProfileId::new("web")),
        [
            ConnectAs::Rdp,
            ConnectAs::Sftp,
            ConnectAs::Vnc,
            ConnectAs::Telnet
        ]
    );
    assert_eq!(
        app.connect_as_choices(&ProfileId::new("dc")),
        [
            ConnectAs::Ssh,
            ConnectAs::Sftp,
            ConnectAs::Vnc,
            ConnectAs::Telnet
        ]
    );
    assert!(
        app.connect_as_choices(&ProfileId::new("sh")).is_empty(),
        "no host"
    );
    assert!(app.connect_as_choices(&ProfileId::new("gone")).is_empty());
    assert_eq!(
        ConnectAs::ALL.map(ConnectAs::label),
        ["SSH", "RDP", "SFTP", "VNC", "Telnet"]
    );
}

#[test]
fn an_rdp_host_opened_as_ssh_takes_the_default_port_and_the_account() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = connect_as(&mut app, "dc", ConnectAs::Ssh);
    let [Effect::Connect { tab, request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.profile.host, "dc.lab");
    assert_eq!(request.profile.port, 22);
    assert_eq!(request.profile.username.as_deref(), Some("admin"));
    assert!(request.route.is_empty(), "no gateway");
    assert!(request.profile.id.as_str().starts_with("connect-as:"));
    assert_eq!(request.purpose, Purpose::Shell);
    let opened = app.tab(*tab).expect("tab");
    assert_eq!(opened.display_title(), "dc.lab", "named after its host");
    assert!(app.tab_profile(opened).is_none(), "saved nowhere");

    let effects = connect_as(&mut app, "dc", ConnectAs::Sftp);
    let [Effect::Connect { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.purpose, Purpose::Files);
    assert_eq!(request.profile.port, 22);

    let effects = connect_as(&mut app, "dc", ConnectAs::Vnc);
    let [Effect::ConnectVnc { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (request.profile.host.as_str(), request.profile.port),
        ("dc.lab", 5900)
    );

    let effects = connect_as(&mut app, "dc", ConnectAs::Telnet);
    let [Effect::ConnectTelnet { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (request.profile.host.as_str(), request.profile.port),
        ("dc.lab", 23)
    );

    assert!(
        connect_as(&mut app, "dc", ConnectAs::Rdp).is_empty(),
        "its own protocol is not offered"
    );
    assert!(connect_as(&mut app, "sh", ConnectAs::Ssh).is_empty());
    assert_eq!(app.tabs.len(), 4);
}

#[test]
fn an_ssh_host_opened_as_rdp_gets_the_desktop_defaults_and_its_sftp_is_its_own_files() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = connect_as(&mut app, "web", ConnectAs::Rdp);
    let [Effect::ConnectRdp { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let profile = &request.profile;
    assert_eq!((profile.host.as_str(), profile.port), ("web.lab", 3389));
    assert_eq!(profile.username.as_deref(), Some("deploy"));
    assert!(
        !profile.allow_tls_only,
        "Network Level Authentication, the default"
    );
    assert!(profile.redirect_clipboard && !profile.redirect_drives);

    // Its own profile, with its port, key and gateway.
    let effects = connect_as(&mut app, "web", ConnectAs::Sftp);
    let [Effect::Connect { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.profile.id, ProfileId::new("web"));
    assert_eq!(request.profile.port, 2222);
    assert_eq!(request.purpose, Purpose::Files);
}

#[test]
fn a_session_never_saved_reconnects_and_duplicates_as_it_was() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = connect_as(&mut app, "dc", ConnectAs::Ssh);
    let [Effect::Connect { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::Timeout),
    });
    assert!(app.can_reconnect(app.tab(tab).expect("tab")));
    let effects = app.update(Message::ReconnectTab(tab));
    let again: TabId = match effects.as_slice() {
        [Effect::Connect { tab, request, .. }] => {
            assert_eq!(
                (request.profile.host.as_str(), request.profile.port),
                ("dc.lab", 22)
            );
            *tab
        }
        other => panic!("{other:?}"),
    };
    // Twice: the tab opened again opens again the same way.
    let effects = app.update(Message::TabMenu(heimdall_app::TabMenuMessage::Duplicate(
        again,
    )));
    match effects.as_slice() {
        [Effect::Connect { request, .. }] => {
            assert_eq!(
                (request.profile.host.as_str(), request.profile.port),
                ("dc.lab", 22)
            );
            assert_eq!(request.purpose, Purpose::Shell);
        }
        other => panic!("{other:?}"),
    }
    let copy = app.tabs.last().expect("copy").id;
    let effects = app.update(Message::TabMenu(heimdall_app::TabMenuMessage::Duplicate(
        copy,
    )));
    assert!(matches!(effects.as_slice(), [Effect::Connect { .. }]));
    assert_eq!(app.tabs.len(), 3);
}
