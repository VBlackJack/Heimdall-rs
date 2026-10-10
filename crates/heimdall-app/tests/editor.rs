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

use heimdall_app::profile_draft::{
    DraftError, DraftProtocol, ProfileChoice, ProfileField, ProfileToggle,
};
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
        extras: heimdall_core::profile::RdpExtras::default(),
        id: ProfileId::new("dc"),
        name: "dc".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: None,
        allow_tls_only: false,
        gateway: None,
        local_tunnel_port: None,
        redirect_clipboard: true,
        redirect_drives: false,
        options: heimdall_core::profile::RdpOptions::default(),
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
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
        passphrase: None,
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
    assert!(!saved.redirect_drives);
    assert!(!saved.allow_tls_only);
    assert_eq!(
        answered(&mut app, "dc", rdp_question()).as_deref(),
        Some("rdp password"),
        "saved for CORP\\admin, the account the RDP question is for"
    );
}

#[test]
fn an_rdp_session_shares_its_drives_once_ticked() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::EditProfile(ProfileId::new("dc")));
    app.update(Message::ProfileToggle {
        toggle: ProfileToggle::RedirectDrives,
        on: true,
    });
    save(&mut app, None);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert!(app.rdp_profiles()[0].redirect_drives);
    app.update(Message::EditProfile(ProfileId::new("dc")));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert!(
        draft.is_on(ProfileToggle::RedirectDrives),
        "shown ticked again"
    );
}

#[test]
fn an_rdp_session_keeps_its_colours_sound_and_administrative_session() {
    use heimdall_core::profile::{AudioPlayback, ColorDepth, RdpOptions};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::EditProfile(ProfileId::new("dc")));
    app.update(Message::ProfileChoice(ProfileChoice::ColorDepth(
        ColorDepth::Bpp16,
    )));
    app.update(Message::ProfileChoice(ProfileChoice::Audio(
        AudioPlayback::OnServer,
    )));
    app.update(Message::ProfileToggle {
        toggle: ProfileToggle::AdminSession,
        on: true,
    });
    save(&mut app, None);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let chosen = RdpOptions {
        color_depth: ColorDepth::Bpp16,
        audio: AudioPlayback::OnServer,
        admin_session: true,
        ..RdpOptions::default()
    };
    assert_eq!(app.rdp_profiles()[0].options, chosen);
    app.update(Message::EditProfile(ProfileId::new("dc")));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(
        (draft.rdp_options.color_depth, draft.rdp_options.audio),
        (ColorDepth::Bpp16, AudioPlayback::OnServer),
        "shown chosen again"
    );
    assert!(
        draft.is_on(ProfileToggle::AdminSession),
        "shown ticked again"
    );
    // Cleared, and saved again: back to the defaults.
    app.update(Message::ProfileChoice(ProfileChoice::ColorDepth(
        ColorDepth::Bpp32,
    )));
    app.update(Message::ProfileChoice(ProfileChoice::Audio(
        AudioPlayback::Off,
    )));
    app.update(Message::ProfileToggle {
        toggle: ProfileToggle::AdminSession,
        on: false,
    });
    save(&mut app, None);
    assert_eq!(app.rdp_profiles()[0].options, RdpOptions::default());
}

