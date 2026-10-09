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

//! Tunnels opened by hand, as the C# "New tunnel" dialog and tunnels panel: the dialog and
//! its checks, the attempt answered only from what is saved for its gateway, a gateway's
//! unknown key asked about in its own dialog, then the rows and their closing.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;

use heimdall_app::profile_draft::ProfileField;
use heimdall_app::tunnel::{TunnelEvent, TunnelField, TunnelId, TunnelProblem};
use heimdall_app::{
    Answer, App, AppConfig, ConnectionEvent, Dialog, Effect, Message, Notice, QuestionId,
    QuestionKind, SystemCredentials, TunnelMessage, UiError,
};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, KnownHosts, PasswordQuestion, PublicKey, Secret};
use heimdall_term::GridSize;

const GATEWAY_PASSWORD: &str = "jump pw";
const GATEWAY_KEY: &str =
    include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519.pub");
const LOCAL_PORT: u16 = 9443;

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    ProfileStore::open(&profiles_file)
        .expect("store")
        .save()
        .expect("save");
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

/// Adds gateway "bastion" at `bastion.lab`, account `jump`, with `GATEWAY_PASSWORD` saved.
fn save_gateway(app: &mut App) {
    app.update(Message::NewGateway);
    for (field, value) in [
        (ProfileField::Name, "bastion"),
        (ProfileField::Host, "bastion.lab"),
        (ProfileField::Username, "jump"),
    ] {
        app.update(Message::GatewayField {
            field,
            value: value.to_owned(),
        });
    }
    app.update(Message::SaveGateway {
        password: Some(Secret::new(GATEWAY_PASSWORD.to_owned())),
        passphrase: None,
    });
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
}

fn tunnel(app: &mut App, message: TunnelMessage) -> Vec<Effect> {
    app.update(Message::Tunnel(message))
}

fn type_in(app: &mut App, field: TunnelField, value: &str) {
    tunnel(
        app,
        TunnelMessage::Field {
            field,
            value: value.to_owned(),
        },
    );
}

/// Fills the dialog for `wiki.lab:443` on `LOCAL_PORT`, labelled "wiki", and opens it: the
/// tunnel's identifier.
fn open_tunnel(app: &mut App) -> TunnelId {
    open_tunnel_on(app, LOCAL_PORT)
}

/// The same, on local port `port`.
fn open_tunnel_on(app: &mut App, port: u16) -> TunnelId {
    tunnel(app, TunnelMessage::New);
    type_in(app, TunnelField::RemoteHost, "wiki.lab");
    type_in(app, TunnelField::RemotePort, "443");
    type_in(app, TunnelField::LocalPort, &port.to_string());
    type_in(app, TunnelField::Label, "wiki");
    match app.update(Message::ConfirmDialog).as_slice() {
        [Effect::OpenTunnel { id, .. }] => *id,
        other => panic!("{other:?}"),
    }
}

fn event(app: &mut App, id: TunnelId, event: TunnelEvent) -> Vec<Effect> {
    tunnel(app, TunnelMessage::Event { id, event })
}

fn local() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, LOCAL_PORT))
}

/// What the attempt `id` is answered with by itself, asked `kind`.
fn answered(app: &mut App, id: TunnelId, kind: QuestionKind) -> Option<String> {
    let question = QuestionId::fresh();
    let effects = event(
        app,
        id,
        TunnelEvent::Route(ConnectionEvent::Question { question, kind }),
    );
    match effects.as_slice() {
        [
            Effect::Answer {
                question: asked,
                answer,
            },
        ] if *asked == question => match answer {
            Some(Answer::Secret(secret)) => Some(secret.expose().to_owned()),
            None => None,
            Some(_) => panic!("not a password"),
        },
        other => panic!("every question is answered, if only by no: {other:?}"),
    }
}

fn gateway_password(host: &str, attempt: u32) -> QuestionKind {
    QuestionKind::Password(PasswordQuestion {
        host: host.to_owned(),
        port: 22,
        username: "jump".to_owned(),
        attempt,
    })
}

