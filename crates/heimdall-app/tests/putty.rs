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

//! SSH profiles opened in `PuTTY`, as the C# external mode: no tab, the host key probed and
//! asked about first, then `PuTTY` started on the key trusted, and the status bar told.
//! Behind a gateway, the launch goes through it, its forward held until it is released.

use std::path::Path;
use std::sync::Arc;

use heimdall_app::putty::{HostKeyProbe, PuttyRefusal, PuttyStarted};
use heimdall_app::putty_driver::{PuttyRouteEvent, PuttyRouteId, PuttyRouteRequest};
use heimdall_app::x11_server::{X11Outcome, X11Settings};
use heimdall_app::{
    App, AppConfig, ConnectionEvent, Dialog, Effect, Message, Notice, QuestionKind,
    SettingsMessage, SystemCredentials, TunnelMessage, UiError,
};
use heimdall_core::post_connect::{PostConnect, PostConnectStep};
use heimdall_core::profile::{Forwards, ProfileId, SshGateway, SshMode, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, KnownHosts, PublicKey};
use heimdall_term::GridSize;

const SERVER_KEY: &str =
    include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519.pub");
const FINGERPRINT: &str = "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU";

fn profile() -> SshProfile {
    SshProfile {
        id: ProfileId::new("web"),
        name: "Web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 2222,
        username: Some("ops".to_owned()),
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: Forwards::default(),
        post_connect: PostConnect::default(),
        forward_agent: true,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
        ssh_mode: SshMode::External,
        x11_forwarding: false,
    }
}

/// An application with `profile` changed by `change` saved.
fn app_with(dir: &Path, change: impl FnOnce(&mut SshProfile)) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    let mut saved = profile();
    change(&mut saved);
    store.merge([saved]);
    store.merge_gateways([SshGateway {
        id: ProfileId::new("jump"),
        name: "JUMP".to_owned(),
        host: "jump.lab".to_owned(),
        port: 22,
        username: Some("ops".to_owned()),
        key_path: None,
        parent: None,
    }]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

/// Opens the profile: the probe it asks for, checked against the application's own
/// `known_hosts`.
fn open(app: &mut App, dir: &Path) -> SshProfile {
    let effects = app.update(Message::OpenProfile(ProfileId::new("web")));
    let [Effect::ProbePuttyHostKey { profile, options }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(options.known_hosts, dir.join("known_hosts"));
    assert!(app.tabs.is_empty(), "no tab");
    profile.as_ref().clone()
}

/// A change made to the profile saved.
type Change = fn(&mut SshProfile);

fn server_key() -> PublicKey {
    PublicKey::from_openssh(SERVER_KEY.trim()).expect("key")
}

fn unknown() -> HostKeyProbe {
    HostKeyProbe::Unknown {
        host: "web.lab".to_owned(),
        port: 2222,
        fingerprint: FINGERPRINT.to_owned(),
        key: Arc::new(server_key()),
    }
}

#[test]
fn a_trusted_key_starts_putty_with_it_and_no_tab_opens() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |_| {});
    let profile = open(&mut app, dir.path());
    let effects = app.update(Message::PuttyHostKey {
        profile: Box::new(profile),
        probe: HostKeyProbe::Trusted(FINGERPRINT.to_owned()),
    });
    let [Effect::LaunchPutty { name, launch }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(name, "Web");
    assert_eq!(launch.host_key, FINGERPRINT);
    assert!(launch.forward_agent && !launch.compression);
    assert_eq!(launch.x11, None);
    assert!(app.tabs.is_empty());

    app.update(Message::PuttyLaunched {
        name: name.clone(),
        result: Ok(PuttyStarted { x11: None }),
    });
    assert_eq!(app.notice(), Some(&Notice::PuttyLaunched("Web".to_owned())));
    app.update(Message::PuttyLaunched {
        name: name.clone(),
        result: Err(PuttyRefusal::NotFound),
    });
    assert_eq!(
        app.notice(),
        Some(&Notice::PuttyRefused(PuttyRefusal::NotFound))
    );
}

#[test]
fn an_embedded_profile_and_an_external_one_s_files_open_in_tabs() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |profile| profile.ssh_mode = SshMode::Embedded);
    let effects = app.update(Message::OpenProfile(ProfileId::new("web")));
    assert!(
        matches!(effects.as_slice(), [Effect::Connect { .. }]),
        "{effects:?}"
    );
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |_| {});
    let effects = app.update(Message::OpenFiles(ProfileId::new("web")));
    assert!(
        matches!(effects.as_slice(), [Effect::Connect { .. }]),
        "{effects:?}"
    );
}

