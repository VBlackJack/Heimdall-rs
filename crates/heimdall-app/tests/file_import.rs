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
    let Some(Dialog::ConfirmImportFile(pending)) = &app.dialog else {
        panic!("not asked: {:?}", app.dialog);
    };
    assert_eq!(pending.kind, FileKind::Heimdall);
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

    read(&mut app, "servers.json", servers, Some(settings));
    app.update(Message::ConfirmDialog);
    assert_eq!(
        app.profiles().len(),
        1,
        "the same id updates, it does not add"
    );
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
