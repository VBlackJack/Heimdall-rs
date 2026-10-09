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

//! "Import Sessions": a picked file told by its kind, the number of its sessions asked
//! about, then merged, as the C# Heimdall's.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, Dialog, Effect, FileKind, ImportFile, ImportSummary, Message, SessionsMessage,
    SystemCredentials,
};
use heimdall_core::import::foreign::FileWarning;
use heimdall_core::store::MergeReport;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn app(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

fn read(app: &mut App, name: &str, text: &str, settings: Option<&str>) -> Vec<Effect> {
    app.update(Message::Sessions(SessionsMessage::FileRead(Ok(
        ImportFile {
            name: name.to_owned(),
            text: text.to_owned(),
            settings: settings.map(str::to_owned),
            rename: "{name} (Imported {n})".to_owned(),
        },
    ))))
}

const MOBA: &str = "[Bookmarks]\nSubRep=Lab\nweb= #109#0%web.lab%22%root\ndc= #91#4%dc.lab%3389%admin\n[Passwords]\nroot@web.lab=x\n";

#[test]
fn the_kind_is_told_by_the_extension_and_an_xml_file_by_its_root() {
    assert_eq!(FileKind::of("s.MXTSESSIONS", ""), FileKind::MobaXterm);
    assert_eq!(FileKind::of("MobaXterm.ini", ""), FileKind::MobaXterm);
    assert_eq!(FileKind::of("a.mobaconf", ""), FileKind::MobaXterm);
    assert_eq!(FileKind::of("estate.rdg", ""), FileKind::RdcMan);
    assert_eq!(
        FileKind::of("confCons.xml", "<Connections/>"),
        FileKind::MRemoteNg
    );
    assert_eq!(
        FileKind::of("old.xml", "<RDCMan><file/></RDCMan>"),
        FileKind::RdcMan
    );
    assert_eq!(FileKind::of("servers.json", "[]"), FileKind::Heimdall);
    assert_eq!(FileKind::of("no-extension", "[]"), FileKind::Heimdall);
}

#[test]
fn the_import_starts_by_asking_for_the_file() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = app.update(Message::Sessions(SessionsMessage::File));
    assert!(matches!(effects.as_slice(), [Effect::PickSessionsFile]));
}

#[test]
fn a_mobaxterm_file_is_asked_about_then_merged_with_new_ids_and_its_passwords_said() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(&mut app, "sessions.mxtsessions", MOBA, None);
    let Some(Dialog::ConfirmImportFile(pending)) = &app.dialog else {
        panic!("not asked: {:?}", app.dialog);
    };
    assert_eq!(pending.kind, FileKind::MobaXterm);
    assert_eq!(pending.count(), 2);
    assert!(
        app.profiles().is_empty(),
        "nothing merged before the answer"
    );

    app.update(Message::ConfirmDialog);
    assert_eq!(
        app.dialog,
        Some(Dialog::ImportDone(ImportSummary {
            merged: MergeReport {
                added: 2,
                updated: 0,
                unchanged: 0
            },
            skipped: Vec::new(),
            warnings: Vec::new(),
            stored_credentials: Some(1),
            dropped: Vec::new(),
            host_keys: None,
            gateways: heimdall_core::import::gateways::Reconciliation::default(),
            actions: None,
        }))
    );
    let web = &app.profiles()[0];
    assert_eq!(
        (web.host.as_str(), web.group.as_deref()),
        ("web.lab", Some("Lab"))
    );
    assert!(
        !web.id.as_str().starts_with("foreign-"),
        "{}",
        web.id.as_str()
    );
    assert_ne!(web.id, app.rdp_profiles()[0].id);

    // As the C#: every import gives new identifiers, so the file's sessions are added again.
    read(&mut app, "sessions.mxtsessions", MOBA, None);
    app.update(Message::ConfirmDialog);
    assert_eq!(app.profiles().len(), 2);
}

