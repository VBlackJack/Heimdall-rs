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

//! The offer to migrate from the legacy PowerShell Heimdall at start, as the C# makes it,
//! after the PIN and the master password, and only while no profile is saved.

use std::fs;
use std::path::{Path, PathBuf};

use heimdall_app::rdpmanager::{self, OFFER_VERSION, Offer};
use heimdall_app::{
    App, AppConfig, Dialog, Effect, LegacyMigrationMessage, Message, Notice, PinMessage,
    SystemCredentials,
};
use heimdall_core::import::rdpmanager::{
    LEGACY_APP_FOLDER_NAME, LeftOutBecause, servers_file, settings_file,
};
use heimdall_core::profile::{ProfileId, SshGateway};
use heimdall_core::settings::{AppTheme, SETTINGS_FILE_NAME, Settings};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::GridSize;

const PIN: &str = "2468";

const SETTINGS: &str = r#"{
  "DefaultTheme": "Tarn",
  "PuttyPath": "C:\\RDPManager\\evil\\putty.exe",
  "PinHash": "LEGACY-PIN-HASH",
  "PinSalt": "LEGACY-PIN-SALT",
  "SshGateways": [
    { "Id": "gw-1", "Name": "Bastion", "Host": "bastion.legacy", "Port": 22, "User": "jump",
      "SshPasswordEncrypted": "DPAPI-GATEWAY-SECRET", "KeyPath": "//attacker/share/key" }
  ],
  "Projects": [ { "Id": "p1", "Name": "Alpha" } ]
}"#;

const SERVERS: &str = r#"[
  { "Id": "srv-1", "DisplayName": "Web", "RemoteServer": "10.0.0.1", "ConnectionType": "SSH",
    "SshGatewayId": "gw-1", "SshPasswordEncrypted": "DPAPI-SSH-SECRET" },
  { "Id": "srv-2", "DisplayName": "Broken", "RemoteServer": "10.0.0.2", "ConnectionType": "SSH",
    "SshPort": 1.5 }
]"#;

fn config(dir: &Path) -> AppConfig {
    AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    }
}

fn app(dir: &Path) -> App {
    App::new(config(dir))
}

/// A legacy installation in `dir` holding these files.
fn installed(dir: &Path, settings: &str, servers: &str) -> PathBuf {
    let folder = dir.join(LEGACY_APP_FOLDER_NAME);
    fs::create_dir_all(settings_file(&folder).parent().expect("config")).expect("config");
    fs::write(settings_file(&folder), settings).expect("settings");
    fs::write(servers_file(&folder), servers).expect("servers");
    folder
}

/// The offer for the installation in `dir`, as the look at start finds it.
fn found(dir: &Path) -> Message {
    let offer = rdpmanager::offer(&dir.join(LEGACY_APP_FOLDER_NAME)).expect("offered");
    Message::LegacyMigration(LegacyMigrationMessage::Found(Some(Box::new(offer))))
}

fn offered(app: &App) -> &Offer {
    match &app.dialog {
        Some(Dialog::LegacyMigrationOffer(offer)) => offer,
        other => panic!("{other:?}"),
    }
}

fn saved(dir: &Path) -> Settings {
    Settings::load(&dir.join(SETTINGS_FILE_NAME)).expect("settings")
}

#[test]
fn the_look_at_start_runs_once_and_only_while_no_profile_is_saved() {
    let dir = tempfile::tempdir().expect("dir");
    let start = dir.path().join("bin");
    let mut empty = app(dir.path());
    let effects = empty.look_for_legacy_installation(Some(start.clone()));
    assert!(
        matches!(effects.as_slice(), [Effect::FindLegacyInstallation(from)] if *from == start),
        "{effects:?}"
    );
    assert!(
        empty
            .look_for_legacy_installation(Some(start.clone()))
            .is_empty(),
        "once a run"
    );
    assert!(
        app(dir.path())
            .look_for_legacy_installation(None)
            .is_empty()
    );

    // A profile saved: nothing to migrate into.
    let mut store = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    let report = heimdall_core::import::csharp::import(
        r#"[{"id": "mine", "displayName": "Mine", "remoteServer": "h", "connectionType": "SSH"}]"#,
        None,
    )
    .expect("profile");
    store.merge(report.profiles);
    store.save().expect("saved");
    let mut used = app(dir.path());
    assert!(
        used.look_for_legacy_installation(Some(start.clone()))
            .is_empty()
    );
    // Found anyway: never offered over profiles.
    installed(dir.path(), SETTINGS, SERVERS);
    used.update(found(dir.path()));
    assert_eq!(used.dialog, None);
}

#[test]
fn an_unreadable_profile_file_is_never_taken_for_an_empty_one() {
    let dir = tempfile::tempdir().expect("dir");
    fs::write(dir.path().join("profiles.toml"), "not = [toml").expect("written");
    let mut app = app(dir.path());
    assert!(matches!(app.dialog, Some(Dialog::StoreUnreadable { .. })));
    assert!(
        app.look_for_legacy_installation(Some(dir.path().to_owned()))
            .is_empty()
    );
    installed(dir.path(), SETTINGS, SERVERS);
    app.update(Message::DismissDialog);
    app.update(found(dir.path()));
    app.offer_legacy_migration();
    assert_eq!(app.dialog, None);
}