#[test]
fn the_dialog_starts_on_the_first_gateway_with_the_csharp_defaults_and_says_what_is_missing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    tunnel(&mut app, TunnelMessage::New);
    let Some(Dialog::NewTunnel(form)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(form.gateway, None, "no gateway saved");
    app.update(Message::DismissDialog);

    save_gateway(&mut app);
    tunnel(&mut app, TunnelMessage::New);
    let Some(Dialog::NewTunnel(form)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(form.gateway.as_ref(), app.gateways().first().map(|g| &g.id));
    assert_eq!(
        (form.remote_port.as_str(), form.local_port.as_str()),
        ("22", "9090")
    );
    assert_eq!(app.tunnel_problem(), Some(TunnelProblem::RemoteHost));

    // Refused while something is missing: the dialog stays, nothing starts.
    assert!(app.update(Message::ConfirmDialog).is_empty());
    assert!(matches!(app.dialog, Some(Dialog::NewTunnel(_))));
    type_in(&mut app, TunnelField::RemoteHost, "wiki.lab");
    assert_eq!(app.tunnel_problem(), None);
}

#[test]
fn a_tunnel_opens_through_its_gateway_and_becomes_a_row_whose_port_is_taken() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    save_gateway(&mut app);
    tunnel(&mut app, TunnelMessage::New);
    type_in(&mut app, TunnelField::RemoteHost, "wiki.lab");
    type_in(&mut app, TunnelField::RemotePort, "443");
    type_in(&mut app, TunnelField::LocalPort, &LOCAL_PORT.to_string());
    let effects = app.update(Message::ConfirmDialog);
    let [Effect::OpenTunnel { id, request }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let id = *id;
    assert!(app.dialog.is_none(), "the dialog closes at once, as the C#");
    assert_eq!(
        (request.gateway.host.as_str(), request.before.len()),
        ("bastion.lab", 0)
    );
    assert_eq!(
        (
            request.remote_host.as_str(),
            request.remote_port,
            request.local_port
        ),
        ("wiki.lab", 443, LOCAL_PORT)
    );
    assert!(app.tunnels.is_empty(), "a row once it listens");

    event(&mut app, id, TunnelEvent::Opened(local()));
    let [row] = app.tunnels.as_slice() else {
        panic!("{:?}", app.tunnels);
    };
    assert_eq!((row.gateway_name.as_str(), row.local), ("bastion", local()));
    assert_eq!(
        app.notice(),
        Some(&Notice::TunnelOpened {
            port: LOCAL_PORT,
            host: "wiki.lab".to_owned(),
            remote_port: 443,
        })
    );

    tunnel(&mut app, TunnelMessage::New);
    type_in(&mut app, TunnelField::RemoteHost, "other.lab");
    type_in(&mut app, TunnelField::LocalPort, &LOCAL_PORT.to_string());
    assert_eq!(
        app.tunnel_problem(),
        Some(TunnelProblem::LocalPortInUse(LOCAL_PORT))
    );
}

#[test]
fn only_the_gateway_s_saved_password_is_given_once_and_anything_else_is_declined() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    save_gateway(&mut app);
    let id = open_tunnel(&mut app);
    assert_eq!(
        answered(&mut app, id, gateway_password("other.lab", 1)),
        None,
        "not the gateway's endpoint"
    );
    assert_eq!(
        answered(&mut app, id, gateway_password("bastion.lab", 1)).as_deref(),
        Some(GATEWAY_PASSWORD)
    );
    assert_eq!(
        answered(&mut app, id, gateway_password("bastion.lab", 2)),
        None,
        "asked again: refused, and nobody is asked"
    );
    event(
        &mut app,
        id,
        TunnelEvent::Route(ConnectionEvent::Failed(UiError::Timeout)),
    );
    assert_eq!(app.notice(), Some(&Notice::TunnelFailed(UiError::Timeout)));

    // Refused once, never given again this session.
    let again = open_tunnel(&mut app);
    assert_eq!(
        answered(&mut app, again, gateway_password("bastion.lab", 1)),
        None
    );
}

