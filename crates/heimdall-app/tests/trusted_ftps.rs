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

//! The FTPS certificates trusted, as the Settings page lists them beside the RDP ones:
//! pinned with their names once accepted, listed, forgotten one key or one server at a
//! time; and once forgotten, the next connection asks again.

use std::net::{Ipv4Addr, TcpListener};
use std::path::Path;
use std::time::Duration;

use heimdall_app::ftp_driver::{FtpRequest, ftp_events};
use heimdall_app::{
    AnswerRegistry, App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, Message, Notice,
    SettingsMessage, SystemCredentials, TabId, TrustedKey, TrustedKeysMessage, UiError,
};
use heimdall_core::profile::{FtpProfile, ProfileId};
use heimdall_core::store::ProfileStore;
use heimdall_rdp::{CertificateHash, Fingerprint, KnownRdpHosts, Verdict};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio_stream::StreamExt as _;
use unftp_sbe_fs::Filesystem;

/// Longest wait for the test server, or for one event.
const STEP: Duration = Duration::from_secs(20);
/// A pin, as the file writes it.
const PIN: &str = "SHA256:rgJ0Y04RcqyBpgUaWIbkDqk3bEPjR9jNUxyEc7cvFq0";
/// `CN=ftp.lab`, as the file writes a name.
const SUBJECT: &str = "Q049ZnRwLmxhYg";
/// 2026-03-15 12:00:30 UTC, as the file writes a time.
const TRUSTED: u64 = 1_773_576_030;

/// Another pin than [`PIN`].
fn other_pin() -> Fingerprint {
    format!("SHA256:{}", "A".repeat(43)).parse().expect("pin")
}

/// Serves `root` over explicit FTPS, with a fresh self-signed certificate for `localhost`
/// written in `keys`; returns the port.
async fn serve(root: &Path, keys: &Path) -> u16 {
    let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("free port")
        .local_addr()
        .expect("address")
        .port();
    let home = root.to_owned();
    let issued = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("cert");
    let (cert, key) = (keys.join("cert.pem"), keys.join("key.pem"));
    std::fs::write(&cert, issued.cert.pem()).expect("cert file");
    std::fs::write(&key, issued.signing_key.serialize_pem()).expect("key file");
    let server = libunftp::ServerBuilder::new(Box::new(move || {
        Filesystem::new(home.clone()).expect("root")
    }))
    .ftps(cert, key)
    .build()
    .expect("server");
    tokio::spawn(server.listen(format!("127.0.0.1:{port}")));
    tokio::time::timeout(STEP, async {
        while tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .is_err()
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("started");
    port
}

/// The anonymous FTPS profile "files", on `host:port`.
fn profile(host: &str, port: u16) -> FtpProfile {
    FtpProfile {
        id: ProfileId::new("files"),
        name: "files".to_owned(),
        group: None,
        host: host.to_owned(),
        port,
        username: None,
        passive: true,
        tls: true,
        vault_entry: None,
    }
}

/// An application knowing `profiles`, its files in `dir`.
fn app(dir: &Path, profiles: Vec<FtpProfile>) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_ftp(profiles);
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

fn trusted(app: &mut App, message: TrustedKeysMessage) -> Vec<Effect> {
    app.update(Message::Settings(SettingsMessage::TrustedKeys(message)))
}

/// The FTP attempt `effects` start: its tab, its attempt and what it needs.
fn attempt_of(effects: &[Effect]) -> (TabId, AttemptId, FtpRequest) {
    let [
        Effect::ConnectFtp {
            tab,
            attempt,
            request,
        },
    ] = effects
    else {
        panic!("{effects:?}");
    };
    (*tab, *attempt, (**request).clone())
}

/// The first thing the attempt of `request` says.
async fn first(request: FtpRequest) -> ConnectionEvent {
    let mut events = ftp_events(request, AnswerRegistry::default());
    tokio::time::timeout(STEP, events.next())
        .await
        .expect("in time")
        .expect("an event")
}

#[tokio::test]
async fn an_accepted_ftps_certificate_is_listed_with_its_names_and_once_forgotten_asked_again() {
    let (root, keys, dir) = (
        tempfile::tempdir().expect("root"),
        tempfile::tempdir().expect("keys"),
        tempfile::tempdir().expect("dir"),
    );
    let port = serve(root.path(), keys.path()).await;
    let mut app = app(dir.path(), vec![profile("localhost", port)]);

    // First contact: the question, with the certificate's subject.
    let (tab, attempt, request) =
        attempt_of(&app.update(Message::ConnectProfile(ProfileId::new("files"))));
    let event = first(request).await;
    let (fingerprint, subject, whole) = match &event {
        ConnectionEvent::UnknownRdpCertificate {
            fingerprint,
            subject,
            details: Some(details),
            ..
        } => (*fingerprint, subject.clone(), details.certificate),
        other => panic!("the certificate question, got {other:?}"),
    };
    assert!(subject.is_some(), "{event:?}");
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    });

    // Trusted: the next attempt goes through and records it with its names and the time.
    let (tab, attempt, request) =
        attempt_of(&app.update(Message::HostKeyDecision { tab, accept: true }));
    assert_eq!(
        request.accepted,
        Some(whole),
        "the very certificate asked about"
    );
    let event = first(request).await;
    assert!(
        matches!(event, ConnectionEvent::FilesReady { .. }),
        "{event:?}"
    );
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    });
    trusted(&mut app, TrustedKeysMessage::Refresh);
    let keys = app.trusted_keys();
    assert!(keys.rdp.is_empty(), "the RDP list keeps to its own file");
    let [entry] = keys.ftps.as_slice() else {
        panic!("{:?}", keys.ftps);
    };
    assert_eq!(
        (entry.host.as_str(), entry.port, entry.fingerprint),
        ("localhost", port, fingerprint)
    );
    assert_eq!(entry.subject, subject);
    assert_eq!(entry.issuer, entry.subject, "self-signed");
    assert_eq!(entry.certificate, Some(whole), "pinned whole");
    let key = TrustedKey::Ftps(entry.clone());
    assert!(key.trusted_since().is_some(), "{entry:?}");
    assert_eq!(key.address(), format!("localhost:{port}"));

    // Forgotten once confirmed.
    trusted(&mut app, TrustedKeysMessage::RequestForget(key.clone()));
    assert_eq!(app.dialog, Some(Dialog::ForgetTrustedKey(key.clone())));
    app.update(Message::ConfirmDialog);
    assert!(app.trusted_keys().ftps.is_empty());
    assert_eq!(
        app.notice(),
        Some(&Notice::CertificateForgotten(key.address()))
    );

    // The next connection asks again.
    let (_, _, request) = attempt_of(&app.update(Message::ConnectProfile(ProfileId::new("files"))));
    let event = first(request).await;
    assert!(
        matches!(
            event,
            ConnectionEvent::UnknownRdpCertificate { fingerprint: asked, .. } if asked == fingerprint
        ),
        "{event:?}"
    );
}

