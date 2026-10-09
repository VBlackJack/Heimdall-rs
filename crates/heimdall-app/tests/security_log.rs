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

//! The security events of the diagnostics log, as the C# `FileLogger` writes them: the PIN
//! gate, the vault and the workspace lock, the user's answers about a server's key; each at
//! its decision, and never a PIN or a password.

#[path = "support/log_capture.rs"]
mod log_capture;

use std::path::Path;
use std::sync::Arc;

use heimdall_app::{
    App, AppConfig, ConnectionEvent, Effect, Message, OpenedVault, PinMessage, SystemCredentials,
    VaultProblem, VaultStatus, open_vault,
};
use heimdall_core::lockout::MAX_FAILED_ATTEMPTS;
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, PublicKey, Secret, fingerprint};
use heimdall_term::GridSize;

const PIN: &str = "2468";
const WRONG_PIN: &str = "1357";
const MASTER: &str = "correct horse battery staple";
const WRONG_MASTER: &str = "not the master password at all";
const HOST_KEY: &str = include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519.pub");

fn profile(id: &str, host: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: None,
        host: host.to_owned(),
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

/// The application over `dir`, knowing the SSH servers `a.lab`, `b.lab` and `c.lab`.
fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([
        profile("a", "a.lab"),
        profile("b", "b.lab"),
        profile("c", "c.lab"),
    ]);
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

fn secret(text: &str) -> Secret {
    Secret::new(text.to_owned())
}

fn submit_pin(app: &mut App, pin: &str) -> Vec<Effect> {
    app.update(Message::Pin(PinMessage::Submit(secret(pin))))
}

/// Submits `password` to the vault dialog shown, and hands the job's result back.
async fn submit_master(app: &mut App, password: &str, confirm: bool) {
    let effects = app.update(Message::SubmitVault {
        password: secret(password),
        new: None,
        confirm: confirm.then(|| secret(password)),
    });
    let (ticket, result): (_, Result<OpenedVault, VaultProblem>) =
        match <[Effect; 1]>::try_from(effects) {
            Ok(
                [
                    Effect::OpenVault {
                        path,
                        password,
                        job,
                        ticket,
                    },
                ],
            ) => (ticket, open_vault(path, password, job).await),
            other => panic!("expected OpenVault, got {other:?}"),
        };
    app.update(Message::VaultOpened(ticket, result));
}

#[test]
fn the_pin_gate_its_wrong_tries_its_lockout_and_its_changes_are_logged_never_a_pin() {
    let dir = tempfile::tempdir().expect("dir");
    log_capture::start();
    let mut app = app(dir.path());
    app.update(Message::Pin(PinMessage::Configure));
    app.update(Message::Pin(PinMessage::Save {
        current: secret(""),
        new: secret(PIN),
        confirm: secret(PIN),
    }));
    assert!(log_capture::has("INFO", &["PIN configured: set."]));

    let mut app = self::app(dir.path());
    submit_pin(&mut app, WRONG_PIN);
    assert!(log_capture::has("WARN", &["wrong PIN", "1 in a row"]));
    submit_pin(&mut app, PIN);
    assert!(log_capture::has("INFO", &["PIN gate satisfied."]));

    let mut app = self::app(dir.path());
    for _ in 0..MAX_FAILED_ATTEMPTS {
        submit_pin(&mut app, WRONG_PIN);
    }
    assert!(log_capture::has("WARN", &["wrong PIN", "locked out"]));
    submit_pin(&mut app, PIN);
    assert!(log_capture::has("WARN", &["not checked", "locked out"]));
    let effects = app.update(Message::DismissDialog);
    assert!(matches!(effects.as_slice(), [Effect::Exit]), "{effects:?}");
    assert!(log_capture::has(
        "INFO",
        &["PIN gate not satisfied; exiting."]
    ));

    assert!(log_capture::never(PIN), "{:?}", log_capture::lines());
    assert!(log_capture::never(WRONG_PIN), "{:?}", log_capture::lines());
}

#[test]
fn removing_the_pin_is_logged() {
    let dir = tempfile::tempdir().expect("dir");
    log_capture::start();
    let mut app = app(dir.path());
    app.update(Message::Pin(PinMessage::Configure));
    app.update(Message::Pin(PinMessage::Save {
        current: secret(""),
        new: secret(PIN),
        confirm: secret(PIN),
    }));
    let mut app = self::app(dir.path());
    submit_pin(&mut app, PIN);
    app.update(Message::Pin(PinMessage::Configure));
    app.update(Message::Pin(PinMessage::Remove(secret(PIN))));
    assert!(log_capture::has("INFO", &["PIN configured: removed."]));
    assert!(log_capture::never(PIN));
}

#[tokio::test]
async fn the_vault_gate_the_lock_and_wrong_master_passwords_are_logged_never_a_password() {
    let dir = tempfile::tempdir().expect("dir");
    log_capture::start();
    let mut app = app(dir.path());
    app.update(Message::ShowVault);
    submit_master(&mut app, MASTER, true).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    assert!(log_capture::has("INFO", &["vault created"]));

    app.update(Message::LockVault);
    assert!(log_capture::has("INFO", &["Workspace locked."]));
    submit_master(&mut app, WRONG_MASTER, false).await;
    assert!(log_capture::has(
        "WARN",
        &["wrong master password", "at the lock screen"]
    ));
    submit_master(&mut app, MASTER, false).await;
    assert!(log_capture::has("INFO", &["Workspace unlocked."]));

    let mut app = self::app(dir.path());
    submit_master(&mut app, WRONG_MASTER, false).await;
    assert!(log_capture::has(
        "WARN",
        &["wrong master password", "at start"]
    ));
    submit_master(&mut app, MASTER, false).await;
    assert!(log_capture::has("INFO", &["Vault unlock gate satisfied."]));

    let mut app = self::app(dir.path());
    let effects = app.update(Message::DismissDialog);
    assert!(matches!(effects.as_slice(), [Effect::Exit]), "{effects:?}");
    assert!(log_capture::has(
        "INFO",
        &["Vault unlock gate not satisfied; exiting."]
    ));

    assert!(log_capture::never(MASTER), "{:?}", log_capture::lines());
    assert!(
        log_capture::never(WRONG_MASTER),
        "{:?}",
        log_capture::lines()
    );
}

#[test]
fn each_answer_about_an_unknown_host_key_is_logged_with_its_fingerprint() {
    let dir = tempfile::tempdir().expect("dir");
    log_capture::start();
    let mut app = app(dir.path());
    let key = Arc::new(PublicKey::from_openssh(HOST_KEY.trim()).expect("key"));
    let presented = fingerprint(&key);
    let ask = |app: &mut App, id: &str, host: &str| {
        let effects = app.update(Message::OpenProfile(ProfileId::new(id)));
        let [Effect::Connect { tab, attempt, .. }] = effects.as_slice() else {
            panic!("expected Connect, got {effects:?}");
        };
        let tab = *tab;
        app.update(Message::Connection {
            tab,
            attempt: *attempt,
            event: ConnectionEvent::UnknownHostKey {
                host: host.to_owned(),
                port: 22,
                fingerprint: presented.clone(),
                key: Arc::clone(&key),
            },
        });
        tab
    };

    let tab = ask(&mut app, "a", "a.lab");
    app.update(Message::HostKeyTrustOnce(tab));
    assert!(log_capture::has(
        "INFO",
        &["a.lab:22", "for this run", &presented]
    ));
    let tab = ask(&mut app, "b", "b.lab");
    app.update(Message::HostKeyDecision { tab, accept: true });
    assert!(log_capture::has(
        "INFO",
        &["b.lab:22", "recorded", &presented]
    ));
    let tab = ask(&mut app, "c", "c.lab");
    app.update(Message::HostKeyDecision { tab, accept: false });
    assert!(log_capture::has(
        "INFO",
        &["c.lab:22", "refused by the user"]
    ));
}