#[test]
fn a_gateway_s_unknown_key_is_asked_about_then_learnt_and_the_tunnel_tried_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    save_gateway(&mut app);
    let id = open_tunnel(&mut app);
    let key = PublicKey::from_openssh(GATEWAY_KEY.trim()).expect("key");
    let unknown = || {
        TunnelEvent::Route(ConnectionEvent::UnknownHostKey {
            host: "bastion.lab".to_owned(),
            port: 22,
            fingerprint: "SHA256:fingerprint".to_owned(),
            key: Arc::new(key.clone()),
        })
    };
    assert!(event(&mut app, id, unknown()).is_empty());
    assert!(matches!(
        &app.dialog,
        Some(Dialog::TunnelHostKey { host, port: 22, .. }) if host == "bastion.lab"
    ));
    assert!(
        app.dialog
            .as_ref()
            .is_some_and(|dialog| !dialog.confirms_on_enter()),
        "trusted for good by a click, never by an Enter"
    );
    let effects = app.update(Message::ConfirmDialog);
    let [Effect::OpenTunnel { id: retried, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_ne!(*retried, id, "a new attempt");
    let known = KnownHosts::new(dir.path().join("known_hosts"));
    let recorded = known.recorded("bastion.lab", 22).expect("read");
    assert!(
        matches!(recorded.as_slice(), [learnt] if learnt.key_data() == key.key_data()),
        "{recorded:?}"
    );

    // Refused: not learnt, not tried again.
    let second = *retried;
    event(&mut app, second, unknown());
    assert!(app.update(Message::DismissDialog).is_empty());
    assert_eq!(
        app.notice(),
        Some(&Notice::TunnelFailed(UiError::Cancelled))
    );
}

#[test]
fn a_pin_carried_over_while_the_gateway_s_key_is_asked_about_refuses_it_as_a_changed_key() {
    const OTHER_KEY: &str =
        include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519-other.pub");

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    save_gateway(&mut app);
    let id = open_tunnel(&mut app);
    let key = PublicKey::from_openssh(GATEWAY_KEY.trim()).expect("key");
    event(
        &mut app,
        id,
        TunnelEvent::Route(ConnectionEvent::UnknownHostKey {
            host: "bastion.lab".to_owned(),
            port: 22,
            fingerprint: "SHA256:fingerprint".to_owned(),
            key: Arc::new(key.clone()),
        }),
    );
    assert!(matches!(&app.dialog, Some(Dialog::TunnelHostKey { .. })));
    // The C# trust carried over meanwhile: the gateway pinned to another key.
    let pinned =
        heimdall_ssh::fingerprint(&PublicKey::from_openssh(OTHER_KEY.trim()).expect("key"));
    let known = KnownHosts::new(dir.path().join("known_hosts"));
    heimdall_ssh::carry_over(
        &known,
        &[heimdall_core::import::csharp::TrustedHostKey {
            host: "bastion.lab".to_owned(),
            port: 22,
            fingerprint: pinned.clone(),
            key: None,
            source: heimdall_core::import::csharp::TrustedHostKeySource::Unknown,
            first_seen: None,
            last_seen: None,
        }],
    )
    .expect("carried over");

    let effects = app.update(Message::ConfirmDialog);
    assert!(effects.is_empty(), "not tried again: {effects:?}");
    assert!(
        matches!(
            app.notice(),
            Some(Notice::TunnelFailed(UiError::HostKeyChanged {
                target: Some(target),
                recorded,
                offered,
            })) if target.host == "bastion.lab" && target.port == 22
                && *recorded == pinned && *offered == heimdall_ssh::fingerprint(&key)
        ),
        "{:?}",
        app.notice()
    );
    assert!(known.recorded("bastion.lab", 22).expect("read").is_empty());
    assert!(!known.path().exists(), "the key never written");
    assert_eq!(
        heimdall_ssh::Pins::beside(known.path())
            .pinned("bastion.lab", 22)
            .expect("pins"),
        [pinned],
        "the pin kept"
    );
}

#[test]
fn a_key_arriving_while_another_dialog_is_open_is_never_asked_nor_accepted() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    save_gateway(&mut app);
    let id = open_tunnel(&mut app);
    tunnel(&mut app, TunnelMessage::New);
    event(
        &mut app,
        id,
        TunnelEvent::Route(ConnectionEvent::UnknownHostKey {
            host: "bastion.lab".to_owned(),
            port: 22,
            fingerprint: "SHA256:fingerprint".to_owned(),
            key: Arc::new(PublicKey::from_openssh(GATEWAY_KEY.trim()).expect("key")),
        }),
    );
    assert!(
        matches!(app.dialog, Some(Dialog::NewTunnel(_))),
        "untouched"
    );
    assert_eq!(
        app.notice(),
        Some(&Notice::TunnelFailed(UiError::Cancelled))
    );
    let known = KnownHosts::new(dir.path().join("known_hosts"));
    assert!(known.recorded("bastion.lab", 22).expect("read").is_empty());
}