#[test]
fn a_heimdall_document_keeps_its_ids_and_reads_its_settings_beside_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let servers = r#"[{"id":"a1","displayName":"web","remoteServer":"web.lab","connectionType":"SSH","group":"Lab"}]"#;
    let settings = r#"{"groupDefaults":{"Lab":{"sshUsername":"deploy"}}}"#;
    read(&mut app, "servers.json", servers, Some(settings));
    assert!(
        matches!(app.dialog, Some(Dialog::ProfileImportPreview(_))),
        "previewed: {:?}",
        app.dialog
    );
    app.update(Message::ConfirmDialog);
    let web = &app.profiles()[0];
    assert_eq!(
        (web.id.as_str(), web.username.as_deref()),
        ("a1", Some("deploy")),
        "the id kept, the group default applied"
    );
    let Some(Dialog::ImportDone(summary)) = &app.dialog else {
        panic!("no report");
    };
    assert_eq!(summary.stored_credentials, None, "no password notice");

    // As the C#: the same id again is a clash, auto-renamed at first.
    read(&mut app, "servers.json", servers, Some(settings));
    app.update(Message::ConfirmDialog);
    let names: Vec<&str> = app.profiles().iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["web", "web (Imported 2)"]);
    assert_ne!(app.profiles()[1].id.as_str(), "a1", "a fresh id");
}

#[test]
fn a_file_giving_nothing_says_so_with_why() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(
        &mut app,
        "confCons.xml",
        r#"<Connections FullFileEncryption="true"/>"#,
        None,
    );
    assert_eq!(
        app.dialog,
        Some(Dialog::ImportNothing {
            skipped: Vec::new(),
            warnings: vec![FileWarning::FullyEncrypted],
        })
    );

    read(
        &mut app,
        "confCons.xml",
        r#"<Connections><Node Name="site" Protocol="HTTP" Hostname="s.lab"/></Connections>"#,
        None,
    );
    let Some(Dialog::ImportNothing { skipped, warnings }) = &app.dialog else {
        panic!("not nothing: {:?}", app.dialog);
    };
    assert_eq!(skipped.len(), 1, "the session left out is said");
    assert!(warnings.is_empty());
}

#[test]
fn an_unreadable_file_or_document_is_a_failed_import() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Sessions(SessionsMessage::FileRead(Err(
        "gone".to_owned()
    ))));
    assert_eq!(
        app.dialog,
        Some(Dialog::ImportFailed {
            detail: "gone".to_owned()
        })
    );
    read(&mut app, "servers.json", "{not json", None);
    assert!(matches!(app.dialog, Some(Dialog::ImportFailed { .. })));
}

#[test]
fn the_file_read_never_appears_in_a_log() {
    let message = Message::Sessions(SessionsMessage::FileRead(Ok(ImportFile {
        name: "sessions.mxtsessions".to_owned(),
        text: "secret.lab".to_owned(),
        settings: Some("hidden".to_owned()),
        rename: "{name} (Imported {n})".to_owned(),
    })));
    let logged = format!("{message:?}");
    assert!(
        !logged.contains("secret.lab") && !logged.contains("hidden"),
        "{logged}"
    );
    assert!(logged.contains("sessions.mxtsessions"));
}

/// A `WinRM` profile over HTTPS that skips the certificate check, as the C# writes it.
const WINRM_SKIPPING_CHECKS: &str = r#"[{"id":"w1","displayName":"dc","remoteServer":"dc.lab","connectionType":"WINRM","winRmUseSsl":true,"winRmSkipCertificateCheck":true}]"#;

#[test]
fn an_imported_winrm_profile_checks_its_host_certificate_whatever_the_file_says() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(&mut app, "servers.json", WINRM_SKIPPING_CHECKS, None);
    app.update(Message::ConfirmDialog);
    let [dc] = app.winrm_profiles() else {
        panic!("one profile: {:?}", app.winrm_profiles());
    };
    assert!(dc.use_ssl, "HTTPS is kept");
    assert!(
        !dc.skip_certificate_check,
        "a file written elsewhere does not decide that this host's certificate goes unchecked"
    );
}

