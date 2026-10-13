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

//! SSH gateways as the C# Heimdall handles them: added from a session's form or the menus,
//! chosen in the form's gateway routing, their own password saved and given only to them.

use std::path::Path;

use heimdall_app::profile_draft::{DraftError, ProfileField, ProfileToggle};
use heimdall_app::{
    Answer, App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, GatewayBadge, Message,
    QuestionId, QuestionKind, SystemCredentials, TabId, UiError,
};
use heimdall_core::profile::{ProfileId, SshGateway, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{
    AgentSource, AuthMethod, KnownHosts, PasswordQuestion, Pins, PublicKey, Secret, fingerprint,
};
use heimdall_term::GridSize;

fn id(value: &str) -> ProfileId {
    ProfileId::new(value)
}

fn gateway(gateway_id: &str, parent: Option<&str>) -> SshGateway {
    SshGateway {
        id: id(gateway_id),
        name: format!("{gateway_id} gateway"),
        host: format!("{gateway_id}.lab"),
        port: 22,
        username: Some("jump".to_owned()),
        key_path: None,
        parent: parent.map(id),
    }
}

/// Server `web` behind `gateway`, if any, and the gateways given.
fn app(dir: &Path, route: Option<&str>, gateways: Vec<SshGateway>) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_gateways(gateways);
    store.merge([SshProfile {
        id: id("web"),
        name: "web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: route.map(id),
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

fn gateway_field(app: &mut App, field: ProfileField, value: &str) {
    app.update(Message::GatewayField {
        field,
        value: value.to_owned(),
    });
}

fn fill_gateway(app: &mut App, name: &str) {
    gateway_field(app, ProfileField::Name, name);
    gateway_field(app, ProfileField::Host, &format!("{name}.lab"));
    gateway_field(app, ProfileField::Username, "jump");
}

fn session_gateway(app: &App) -> Option<ProfileId> {
    app.profiles()
        .iter()
        .find(|profile| profile.id == id("web"))
        .and_then(|profile| profile.gateway.clone())
}

#[test]
fn a_gateway_added_from_a_sessions_form_returns_to_it_chosen() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), None, Vec::new());
    app.update(Message::EditProfile(id("web")));
    app.update(Message::NewGateway);
    assert!(matches!(app.dialog, Some(Dialog::EditGateway { .. })));
    fill_gateway(&mut app, "bastion");
    app.update(Message::SaveGateway {
        password: None,
        passphrase: None,
    });
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("back to the form: {:?}", app.dialog);
    };
    let added = app.gateways()[0].id.clone();
    assert_eq!(
        draft.gateway,
        Some(added.clone()),
        "the new gateway is chosen"
    );
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
    assert_eq!(session_gateway(&app), Some(added));
}

#[test]
fn cancelling_the_gateway_dialog_returns_to_the_form_as_it_was() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), None, Vec::new());
    app.update(Message::EditProfile(id("web")));
    app.update(Message::NewGateway);
    fill_gateway(&mut app, "bastion");
    app.update(Message::DismissDialog);
    assert!(matches!(app.dialog, Some(Dialog::EditProfile { .. })));
    assert!(app.gateways().is_empty());
    // From the menu, with no form under it, the dialog just closes.
    app.update(Message::DismissDialog);
    app.update(Message::NewGateway);
    app.update(Message::DismissDialog);
    assert!(app.dialog.is_none());
}

const HOST_KEY: &str = include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519.pub");

fn save_gateway(app: &mut App) {
    app.update(Message::SaveGateway {
        password: None,
        passphrase: None,
    });
}