#[test]
fn steps_to_type_ask_nothing_as_putty_types_none() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |profile| {
        profile.post_connect = PostConnect {
            steps: vec![PostConnectStep::new("sudo -i")],
            approved: None,
        };
    });
    open(&mut app, dir.path());
    assert_eq!(app.dialog, None);
}

#[test]
fn an_unknown_key_is_asked_about_learnt_then_probed_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |_| {});
    let profile = open(&mut app, dir.path());
    let asked = app.update(Message::PuttyHostKey {
        profile: Box::new(profile),
        probe: unknown(),
    });
    assert!(asked.is_empty(), "{asked:?}");
    assert!(matches!(
        &app.dialog,
        Some(Dialog::TunnelHostKey { host, port: 2222, fingerprint, algorithm })
            if host == "web.lab" && fingerprint == FINGERPRINT && algorithm == "ssh-ed25519"
    ));
    let effects = app.update(Message::ConfirmDialog);
    assert!(
        matches!(effects.as_slice(), [Effect::ProbePuttyHostKey { profile, .. }] if profile.host == "web.lab"),
        "{effects:?}"
    );
    let recorded = KnownHosts::new(dir.path().join("known_hosts"))
        .recorded("web.lab", 2222)
        .expect("read");
    assert!(
        matches!(recorded.as_slice(), [learnt] if learnt.key_data() == server_key().key_data()),
        "{recorded:?}"
    );
}

#[test]
fn a_key_trusted_once_is_not_written_and_a_key_refused_stops_the_launch() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |_| {});
    let profile = open(&mut app, dir.path());
    app.update(Message::PuttyHostKey {
        profile: Box::new(profile.clone()),
        probe: unknown(),
    });
    let effects = app.update(Message::Tunnel(TunnelMessage::TrustKeyOnce));
    let [Effect::ProbePuttyHostKey { options, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert!(
        !options.run_trust.keys("web.lab", 2222).is_empty(),
        "trusted for the run"
    );
    assert!(!dir.path().join("known_hosts").exists(), "never written");

    app.update(Message::PuttyHostKey {
        profile: Box::new(profile),
        probe: unknown(),
    });
    assert!(app.update(Message::DismissDialog).is_empty());
    assert_eq!(
        app.notice(),
        Some(&Notice::PuttyRefused(PuttyRefusal::HostKey(
            UiError::Cancelled
        )))
    );
}

#[test]
fn a_changed_key_or_an_unreachable_server_stops_the_launch() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |_| {});
    let profile = open(&mut app, dir.path());
    let changed = UiError::HostKeyChanged {
        target: None,
        recorded: "SHA256:old".to_owned(),
        offered: FINGERPRINT.to_owned(),
    };
    let effects = app.update(Message::PuttyHostKey {
        profile: Box::new(profile),
        probe: HostKeyProbe::Failed(changed.clone()),
    });
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(
        app.notice(),
        Some(&Notice::PuttyRefused(PuttyRefusal::HostKey(changed)))
    );
    assert_eq!(app.dialog, None);
}

#[test]
fn a_host_or_user_read_as_an_option_is_refused_before_anything_is_dialled() {
    let cases: [(Change, PuttyRefusal); 2] = [
        (
            |profile| profile.host = "-oProxyCommand=calc".to_owned(),
            PuttyRefusal::InvalidHost,
        ),
        (
            |profile| profile.username = Some("-l root".to_owned()),
            PuttyRefusal::InvalidUsername,
        ),
    ];
    for (change, refusal) in cases {
        let dir = tempfile::tempdir().expect("dir");
        let mut app = app_with(dir.path(), change);
        let effects = app.update(Message::OpenProfile(ProfileId::new("web")));
        assert!(effects.is_empty(), "{effects:?}");
        assert!(app.tabs.is_empty());
        assert_eq!(app.notice(), Some(&Notice::PuttyRefused(refusal)));
    }
}