#[test]
fn a_picked_file_never_carries_the_servers_it_trusts_over() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let settings = r#"{"trustedHostKeysV2": {"dc.lab:22":
        {"fingerprint": "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}}}"#;
    read(
        &mut app,
        "servers.json",
        WINRM_SKIPPING_CHECKS,
        Some(settings),
    );
    app.update(Message::ConfirmDialog);
    let Some(Dialog::ImportDone(summary)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(summary.host_keys, None, "not said: not done");
    let pins = heimdall_ssh::Pins::beside(&dir.path().join("known_hosts"));
    assert!(
        !pins.path().exists(),
        "a file written elsewhere trusts nothing here"
    );
}

/// A file sharing a bastion with this machine's saved one under its own identifier, and two
/// gateways on the same host that are not that one: another user, another port.
const SHARED_BASTION: &str = r#"{"schemaVersion": 2,
    "servers": [
        {"id": "db", "displayName": "DB", "remoteServer": "db.internal",
         "connectionType": "SSH", "sshGatewayId": "theirs"},
        {"id": "dc", "displayName": "DC", "remoteServer": "dc.internal",
         "connectionType": "RDP", "sshGatewayId": "theirs"},
        {"id": "web", "displayName": "Web", "remoteServer": "web.internal",
         "connectionType": "SSH", "sshGatewayId": "as-admin"},
        {"id": "api", "displayName": "API", "remoteServer": "api.internal",
         "connectionType": "SSH", "sshGatewayId": "other-port"}],
    "gateways": [
        {"id": "theirs", "name": "Their bastion", "host": "bastion.example.org",
         "port": 2222, "user": "jump"},
        {"id": "as-admin", "name": "As admin", "host": "bastion.example.org",
         "port": 2222, "user": "admin"},
        {"id": "other-port", "name": "Other port", "host": "bastion.example.org",
         "port": 22, "user": "jump"}]}"#;

