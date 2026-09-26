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

//! The client against a scripted server that misbehaves on purpose. The reference server
//! (OpenSSH) is exercised in `openssh_client.rs`.

use std::time::Duration;

use heimdall_sftp::protocol::{Attributes, NameEntry, Request, Response, SFTP_VERSION, StatusCode};
use heimdall_sftp::{ClientConfig, Closed, RemotePath, SftpClient, SftpError};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, DuplexStream};

/// Bytes buffered by the in-memory pipe between client and fake server.
const PIPE: usize = 1 << 20;

/// Bound on anything these tests wait for.
const STEP: Duration = Duration::from_secs(10);

/// A request timeout short enough for the timeout test.
const SHORT_TIMEOUT: Duration = Duration::from_millis(200);

async fn read_request(server: &mut DuplexStream) -> Option<Request> {
    let mut length = [0; 4];
    server.read_exact(&mut length).await.ok()?;
    let mut body = vec![0; u32::from_be_bytes(length) as usize];
    server.read_exact(&mut body).await.ok()?;
    Some(Request::decode(&body).expect("the client sends well-formed requests"))
}

async fn send(server: &mut DuplexStream, response: &Response) {
    server.write_all(&response.encode()).await.expect("sent");
}

/// Answers the version exchange with `version`.
async fn handshake(server: &mut DuplexStream, version: u32) {
    let init = read_request(server).await.expect("init");
    assert!(matches!(init, Request::Init { version: 3, .. }), "{init:?}");
    send(
        server,
        &Response::Version {
            version,
            extensions: Vec::new(),
        },
    )
    .await;
}

fn request_id(request: &Request) -> u32 {
    match request {
        Request::Stat { id, .. }
        | Request::Opendir { id, .. }
        | Request::Readdir { id, .. }
        | Request::Close { id, .. }
        | Request::Read { id, .. }
        | Request::Open { id, .. } => *id,
        other => panic!("unexpected request {other:?}"),
    }
}

async fn client_with(config: ClientConfig) -> (SftpClient, DuplexStream) {
    let (client_end, mut server) = tokio::io::duplex(PIPE);
    let serving = tokio::spawn(async move {
        handshake(&mut server, SFTP_VERSION).await;
        server
    });
    let client = SftpClient::start(client_end, config)
        .await
        .expect("started");
    (client, serving.await.expect("served"))
}

async fn client() -> (SftpClient, DuplexStream) {
    client_with(ClientConfig::default()).await
}

fn protocol_violation(error: &SftpError) -> bool {
    matches!(error, SftpError::Closed(Closed::Protocol(_)))
}

async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(STEP, future).await.expect("in time")
}

#[tokio::test]
async fn responses_out_of_order_reach_their_requests() {
    let (client, mut server) = client().await;
    let (a, b) = (client.clone(), client.clone());
    let first = tokio::spawn(async move { a.stat(&RemotePath::from("/a")).await });
    let second = tokio::spawn(async move { b.stat(&RemotePath::from("/b")).await });
    let one = within(read_request(&mut server)).await.expect("request");
    let two = within(read_request(&mut server)).await.expect("request");
    // Answer the later one first, with sizes telling the paths apart.
    for request in [&two, &one] {
        let Request::Stat { id, path } = request else {
            panic!("a stat")
        };
        let size = if path.as_bytes() == b"/a" { 100 } else { 200 };
        send(
            &mut server,
            &Response::Attrs {
                id: *id,
                attributes: Attributes {
                    size: Some(size),
                    ..Attributes::default()
                },
            },
        )
        .await;
    }
    let first = within(first).await.expect("joined").expect("answered");
    let second = within(second).await.expect("joined").expect("answered");
    assert_eq!((first.size, second.size), (Some(100), Some(200)));
}

#[tokio::test]
async fn a_response_to_an_unknown_request_ends_the_session() {
    let (client, mut server) = client().await;
    let asking = client.clone();
    let pending = tokio::spawn(async move { asking.stat(&RemotePath::from("/a")).await });
    let request = within(read_request(&mut server)).await.expect("request");
    send(
        &mut server,
        &Response::Status {
            id: request_id(&request).wrapping_add(1000),
            code: StatusCode::Ok,
            message: Vec::new(),
        },
    )
    .await;
    let error = within(pending).await.expect("joined").expect_err("closed");
    assert!(protocol_violation(&error), "{error:?}");
    assert!(matches!(client.closed(), Some(Closed::Protocol(_))));
    let after = client
        .stat(&RemotePath::from("/b"))
        .await
        .expect_err("closed");
    assert!(
        protocol_violation(&after),
        "no request goes out after: {after:?}"
    );
}

#[tokio::test]
async fn a_response_of_the_wrong_type_ends_the_session() {
    let (client, mut server) = client().await;
    let asking = client.clone();
    let pending = tokio::spawn(async move { asking.stat(&RemotePath::from("/a")).await });
    let request = within(read_request(&mut server)).await.expect("request");
    send(
        &mut server,
        &Response::Data {
            id: request_id(&request),
            data: vec![1],
        },
    )
    .await;
    let error = within(pending).await.expect("joined").expect_err("closed");
    assert!(protocol_violation(&error), "{error:?}");
    assert!(
        matches!(client.closed(), Some(Closed::Protocol(_))),
        "the whole session ends, not only the request"
    );
}

