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

//! The keys trusted for servers, read, copied and forgotten from the Settings page.

use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use heimdall_app::{
    App, AppConfig, Dialog, Effect, Message, Notice, SettingsMessage, SystemCredentials,
    TrustedKey, TrustedKeysMessage,
};
use heimdall_rdp::{Fingerprint, KnownRdpHosts};
use heimdall_ssh::{AgentSource, KnownHosts, PublicKey, fingerprint};
use heimdall_term::GridSize;

const ED25519: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEdv/0kqpfKUkuXCpQIlyU34zlRbf2MM2wBP+uTTnDTR";
const ECDSA: &str = "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBA2GWy5WFXBq3a2EBW/bPO3xRNWqgOqM5Uc3eIv2olY8KlWjHIGhsSG3nAZNs2/c/eZD6BT6nLEyjCrk8wnoT+s=";
const PIN: &str = "SHA256:rgJ0Y04RcqyBpgUaWIbkDqk3bEPjR9jNUxyEc7cvFq0";

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

fn trusted(app: &mut App, message: TrustedKeysMessage) -> Vec<Effect> {
    app.update(Message::Settings(SettingsMessage::TrustedKeys(message)))
}

fn key(text: &str) -> PublicKey {
    PublicKey::from_openssh(text).expect("key")
}

fn other_pin() -> Fingerprint {
    format!("SHA256:{}", "A".repeat(43)).parse().expect("pin")
}

/// An application whose files trust two SSH servers and two keys of one RDP server.
fn trusting(dir: &Path) -> App {
    let hosts = KnownHosts::new(dir.join("known_hosts"));
    hosts.learn("web.lab", 22, &key(ED25519)).expect("learn");
    hosts.learn("db.lab", 2222, &key(ECDSA)).expect("learn");
    let rdp = KnownRdpHosts::new(dir.join("known_rdp_hosts"));
    rdp.record("dc.lab", 3389, &PIN.parse().expect("pin"))
        .expect("record");
    rdp.record("dc.lab", 3389, &other_pin()).expect("record");
    let mut app = app(dir);
    trusted(&mut app, TrustedKeysMessage::Refresh);
    app
}

#[test]
fn the_keys_are_read_when_asked_and_not_before() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = trusting(dir.path());
    let keys = app.trusted_keys();
    let ssh: Vec<(String, u16, String)> = keys
        .ssh
        .iter()
        .map(|entry| (entry.host.clone(), entry.port, entry.algorithm.clone()))
        .collect();
    assert_eq!(
        ssh,
        [
            ("web.lab".to_owned(), 22, "ssh-ed25519".to_owned()),
            ("db.lab".to_owned(), 2222, "ecdsa-sha2-nistp256".to_owned()),
        ]
    );
    assert_eq!(keys.ssh[0].fingerprint, fingerprint(&key(ED25519)));
    assert_eq!(keys.rdp.len(), 2);
    assert_eq!(keys.unreadable, None);
    // Learnt meanwhile: shown once read again.
    KnownHosts::new(dir.path().join("known_hosts"))
        .learn("new.lab", 22, &key(ED25519))
        .expect("learn");
    assert_eq!(app.trusted_keys().ssh.len(), 2, "not read yet");
    trusted(&mut app, TrustedKeysMessage::Refresh);
    assert_eq!(app.trusted_keys().ssh.len(), 3);
}

#[test]
fn a_certificate_says_when_it_was_trusted_in_local_time_and_an_old_line_says_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    // 2026-03-15 12:00:30 UTC, recorded with its names; then a line from before them.
    std::fs::write(
        dir.path().join("known_rdp_hosts"),
        format!("dc.lab:3389 {PIN} trusted=1773576030 subject=Q049ZGMubGFi\nweb.lab:3389 {PIN}\n"),
    )
    .expect("write");
    KnownHosts::new(dir.path().join("known_hosts"))
        .learn("web.lab", 22, &key(ED25519))
        .expect("learn");
    let mut app = app(dir.path());
    trusted(&mut app, TrustedKeysMessage::Refresh);
    let rdp = &app.trusted_keys().rdp;
    assert_eq!(rdp[0].subject.as_deref(), Some("CN=dc.lab"));
    let local =
        chrono::DateTime::<chrono::Local>::from(UNIX_EPOCH + Duration::from_secs(1_773_576_030));
    assert_eq!(
        TrustedKey::Rdp(rdp[0].clone()).trusted_since(),
        Some(local.format("%Y-%m-%d %H:%M").to_string())
    );
    assert_eq!(TrustedKey::Rdp(rdp[1].clone()).trusted_since(), None);
    assert_eq!(
        TrustedKey::Ssh(app.trusted_keys().ssh[0].clone()).trusted_since(),
        None,
        "an SSH key has no such time"
    );
}

