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

//! RDP tabs through SSH gateways: the route asked for, a gateway's SSH key asked about and
//! learnt in an RDP tab, and a gateway's changed key kept apart from the server's certificate.

use std::path::Path;
use std::sync::Arc;

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, Phase, TabId, UiError,
};
use heimdall_core::profile::{ProfileId, RdpProfile, SshGateway};
use heimdall_core::store::{ProfileStore, RouteError};
use heimdall_rdp::KnownRdpHosts;
use heimdall_ssh::{AgentSource, KnownHosts, PublicKey};
use heimdall_term::GridSize;

const GATEWAY_KEY: &str =
    include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519.pub");
const CERTIFICATE: &str = "SHA256:rgJ0Y04RcqyBpgUaWIbkDqk3bEPjR9jNUxyEc7cvFq0";

fn app(dir: &Path, gateway: Option<&str>, gateways: Vec<SshGateway>) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_rdp([RdpProfile {
        id: ProfileId::new("dc"),
        name: "Domain controller".to_owned(),
        group: None,
        host: "dc.internal".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: None,
        allow_tls_only: false,
        gateway: gateway.map(ProfileId::new),
        redirect_clipboard: true,
        redirect_drives: false,
        options: heimdall_core::profile::RdpOptions::default(),
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        follow_defaults: false,
        several_servers: false,
    }]);
    store.merge_gateways(gateways);
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

fn bastion() -> SshGateway {
    SshGateway {
        id: ProfileId::new("bastion"),
        name: "Bastion".to_owned(),
        host: "bastion.example.org".to_owned(),
        port: 2222,
        username: Some("jump".to_owned()),
        key_path: None,
        parent: None,
    }
}

/// The RDP tab opened, and the attempt it started.
fn open(app: &mut App) -> (TabId, AttemptId) {
    match app
        .update(Message::OpenRdp(ProfileId::new("dc")))
        .as_slice()
    {
        [Effect::ConnectRdp { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_rdp_profile_asks_for_its_gateways() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), Some("bastion"), vec![bastion()]);
    let effects = app.update(Message::OpenRdp(ProfileId::new("dc")));
    let [Effect::ConnectRdp { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let hops: Vec<(&str, u16)> = request
        .route
        .iter()
        .map(|hop| (hop.host.as_str(), hop.port))
        .collect();
    assert_eq!(hops, [("bastion.example.org", 2222)]);
    assert_eq!(request.ssh.known_hosts, dir.path().join("known_hosts"));
    assert_eq!(
        request.profile.host, "dc.internal",
        "the server, reached through it"
    );
}

#[test]
fn an_rdp_profile_whose_gateway_is_gone_fails_its_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), Some("gone"), Vec::new());
    assert!(
        app.update(Message::OpenRdp(ProfileId::new("dc")))
            .is_empty()
    );
    assert_eq!(
        app.active_tab().expect("tab").phase,
        Phase::Failed(UiError::Route(RouteError::MissingGateway(ProfileId::new(
            "gone"
        ))))
    );
}

#[test]
fn a_gateways_key_in_an_rdp_tab_is_learnt_as_the_gateways_and_the_rdp_attempt_starts_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), Some("bastion"), vec![bastion()]);
    let (tab, attempt) = open(&mut app);
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::UnknownHostKey {
            host: "bastion.example.org".to_owned(),
            port: 2222,
            fingerprint: "SHA256:x".to_owned(),
            key: Arc::new(PublicKey::from_openssh(GATEWAY_KEY.trim()).expect("key")),
        },
    });
    assert!(matches!(
        app.tab(tab).expect("tab").phase,
        Phase::HostKey { .. }
    ));
    let effects = app.update(Message::HostKeyDecision { tab, accept: true });
    assert!(
        matches!(effects.as_slice(), [Effect::ConnectRdp { request, .. }] if request.accepted.is_none()),
        "{effects:?}"
    );
    let recorded = KnownHosts::new(dir.path().join("known_hosts"))
        .recorded("bastion.example.org", 2222)
        .expect("read");
    assert_eq!(recorded.len(), 1, "learnt as the gateway's SSH key");
    assert!(
        !KnownRdpHosts::new(dir.path().join("known_rdp_hosts"))
            .knows("dc.internal", 3389)
            .expect("read"),
        "not as the server's certificate"
    );
}

#[test]
fn a_gateways_changed_key_is_forgotten_and_the_rdp_attempt_starts_again_its_certificate_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let known_rdp_hosts = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    known_rdp_hosts
        .record(
            "dc.internal",
            3389,
            &CERTIFICATE.parse().expect("fingerprint"),
        )
        .expect("recorded");
    let known_hosts = KnownHosts::new(dir.path().join("known_hosts"));
    known_hosts
        .learn(
            "bastion.example.org",
            2222,
            &PublicKey::from_openssh(GATEWAY_KEY.trim()).expect("key"),
        )
        .expect("learnt");
    let mut app = app(dir.path(), Some("bastion"), vec![bastion()]);
    let (tab, attempt) = open(&mut app);
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::HostKeyChanged {
            target: Some(heimdall_app::ServerAddress {
                host: "bastion.example.org".to_owned(),
                port: 2222,
            }),
            recorded: "SHA256:old".to_owned(),
            offered: "SHA256:new".to_owned(),
        }),
    });
    let effects = app.update(Message::ForgetServer(tab));
    assert!(
        matches!(effects.as_slice(), [Effect::ConnectRdp { tab: again, .. }] if *again == tab),
        "{effects:?}"
    );
    assert!(
        known_hosts
            .recorded("bastion.example.org", 2222)
            .expect("read")
            .is_empty(),
        "the gateway's key is forgotten: its new one is asked about"
    );
    assert!(
        known_rdp_hosts.knows("dc.internal", 3389).expect("read"),
        "the server's certificate stays"
    );
}
