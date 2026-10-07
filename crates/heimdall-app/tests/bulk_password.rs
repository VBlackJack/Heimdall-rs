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

//! The password of several profiles set at once, as the C# bulk edit does: typed twice,
//! saved for each profile at its own server and account, in one write of the vault, and
//! never for a `WinRM` profile, whose password is not saved.

#[path = "support/log_capture.rs"]
mod log_capture;

use std::path::Path;

use heimdall_app::{
    Answer, App, AppConfig, AttemptId, BulkPasswordRefusal, BulkPasswordSkips, ConnectionEvent,
    Dialog, Effect, Message, Notice, QuestionId, QuestionKind, SelectionMessage, SystemCredentials,
    TabId, VaultStatus, open_vault,
};
use heimdall_core::credentials::{CredentialProtocol, decode, password_entry};
use heimdall_core::profile::{
    FtpProfile, ProfileId, RdpProfile, SshProfile, TelnetProfile, VncProfile, WinRmProfile,
};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, PasswordQuestion, Secret};
use heimdall_term::GridSize;

const MASTER: &str = "correct horse battery staple";
const PASSWORD: &str = "sh4red p4ss";

fn id(value: &str) -> ProfileId {
    ProfileId::new(value)
}

fn ssh(name: &str, host: &str, port: u16, username: Option<&str>) -> SshProfile {
    SshProfile {
        id: id(name),
        name: name.to_owned(),
        group: None,
        host: host.to_owned(),
        port,
        username: username.map(str::to_owned),
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

/// Two SSH profiles on different servers, one naming no account, an RDP profile in a
/// domain, an FTP, a VNC, a `WinRM` with an account and a Telnet profile.
fn app(dir: &Path, system: &SystemCredentials) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([
        ssh("web", "web.lab", 22, Some("admin")),
        ssh("db", "db.lab", 2222, Some("root")),
        ssh("bare", "bare.lab", 22, None),
    ]);
    store.merge_rdp([RdpProfile {
        extras: heimdall_core::profile::RdpExtras::default(),
        id: id("rdp"),
        name: "dc".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: Some("CORP".to_owned()),
        allow_tls_only: false,
        gateway: None,
        redirect_clipboard: false,
        redirect_drives: false,
        options: heimdall_core::profile::RdpOptions::default(),
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
    }]);
    store.merge_ftp([FtpProfile {
        id: id("ftp"),
        name: "files".to_owned(),
        group: None,
        host: "f.lab".to_owned(),
        port: 21,
        username: Some("ops".to_owned()),
        passive: true,
        tls: false,
        vault_entry: None,
    }]);
    store.merge_vnc([VncProfile {
        id: id("vnc"),
        name: "kiosk".to_owned(),
        group: None,
        host: "kiosk.lab".to_owned(),
        port: 5901,
        view_only: false,
        allow_no_password: false,
        vault_entry: None,
    }]);
    store.merge_winrm([WinRmProfile {
        id: id("winrm"),
        name: "ps".to_owned(),
        group: None,
        host: "ps.lab".to_owned(),
        port: 5985,
        use_ssl: false,
        skip_certificate_check: false,
        username: Some("admin".to_owned()),
        gateway: None,
    }]);
    store.merge_telnet([TelnetProfile {
        id: id("telnet"),
        name: "switch".to_owned(),
        group: None,
        host: "sw.lab".to_owned(),
        port: 23,
        session_logging: None,
    }]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: system.clone(),
    })
}

fn select(app: &mut App, ids: &[&str]) {
    app.update(Message::SelectProfile(id(ids[0])));
    for other in &ids[1..] {
        app.update(Message::Selection(SelectionMessage::Toggle(id(other))));
    }
}

/// Opens the bulk password dialog for `ids`, selected together.
fn open_dialog(app: &mut App, ids: &[&str]) {
    select(app, ids);
    app.update(Message::Selection(SelectionMessage::EditPassword));
}

/// Confirms the dialog with `password`, typed again as `confirm`.
fn confirm(app: &mut App, password: &str, confirm: &str) {
    app.update(Message::SetBulkPassword {
        password: Secret::new(password.to_owned()),
        confirm: Secret::new(confirm.to_owned()),
    });
}

/// Creates the vault through its dialog, as the UI would.
async fn create_vault(app: &mut App) {
    app.update(Message::ShowVault);
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(MASTER.to_owned()),
        new: None,
        confirm: Some(Secret::new(MASTER.to_owned())),
    });
    let Ok(
        [
            Effect::OpenVault {
                path,
                password,
                job,
            },
        ],
    ) = <[Effect; 1]>::try_from(effects)
    else {
        panic!("expected OpenVault");
    };
    let result = open_vault(path, password, job).await;
    app.update(Message::VaultOpened(result));
    assert_eq!(app.vault_status(), VaultStatus::Open);
}