#[tokio::test]
async fn an_unanswered_request_times_out_and_ends_the_session() {
    let (client, mut server) = client_with(ClientConfig {
        request_timeout: SHORT_TIMEOUT,
        ..ClientConfig::default()
    })
    .await;
    let asking = client.clone();
    let pending = tokio::spawn(async move { asking.stat(&RemotePath::from("/a")).await });
    let _ignored = within(read_request(&mut server)).await.expect("request");
    let error = within(pending)
        .await
        .expect("joined")
        .expect_err("timed out");
    assert_eq!(error, SftpError::Closed(Closed::Timeout));
}

#[tokio::test]
async fn an_oversized_packet_ends_the_session_without_reading_it() {
    let (client, mut server) = client_with(ClientConfig {
        max_packet: 1024,
        ..ClientConfig::default()
    })
    .await;
    let asking = client.clone();
    let pending = tokio::spawn(async move { asking.stat(&RemotePath::from("/a")).await });
    let _request = within(read_request(&mut server)).await.expect("request");
    server
        .write_all(&0x7FFF_FFFFu32.to_be_bytes())
        .await
        .expect("sent");
    let error = within(pending).await.expect("joined").expect_err("closed");
    assert!(protocol_violation(&error), "{error:?}");
}

#[tokio::test]
async fn a_handle_longer_than_the_protocol_allows_ends_the_session() {
    let (client, mut server) = client().await;
    let asking = client.clone();
    let pending = tokio::spawn(async move {
        asking
            .open(&RemotePath::from("/a"), 1, Attributes::default())
            .await
    });
    let request = within(read_request(&mut server)).await.expect("request");
    send(
        &mut server,
        &Response::Handle {
            id: request_id(&request),
            handle: vec![0; 257],
        },
    )
    .await;
    let error = within(pending).await.expect("joined").expect_err("closed");
    assert!(protocol_violation(&error), "{error:?}");
}

#[tokio::test]
async fn more_data_than_asked_ends_the_session() {
    let (client, mut server) = client().await;
    let asking = client.clone();
    let pending = tokio::spawn(async move {
        let handle = asking
            .open(&RemotePath::from("/a"), 1, Attributes::default())
            .await?;
        asking.read(&handle, 0, 4).await
    });
    let open = within(read_request(&mut server)).await.expect("open");
    send(
        &mut server,
        &Response::Handle {
            id: request_id(&open),
            handle: b"h".to_vec(),
        },
    )
    .await;
    let read = within(read_request(&mut server)).await.expect("read");
    send(
        &mut server,
        &Response::Data {
            id: request_id(&read),
            data: vec![0; 5],
        },
    )
    .await;
    let error = within(pending).await.expect("joined").expect_err("closed");
    assert!(protocol_violation(&error), "{error:?}");
}

#[tokio::test]
async fn another_version_is_refused() {
    let (client_end, mut server) = tokio::io::duplex(PIPE);
    tokio::spawn(async move {
        handshake(&mut server, 4).await;
        server
    });
    let error = within(SftpClient::start(client_end, ClientConfig::default()))
        .await
        .expect_err("refused");
    assert_eq!(error, SftpError::UnsupportedVersion(4));
}

#[tokio::test]
async fn an_endless_listing_stops_at_the_limit_and_its_handle_is_closed() {
    let (client, mut server) = client_with(ClientConfig {
        max_listing_entries: 10,
        ..ClientConfig::default()
    })
    .await;
    let asking = client.clone();
    let listing = tokio::spawn(async move { asking.read_dir(&RemotePath::from("/")).await });
    let mut closed = false;
    // Bounded as a whole: without the client's limit the listing never ends.
    let deadline = tokio::time::Instant::now() + STEP;
    while let Some(request) = within(read_request(&mut server)).await {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the listing did not stop at its limit"
        );
        let id = request_id(&request);
        match request {
            Request::Opendir { .. } => {
                send(
                    &mut server,
                    &Response::Handle {
                        id,
                        handle: b"d".to_vec(),
                    },
                )
                .await;
            }
            Request::Readdir { .. } => {
                let entries = (0..4)
                    .map(|n| NameEntry {
                        filename: format!("f{id}-{n}").into_bytes(),
                        longname: Vec::new(),
                        attributes: Attributes::default(),
                    })
                    .collect();
                send(&mut server, &Response::Name { id, entries }).await;
            }
            Request::Close { .. } => {
                closed = true;
                send(
                    &mut server,
                    &Response::Status {
                        id,
                        code: StatusCode::Ok,
                        message: Vec::new(),
                    },
                )
                .await;
                break;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    let error = within(listing)
        .await
        .expect("joined")
        .expect_err("too large");
    assert_eq!(error, SftpError::ListingTooLarge(10));
    assert!(closed, "the listing handle was closed on the server");
}

#[tokio::test]
async fn dropping_the_client_ends_the_stream() {
    let (client, mut server) = client().await;
    drop(client);
    let mut byte = [0; 1];
    let read = within(server.read(&mut byte)).await.expect("readable");
    assert_eq!(read, 0, "the client side closed");
}