#[test]
fn a_row_is_closed_by_the_user_or_by_its_gateway_and_its_port_copied() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    save_gateway(&mut app);
    let id = open_tunnel(&mut app);
    event(&mut app, id, TunnelEvent::Opened(local()));

    let copied = tunnel(&mut app, TunnelMessage::CopyPort(id));
    assert!(
        matches!(copied.as_slice(), [Effect::WriteClipboard(port)] if *port == LOCAL_PORT.to_string()),
        "{copied:?}"
    );
    assert_eq!(app.notice(), Some(&Notice::PortCopied(LOCAL_PORT)));

    tunnel(&mut app, TunnelMessage::Close(id));
    assert!(app.tunnels.is_empty());
    assert_eq!(
        app.notice(),
        Some(&Notice::TunnelClosed {
            port: LOCAL_PORT,
            error: None
        })
    );
    // A question still on its way once closed is declined.
    assert_eq!(
        answered(&mut app, id, gateway_password("bastion.lab", 1)),
        None
    );

    let lost = open_tunnel(&mut app);
    event(&mut app, lost, TunnelEvent::Opened(local()));
    event(&mut app, lost, TunnelEvent::Closed);
    assert_eq!(
        app.notice(),
        Some(&Notice::TunnelClosed {
            port: LOCAL_PORT,
            error: Some(UiError::ConnectionLost)
        })
    );
    // Its row stays, said interrupted, as the C# one; its port is free again.
    assert!(app.tunnel(lost).expect("kept").interrupted);
    assert_eq!(app.live_tunnels(), 0);
    assert!(app.tunnel_ports().is_empty());
    // Opened again as it was asked for, its row replaced by the new attempt's.
    let effects = tunnel(&mut app, TunnelMessage::Reopen(lost));
    let [Effect::OpenTunnel { id: again, request }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (request.local_port, request.remote_host.as_str()),
        (LOCAL_PORT, "wiki.lab")
    );
    assert!(app.tunnel(lost).is_none());
    let again = *again;
    event(&mut app, again, TunnelEvent::Opened(local()));
    assert_eq!(app.live_tunnels(), 1);
    assert!(
        tunnel(&mut app, TunnelMessage::Reopen(again)).is_empty(),
        "only an interrupted one"
    );
    // Lost again, then closed: the row goes without a second word.
    event(&mut app, again, TunnelEvent::Closed);
    tunnel(&mut app, TunnelMessage::Close(again));
    assert!(app.tunnels.is_empty());
    assert_eq!(
        app.notice(),
        Some(&Notice::TunnelClosed {
            port: LOCAL_PORT,
            error: Some(UiError::ConnectionLost)
        }),
        "the loss said once"
    );

    for port in [LOCAL_PORT, LOCAL_PORT + 1] {
        let id = open_tunnel_on(&mut app, port);
        event(
            &mut app,
            id,
            TunnelEvent::Opened(SocketAddr::from((Ipv4Addr::LOCALHOST, port))),
        );
    }
    assert_eq!(app.tunnels.len(), 2);
    let first = app.tunnels[0].id;
    event(&mut app, first, TunnelEvent::Closed);
    tunnel(&mut app, TunnelMessage::CloseAll);
    assert!(app.tunnels.is_empty());
    assert!(app.tunnel_ports().is_empty());
    assert_eq!(app.notice(), Some(&Notice::AllTunnelsClosed));
}

#[test]
fn the_panel_starts_as_the_settings_say_collapsed_unless_chosen() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert!(!app.tunnels_panel(), "collapsed, as the C# default");
    assert!(app.settings().collapse_tunnels_panel);
    app.update(Message::Settings(
        heimdall_app::SettingsMessage::CollapseTunnelsPanel(false),
    ));
    assert!(!app.tunnels_panel(), "the panel shown now is left as it is");
    assert!(
        self::app(dir.path()).tunnels_panel(),
        "open at the next start"
    );
}