/// What `app` answers by itself to `kind`, asked in `tab`.
fn answer(app: &mut App, tab: TabId, attempt: AttemptId, kind: QuestionKind) -> Option<String> {
    let question = QuestionId::fresh();
    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Question { question, kind },
    });
    match effects.as_slice() {
        [] => None,
        [
            Effect::Answer {
                question: answered,
                answer: Some(Answer::Secret(secret)),
            },
        ] if *answered == question => Some(secret.expose().to_owned()),
        other => panic!("unexpected {other:?}"),
    }
}

fn asked(host: &str, port: u16, username: &str, attempt: u32) -> QuestionKind {
    QuestionKind::Password(PasswordQuestion {
        host: host.to_owned(),
        port,
        username: username.to_owned(),
        attempt,
    })
}

/// What a new SSH connection to `profile` is answered with, asked by its own server.
fn ssh_answer(app: &mut App, profile: &str, host: &str, port: u16, user: &str) -> Option<String> {
    let effects = app.update(Message::OpenProfile(id(profile)));
    let [Effect::Connect { tab, attempt, .. }] = effects.as_slice() else {
        panic!("expected Connect, got {effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    answer(app, tab, attempt, asked(host, port, user, 1))
}

fn rdp_answer(app: &mut App) -> Option<String> {
    let effects = app.update(Message::OpenRdp(id("rdp")));
    let [Effect::ConnectRdp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("expected ConnectRdp, got {effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    answer(app, tab, attempt, asked("dc.lab", 3389, "admin", 1))
}

fn ftp_answer(app: &mut App) -> Option<String> {
    let effects = app.update(Message::OpenFtp(id("ftp")));
    let [Effect::ConnectFtp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("expected ConnectFtp, got {effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    answer(app, tab, attempt, asked("f.lab", 21, "ops", 1))
}

const ALL: [&str; 8] = ["web", "db", "bare", "rdp", "ftp", "vnc", "winrm", "telnet"];

#[test]
fn winrm_and_the_profiles_without_a_saved_password_are_left_alone_and_counted() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &SystemCredentials::memory());
    select(&mut app, &ALL);
    let selected = app.selected_profiles();
    assert_eq!(app.bulk_password_targets(&selected), 5);
    app.update(Message::Selection(SelectionMessage::EditPassword));
    let Some(Dialog::BulkPassword {
        ids,
        skipped,
        refused,
    }) = &app.dialog
    else {
        panic!("{:?}", app.dialog);
    };
    let mut ids = ids.clone();
    ids.sort();
    assert_eq!(ids, [id("db"), id("ftp"), id("rdp"), id("vnc"), id("web")]);
    assert_eq!(
        *skipped,
        BulkPasswordSkips {
            winrm: 1,
            no_account: 1,
            other: 1,
        },
        "WinRM is counted though it names an account"
    );
    assert_eq!(*refused, None);

    // The WinRM profile alone with one that takes none: nothing to set, and said.
    app.update(Message::DismissDialog);
    assert!(app.dialog.is_none());
    open_dialog(&mut app, &["winrm", "telnet"]);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(app.notice(), Some(&Notice::BulkPasswordWinRmSkipped(1)));

    // One profile alone is not a bulk edit.
    open_dialog(&mut app, &["web"]);
    assert!(app.dialog.is_none());
}

#[test]
fn a_control_character_or_a_mismatch_is_refused_and_nothing_is_saved() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), &system);
    open_dialog(&mut app, &["web", "db"]);
    let refused = |app: &App| match &app.dialog {
        Some(Dialog::BulkPassword { refused, .. }) => *refused,
        other => panic!("{other:?}"),
    };
    confirm(&mut app, "line\nbreak", "line\nbreak");
    assert_eq!(refused(&app), Some(BulkPasswordRefusal::Control));
    app.update(Message::Selection(SelectionMessage::BulkPasswordEdited));
    assert_eq!(refused(&app), None, "typing again clears it");
    confirm(&mut app, PASSWORD, "another");
    assert_eq!(refused(&app), Some(BulkPasswordRefusal::Mismatch));
    confirm(&mut app, "", "");
    assert!(
        matches!(app.dialog, Some(Dialog::BulkPassword { .. })),
        "an empty password does nothing"
    );
    let SystemCredentials::Memory(entries) = &system else {
        unreachable!()
    };
    assert!(entries.lock().expect("entries").is_empty());
}

#[test]
fn each_profile_gets_the_password_for_its_own_server_and_account() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), &system);
    log_capture::start();
    open_dialog(&mut app, &ALL);
    confirm(&mut app, PASSWORD, PASSWORD);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(
        app.notice(),
        Some(&Notice::BulkPasswordUpdated {
            count: 5,
            winrm_skipped: 1,
        })
    );
    assert!(log_capture::has("INFO", &["5 of 5 profile(s)"]));
    for secret in [PASSWORD, "web.lab", "dc.lab", "kiosk.lab"] {
        assert!(log_capture::never(secret), "{secret} logged");
    }

    assert_eq!(
        ssh_answer(&mut app, "web", "web.lab", 22, "admin").as_deref(),
        Some(PASSWORD)
    );
    assert_eq!(
        ssh_answer(&mut app, "db", "db.lab", 2222, "root").as_deref(),
        Some(PASSWORD)
    );
    assert_eq!(
        ssh_answer(&mut app, "db", "web.lab", 22, "admin"),
        None,
        "never another profile's server"
    );
    assert_eq!(rdp_answer(&mut app).as_deref(), Some(PASSWORD));
    assert_eq!(ftp_answer(&mut app).as_deref(), Some(PASSWORD));

    let SystemCredentials::Memory(entries) = &system else {
        unreachable!()
    };
    let entries = entries.lock().expect("entries");
    let saved = |profile: &str| {
        entries
            .get(&password_entry(&id(profile)))
            .and_then(|bytes| decode(bytes))
    };
    let rdp = saved("rdp").expect("rdp saved").endpoint;
    assert_eq!(rdp.username.as_deref(), Some("CORP\\admin"), "its domain");
    let vnc = saved("vnc").expect("vnc saved").endpoint;
    assert_eq!(
        (vnc.protocol, vnc.host.as_str(), vnc.port, vnc.username),
        (CredentialProtocol::Vnc, "kiosk.lab", 5901, None)
    );
    for left in ["bare", "winrm", "telnet"] {
        assert!(saved(left).is_none(), "{left}");
    }
    let profiles = std::fs::read_to_string(dir.path().join("profiles.toml")).expect("read");
    assert!(!profiles.contains(PASSWORD), "never in the profiles file");
}