/// "Host Key Fingerprint" of the gateway dialog open.
fn shown_fingerprint(app: &App) -> String {
    let Some(Dialog::EditGateway { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    draft.trusted_fingerprint.clone()
}

/// The trust files beside `known_hosts`, as their bytes, `None` for one absent.
fn trust_files(dir: &Path) -> Vec<Option<Vec<u8>>> {
    ["known_hosts", "known_hosts.pins"]
        .iter()
        .map(|name| std::fs::read(dir.join(name)).ok())
        .collect()
}

#[test]
fn the_host_key_fingerprint_shows_the_pin_or_the_recorded_key_and_is_never_saved() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(
        dir.path(),
        None,
        vec![
            gateway("bare", None),
            gateway("pinned", None),
            gateway("full", None),
        ],
    );
    let known = KnownHosts::new(dir.path().join("known_hosts"));
    let pinned = format!("SHA256:{}", "A".repeat(43));
    assert!(
        Pins::beside(known.path())
            .pin("pinned.lab", 22, &pinned)
            .expect("pinned")
    );
    let key = PublicKey::from_openssh(HOST_KEY.trim()).expect("key");
    known.learn("full.lab", 22, &key).expect("learnt");

    // Nothing trusted: empty, the C# hint under it says when it fills.
    app.update(Message::EditGateway(id("bare")));
    assert_eq!(shown_fingerprint(&app), "");
    app.update(Message::DismissDialog);
    app.update(Message::NewGateway);
    assert_eq!(shown_fingerprint(&app), "", "a new gateway");
    app.update(Message::DismissDialog);
    app.update(Message::EditGateway(id("pinned")));
    assert_eq!(shown_fingerprint(&app), pinned, "the pin");
    app.update(Message::DismissDialog);
    app.update(Message::EditGateway(id("full")));
    assert_eq!(
        shown_fingerprint(&app),
        fingerprint(&key),
        "the key recorded"
    );

    // Saved, renamed or moved to another host, the trust files stay as they were.
    let before = trust_files(dir.path());
    gateway_field(&mut app, ProfileField::Name, "renamed");
    gateway_field(&mut app, ProfileField::Host, "elsewhere.lab");
    save_gateway(&mut app);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    app.update(Message::EditGateway(id("pinned")));
    save_gateway(&mut app);
    app.update(Message::NewGateway);
    fill_gateway(&mut app, "added");
    save_gateway(&mut app);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(trust_files(dir.path()), before, "never touched");
}

#[test]
fn the_parent_picker_offers_neither_descendants_nor_a_chain_too_deep() {
    let dir = tempfile::tempdir().expect("dir");
    // a <- b <- c <- d <- e: five deep; f on its own, with g below it.
    let mut app = app(
        dir.path(),
        None,
        vec![
            gateway("a", None),
            gateway("b", Some("a")),
            gateway("c", Some("b")),
            gateway("d", Some("c")),
            gateway("e", Some("d")),
            gateway("f", None),
            gateway("g", Some("f")),
        ],
    );
    let offered = |editing: Option<&str>| -> Vec<String> {
        let editing = editing.map(id);
        heimdall_core::gateway_parents::parent_options(app.gateways(), editing.as_ref())
            .into_iter()
            .map(|gateway| gateway.id.as_str().to_owned())
            .collect()
    };
    assert_eq!(
        offered(Some("b")),
        ["a", "f"],
        "neither itself, nor below it, nor g: b to e would be six deep under f"
    );
    assert_eq!(
        offered(Some("f")),
        ["a", "b", "c"],
        "f and g need room for two"
    );
    assert_eq!(offered(None), ["a", "b", "c", "d", "f", "g"], "not under e");
    // Saving still refuses a loop, should one be chosen another way.
    app.update(Message::EditGateway(id("a")));
    app.update(Message::ChooseParentGateway(Some(id("e"))));
    save_gateway(&mut app);
    let Some(Dialog::EditGateway { error, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(*error, Some(DraftError::GatewayLoop));
}

#[test]
fn a_gateway_reached_through_itself_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(
        dir.path(),
        None,
        vec![gateway("outer", Some("inner")), gateway("inner", None)],
    );
    app.update(Message::EditGateway(id("inner")));
    app.update(Message::ChooseParentGateway(Some(id("outer"))));
    app.update(Message::SaveGateway {
        password: None,
        passphrase: None,
    });
    let Some(Dialog::EditGateway { error, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(*error, Some(DraftError::GatewayLoop));
    assert_eq!(
        app.gateways()
            .iter()
            .find(|g| g.id == id("inner"))
            .expect("inner")
            .parent,
        None,
        "nothing saved"
    );
}

#[test]
fn connect_directly_wins_over_the_gateway_chosen_which_the_form_keeps() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), None, vec![gateway("bastion", None)]);
    app.update(Message::EditProfile(id("web")));
    app.update(Message::ChooseGateway(id("bastion")));
    app.update(Message::ProfileToggle {
        toggle: ProfileToggle::DirectConnection,
        on: true,
    });
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(
        draft.gateway,
        Some(id("bastion")),
        "kept, as the C# combo keeps it"
    );
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
    assert_eq!(session_gateway(&app), None);
}

#[test]
fn the_tree_says_which_gateway_a_session_goes_through_or_that_it_is_missing() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path(), Some("bastion"), vec![gateway("bastion", None)]);
    let badge = app.profile_summary(&id("web")).expect("web").gateway;
    assert_eq!(badge, Some(GatewayBadge::Via("bastion gateway".to_owned())));

    let dir = tempfile::tempdir().expect("dir");
    let app = self::app(dir.path(), Some("gone"), Vec::new());
    let badge = app.profile_summary(&id("web")).expect("web").gateway;
    assert_eq!(badge, Some(GatewayBadge::Missing));
}