#[test]
fn older_lines_are_listed_and_one_key_or_a_whole_server_is_forgotten_in_its_own_file() {
    let dir = tempfile::tempdir().expect("dir");
    let other = other_pin();
    // Recorded with its names and time; then lines from before them.
    std::fs::write(
        dir.path().join("known_ftps_hosts"),
        format!(
            "ftp.lab:21 {PIN} trusted={TRUSTED} subject={SUBJECT} issuer={SUBJECT}\n\
             ftp.lab:21 {other}\nfiles.lab:990 {PIN}\n"
        ),
    )
    .expect("write");
    // The same server in the RDP file: never touched by the FTPS list.
    let rdp = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    rdp.record("ftp.lab", 21, &PIN.parse().expect("pin"))
        .expect("record");
    let mut app = app(dir.path(), Vec::new());
    trusted(&mut app, TrustedKeysMessage::Refresh);
    let ftps = app.trusted_keys().ftps.clone();
    assert_eq!(ftps.len(), 3);
    assert_eq!(ftps[0].subject.as_deref(), Some("CN=ftp.lab"));
    assert_eq!(ftps[0].issuer.as_deref(), Some("CN=ftp.lab"));
    assert!(TrustedKey::Ftps(ftps[0].clone()).trusted_since().is_some());
    assert_eq!(
        (ftps[1].subject.as_ref(), ftps[1].issuer.as_ref()),
        (None, None)
    );
    assert_eq!(TrustedKey::Ftps(ftps[1].clone()).trusted_since(), None);
    assert_eq!(
        app.trusted_keys()
            .keys_of_server(&TrustedKey::Ftps(ftps[0].clone())),
        2
    );

    // One key: the other of its server stays.
    trusted(
        &mut app,
        TrustedKeysMessage::RequestForget(TrustedKey::Ftps(ftps[0].clone())),
    );
    app.update(Message::ConfirmDialog);
    let left: Vec<(String, Fingerprint)> = app
        .trusted_keys()
        .ftps
        .iter()
        .map(|entry| (entry.host.clone(), entry.fingerprint))
        .collect();
    let pin: Fingerprint = PIN.parse().expect("pin");
    assert_eq!(
        left,
        [("ftp.lab".to_owned(), other), ("files.lab".to_owned(), pin)]
    );

    // A whole server, after its own question.
    KnownRdpHosts::new(dir.path().join("known_ftps_hosts"))
        .record("ftp.lab", 21, &pin)
        .expect("record");
    trusted(&mut app, TrustedKeysMessage::Refresh);
    let key = TrustedKey::Ftps(app.trusted_keys().ftps[0].clone());
    trusted(
        &mut app,
        TrustedKeysMessage::RequestForgetServer(key.clone()),
    );
    assert_eq!(
        app.dialog,
        Some(Dialog::ForgetTrustedServer {
            key: key.clone(),
            count: 2
        })
    );
    app.update(Message::DismissDialog);
    assert_eq!(app.trusted_keys().ftps.len(), 3, "kept when not confirmed");
    trusted(&mut app, TrustedKeysMessage::RequestForgetServer(key));
    app.update(Message::ConfirmDialog);
    let left: Vec<&str> = app
        .trusted_keys()
        .ftps
        .iter()
        .map(|entry| entry.host.as_str())
        .collect();
    assert_eq!(left, ["files.lab"]);
    assert_eq!(
        app.notice(),
        Some(&Notice::ServerCertificatesForgotten(
            "ftp.lab:21".to_owned()
        ))
    );
    assert_eq!(
        rdp.verdict("ftp.lab", 21, &pin).expect("read"),
        Verdict::Known,
        "the RDP pin of the same address stays"
    );
    assert_eq!(app.trusted_keys().rdp.len(), 1);
}