#[test]
fn a_fingerprint_is_copied_whole_and_said() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = trusting(dir.path());
    let entry = TrustedKey::Ssh(app.trusted_keys().ssh[1].clone());
    let effects = trusted(&mut app, TrustedKeysMessage::CopyFingerprint(entry));
    let [Effect::WriteClipboard(copied)] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(*copied, fingerprint(&key(ECDSA)));
    assert_eq!(
        app.notice(),
        Some(&Notice::FingerprintCopied("db.lab:2222".to_owned()))
    );
}

#[test]
fn an_ssh_server_is_forgotten_once_confirmed_and_the_others_stay() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = trusting(dir.path());
    let entry = TrustedKey::Ssh(app.trusted_keys().ssh[0].clone());
    trusted(&mut app, TrustedKeysMessage::RequestForget(entry.clone()));
    assert_eq!(app.dialog, Some(Dialog::ForgetTrustedKey(entry.clone())));
    app.update(Message::DismissDialog);
    assert_eq!(app.trusted_keys().ssh.len(), 2, "kept when not confirmed");

    trusted(&mut app, TrustedKeysMessage::RequestForget(entry));
    app.update(Message::ConfirmDialog);
    assert_eq!(app.dialog, None);
    let left: Vec<&str> = app
        .trusted_keys()
        .ssh
        .iter()
        .map(|entry| entry.host.as_str())
        .collect();
    assert_eq!(left, ["db.lab"]);
    assert!(
        KnownHosts::new(dir.path().join("known_hosts"))
            .recorded("web.lab", 22)
            .expect("read")
            .is_empty(),
        "gone from the file"
    );
    assert_eq!(
        app.notice(),
        Some(&Notice::HostKeyRemoved("web.lab:22".to_owned()))
    );
}

#[test]
fn one_rdp_certificate_is_forgotten_and_the_other_of_its_server_stays() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = trusting(dir.path());
    let entry = app
        .trusted_keys()
        .rdp
        .iter()
        .find(|entry| entry.fingerprint.to_string() == PIN)
        .cloned()
        .expect("the pin");
    trusted(
        &mut app,
        TrustedKeysMessage::RequestForget(TrustedKey::Rdp(entry)),
    );
    app.update(Message::ConfirmDialog);
    let left: Vec<Fingerprint> = app
        .trusted_keys()
        .rdp
        .iter()
        .map(|entry| entry.fingerprint)
        .collect();
    assert_eq!(left, [other_pin()]);
    assert_eq!(
        app.notice(),
        Some(&Notice::CertificateForgotten("dc.lab:3389".to_owned()))
    );
}

#[test]
fn a_key_that_cannot_be_forgotten_is_said_and_the_list_is_read_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = trusting(dir.path());
    let entry = TrustedKey::Ssh(app.trusted_keys().ssh[0].clone());
    // A folder where the file was: it can be neither read nor written.
    std::fs::remove_file(dir.path().join("known_hosts")).expect("remove");
    std::fs::create_dir(dir.path().join("known_hosts")).expect("folder");
    trusted(&mut app, TrustedKeysMessage::RequestForget(entry));
    app.update(Message::ConfirmDialog);
    assert!(
        matches!(app.dialog, Some(Dialog::StoreError { .. })),
        "{:?}",
        app.dialog
    );
    assert!(app.trusted_keys().ssh.is_empty());
    assert!(app.trusted_keys().unreadable.is_some(), "said on the page");
}