#[test]
fn the_offer_waits_for_the_pin_and_names_the_folder_and_what_it_holds() {
    let dir = tempfile::tempdir().expect("dir");
    let mut first = app(dir.path());
    first.update(Message::Pin(PinMessage::Configure));
    first.update(Message::Pin(PinMessage::Save {
        current: Secret::new(String::new()),
        new: Secret::new(PIN.to_owned()),
        confirm: Secret::new(PIN.to_owned()),
    }));
    let folder = installed(dir.path(), SETTINGS, SERVERS);

    let mut app = app(dir.path());
    assert!(app.pin_asked());
    app.update(found(dir.path()));
    app.offer_legacy_migration();
    assert!(app.pin_asked(), "never over the PIN");
    app.update(Message::Pin(PinMessage::Submit(Secret::new(
        PIN.to_owned(),
    ))));
    app.offer_legacy_migration();
    let offer = offered(&app);
    assert_eq!(offer.source, folder);
    assert_eq!(offer.conversion.examined, 2);
    assert_eq!(offer.conversion.gateways, 1);
}

#[test]
fn declined_it_is_not_offered_again_until_the_files_change() {
    let dir = tempfile::tempdir().expect("dir");
    installed(dir.path(), SETTINGS, SERVERS);
    let mut app = app(dir.path());
    app.update(found(dir.path()));
    let fingerprint = offered(&app).fingerprint.clone();
    app.update(Message::DismissDialog);
    assert_eq!(app.dialog, None);
    assert!(app.profiles().is_empty(), "nothing imported");
    let declined = saved(dir.path()).legacy_migration;
    assert_eq!(declined.declined_offer_version, OFFER_VERSION);
    assert_eq!(declined.declined_source_fingerprint, Some(fingerprint));

    // Restarted with the same files: not offered.
    let mut again = self::app(dir.path());
    again.update(found(dir.path()));
    again.offer_legacy_migration();
    assert_eq!(again.dialog, None);

    // A file changed: offered again.
    installed(dir.path(), SETTINGS, "[]");
    again.update(found(dir.path()));
    assert_eq!(offered(&again).conversion.examined, 0);
}

#[test]
fn the_settings_page_offers_it_again_at_the_next_start_once_declined() {
    let dir = tempfile::tempdir().expect("dir");
    installed(dir.path(), SETTINGS, SERVERS);
    let mut app = app(dir.path());
    // Nothing declined: nothing to undo.
    app.update(Message::LegacyMigration(LegacyMigrationMessage::OfferAgain));
    assert_eq!(app.notice(), None);
    app.update(found(dir.path()));
    app.update(Message::DismissDialog);
    assert!(app.settings().legacy_migration.has_decline());

    app.update(Message::LegacyMigration(LegacyMigrationMessage::OfferAgain));
    assert_eq!(app.notice(), Some(&Notice::LegacyMigrationReoffered));
    assert!(!app.settings().legacy_migration.has_decline());
    assert!(!saved(dir.path()).legacy_migration.has_decline(), "saved");
    assert_eq!(app.dialog, None, "no migration starts now");

    let mut restarted = self::app(dir.path());
    restarted.update(found(dir.path()));
    offered(&restarted);
}

#[test]
fn accepted_the_profiles_and_settings_are_imported_without_any_secret_and_reported() {
    let dir = tempfile::tempdir().expect("dir");
    installed(dir.path(), SETTINGS, SERVERS);
    // A gateway of this computer's under the identifier the legacy one uses.
    let mut store = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    store.merge_gateways([SshGateway {
        id: ProfileId::new("gw-1"),
        name: "Mine".to_owned(),
        host: "mine.lab".to_owned(),
        port: 22,
        username: Some("ops".to_owned()),
        key_path: None,
        parent: None,
    }]);
    store.save().expect("saved");

    let mut app = app(dir.path());
    app.update(found(dir.path()));
    offered(&app);
    app.update(Message::ConfirmDialog);

    let Some(Dialog::LegacyMigrationDone(done)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(done.examined, 2);
    assert_eq!(done.imported, 1);
    assert_eq!(done.left_out.len(), 1);
    assert_eq!(done.left_out[0].index, 2);
    assert_eq!(done.left_out[0].name.as_deref(), Some("Broken"));
    assert_eq!(done.left_out[0].reason, LeftOutBecause::InvalidLegacyField);
    assert_eq!(done.projects, 1);
    assert_eq!(done.key_paths_left_out, 1, "the planted gateway key");
    assert!(done.settings_saved);
    assert!(done.partial(), "a profile left out warns");

    // Merged by identity: the gateway already there is kept as it is, the legacy one added
    // under a new identifier, and its profile goes through it.
    let mine = app
        .gateways()
        .iter()
        .find(|gateway| gateway.id.as_str() == "gw-1")
        .expect("kept");
    assert_eq!(mine.host, "mine.lab");
    let legacy = app
        .gateways()
        .iter()
        .find(|gateway| gateway.host == "bastion.legacy")
        .expect("added");
    assert_ne!(legacy.id.as_str(), "gw-1");
    let web = app
        .profiles()
        .iter()
        .find(|profile| profile.id.as_str() == "srv-1")
        .expect("imported");
    assert_eq!(web.gateway.as_ref(), Some(&legacy.id));

    // The settings Heimdall has, none of the program paths nor the PIN.
    let settings = saved(dir.path());
    assert_eq!(settings.theme, AppTheme::Tarn);
    assert_eq!(settings.putty_path, "");
    assert_eq!(settings.pin, None);
    assert!(!app.pin_asked());

    let written = fs::read_to_string(dir.path().join("profiles.toml")).expect("profiles");
    let settings_text = fs::read_to_string(dir.path().join(SETTINGS_FILE_NAME)).expect("text");
    for planted in ["DPAPI-", "LEGACY-", "evil", "attacker"] {
        assert!(!written.contains(planted), "{planted}");
        assert!(!settings_text.contains(planted), "{planted}");
    }

    // Done: the result closes, and nothing is offered again in this run.
    app.update(Message::ConfirmDialog);
    assert_eq!(app.dialog, None);
    app.offer_legacy_migration();
    assert_eq!(app.dialog, None);
}
