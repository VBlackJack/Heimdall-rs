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

//! Web consoles: opened in the system's browser, never in a tab; through an SSH gateway, on
//! a tunnel of this computer's loopback address listed on the Tunnels page; created and
//! edited in the form as the C# dialog's WEB profile.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use heimdall_app::profile_draft::{DraftError, DraftProtocol, ProfileField, ProfileToggle};
use heimdall_app::tunnel::{SYSTEM_PICKED_PORT, TunnelEvent, TunnelId};
use heimdall_app::web_console::{WebRefusal, loopback_for};
use heimdall_app::{
    App, AppConfig, Dialog, Effect, Message, Notice, ProfileKind, SystemCredentials, TunnelMessage,
};
use heimdall_core::profile::{ProfileId, SshGateway, WebProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

/// The port the system gives the tunnel, in these tests.
const PICKED: u16 = 50_123;

fn ilo(gateway: Option<&str>) -> WebProfile {
    WebProfile {
        remote_os: heimdall_core::profile::RemoteOs::default(),
        id: ProfileId::new("ilo"),
        name: "iLO".to_owned(),
        group: Some("BMC".to_owned()),
        host: "ilo.lab".to_owned(),
        port: 443,
        use_http: false,
        path: Some("/html/login.html".to_owned()),
        username: Some("admin".to_owned()),
        gateway: gateway.map(ProfileId::new),
    }
}

fn bastion() -> SshGateway {
    SshGateway {
        id: ProfileId::new("bastion"),
        name: "Bastion".to_owned(),
        host: "127.0.0.1".to_owned(),
        port: 2222,
        username: Some("jump".to_owned()),
        key_path: None,
        parent: None,
    }
}

fn profiles_file(dir: &Path) -> PathBuf {
    dir.join("profiles.toml")
}

fn app(dir: &Path, profiles: &[WebProfile]) -> App {
    let mut store = ProfileStore::open(profiles_file(dir)).expect("store");
    store.merge_gateways([bastion()]);
    store.merge_web(profiles.iter().cloned());
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file: profiles_file(dir),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

fn type_in(app: &mut App, field: ProfileField, value: &str) {
    app.update(Message::ProfileField {
        field,
        value: value.to_owned(),
    });
}

/// Where the console `ilo`'s tunnel listens: its own loopback address, the port the system
/// gave.
fn picked() -> SocketAddr {
    SocketAddr::from((loopback_for(&ProfileId::new("ilo")), PICKED))
}

/// Opens `ilo` through its gateway: the tunnel asked for.
fn open_through(app: &mut App) -> TunnelId {
    let effects = app.update(Message::ConnectProfile(ProfileId::new("ilo")));
    let [Effect::OpenTunnel { id, request }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (
            request.remote_host.as_str(),
            request.remote_port,
            request.local_port,
            request.gateway.host.as_str()
        ),
        ("ilo.lab", 443, SYSTEM_PICKED_PORT, "127.0.0.1"),
        "to the console, from the gateway, on a port the system picks"
    );
    assert_eq!(
        request.bind,
        loopback_for(&ProfileId::new("ilo")),
        "on a loopback address of its own, never beyond"
    );
    assert!(request.bind.is_loopback() && request.bind != Ipv4Addr::LOCALHOST);
    *id
}

#[test]
fn a_console_reached_directly_opens_in_the_browser_and_no_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[ilo(None)]);
    let effects = app.update(Message::ConnectProfile(ProfileId::new("ilo")));
    let [Effect::OpenWebConsole { name, url, through }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (name.as_str(), url.as_str(), *through),
        ("iLO", "https://ilo.lab/html/login.html", None)
    );
    assert!(!url.contains("admin"), "never its account, nor a password");
    assert!(app.tabs.is_empty(), "nothing embedded");
    app.update(Message::WebOpened {
        name: "iLO".to_owned(),
        through: None,
        result: Ok(()),
    });
    assert_eq!(app.notice(), Some(&Notice::WebOpened("iLO".to_owned())));
    app.update(Message::WebOpened {
        name: "iLO".to_owned(),
        through: None,
        result: Err(WebRefusal::NotStarted("no browser".to_owned())),
    });
    assert_eq!(
        app.notice(),
        Some(&Notice::WebRefused(WebRefusal::NotStarted(
            "no browser".to_owned()
        )))
    );
}

