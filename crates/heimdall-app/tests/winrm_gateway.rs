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

//! `WinRM` tabs through SSH gateways, as the C# Heimdall's tunnel: the route asked for, HTTPS
//! refused before anything connects, a gateway's SSH key learnt in the tab, and the NTLM
//! warning once `PowerShell` runs.

use std::path::Path;
use std::sync::Arc;

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, Notice, Phase, TabId, TabProfile,
    UiError,
};
use heimdall_core::profile::{
    DEFAULT_WINRM_HTTP_PORT, DEFAULT_WINRM_HTTPS_PORT, ProfileId, SshGateway, WinRmProfile,
};
use heimdall_core::store::{ProfileStore, RouteError};
use heimdall_ssh::{AgentSource, KnownHosts, PublicKey};
use heimdall_term::GridSize;

/// A session's input that takes everything.
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

const GATEWAY_KEY: &str =
    include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519.pub");

fn app(dir: &Path, profile: WinRmProfile) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_gateways(vec![bastion()]);
    store.merge_winrm([profile]);
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

/// "DC", through `gateway`.
fn profile(gateway: &str) -> WinRmProfile {
    WinRmProfile {
        id: ProfileId::new("w"),
        name: "DC".to_owned(),
        group: None,
        host: "dc.internal".to_owned(),
        port: DEFAULT_WINRM_HTTP_PORT,
        use_ssl: false,
        skip_certificate_check: false,
        username: Some(r"LAB\admin".to_owned()),
        gateway: Some(ProfileId::new(gateway)),
    }
}

/// The tab opened, and the attempt it started.
fn open(app: &mut App) -> (TabId, AttemptId) {
    match app
        .update(Message::OpenWinRm(ProfileId::new("w")))
        .as_slice()
    {
        [Effect::ConnectWinRm { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_winrm_profile_through_a_gateway_asks_for_its_route_before_powershell() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), profile("bastion"));
    let effects = app.update(Message::OpenWinRm(ProfileId::new("w")));
    let [Effect::ConnectWinRm { request, .. }] = effects.as_slice() else {
        panic!("not a local shell started at once: {effects:?}");
    };
    let hops: Vec<(&str, u16)> = request
        .route
        .iter()
        .map(|hop| (hop.host.as_str(), hop.port))
        .collect();
    assert_eq!(hops, [("bastion.example.org", 2222)]);
    assert_eq!(request.ssh.known_hosts, dir.path().join("known_hosts"));
    assert_eq!(
        (request.profile.host.as_str(), request.profile.port),
        ("dc.internal", DEFAULT_WINRM_HTTP_PORT),
        "the server, reached through it"
    );
    let tab = app.active_tab().expect("tab");
    assert!(matches!(tab.profile, TabProfile::WinRm(_)));
    assert_eq!(tab.phase, Phase::Connecting);
}

#[test]
fn https_through_a_gateway_starts_nothing_and_says_why_as_the_csharp() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(
        dir.path(),
        WinRmProfile {
            use_ssl: true,
            port: DEFAULT_WINRM_HTTPS_PORT,
            ..profile("bastion")
        },
    );
    assert!(
        app.update(Message::OpenWinRm(ProfileId::new("w")))
            .is_empty()
    );
    assert_eq!(
        app.active_tab().expect("tab").phase,
        Phase::Failed(UiError::WinRmHttpsThroughGateway)
    );
}

#[test]
fn a_winrm_profile_whose_gateway_is_gone_fails_its_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), profile("gone"));
    assert!(
        app.update(Message::OpenWinRm(ProfileId::new("w")))
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
fn a_gateways_key_is_learnt_in_the_tab_and_the_attempt_starts_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), profile("bastion"));
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
    let [
        Effect::ConnectWinRm {
            tab: again,
            attempt: next,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    assert_eq!(*again, tab, "in the same tab");
    assert_ne!(*next, attempt, "a new attempt");
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connecting);
    let recorded = KnownHosts::new(dir.path().join("known_hosts"))
        .recorded("bastion.example.org", 2222)
        .expect("read");
    assert_eq!(recorded.len(), 1, "learnt as the gateway's SSH key");
}

#[test]
fn once_powershell_runs_the_ntlm_fallback_is_said_as_the_csharp_warns() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), profile("bastion"));
    let (tab, attempt) = open(&mut app);
    assert_eq!(app.notice(), None);
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    });
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connected);
    assert_eq!(app.notice(), Some(&Notice::WinRmGatewayNtlm));
}

#[test]
fn a_winrm_profile_reached_directly_is_probed_with_no_route() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(
        dir.path(),
        WinRmProfile {
            gateway: None,
            ..profile("bastion")
        },
    );
    let effects = app.update(Message::OpenWinRm(ProfileId::new("w")));
    assert!(
        matches!(effects.as_slice(), [Effect::ConnectWinRm { request, .. }] if request.route.is_empty()),
        "{effects:?}"
    );
    let tab = app.active_tab().expect("tab");
    assert!(matches!(tab.profile, TabProfile::WinRm(_)));
}

#[test]
fn skipped_certificate_checks_are_said_once_powershell_runs_and_before_ntlm() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(
        dir.path(),
        WinRmProfile {
            gateway: None,
            use_ssl: true,
            skip_certificate_check: true,
            port: DEFAULT_WINRM_HTTPS_PORT,
            ..profile("bastion")
        },
    );
    let (tab, attempt) = open(&mut app);
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    });
    assert_eq!(app.notice(), Some(&Notice::WinRmCertificateSkipped));
}