fn open(app: &mut App) -> (TabId, AttemptId) {
    match app.update(Message::ConnectProfile(id("web"))).as_slice() {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    }
}

fn ask(app: &mut App, tab: TabId, attempt: AttemptId, host: &str, user: &str) -> Option<String> {
    let kind = QuestionKind::Password(PasswordQuestion {
        host: host.to_owned(),
        port: 22,
        username: user.to_owned(),
        attempt: 1,
    });
    match app
        .update(Message::Connection {
            tab,
            attempt,
            event: ConnectionEvent::Question {
                question: QuestionId::fresh(),
                kind,
            },
        })
        .as_slice()
    {
        [
            Effect::Answer {
                answer: Some(Answer::Secret(secret)),
                ..
            },
        ] => Some(secret.expose().to_owned()),
        _ => None,
    }
}

/// Server `web` behind `bastion`, each with its own saved password.
fn both_saved(dir: &Path) -> App {
    let mut app = app(dir, Some("bastion"), vec![gateway("bastion", None)]);
    app.update(Message::EditGateway(id("bastion")));
    app.update(Message::SaveGateway {
        password: Some(Secret::new("gateway pw".to_owned())),
        passphrase: None,
    });
    app.update(Message::EditProfile(id("web")));
    app.update(Message::SaveProfile {
        password: Some(Secret::new("server pw".to_owned())),
        passphrase: None,
    });
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    app
}

#[test]
fn each_hop_is_given_its_own_saved_password_in_one_attempt() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = both_saved(dir.path());
    let (tab, attempt) = open(&mut app);
    assert_eq!(
        ask(&mut app, tab, attempt, "bastion.lab", "jump").as_deref(),
        Some("gateway pw")
    );
    assert_eq!(
        ask(&mut app, tab, attempt, "web.lab", "admin").as_deref(),
        Some("server pw"),
        "the server's own, in the same attempt"
    );
    // Another account on the gateway's host gets nothing.
    let (tab, attempt) = open(&mut app);
    assert_eq!(ask(&mut app, tab, attempt, "bastion.lab", "root"), None);
}

#[test]
fn the_servers_password_never_goes_to_the_gateway() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), Some("bastion"), vec![gateway("bastion", None)]);
    app.update(Message::EditProfile(id("web")));
    app.update(Message::SaveProfile {
        password: Some(Secret::new("server pw".to_owned())),
        passphrase: None,
    });
    let (tab, attempt) = open(&mut app);
    assert_eq!(ask(&mut app, tab, attempt, "bastion.lab", "jump"), None);
}

#[test]
fn a_failed_attempt_takes_every_saved_password_it_gave_as_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = both_saved(dir.path());
    let (tab, attempt) = open(&mut app);
    ask(&mut app, tab, attempt, "bastion.lab", "jump");
    ask(&mut app, tab, attempt, "web.lab", "admin");
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::AuthenticationFailed {
            tried: vec![AuthMethod::Password],
            agent_keys: None,
        }),
    });
    let (tab, attempt) = open(&mut app);
    assert_eq!(ask(&mut app, tab, attempt, "bastion.lab", "jump"), None);
    assert_eq!(ask(&mut app, tab, attempt, "web.lab", "admin"), None);
}