#[test]
fn x11_brings_the_settings_x_server_and_without_one_the_csharp_notice_is_said() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |profile| profile.x11_forwarding = true);
    for message in [
        SettingsMessage::PuttyPath("  /opt/putty/putty ".to_owned()),
        SettingsMessage::X11ServerPath("/opt/x/vcxsrv".to_owned()),
        SettingsMessage::X11AutoStart(false),
    ] {
        app.update(Message::Settings(message));
    }
    assert_eq!(app.settings().putty_path, "/opt/putty/putty", "trimmed");
    let profile = open(&mut app, dir.path());
    let effects = app.update(Message::PuttyHostKey {
        profile: Box::new(profile),
        probe: HostKeyProbe::Trusted(FINGERPRINT.to_owned()),
    });
    let [Effect::LaunchPutty { name, launch }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(launch.putty_path, "/opt/putty/putty");
    assert_eq!(
        launch.x11,
        Some(X11Settings {
            server_path: "/opt/x/vcxsrv".to_owned(),
            auto_start: false,
        })
    );
    app.update(Message::PuttyLaunched {
        name: name.clone(),
        result: Ok(PuttyStarted {
            x11: Some(X11Outcome::Unavailable),
        }),
    });
    assert_eq!(app.notice(), Some(&Notice::X11ServerNotFound));
    app.update(Message::PuttyLaunched {
        name: name.clone(),
        result: Ok(PuttyStarted {
            x11: Some(X11Outcome::Running),
        }),
    });
    assert_eq!(app.notice(), Some(&Notice::PuttyLaunched("Web".to_owned())));
}

/// Opens the profile behind the gateway "jump": the launch through it it asks for.
fn open_routed(app: &mut App) -> (PuttyRouteId, Box<PuttyRouteRequest>) {
    let effects = app.update(Message::OpenProfile(ProfileId::new("web")));
    let mut effects = effects.into_iter();
    let (Some(Effect::OpenPuttyRoute { id, request }), None) = (effects.next(), effects.next())
    else {
        panic!("one launch through the gateway");
    };
    assert!(app.tabs.is_empty(), "no tab");
    (id, request)
}

fn routed(profile: &mut SshProfile) {
    profile.gateway = Some(ProfileId::new("jump"));
}

#[test]
fn a_profile_behind_a_gateway_is_launched_through_it_not_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), routed);
    let (id, request) = open_routed(&mut app);
    assert!(request.before.is_empty());
    assert_eq!(request.gateway.host, "jump.lab");
    assert_eq!(
        (request.profile.host.as_str(), request.profile.port),
        ("web.lab", 2222)
    );
    assert_eq!(request.ssh.known_hosts, dir.path().join("known_hosts"));
    assert!(request.launch.host_key.is_empty(), "filled once probed");
    assert_eq!(app.notice(), None);

    app.update(Message::PuttyRoute {
        id,
        event: PuttyRouteEvent::Launched(Ok(PuttyStarted { x11: None })),
    });
    assert_eq!(
        app.notice(),
        Some(&Notice::PuttyLaunchedThrough {
            name: "Web".to_owned(),
            gateway: "JUMP".to_owned(),
        })
    );
    // Its gateway's questions are answered from what is saved, here nothing.
    let question = heimdall_app::QuestionId::fresh();
    let asked = |app: &mut App| {
        app.update(Message::PuttyRoute {
            id,
            event: PuttyRouteEvent::Route(ConnectionEvent::Question {
                question,
                kind: QuestionKind::Password(heimdall_ssh::PasswordQuestion {
                    host: "jump.lab".to_owned(),
                    port: 22,
                    username: "ops".to_owned(),
                    attempt: 1,
                }),
            }),
        })
    };
    assert!(
        matches!(
            asked(&mut app).as_slice(),
            [Effect::Answer { answer: None, .. }]
        ),
        "nothing saved"
    );
    app.update(Message::PuttyRoute {
        id,
        event: PuttyRouteEvent::Released,
    });
    assert!(
        matches!(
            asked(&mut app).as_slice(),
            [Effect::Answer { answer: None, .. }]
        ),
        "released: declined"
    );
}