#[test]
fn a_console_whose_host_carries_a_scheme_is_never_handed_to_the_browser() {
    let dir = tempfile::tempdir().expect("dir");
    let mut hostile = ilo(None);
    hostile.host = "javascript:alert(1)".to_owned();
    let mut app = app(dir.path(), &[hostile.clone()]);
    let effects = app.update(Message::ConnectProfile(ProfileId::new("ilo")));
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(
        app.notice(),
        Some(&Notice::WebRefused(WebRefusal::InvalidAddress))
    );
    // Nor through a gateway: no tunnel is opened for it.
    hostile.gateway = Some(ProfileId::new("bastion"));
    let dir = tempfile::tempdir().expect("dir");
    let mut app = self::app(dir.path(), &[hostile]);
    assert!(
        app.update(Message::ConnectProfile(ProfileId::new("ilo")))
            .is_empty()
    );
}

#[test]
fn through_a_gateway_the_browser_opens_the_tunnel_once_it_listens_on_loopback() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[ilo(Some("bastion"))]);
    let id = open_through(&mut app);
    // A second double click while it opens holds no second port.
    assert!(
        app.update(Message::ConnectProfile(ProfileId::new("ilo")))
            .is_empty()
    );
    let effects = app.update(Message::Tunnel(TunnelMessage::Event {
        id,
        event: TunnelEvent::Opened(picked()),
    }));
    let [Effect::OpenWebConsole { url, through, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (url.as_str(), *through),
        (
            format!("https://{}/html/login.html", picked()).as_str(),
            Some(PICKED)
        )
    );
    // Listed on the Tunnels page, named after the console, until it is closed there.
    let listed = app.tunnel(id).expect("a row of the Tunnels page");
    assert_eq!(listed.spec.label.as_deref(), Some("iLO"));
    assert_eq!(listed.local, picked());
    assert_eq!(app.tunnel_ports(), [PICKED]);
    app.update(Message::WebOpened {
        name: "iLO".to_owned(),
        through: Some(PICKED),
        result: Ok(()),
    });
    assert_eq!(
        app.notice(),
        Some(&Notice::WebOpenedThrough {
            name: "iLO".to_owned(),
            port: PICKED,
        })
    );

    // Opened again: through the same tunnel, as the C# consoles share their gateway's.
    let effects = app.update(Message::ConnectProfile(ProfileId::new("ilo")));
    let [Effect::OpenWebConsole { url, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(*url, format!("https://{}/html/login.html", picked()));

    app.update(Message::Tunnel(TunnelMessage::Close(id)));
    assert!(app.tunnel(id).is_none());
    assert!(app.tunnel_ports().is_empty());
    assert!(matches!(
        app.update(Message::ConnectProfile(ProfileId::new("ilo")))
            .as_slice(),
        [Effect::OpenTunnel { .. }]
    ));
}

#[test]
fn a_tunnel_said_to_listen_beyond_loopback_is_never_browsed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[ilo(Some("bastion"))]);
    let id = open_through(&mut app);
    let effects = app.update(Message::Tunnel(TunnelMessage::Event {
        id,
        event: TunnelEvent::Opened(SocketAddr::from((Ipv4Addr::UNSPECIFIED, PICKED))),
    }));
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(
        app.notice(),
        Some(&Notice::WebRefused(WebRefusal::NotLoopback))
    );
}

