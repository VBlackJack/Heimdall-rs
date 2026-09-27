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

//! The session dialog for every protocol, as the C# Heimdall's: a new session's protocol
//! first, a saved one edited in its own, and its password saved for its own account.

use std::path::Path;

use heimdall_app::profile_draft::{DraftError, DraftProtocol, ProfileField, ProfileToggle};
use heimdall_app::{
    Answer, App, AppConfig, ConnectionEvent, Dialog, Effect, Message, QuestionId, QuestionKind,
    ServerPasswordQuestion, SystemCredentials,
};
use heimdall_core::profile::{ProfileId, RdpProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, PasswordQuestion, Secret};
use heimdall_term::GridSize;

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_rdp([RdpProfile {
        id: ProfileId::new("dc"),
        name: "dc".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: None,
        allow_tls_only: false,
        gateway: None,
        redirect_clipboard: true,
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

fn field(app: &mut App, field: ProfileField, value: &str) {
    app.update(Message::ProfileField {
        field,
        value: value.to_owned(),
    });
}

fn save(app: &mut App, password: Option<&str>) {
    app.update(Message::SaveProfile {
        password: password.map(|typed| Secret::new(typed.to_owned())),
    });
}

/// What a new connection to `id` is answered with by itself, when asked `kind`.
fn answered(app: &mut App, id: &str, kind: QuestionKind) -> Option<String> {
    let effects = app.update(Message::ConnectProfile(ProfileId::new(id)));
    let (tab, attempt) = match effects.as_slice() {
        [Effect::ConnectRdp { tab, attempt, .. } | Effect::ConnectVnc { tab, attempt, .. }] => {
            (*tab, *attempt)
        }
        other => panic!("{other:?}"),
    };
    let question = QuestionId::fresh();
    match app
        .update(Message::Connection {
            tab,
            attempt,
            event: ConnectionEvent::Question { question, kind },
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

fn rdp_question() -> QuestionKind {
    QuestionKind::Password(PasswordQuestion {
        host: "dc.lab".to_owned(),
        port: 3389,
        username: "admin".to_owned(),
        attempt: 1,
    })
}

#[test]
fn an_rdp_profile_is_edited_in_its_own_form_and_its_password_goes_to_its_domain_account() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::EditProfile(ProfileId::new("dc")));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(draft.protocol, DraftProtocol::Rdp);
    assert!(draft.is_on(ProfileToggle::Nla));
    // A saved profile keeps its protocol.
    app.update(Message::ChooseProtocol(DraftProtocol::Ssh));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(draft.protocol, DraftProtocol::Rdp);
    field(&mut app, ProfileField::Domain, "CORP");
    app.update(Message::ProfileToggle {
        toggle: ProfileToggle::RedirectClipboard,
        on: false,
    });
    save(&mut app, Some("rdp password"));
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let saved = &app.rdp_profiles()[0];
    assert_eq!(saved.domain.as_deref(), Some("CORP"));
    assert!(!saved.redirect_clipboard);
    assert!(!saved.allow_tls_only);
    assert_eq!(
        answered(&mut app, "dc", rdp_question()).as_deref(),
        Some("rdp password"),
        "saved for CORP\\admin, the account the RDP question is for"
    );
}

#[test]
fn an_rdp_password_needs_a_username() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::EditProfile(ProfileId::new("dc")));
    field(&mut app, ProfileField::Username, "");
    save(&mut app, Some("rdp password"));
    let Some(Dialog::EditProfile { error, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(*error, Some(DraftError::UsernameForPassword));
}

#[test]
fn a_new_vnc_session_saves_its_password_without_an_account() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::NewProfile);
    // Enter on the picker saves nothing.
    save(&mut app, Some("ignored"));
    assert!(matches!(app.dialog, Some(Dialog::EditProfile { .. })));
    assert_eq!(app.vnc_profiles().len(), 0);
    app.update(Message::ChooseProtocol(DraftProtocol::Vnc));
    field(&mut app, ProfileField::Name, "kiosk");
    field(&mut app, ProfileField::Host, "kiosk.lab");
    save(&mut app, Some("vnc password"));
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let kiosk = app.vnc_profiles()[0].clone();
    assert_eq!(kiosk.port, 5900);
    let question = QuestionKind::ServerPassword(ServerPasswordQuestion {
        host: "kiosk.lab".to_owned(),
        port: 5900,
    });
    assert_eq!(
        answered(&mut app, kiosk.id.as_str(), question).as_deref(),
        Some("vnc password")
    );
}

#[test]
fn a_new_winrm_session_saves_no_password_and_https_moves_the_port() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::NewProfile);
    app.update(Message::ChooseProtocol(DraftProtocol::WinRm));
    field(&mut app, ProfileField::Name, "ps");
    field(&mut app, ProfileField::Host, "ps.lab");
    for toggle in [ProfileToggle::StoredCredential, ProfileToggle::UseSsl] {
        app.update(Message::ProfileToggle { toggle, on: true });
    }
    field(&mut app, ProfileField::Username, "LAB\\admin");
    save(&mut app, Some("not saved: PowerShell asks for it"));
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let ps = app.winrm_profiles()[0].clone();
    assert_eq!(ps.port, 5986);
    assert!(ps.use_ssl);
    assert_eq!(ps.username.as_deref(), Some("LAB\\admin"));
    app.update(Message::EditProfile(ps.id));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert!(!draft.password_saved, "nothing saved for WinRM");
}

#[test]
fn a_new_telnet_session_is_saved_with_its_default_port() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::NewProfile);
    app.update(Message::ChooseProtocol(DraftProtocol::Telnet));
    field(&mut app, ProfileField::Name, "switch");
    field(&mut app, ProfileField::Host, "sw.lab");
    save(&mut app, None);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(app.telnet_profiles()[0].port, 23);
}
