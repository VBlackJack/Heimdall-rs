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

//! The network tools through a gateway, tested over openers that stand in for one: a
//! gateway that connects onward to a listener of this computer, and one that refuses every
//! channel. The refusing one is the test of failing closed: a listener where the tool is
//! pointed is never reached, which a connection from here would have done.

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use heimdall_core::tools::firewall_tester::FwProbeStatus;
use heimdall_core::tools::http_headers::parse_target;
use heimdall_ssh::ConnectError;
use heimdall_ssh::socks::Opener;
use heimdall_ssh::tool_route::ToolGateway;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

use super::http::{HttpFetchError, fetch_via};
use super::{banner, connect_routed, firewall, tcp_ping, whois};
use crate::reachability::DownReason;

/// A gateway that connects onward to one address of this computer, whatever it is asked.
pub struct Loopback(pub SocketAddr);

impl Opener for Loopback {
    type Stream = TcpStream;

    async fn open(&self, _host: String, _port: u16) -> Result<TcpStream, ConnectError> {
        TcpStream::connect(self.0)
            .await
            .map_err(ConnectError::Network)
    }
}

/// A gateway that opens no channel.
pub struct Refusing;

impl Opener for Refusing {
    type Stream = TcpStream;

    async fn open(&self, host: String, port: u16) -> Result<TcpStream, ConnectError> {
        tokio::task::yield_now().await;
        Err(ConnectError::JumpRefused { host, port })
    }
}

/// A listener where a direct connection would land: it answers whoever comes.
async fn bait() -> (u16, tokio::task::JoinHandle<bool>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    let watch = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_millis(500), listener.accept())
            .await
            .is_ok()
    });
    (port, watch)
}

#[tokio::test]
async fn a_banner_is_read_over_the_channel() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        socket
            .write_all(b"SSH-2.0-OpenSSH_9.6\r\n")
            .await
            .expect("write");
    });
    let gateway = ToolGateway::over(Loopback(address), 4);
    let result = banner::grab_via(
        &gateway,
        "internal.example",
        22,
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(result.banner.as_deref(), Some("SSH-2.0-OpenSSH_9.6"));
    assert!(result.response_ms.is_some());
}

#[tokio::test]
async fn a_refused_channel_gives_no_banner_and_no_direct_connection() {
    let (port, watch) = bait().await;
    let gateway = ToolGateway::over(Refusing, 4);
    let result = banner::grab_via(
        &gateway,
        "127.0.0.1",
        port,
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .await;
    assert!(result.response_ms.is_none());
    assert!(!watch.await.expect("watch"), "nothing connected from here");
}

#[tokio::test]
async fn a_tcp_ping_through_the_gateway_is_the_channel_opening() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let open = ToolGateway::over(Loopback(address), 4);
    let timed =
        tcp_ping::connect_time_via(&open, "db.internal", 5432, Duration::from_secs(5)).await;
    assert!(timed.is_ok(), "{timed:?}");
    let (port, watch) = bait().await;
    let refusing = ToolGateway::over(Refusing, 4);
    let refused =
        tcp_ping::connect_time_via(&refusing, "127.0.0.1", port, Duration::from_secs(5)).await;
    assert_eq!(refused, Err(DownReason::Refused));
    assert!(!watch.await.expect("watch"), "nothing connected from here");
}

#[tokio::test]
async fn headers_are_read_over_the_channel_and_a_refusal_is_a_connection_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut request = vec![0_u8; 1024];
            let _ = stream.read(&mut request).await;
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nServer: internal\r\n\r\n")
                .await;
            let _ = stream.shutdown().await;
        }
    });
    let target = parse_target("http://intranet.example/").expect("target");
    let open = ToolGateway::over(Loopback(address), 4);
    let fetched = fetch_via(&target, &open).await.expect("fetched");
    assert_eq!(fetched.info.status_code, 200);

    let (port, watch) = bait().await;
    let target = parse_target(&format!("http://127.0.0.1:{port}/")).expect("target");
    let refusing = ToolGateway::over(Refusing, 4);
    let refused = fetch_via(&target, &refusing).await;
    assert!(
        matches!(refused, Err(HttpFetchError::Connection(_))),
        "{refused:?}"
    );
    assert!(!watch.await.expect("watch"), "nothing connected from here");
}

#[tokio::test]
async fn a_channel_error_is_not_an_io_error_of_this_computer() {
    let gateway = ToolGateway::over(Refusing, 1);
    let error = gateway.dial("host", 80).await.expect_err("refused");
    assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
}

#[tokio::test]
async fn a_whois_reply_is_read_over_the_channel_and_a_refusal_is_a_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut query = [0_u8; 256];
        let _ = socket.read(&mut query).await;
        socket
            .write_all(b"Domain Name: EXAMPLE.COM\r\n")
            .await
            .expect("write");
    });
    let open = ToolGateway::over(Loopback(address), 4);
    let reply = whois::lookup_via_at(
        &open,
        "whois.internal",
        43,
        "example.com",
        Duration::from_secs(5),
        1024,
    )
    .await
    .expect("reply");
    assert_eq!(reply.text, "Domain Name: EXAMPLE.COM\n");

    let (port, watch) = bait().await;
    let refusing = ToolGateway::over(Refusing, 4);
    let refused = whois::lookup_via_at(
        &refusing,
        "127.0.0.1",
        port,
        "example.com",
        Duration::from_secs(5),
        1024,
    )
    .await;
    assert_eq!(
        refused,
        Err(whois::WhoisFailure::Failed(DownReason::Refused))
    );
    assert!(!watch.await.expect("watch"), "nothing connected from here");
}

#[tokio::test]
async fn a_firewall_probe_is_the_channel_opening_and_never_a_direct_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let open = ToolGateway::over(Loopback(address), 4);
    let probed = firewall::probe_via(&open, "web.internal", 443, 5000).await;
    assert_eq!(probed.status, FwProbeStatus::Open);
    assert!(probed.latency_ms.is_some());

    let (port, watch) = bait().await;
    let refusing = ToolGateway::over(Refusing, 4);
    let refused = firewall::probe_via(&refusing, "127.0.0.1", port, 5000).await;
    assert_eq!(refused.status, FwProbeStatus::Closed);
    assert!(!watch.await.expect("watch"), "nothing connected from here");
}

#[tokio::test]
async fn a_routed_connection_is_a_channel_and_a_refused_one_is_not_made_directly() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let open = ToolGateway::over(Loopback(address), 4);
    let timeout = Duration::from_secs(5);
    assert!(
        connect_routed(Some(&open), "smb.internal", 445, timeout)
            .await
            .is_ok()
    );

    let (port, watch) = bait().await;
    let refusing = ToolGateway::over(Refusing, 4);
    let refused = connect_routed(Some(&refusing), "127.0.0.1", port, timeout).await;
    assert_eq!(
        refused.expect_err("refused").kind(),
        io::ErrorKind::ConnectionRefused
    );
    assert!(!watch.await.expect("watch"), "nothing connected from here");

    // Without a gateway the connection is made from here.
    let (port, watch) = bait().await;
    let direct = connect_routed::<Refusing>(None, "127.0.0.1", port, timeout).await;
    assert!(direct.is_ok());
    assert!(watch.await.expect("watch"));
}