#[test]
fn an_rdp_server_trusted_with_two_certificates_is_forgotten_whole_too() {
    let dir = tempfile::tempdir().expect("dir");
    let rdp = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    let pin: Fingerprint = PIN.parse().expect("pin");
    rdp.record("dc.lab", 3389, &pin).expect("record");
    rdp.record("dc.lab", 3389, &other_pin()).expect("record");
    rdp.record("web.lab", 3389, &pin).expect("record");
    let mut app = app(dir.path(), Vec::new());
    trusted(&mut app, TrustedKeysMessage::Refresh);
    let key = TrustedKey::Rdp(app.trusted_keys().rdp[1].clone());
    trusted(&mut app, TrustedKeysMessage::RequestForgetServer(key));
    app.update(Message::ConfirmDialog);
    assert_eq!(
        rdp.verdict("dc.lab", 3389, &pin).expect("read"),
        Verdict::Unknown,
        "asked about again"
    );
    assert_eq!(
        rdp.verdict("web.lab", 3389, &pin).expect("read"),
        Verdict::Known
    );
    assert_eq!(
        app.notice(),
        Some(&Notice::ServerCertificatesForgotten(
            "dc.lab:3389".to_owned()
        ))
    );
}

#[test]
fn an_ftp_tab_whose_certificate_changed_forgets_its_server_and_asks_again() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_ftps_hosts"));
    let pin: Fingerprint = PIN.parse().expect("pin");
    known.record("ftp.lab", 21, &pin).expect("record");
    known.record("other.lab", 21, &pin).expect("record");
    let mut app = app(dir.path(), vec![profile("ftp.lab", 21)]);
    let (tab, attempt, _) =
        attempt_of(&app.update(Message::ConnectProfile(ProfileId::new("files"))));

    // Only a changed key is forgotten.
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::Timeout),
    });
    assert!(app.update(Message::ForgetServer(tab)).is_empty());
    assert_eq!(
        known.verdict("ftp.lab", 21, &pin).expect("read"),
        Verdict::Known
    );

    let (tab, attempt, _) = attempt_of(&app.update(Message::ReconnectTab(tab)));
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::HostKeyChanged {
            target: None,
            recorded: PIN.to_owned(),
            offered: other_pin().to_string(),
        }),
    });
    let (again, _, request) = attempt_of(&app.update(Message::ForgetServer(tab)));
    assert_eq!(again, tab);
    assert_eq!(request.accepted, None, "asked about, not trusted");
    assert_eq!(
        known.verdict("ftp.lab", 21, &pin).expect("read"),
        Verdict::Unknown,
        "forgotten"
    );
    assert_eq!(
        known.verdict("other.lab", 21, &pin).expect("read"),
        Verdict::Known,
        "the other servers stay"
    );
}