#[test]
fn an_rdp_session_keeps_a_fixed_size_typed_or_picked_within_the_csharp_limits() {
    use heimdall_core::profile::{RdpOptions, Resolution};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let error = |app: &App| match &app.dialog {
        Some(Dialog::EditProfile { error, .. }) => *error,
        other => panic!("{other:?}"),
    };
    app.update(Message::EditProfile(ProfileId::new("dc")));
    // Hidden, the size is never checked.
    field(&mut app, ProfileField::FixedWidth, "wide");
    save(&mut app, None);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(app.rdp_profiles()[0].options, RdpOptions::default());

    app.update(Message::EditProfile(ProfileId::new("dc")));
    app.update(Message::ProfileChoice(ProfileChoice::Resolution(
        Resolution::Fixed,
    )));
    app.update(Message::ProfileChoice(ProfileChoice::Preset(2560, 1440)));
    app.update(Message::ProfileChoice(ProfileChoice::ScaleFixed(false)));
    app.update(Message::ProfileChoice(ProfileChoice::DynamicResolution(
        false,
    )));
    save(&mut app, None);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let options = app.rdp_profiles()[0].options;
    assert_eq!(
        (
            options.resolution,
            options.fixed_width,
            options.fixed_height,
            options.scale_fixed,
            options.dynamic_resolution
        ),
        (Resolution::Fixed, 2560, 1440, false, false)
    );

    app.update(Message::EditProfile(ProfileId::new("dc")));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(
        (draft.fixed_width.as_str(), draft.fixed_height.as_str()),
        ("2560", "1440")
    );
    for (width, height, refused) in [
        ("199", "720", Some(DraftError::FixedWidthInvalid)),
        ("7681", "720", Some(DraftError::FixedWidthInvalid)),
        ("wide", "720", Some(DraftError::FixedWidthInvalid)),
        ("1280", "199", Some(DraftError::FixedHeightInvalid)),
        ("1280", "4321", Some(DraftError::FixedHeightInvalid)),
    ] {
        field(&mut app, ProfileField::FixedWidth, width);
        field(&mut app, ProfileField::FixedHeight, height);
        save(&mut app, None);
        assert_eq!(error(&app), refused, "{width}x{height}");
    }
    assert_eq!(
        DraftError::FixedWidthInvalid.field(),
        ProfileField::FixedWidth
    );
    assert_eq!(
        DraftError::FixedHeightInvalid.field(),
        ProfileField::FixedHeight
    );
    // The limits themselves, and a width brought down to a multiple of 4.
    field(&mut app, ProfileField::FixedWidth, " 7679 ");
    field(&mut app, ProfileField::FixedHeight, "4320");
    save(&mut app, None);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let options = app.rdp_profiles()[0].options;
    assert_eq!((options.fixed_width, options.fixed_height), (7676, 4320));
    app.update(Message::EditProfile(ProfileId::new("dc")));
    field(&mut app, ProfileField::FixedWidth, "200");
    field(&mut app, ProfileField::FixedHeight, "200");
    save(&mut app, None);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let options = app.rdp_profiles()[0].options;
    assert_eq!((options.fixed_width, options.fixed_height), (200, 200));
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

/// The password a new `WinRM` session of `id` is started with.
fn winrm_password(app: &mut App, id: &ProfileId) -> Option<String> {
    let effects = app.update(Message::OpenWinRm(id.clone()));
    let [Effect::ConnectWinRm { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    request
        .password
        .as_ref()
        .map(|password| password.expose().to_owned())
}

/// Whether the form of `id` says a password is saved.
fn form_says_saved(app: &mut App, id: &ProfileId) -> bool {
    app.update(Message::EditProfile(id.clone()));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    draft.password_saved
}

#[test]
fn a_winrm_stored_credential_saves_its_password_and_https_moves_the_port() {
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
    save(&mut app, Some("winrm password"));
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let ps = app.winrm_profiles()[0].clone();
    assert_eq!(ps.port, 5986);
    assert!(ps.use_ssl);
    assert_eq!(ps.username.as_deref(), Some("LAB\\admin"));
    assert!(form_says_saved(&mut app, &ps.id), "read back in the form");
    // Saved again with the field empty: the password stays.
    save(&mut app, None);
    assert_eq!(
        winrm_password(&mut app, &ps.id).as_deref(),
        Some("winrm password")
    );
    let profiles = std::fs::read_to_string(dir.path().join("profiles.toml")).expect("read");
    assert!(
        !profiles.contains("winrm password"),
        "never in the profiles file"
    );

    // The current Windows identity has none: the stored one goes.
    assert!(form_says_saved(&mut app, &ps.id));
    app.update(Message::ProfileToggle {
        toggle: ProfileToggle::StoredCredential,
        on: false,
    });
    save(&mut app, None);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert!(!form_says_saved(&mut app, &ps.id));
    app.update(Message::DismissDialog);
    assert_eq!(winrm_password(&mut app, &ps.id), None);
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