#[test]
fn a_console_is_created_in_the_form_its_port_following_its_scheme() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[]);
    app.update(Message::NewProfile);
    app.update(Message::ChooseProtocol(DraftProtocol::Web));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(draft.port, "443");
    assert!(draft.shows(ProfileField::WebPath) && draft.shows(ProfileField::Username));
    assert!(!draft.shows_password(), "the browser asks for it");
    assert!(!draft.shows(ProfileField::LocalTunnelPort));
    assert!(draft.protocol.routes_through_gateway());

    app.update(Message::ProfileToggle {
        toggle: ProfileToggle::PlainHttp,
        on: true,
    });
    type_in(&mut app, ProfileField::Name, "NAS");
    type_in(&mut app, ProfileField::Host, "[fd00::20]");
    type_in(&mut app, ProfileField::WebPath, "admin/index.cgi");
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(draft.port, "80", "plain HTTP's port");
    assert_eq!(
        draft.web_address().as_deref(),
        Some("http://[fd00::20]/admin/index.cgi")
    );
    app.update(Message::ChooseGateway(ProfileId::new("bastion")));
    app.update(Message::ConfirmDialog);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let saved = ProfileStore::open(profiles_file(dir.path()))
        .expect("readable")
        .web_profiles()
        .to_vec();
    assert_eq!(saved.len(), 1);
    let nas = &saved[0];
    assert_eq!(
        (
            nas.host.as_str(),
            nas.port,
            nas.use_http,
            nas.path.as_deref(),
            nas.gateway.as_ref().map(ProfileId::as_str)
        ),
        (
            "fd00::20",
            80,
            true,
            Some("admin/index.cgi"),
            Some("bastion")
        )
    );
    let summary = app
        .profile_summaries()
        .into_iter()
        .find(|summary| summary.id == nas.id)
        .expect("in the tree");
    assert_eq!(summary.kind, ProfileKind::Web);
    assert!(summary.matches("web nas"), "found by its protocol and name");
}

#[test]
fn the_form_refuses_a_url_pasted_into_the_host() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[]);
    app.update(Message::NewProfile);
    app.update(Message::ChooseProtocol(DraftProtocol::Web));
    type_in(&mut app, ProfileField::Name, "pasted");
    for host in [
        "https://ilo.lab",
        "javascript:alert(1)",
        "ilo.lab/x",
        "ilo lab",
    ] {
        type_in(&mut app, ProfileField::Host, host);
        let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
            panic!("{:?}", app.dialog);
        };
        assert!(
            draft
                .errors()
                .iter()
                .any(|error| error.field() == ProfileField::Host),
            "{host}: {:?}",
            draft.errors()
        );
        assert_eq!(draft.web_address(), None, "{host}");
    }
    type_in(&mut app, ProfileField::Host, "ilo.lab");
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert!(
        !draft.errors().contains(&DraftError::HostInvalid),
        "{:?}",
        draft.errors()
    );
}

#[test]
fn a_console_is_edited_in_its_own_protocol_and_keeps_its_gateway() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[ilo(Some("bastion"))]);
    app.update(Message::EditProfile(ProfileId::new("ilo")));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(draft.protocol, DraftProtocol::Web);
    assert_eq!(draft.web_path, "/html/login.html");
    assert_eq!(draft.routed_gateway(), Some(ProfileId::new("bastion")));
    type_in(&mut app, ProfileField::Port, "8443");
    app.update(Message::ConfirmDialog);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let saved = ProfileStore::open(profiles_file(dir.path()))
        .expect("readable")
        .web_profiles()
        .to_vec();
    assert_eq!(
        saved,
        [WebProfile {
            remote_os: heimdall_core::profile::RemoteOs::default(),
            port: 8443,
            ..ilo(Some("bastion"))
        }]
    );
}

#[test]
fn consoles_behind_one_gateway_never_share_a_browser_origin_nor_a_manual_tunnel() {
    let dir = tempfile::tempdir().expect("dir");
    let idrac = WebProfile {
        remote_os: heimdall_core::profile::RemoteOs::default(),
        id: ProfileId::new("idrac"),
        name: "iDRAC".to_owned(),
        ..ilo(Some("bastion"))
    };
    let mut app = app(dir.path(), &[ilo(Some("bastion")), idrac]);
    let first = open_through(&mut app);
    app.update(Message::Tunnel(TunnelMessage::Event {
        id: first,
        event: TunnelEvent::Opened(picked()),
    }));
    // The same host and port, another profile: its own tunnel on its own address, so the
    // browser keeps the two consoles' cookies apart.
    let effects = app.update(Message::ConnectProfile(ProfileId::new("idrac")));
    let [Effect::OpenTunnel { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.bind, loopback_for(&ProfileId::new("idrac")));
    assert_ne!(request.bind, picked().ip());
}