#[test]
fn an_imported_gateway_with_the_address_of_a_saved_one_is_that_one_its_profiles_rewired() {
    use heimdall_core::import::gateways::Reconciliation;
    use heimdall_core::profile::{ProfileId, SshGateway};
    use heimdall_core::store::ProfileStore;

    let dir = tempfile::tempdir().expect("dir");
    let profiles_file = dir.path().join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_gateways([SshGateway {
        id: ProfileId::new("saved"),
        name: "Bastion".to_owned(),
        // The host in other case: the same machine, as the C# compares it.
        host: "Bastion.Example.ORG".to_owned(),
        port: 2222,
        username: Some("jump".to_owned()),
        key_path: None,
        parent: None,
    }]);
    store.save().expect("save");
    let mut app = app(dir.path());
    read(&mut app, "servers.json", SHARED_BASTION, None);
    app.update(Message::ConfirmDialog);
    let Some(Dialog::ImportDone(summary)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(
        summary.gateways,
        Reconciliation {
            created: 2,
            merged: 1,
            orphans: 0
        },
        "as the C# summary counts them"
    );
    assert_eq!(summary.merged.added, 4, "the profiles alone");

    let saved = ProfileStore::open(&profiles_file).expect("saved");
    let ids: Vec<&str> = saved.gateways().iter().map(|g| g.id.as_str()).collect();
    assert_eq!(
        ids,
        ["saved", "as-admin", "other-port"],
        "no second bastion; another user or port stays a gateway of its own"
    );
    let through = |id: &str| {
        saved
            .ssh_profiles()
            .iter()
            .find(|profile| profile.id.as_str() == id)
            .and_then(|profile| profile.gateway.clone())
            .expect("a gateway")
    };
    assert_eq!(through("db"), ProfileId::new("saved"), "rewired");
    assert_eq!(
        saved.rdp_profiles()[0].gateway,
        Some(ProfileId::new("saved")),
        "an RDP profile too"
    );
    assert_eq!(through("web"), ProfileId::new("as-admin"));
    assert_eq!(through("api"), ProfileId::new("other-port"));

    // The same file again: every gateway is one already saved.
    app.update(Message::DismissDialog);
    read(&mut app, "servers.json", SHARED_BASTION, None);
    app.update(Message::ConfirmDialog);
    let Some(Dialog::ImportDone(summary)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(
        summary.gateways,
        Reconciliation {
            created: 0,
            merged: 3,
            orphans: 0
        }
    );
    assert_eq!(
        ProfileStore::open(&profiles_file)
            .expect("saved")
            .gateways()
            .len(),
        3
    );
}

#[test]
fn an_imported_gateway_whose_identifier_names_another_saved_one_gets_one_of_its_own() {
    use heimdall_core::profile::{ProfileId, SshGateway};
    use heimdall_core::store::ProfileStore;

    let dir = tempfile::tempdir().expect("dir");
    let profiles_file = dir.path().join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    let elsewhere = SshGateway {
        id: ProfileId::new("theirs"),
        name: "Elsewhere".to_owned(),
        host: "elsewhere.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        parent: None,
    };
    store.merge_gateways([elsewhere.clone()]);
    store.save().expect("save");
    let mut app = app(dir.path());
    read(&mut app, "servers.json", SHARED_BASTION, None);
    app.update(Message::ConfirmDialog);
    let saved = ProfileStore::open(&profiles_file).expect("saved");
    assert_eq!(saved.gateways()[0], elsewhere, "never replaced");
    let bastion = saved
        .gateways()
        .iter()
        .find(|gateway| gateway.name == "Their bastion")
        .expect("added");
    assert_ne!(bastion.id, elsewhere.id, "an identifier of its own");
    let db = saved
        .ssh_profiles()
        .iter()
        .find(|profile| profile.id.as_str() == "db")
        .expect("db");
    assert_eq!(db.gateway.as_ref(), Some(&bastion.id), "rewired to it");
}

/// An export from a build without gateways: its profiles name bastions the file does not
/// hold, one of them saved on this machine.
const WITHOUT_GATEWAYS: &str = r#"{"schemaVersion": 2,
    "servers": [
        {"id": "db", "displayName": "DB", "remoteServer": "db.internal",
         "connectionType": "SSH", "sshGatewayId": "saved"},
        {"id": "dc", "displayName": "DC", "remoteServer": "dc.internal",
         "connectionType": "RDP", "sshGatewayId": "gone"}]}"#;

#[test]
fn a_profile_whose_gateway_is_missing_is_imported_naming_it_and_counted_as_the_csharp_does() {
    use heimdall_core::import::gateways::Reconciliation;
    use heimdall_core::profile::{ProfileId, SshGateway};
    use heimdall_core::store::{ProfileStore, RouteError};

    let dir = tempfile::tempdir().expect("dir");
    let profiles_file = dir.path().join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_gateways([SshGateway {
        id: ProfileId::new("saved"),
        name: "Bastion".to_owned(),
        host: "bastion.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        parent: None,
    }]);
    store.save().expect("save");
    let mut app = app(dir.path());
    read(&mut app, "export.json", WITHOUT_GATEWAYS, None);
    app.update(Message::ConfirmDialog);
    let Some(Dialog::ImportDone(summary)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(summary.merged.added, 2, "neither left out");
    assert!(summary.skipped.is_empty(), "{:?}", summary.skipped);
    assert_eq!(
        summary.gateways,
        Reconciliation {
            created: 0,
            merged: 0,
            orphans: 1
        },
        "the gateway saved here resolves; the other is counted"
    );

    let saved = ProfileStore::open(&profiles_file).expect("saved");
    assert_eq!(
        saved.ssh_profiles()[0].gateway,
        Some(ProfileId::new("saved"))
    );
    let dc = &saved.rdp_profiles()[0];
    assert_eq!(
        dc.gateway,
        Some(ProfileId::new("gone")),
        "kept, as the C# keeps it, never connected direct"
    );
    assert_eq!(
        saved.route(dc.gateway.as_ref()),
        Err(RouteError::MissingGateway(ProfileId::new("gone"))),
        "connecting says the gateway is missing"
    );
}