#[test]
fn a_gateway_s_key_trusted_once_opens_the_tunnel_and_is_never_written_and_its_fingerprint_copies() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    save_gateway(&mut app);
    let id = open_tunnel(&mut app);
    let key = PublicKey::from_openssh(GATEWAY_KEY.trim()).expect("key");
    event(
        &mut app,
        id,
        TunnelEvent::Route(ConnectionEvent::UnknownHostKey {
            host: "bastion.lab".to_owned(),
            port: 22,
            fingerprint: "SHA256:fingerprint".to_owned(),
            key: Arc::new(key.clone()),
        }),
    );
    assert!(matches!(
        &app.dialog,
        Some(Dialog::TunnelHostKey { algorithm, .. }) if algorithm == "ssh-ed25519"
    ));
    let copied = tunnel(&mut app, TunnelMessage::CopyKeyFingerprint);
    assert!(
        matches!(copied.as_slice(), [Effect::WriteClipboard(text)] if text == "SHA256:fingerprint"),
        "{copied:?}"
    );
    assert!(app.dialog.is_some(), "copying answers nothing");

    let effects = tunnel(&mut app, TunnelMessage::TrustKeyOnce);
    let [Effect::OpenTunnel { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert!(app.dialog.is_none());
    assert_eq!(
        request.ssh.run_trust.keys("bastion.lab", 22),
        [key],
        "trusted for this run"
    );
    let known = KnownHosts::new(dir.path().join("known_hosts"));
    assert!(
        known.recorded("bastion.lab", 22).expect("read").is_empty(),
        "never written down"
    );
}

/// An SSH profile `id`, saved nowhere yet.
fn ssh_profile(id: &str) -> heimdall_core::profile::SshProfile {
    heimdall_core::profile::SshProfile {
        id: heimdall_core::profile::ProfileId::new(id),
        name: format!("server {id}"),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
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

/// The tab profile `id` opens.
fn connect(app: &mut App, id: &str) -> heimdall_app::TabId {
    app.update(Message::ConnectProfile(
        heimdall_core::profile::ProfileId::new(id),
    ));
    app.active.expect("a tab shown")
}

#[test]
fn the_panel_is_the_tab_s_then_its_profile_s_choice_as_the_csharp_resolves_it() {
    use heimdall_core::profile::ProfileId;

    let dir = tempfile::tempdir().expect("dir");
    let profiles_file = dir.path().join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([ssh_profile("a"), ssh_profile("b")]);
    store.save().expect("save");
    let mut app = app(dir.path());
    assert!(!app.tunnels_panel(), "collapsed, as the settings say");

    let a = connect(&mut app, "a");
    tunnel(&mut app, TunnelMessage::TogglePanel);
    assert!(app.tunnels_panel(), "opened while a is shown");
    let b = connect(&mut app, "b");
    assert!(!app.tunnels_panel(), "b has chosen nothing: the default");
    app.update(Message::SelectTab(a));
    assert!(app.tunnels_panel(), "a's choice again");
    app.update(Message::SelectTab(b));
    assert!(!app.tunnels_panel());

    // The profile keeps it: a opened again in a new run finds the panel open.
    let store = ProfileStore::open(&profiles_file).expect("store");
    assert_eq!(
        store
            .metadata(&ProfileId::new("a"))
            .and_then(|metadata| metadata.tunnels_expanded),
        Some(true)
    );
    assert_eq!(
        store
            .metadata(&ProfileId::new("b"))
            .and_then(|metadata| metadata.tunnels_expanded),
        None,
        "b untouched"
    );
    let mut again = self::app(dir.path());
    assert!(!again.tunnels_panel(), "no session shown: the default");
    connect(&mut again, "a");
    assert!(again.tunnels_panel(), "as profile a keeps it");
    // Closed there, a's profile keeps that too, the tab's choice winning at once.
    tunnel(&mut again, TunnelMessage::TogglePanel);
    assert!(!again.tunnels_panel());
    assert_eq!(
        ProfileStore::open(&profiles_file)
            .expect("store")
            .metadata(&ProfileId::new("a"))
            .and_then(|metadata| metadata.tunnels_expanded),
        Some(false)
    );
}

#[test]
fn a_session_through_a_gateway_is_listed_as_its_route_and_close_all_leaves_it() {
    let dir = tempfile::tempdir().expect("dir");
    let profiles_file = dir.path().join("profiles.toml");
    let gateway = {
        let mut app = app(dir.path());
        save_gateway(&mut app);
        app.gateways()[0].id.clone()
    };
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([
        heimdall_core::profile::SshProfile {
            gateway: Some(gateway),
            ..ssh_profile("inner")
        },
        ssh_profile("direct"),
    ]);
    store.save().expect("save");
    let mut app = self::app(dir.path());
    assert!(app.session_routes().is_empty(), "no session open");

    let effects = app.update(Message::ConnectProfile(
        heimdall_core::profile::ProfileId::new("inner"),
    ));
    let [
        Effect::Connect {
            tab: inner,
            attempt,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("one connection: {effects:?}");
    };
    let (inner, attempt) = (*inner, *attempt);
    connect(&mut app, "direct");
    let routes = app.session_routes();
    assert_eq!(
        routes.len(),
        1,
        "the direct session goes through nothing: {routes:?}"
    );
    let route = &routes[0];
    assert_eq!(route.tab, inner);
    assert_eq!(route.route, ["bastion"]);
    assert_eq!(route.remote, ("inner.lab".to_owned(), 22));
    assert_eq!(route.title, "server inner");
    assert!(!route.interrupted, "connecting");

    // Close All is the hand-opened tunnels': the session stays.
    tunnel(&mut app, TunnelMessage::CloseAll);
    assert!(app.tab(inner).is_some());
    assert_eq!(app.session_routes().len(), 1);

    // Its session failed: its route is said interrupted.
    app.update(Message::Connection {
        tab: inner,
        attempt,
        event: ConnectionEvent::Failed(UiError::ConnectionLost),
    });
    assert!(app.session_routes()[0].interrupted);
}