#[test]
fn a_gateway_that_fails_stops_the_launch_with_its_reason() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), routed);
    let (id, _) = open_routed(&mut app);
    let effects = app.update(Message::PuttyRoute {
        id,
        event: PuttyRouteEvent::Route(ConnectionEvent::Failed(UiError::ConnectionLost)),
    });
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(
        app.notice(),
        Some(&Notice::PuttyRefused(PuttyRefusal::Gateway(
            UiError::ConnectionLost
        )))
    );
    // Gone: a late launch is not said.
    app.update(Message::PuttyRoute {
        id,
        event: PuttyRouteEvent::Launched(Ok(PuttyStarted { x11: None })),
    });
    assert!(matches!(app.notice(), Some(Notice::PuttyRefused(_))));
}

#[test]
fn the_server_s_unknown_key_seen_through_the_gateway_is_asked_then_launched_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), routed);
    let (id, _) = open_routed(&mut app);
    let asked = app.update(Message::PuttyRoute {
        id,
        event: PuttyRouteEvent::HostKey(unknown()),
    });
    assert!(asked.is_empty(), "{asked:?}");
    assert!(matches!(
        &app.dialog,
        Some(Dialog::TunnelHostKey { host, port: 2222, .. }) if host == "web.lab"
    ));
    let effects = app.update(Message::ConfirmDialog);
    assert!(
        matches!(effects.as_slice(), [Effect::OpenPuttyRoute { id: again, .. }] if *again != id),
        "{effects:?}"
    );
    let recorded = KnownHosts::new(dir.path().join("known_hosts"))
        .recorded("web.lab", 2222)
        .expect("read");
    assert_eq!(
        recorded.len(),
        1,
        "learnt for the server's own host and port"
    );
}

#[test]
fn the_gateway_s_unknown_key_is_asked_about_and_a_changed_server_key_stops_the_launch() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), routed);
    let (id, _) = open_routed(&mut app);
    app.update(Message::PuttyRoute {
        id,
        event: PuttyRouteEvent::Route(ConnectionEvent::UnknownHostKey {
            host: "jump.lab".to_owned(),
            port: 22,
            fingerprint: FINGERPRINT.to_owned(),
            key: Arc::new(server_key()),
        }),
    });
    assert!(matches!(
        &app.dialog,
        Some(Dialog::TunnelHostKey { host, port: 22, .. }) if host == "jump.lab"
    ));
    assert!(app.update(Message::DismissDialog).is_empty());

    let (id, _) = open_routed(&mut app);
    let changed = UiError::HostKeyChanged {
        target: None,
        recorded: "SHA256:old".to_owned(),
        offered: FINGERPRINT.to_owned(),
    };
    app.update(Message::PuttyRoute {
        id,
        event: PuttyRouteEvent::HostKey(HostKeyProbe::Failed(changed.clone())),
    });
    assert_eq!(
        app.notice(),
        Some(&Notice::PuttyRefused(PuttyRefusal::HostKey(changed)))
    );
    assert_eq!(app.dialog, None);
}

#[test]
fn closing_the_application_releases_every_forward() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), routed);
    let (id, request) = open_routed(&mut app);
    app.update(Message::PuttyRoute {
        id,
        event: PuttyRouteEvent::Launched(Ok(PuttyStarted { x11: None })),
    });
    assert!(!request.cancel.is_cancelled());
    let effects = app.update(Message::WindowCloseRequested);
    assert!(
        effects.iter().any(|effect| matches!(effect, Effect::Exit)),
        "{effects:?}"
    );
    assert!(request.cancel.is_cancelled(), "released at exit");
}