/// The anonymous FTPS profile `id`, on `host:port`.
fn profile_as(id: &str, host: &str, port: u16) -> FtpProfile {
    FtpProfile {
        id: ProfileId::new(id),
        ..profile(host, port)
    }
}

/// Trusts once, from the question its attempt asks, the certificate of hash `whole` on key
/// `key` that the server of profile `id` presented; the attempt built on the answer.
fn trust_once(
    app: &mut App,
    id: &str,
    (key, whole): (Fingerprint, CertificateHash),
) -> (TabId, AttemptId, FtpRequest) {
    let (tab, attempt, _) = attempt_of(&app.update(Message::ConnectProfile(ProfileId::new(id))));
    let (host, port) = match app.tab(tab).map(|tab| &tab.profile) {
        Some(heimdall_app::TabProfile::Ftp(profile)) => (profile.host.clone(), profile.port),
        other => panic!("{other:?}"),
    };
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::UnknownRdpCertificate {
            host,
            port,
            fingerprint: key,
            subject: None,
            details: Some(Box::new(heimdall_app::CertificateDetails {
                issuer: "CN=ftp.lab".to_owned(),
                validity: heimdall_rdp::Validity {
                    not_before: std::time::UNIX_EPOCH,
                    not_after: std::time::UNIX_EPOCH,
                },
                issue: heimdall_tls::ValidationIssue::SelfSigned,
                certificate: whole,
                renewal: None,
            })),
        },
    });
    let (again, attempt, request) = attempt_of(&app.update(Message::HostKeyTrustOnce(tab)));
    assert_eq!(request.trusted_once, Some(whole));
    (again, attempt, request)
}

/// What the next attempt of profile `id` carries as trusted for this run.
fn run_trust_of(app: &mut App, id: &str) -> Vec<CertificateHash> {
    attempt_of(&app.update(Message::ConnectProfile(ProfileId::new(id))))
        .2
        .trusted_for_run
}

#[test]
fn a_key_trusted_once_is_the_servers_whatever_the_case_until_its_server_is_forgotten() {
    let dir = tempfile::tempdir().expect("dir");
    let pin: Fingerprint = PIN.parse().expect("pin");
    let whole = CertificateHash::of(b"the certificate");
    let mut app = app(
        dir.path(),
        vec![
            profile_as("upper", "FTP.Lab", 21),
            profile_as("lower", "ftp.lab", 21),
            profile_as("elsewhere", "ftp.lab", 990),
        ],
    );

    // Trusted once under one spelling: the server's under the other too, as its pins are.
    let (tab, attempt, _) = trust_once(&mut app, "upper", (pin, whole));
    assert_eq!(run_trust_of(&mut app, "lower"), [whole]);
    assert!(
        run_trust_of(&mut app, "elsewhere").is_empty(),
        "another port"
    );

    // Refused as no longer valid, then forgotten from the card: the key trusted once too.
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::PinnedCertificateInvalid {
            target: "FTP.Lab:21".to_owned(),
            fingerprint: PIN.to_owned(),
            issue: heimdall_tls::ValidationIssue::Expired,
            not_after: std::time::UNIX_EPOCH,
        }),
    });
    let (_, _, request) = attempt_of(&app.update(Message::ForgetServer(tab)));
    assert!(request.trusted_for_run.is_empty(), "asked about again");
    assert_eq!(request.trusted_once, None);
    assert!(run_trust_of(&mut app, "lower").is_empty());

    // Trusted once again, its server pinned on file under the other spelling, then forgotten
    // from the Settings: the key trusted once too.
    trust_once(
        &mut app,
        "upper",
        (other_pin(), CertificateHash::of(b"another certificate")),
    );
    KnownRdpHosts::new(dir.path().join("known_ftps_hosts"))
        .record("ftp.lab", 21, &pin)
        .expect("record");
    trusted(&mut app, TrustedKeysMessage::Refresh);
    let key = TrustedKey::Ftps(app.trusted_keys().ftps[0].clone());
    trusted(&mut app, TrustedKeysMessage::RequestForgetServer(key));
    app.update(Message::ConfirmDialog);
    assert!(app.trusted_keys().ftps.is_empty(), "forgotten on file");
    assert!(
        run_trust_of(&mut app, "upper").is_empty(),
        "and for this run"
    );
}
