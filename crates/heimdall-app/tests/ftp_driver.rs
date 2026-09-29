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
//! user trusts it.

use std::net::{Ipv4Addr, TcpListener};
use std::path::Path;
use std::time::Duration;

use heimdall_app::ftp_driver::{FtpRequest, ftp_events};
use heimdall_app::{AnswerRegistry, ConnectionEvent};
use heimdall_core::profile::{FtpProfile, ProfileId};
use heimdall_rdp::{KnownRdpHosts, Verdict};
use tokio_stream::StreamExt as _;
use tokio_util::sync::CancellationToken;
use unftp_sbe_fs::Filesystem;

const STEP: Duration = Duration::from_secs(20);

/// Serves `root` over FTP, over explicit FTPS with a fresh certificate when `keys` is given.
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
        let issued =
            rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("cert");
        let (cert, key) = (keys.join("cert.pem"), keys.join("key.pem"));
        std::fs::write(&cert, issued.cert.pem()).expect("cert file");
        std::fs::write(&key, issued.signing_key.serialize_pem()).expect("key file");
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
    let ConnectionEvent::FilesReady { client } = event else {
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
    } = event
    else {
        panic!("the certificate question, got {event:?}");
    };
    assert_eq!((host.as_str(), asked_port), ("localhost", port));

    // Trusted: the next attempt goes through and records it.
    let mut accepted = request(port, true, &known);
    accepted.accepted = Some(fingerprint);
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
    let ConnectionEvent::UnknownRdpCertificate { fingerprint, .. } =
        first(request(port, true, &known)).await
    else {
        panic!("the certificate question");
    };
    let mut once = request(port, true, &known);
    once.trusted_for_run = vec![fingerprint];
    let event = first(once).await;
    assert!(
        matches!(event, ConnectionEvent::FilesReady { .. }),
        "{event:?}"
    );
    assert!(!known.exists(), "never written");
}