#[test]
fn a_password_refused_before_is_given_again_once_set_at_once() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &SystemCredentials::memory());
    open_dialog(&mut app, &["web", "db"]);
    confirm(&mut app, "old password", "old password");
    let effects = app.update(Message::OpenProfile(id("web")));
    let [Effect::Connect { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    let first = answer(&mut app, tab, attempt, asked("web.lab", 22, "admin", 1));
    assert_eq!(first.as_deref(), Some("old password"));
    // Asked again: the server refused it, and the user is asked from then on.
    assert_eq!(
        answer(&mut app, tab, attempt, asked("web.lab", 22, "admin", 2)),
        None
    );
    assert_eq!(ssh_answer(&mut app, "web", "web.lab", 22, "admin"), None);

    open_dialog(&mut app, &["web", "db"]);
    confirm(&mut app, PASSWORD, PASSWORD);
    assert_eq!(
        ssh_answer(&mut app, "web", "web.lab", 22, "admin").as_deref(),
        Some(PASSWORD)
    );
}

#[tokio::test]
async fn in_the_vault_every_password_is_saved_together_or_none_is() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), &system);
    create_vault(&mut app).await;
    open_dialog(&mut app, &["web", "db"]);
    confirm(&mut app, "first", "first");
    assert_eq!(
        app.notice(),
        Some(&Notice::BulkPasswordUpdated {
            count: 2,
            winrm_skipped: 0,
        })
    );
    let SystemCredentials::Memory(entries) = &system else {
        unreachable!()
    };
    assert!(
        entries.lock().expect("entries").is_empty(),
        "with a master password, nothing goes to the system's store"
    );

    // The copy kept before each save cannot be written: a folder is in its place.
    let vault = dir.path().join(heimdall_app::VAULT_FILE_NAME);
    let backup = sealvault::backup_path(&vault);
    if backup.exists() {
        std::fs::remove_file(&backup).expect("copy");
    }
    std::fs::create_dir(&backup).expect("folder");
    open_dialog(&mut app, &["web", "db", "rdp"]);
    confirm(&mut app, PASSWORD, PASSWORD);
    assert!(
        matches!(app.dialog, Some(Dialog::PasswordSaveFailed { .. })),
        "{:?}",
        app.dialog
    );
    app.update(Message::DismissDialog);
    assert_eq!(
        ssh_answer(&mut app, "web", "web.lab", 22, "admin").as_deref(),
        Some("first"),
        "not saved: the password before is kept"
    );
    assert_eq!(
        ssh_answer(&mut app, "db", "db.lab", 2222, "root").as_deref(),
        Some("first")
    );
    assert_eq!(rdp_answer(&mut app), None, "not saved, not used either");

    std::fs::remove_dir(&backup).expect("folder");
    open_dialog(&mut app, &["web", "db", "rdp"]);
    confirm(&mut app, PASSWORD, PASSWORD);
    assert_eq!(rdp_answer(&mut app).as_deref(), Some(PASSWORD));
}

#[tokio::test]
async fn without_a_store_or_with_the_vault_locked_nothing_is_offered() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &SystemCredentials::Unavailable);
    assert!(!app.can_save_passwords());
    open_dialog(&mut app, &["web", "db"]);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);

    let dir = tempfile::tempdir().expect("dir");
    let mut app = self::app(dir.path(), &SystemCredentials::memory());
    create_vault(&mut app).await;
    app.update(Message::LockVault);
    app.update(Message::DismissDialog);
    open_dialog(&mut app, &["web", "db"]);
    assert!(
        !matches!(app.dialog, Some(Dialog::BulkPassword { .. })),
        "{:?}",
        app.dialog
    );
}
