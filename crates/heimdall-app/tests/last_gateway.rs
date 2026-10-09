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

//! The gateway a new profile's form starts on, as the C# "Add server" preselects the last
//! one used: kept when a profile is saved from the form, offered only to a protocol that goes
//! through a gateway, and only while that gateway exists.

use std::path::Path;

use heimdall_app::profile_draft::{DraftProtocol, ProfileField};
use heimdall_app::{App, AppConfig, Dialog, Message};
use heimdall_core::profile::{ProfileId, SshGateway};
use heimdall_core::settings::{SETTINGS_FILE_NAME, Settings};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::GridSize;

fn gateway(id: &str) -> SshGateway {
    SshGateway {
        id: ProfileId::new(id),
        name: id.to_uppercase(),
        host: format!("{id}.lab"),
        port: 22,
        username: Some("ops".to_owned()),
        key_path: None,
        parent: None,
    }
}

fn config(dir: &Path) -> AppConfig {
    AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    }
}

/// An application whose profile file holds `gateways`.
fn app(dir: &Path, gateways: Vec<SshGateway>) -> App {
    let mut store = ProfileStore::open(dir.join("profiles.toml")).expect("store");
    store.merge_gateways(gateways);
    store.save().expect("save");
    App::new(config(dir))
}

/// A new profile's form, on `protocol`: the gateway it starts on.
fn new_form(app: &mut App, protocol: DraftProtocol) -> Option<ProfileId> {
    app.update(Message::NewProfile);
    app.update(Message::ChooseProtocol(protocol));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    draft.routed_gateway()
}

/// Fills the open form with a server named `name` and saves it.
fn fill_and_save(app: &mut App, name: &str) {
    for (field, value) in [(ProfileField::Name, name), (ProfileField::Host, "srv.lab")] {
        app.update(Message::ProfileField {
            field,
            value: value.to_owned(),
        });
    }
    app.update(Message::SaveProfile {
        password: None::<Secret>,
        passphrase: None,
    });
    assert!(app.dialog.is_none(), "saved: {:?}", app.dialog);
}

fn saved(dir: &Path) -> Settings {
    Settings::load(&dir.join(SETTINGS_FILE_NAME)).expect("settings")
}

#[test]
fn a_new_profile_starts_on_the_gateway_of_the_last_profile_saved_while_it_exists() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), vec![gateway("bastion"), gateway("jump")]);
    assert_eq!(
        new_form(&mut app, DraftProtocol::Ssh),
        None,
        "none used yet"
    );
    app.update(Message::ChooseGateway(ProfileId::new("jump")));
    fill_and_save(&mut app, "web");
    assert_eq!(
        app.settings().last_used_gateway,
        Some(ProfileId::new("jump"))
    );
    assert_eq!(
        saved(dir.path()).last_used_gateway,
        Some(ProfileId::new("jump")),
        "kept across runs, as the C# setting"
    );

    // Every protocol that goes through a gateway starts on it; one that does not, never.
    for protocol in [
        DraftProtocol::Ssh,
        DraftProtocol::Sftp,
        DraftProtocol::Rdp,
        DraftProtocol::WinRm,
    ] {
        assert!(protocol.routes_through_gateway(), "{protocol:?}");
        assert_eq!(
            new_form(&mut app, protocol),
            Some(ProfileId::new("jump")),
            "{protocol:?}"
        );
    }
    for protocol in [DraftProtocol::Telnet, DraftProtocol::Local] {
        assert!(!protocol.routes_through_gateway(), "{protocol:?}");
        assert_eq!(new_form(&mut app, protocol), None, "{protocol:?}");
    }

    // A profile saved with none: the next one starts with none, as the C# writes the null.
    new_form(&mut app, DraftProtocol::Ssh);
    app.update(Message::ProfileToggle {
        toggle: heimdall_app::profile_draft::ProfileToggle::DirectConnection,
        on: true,
    });
    fill_and_save(&mut app, "direct");
    assert_eq!(app.settings().last_used_gateway, None);
    assert_eq!(new_form(&mut app, DraftProtocol::Ssh), None);
}

#[test]
fn editing_a_profile_keeps_its_own_gateway_and_one_gone_is_not_offered() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), vec![gateway("bastion")]);
    new_form(&mut app, DraftProtocol::Ssh);
    app.update(Message::ChooseGateway(ProfileId::new("bastion")));
    fill_and_save(&mut app, "web");
    let id = app.profiles()[0].id.clone();

    // A saved profile is opened as it is: the form does not start on another gateway.
    app.update(Message::EditProfile(id));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(draft.routed_gateway(), Some(ProfileId::new("bastion")));
    app.update(Message::DismissDialog);

    // Matched whatever the case, as the C# compares the identifiers.
    std::fs::write(
        dir.path().join(SETTINGS_FILE_NAME),
        "version = 1\n[profile_form]\nlast_used_gateway = \"BASTION\"\n",
    )
    .expect("settings");
    let mut app = App::new(config(dir.path()));
    assert_eq!(
        new_form(&mut app, DraftProtocol::Ssh),
        Some(ProfileId::new("bastion"))
    );

    // Its gateway deleted since: the form starts on none.
    std::fs::write(
        dir.path().join(SETTINGS_FILE_NAME),
        "version = 1\n[profile_form]\nlast_used_gateway = \"gone\"\n",
    )
    .expect("settings");
    let mut app = App::new(config(dir.path()));
    assert_eq!(new_form(&mut app, DraftProtocol::Ssh), None);
}
