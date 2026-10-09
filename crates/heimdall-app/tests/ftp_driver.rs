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

//! The FTP driver against an FTP server run in the test: a Files session for an anonymous
//! profile, and for explicit FTPS the certificate question, then the pin recorded once the
//! user trusts it; a pin no longer valid refused, never asked about.

use std::net::{Ipv4Addr, TcpListener};
use std::path::Path;
use std::time::Duration;

use heimdall_app::ftp_driver::{FtpRequest, ftp_events};
use heimdall_app::{AnswerRegistry, ConnectionEvent};
use heimdall_core::profile::{FtpProfile, ProfileId};
use heimdall_rdp::{KnownRdpHosts, ValidityPeriod, Verdict};
use heimdall_tls::ValidationIssue;
use tokio_stream::StreamExt as _;
use tokio_util::sync::CancellationToken;
use unftp_sbe_fs::Filesystem;

const STEP: Duration = Duration::from_secs(20);

/// Serves `root` over FTP, over explicit FTPS when `keys` is given: with the certificate
/// written there, else a fresh one.
async fn serve(root: &Path, keys: Option<&Path>) -> u16 {
    let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("free port")
        .local_addr()
        .expect("address")
        .port();
    let home = root.to_owned();
    let mut builder = libunftp::ServerBuilder::new(Box::new(move || {
        Filesystem::new(home.clone()).expect("root")
    }));
    if let Some(keys) = keys {
        let (cert, key) = (keys.join("cert.pem"), keys.join("key.pem"));
        if !cert.exists() {
            let issued =
                rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("cert");
            std::fs::write(&cert, issued.cert.pem()).expect("cert file");
            std::fs::write(&key, issued.signing_key.serialize_pem()).expect("key file");
        }
        builder = builder.ftps(cert, key);
    }
    tokio::spawn(
        builder
            .build()
            .expect("server")
            .listen(format!("127.0.0.1:{port}")),
    );
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

fn request(port: u16, tls: bool, known_hosts: &Path) -> FtpRequest {
    FtpRequest {
        profile: FtpProfile {
            id: ProfileId::new("ftp"),
            name: "ftp".to_owned(),
            group: None,
            host: "localhost".to_owned(),
            port,
            username: None,
            passive: true,
            tls,
            vault_entry: None,
        },
        known_hosts: known_hosts.to_owned(),
        accepted: None,
        trusted_once: None,
        trusted_for_run: Vec::new(),
        cancel: CancellationToken::new(),
    }
}

async fn first(request: FtpRequest) -> ConnectionEvent {
    let mut events = ftp_events(request, AnswerRegistry::default());
    tokio::time::timeout(STEP, events.next())
        .await
        .expect("in time")
        .expect("an event")
}

#[tokio::test]
async fn an_anonymous_profile_opens_a_files_session() {
    let root = tempfile::tempdir().expect("root");
    let dir = tempfile::tempdir().expect("dir");
    let port = serve(root.path(), None).await;
    let event = first(request(port, false, &dir.path().join("known_ftps_hosts"))).await;
    let ConnectionEvent::FilesReady { client, .. } = event else {
        panic!("{event:?}");
    };
    let top = client
        .canonical(&heimdall_files::RemotePath::from("."))
        .await
        .expect("a working session");
    assert!(top.is_absolute());
}

#[tokio::test]
async fn an_unknown_ftps_certificate_is_asked_about_then_pinned_once_trusted() {
    let root = tempfile::tempdir().expect("root");
    let keys = tempfile::tempdir().expect("keys");
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_ftps_hosts");
    let port = serve(root.path(), Some(keys.path())).await;

    let event = first(request(port, true, &known)).await;
    let ConnectionEvent::UnknownRdpCertificate {
        host,
        port: asked_port,
        fingerprint,
        subject,
        details,
    } = event
    else {
        panic!("the certificate question, got {event:?}");
    };
    assert_eq!((host.as_str(), asked_port), ("localhost", port));
    // What the C# FTPS prompt shows beside the subject.
    let details = details.expect("the issuer, the validity and the issue");
    assert_eq!(
        Some(details.issuer.as_str()),
        subject.as_deref(),
        "self-signed: its own issuer"
    );
    assert_eq!(
        details.validity.period(std::time::SystemTime::now()),
        ValidityPeriod::Current
    );
    assert!(
        matches!(
            details.issue,
            // A machine without certificate authorities has none to vouch for it.
            ValidationIssue::SelfSigned | ValidationIssue::NoSystemStore
        ),
        "{:?}",
        details.issue
    );

    // Trusted: the next attempt goes through and records it, whole.
    let whole = details.certificate;
    assert_eq!(details.renewal, None, "a first contact");
    let mut accepted = request(port, true, &known);
    accepted.accepted = Some(whole);
    let event = first(accepted).await;
    assert!(
        matches!(event, ConnectionEvent::FilesReady { .. }),
        "{event:?}"
    );
    assert_eq!(
        KnownRdpHosts::new(&known)
            .verdict("localhost", port, &fingerprint)
            .expect("read"),
        Verdict::Known
    );
    // With the names of its certificate, self-signed, and the time.
    let [entry] = KnownRdpHosts::new(&known)
        .entries()
        .expect("read")
        .try_into()
        .expect("one entry");
    assert!(entry.subject.is_some(), "{entry:?}");
    assert_eq!(entry.issuer, entry.subject, "self-signed");
    assert!(entry.trusted.is_some(), "{entry:?}");
    assert_eq!(entry.certificate, Some(whole), "pinned whole");
    assert_eq!(entry.validity, Some(details.validity));
    // Known now: no question.
    let event = first(request(port, true, &known)).await;
    assert!(
        matches!(event, ConnectionEvent::FilesReady { .. }),
        "{event:?}"
    );
}

#[tokio::test]
async fn a_certificate_trusted_for_this_run_goes_through_without_being_recorded() {
    let root = tempfile::tempdir().expect("root");
    let keys = tempfile::tempdir().expect("keys");
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_ftps_hosts");
    let port = serve(root.path(), Some(keys.path())).await;
    let ConnectionEvent::UnknownRdpCertificate {
        details: Some(details),
        ..
    } = first(request(port, true, &known)).await
    else {
        panic!("the certificate question");
    };
    let mut once = request(port, true, &known);
    once.trusted_for_run = vec![details.certificate];
    let event = first(once).await;
    assert!(
        matches!(event, ConnectionEvent::FilesReady { .. }),
        "{event:?}"
    );
    assert!(!known.exists(), "never written");
}

/// The Files session of the FTP server on `port`.
async fn session(port: u16, known_hosts: &Path) -> heimdall_files::RemoteSession {
    match first(request(port, false, known_hosts)).await {
        ConnectionEvent::FilesReady { client, .. } => client,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn entries_copied_on_one_server_are_pasted_on_another_never_over_what_is_there() {
    use heimdall_app::files::{CopySource, copy_across};
    use heimdall_files::RemotePath;

    let (first_root, second_root) = (
        tempfile::tempdir().expect("root"),
        tempfile::tempdir().expect("root"),
    );
    std::fs::write(first_root.path().join("notes.txt"), "from the first").expect("file");
    std::fs::create_dir_all(first_root.path().join("site").join("css")).expect("folder");
    std::fs::write(first_root.path().join("site").join("index.html"), "<p>").expect("file");
    std::fs::write(
        first_root.path().join("site").join("css").join("main.css"),
        "p {}",
    )
    .expect("file");
    // Taken on the second server: the copy takes its first free copy name.
    std::fs::write(second_root.path().join("notes.txt"), "already there").expect("file");

    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_ftps_hosts");
    let from = session(serve(first_root.path(), None).await, &known).await;
    let to = session(serve(second_root.path(), None).await, &known).await;
    let top = |name: &str| RemotePath::from("/").join(name.as_bytes());
    let results = copy_across(
        from,
        to,
        vec![
            CopySource {
                path: top("notes.txt"),
                folder: false,
            },
            CopySource {
                path: top("site"),
                folder: true,
            },
        ],
        RemotePath::from("/"),
        dir.path().join("staging"),
        CancellationToken::new(),
    )
    .await;
    assert!(
        results.iter().all(|(_, result)| result.is_ok()),
        "{results:?}"
    );
    assert_eq!(
        results[0].1.as_ref().map(RemotePath::display),
        Ok("/notes (copy).txt".to_owned())
    );
    let read = |path: &[&str]| {
        let mut file = second_root.path().to_owned();
        for part in path {
            file.push(part);
        }
        std::fs::read_to_string(file).expect("copied")
    };
    assert_eq!(read(&["notes.txt"]), "already there", "never written over");
    assert_eq!(read(&["notes (copy).txt"]), "from the first");
    assert_eq!(read(&["site", "index.html"]), "<p>");
    assert_eq!(read(&["site", "css", "main.css"]), "p {}");
    // Nothing is left on this computer.
    assert_eq!(
        std::fs::read_dir(dir.path().join("staging"))
            .expect("staging")
            .count(),
        0
    );
}

/// Writes in `keys` a fresh self-signed certificate for `localhost`, over since 2001; the
/// key it is pinned by, and the hash of the whole of it.
fn expired_certificate(keys: &Path) -> (heimdall_rdp::Fingerprint, heimdall_rdp::CertificateHash) {
    let mut params = rcgen::CertificateParams::new(vec!["localhost".to_owned()]).expect("params");
    params.not_before = rcgen::date_time_ymd(2000, 1, 1);
    params.not_after = rcgen::date_time_ymd(2001, 1, 1);
    let key = rcgen::KeyPair::generate().expect("key");
    let cert = params.self_signed(&key).expect("cert");
    std::fs::write(keys.join("cert.pem"), cert.pem()).expect("cert file");
    std::fs::write(keys.join("key.pem"), key.serialize_pem()).expect("key file");
    (
        heimdall_rdp::ServerCertificate::from_der(cert.der())
            .expect("readable")
            .fingerprint,
        heimdall_rdp::CertificateHash::of(cert.der()),
    )
}

/// Whether `event` refuses the certificate pinned by `pin` on `port` as no longer valid,
/// over.
fn refused_as_over(event: &ConnectionEvent, port: u16, pin: heimdall_rdp::Fingerprint) -> bool {
    matches!(
        event,
        ConnectionEvent::Failed(heimdall_app::UiError::PinnedCertificateInvalid {
            target,
            fingerprint,
            issue: ValidationIssue::Expired,
            ..
        }) if *target == format!("localhost:{port}") && *fingerprint == pin.to_string()
    )
}

#[tokio::test]
async fn a_pinned_ftps_certificate_over_is_refused_never_asked_about_and_stays_pinned() {
    let root = tempfile::tempdir().expect("root");
    let keys = tempfile::tempdir().expect("keys");
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_ftps_hosts");
    let (pin, _) = expired_certificate(keys.path());
    let port = serve(root.path(), Some(keys.path())).await;
    // By its key alone, as an earlier Heimdall pinned it: refused all the same, and its
    // certificate never adopted.
    KnownRdpHosts::new(&known)
        .record("localhost", port, &pin)
        .expect("pinned");
    let pinned = std::fs::read(&known).expect("pins");

    let mut events = ftp_events(request(port, true, &known), AnswerRegistry::default());
    let event = tokio::time::timeout(STEP, events.next())
        .await
        .expect("in time")
        .expect("an event");
    assert!(refused_as_over(&event, port, pin), "{event:?}");
    assert!(
        tokio::time::timeout(STEP, events.next())
            .await
            .expect("in time")
            .is_none(),
        "nothing more: no question"
    );
    assert_eq!(
        std::fs::read(&known).expect("pins"),
        pinned,
        "the pin stays"
    );
}

#[tokio::test]
async fn an_ftps_certificate_over_goes_through_on_the_answer_only() {
    let root = tempfile::tempdir().expect("root");
    let keys = tempfile::tempdir().expect("keys");
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_ftps_hosts");
    let (pin, whole) = expired_certificate(keys.path());
    let port = serve(root.path(), Some(keys.path())).await;

    // Trusted once: the attempt built on the answer goes through, the next ones refuse it.
    let mut once = request(port, true, &known);
    once.trusted_once = Some(whole);
    once.trusted_for_run = vec![whole];
    let event = first(once).await;
    assert!(
        matches!(event, ConnectionEvent::FilesReady { .. }),
        "{event:?}"
    );
    let mut later = request(port, true, &known);
    later.trusted_for_run = vec![whole];
    let event = first(later).await;
    assert!(refused_as_over(&event, port, pin), "{event:?}");
    assert!(!known.exists(), "never written");

    // Trusted for good: recorded on the answer, refused at the next connection.
    let mut accepted = request(port, true, &known);
    accepted.accepted = Some(whole);
    let event = first(accepted).await;
    assert!(
        matches!(event, ConnectionEvent::FilesReady { .. }),
        "{event:?}"
    );
    assert_eq!(
        KnownRdpHosts::new(&known)
            .certificate_verdict("localhost", port, &pin, &whole)
            .expect("read"),
        heimdall_rdp::CertificateVerdict::Known
    );
    let event = first(request(port, true, &known)).await;
    assert!(refused_as_over(&event, port, pin), "{event:?}");
}
