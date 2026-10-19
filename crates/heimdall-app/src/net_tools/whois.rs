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

//! The WHOIS tool's lookup, as the C# `WhoisLookupService.DirectQueryAsync`: the server
//! resolved ([`super::resolve`]) and dialled on every address it gives (`heimdall_net`), the
//! query sent, the reply read to its end, all under one deadline and within a size, and made
//! safe ([`crate::text::server_report_text`]).

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use heimdall_core::tools::whois::{MAX_REPLY_BYTES, WHOIS_DEADLINE, WHOIS_PORT};
use heimdall_ssh::socks::Opener;
use heimdall_ssh::tool_route::ToolGateway;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

use crate::reachability::{self, DownReason};
use crate::text::server_report_text;

/// Size of one read of a reply.
const CHUNK: usize = 4096;

/// What a WHOIS line ends with (RFC 3912).
const LINE_END: &str = "\r\n";

/// A WHOIS server's reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhoisReply {
    /// What the server wrote, made safe to show and copy.
    pub text: String,
    /// Whether the reply went past the size kept, and was cut there.
    pub truncated: bool,
    /// How long the lookup took.
    pub elapsed: Duration,
}

/// Why a WHOIS lookup gave nothing, as the C# `WhoisLookupService` tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhoisFailure {
    /// The deadline went by, the C# `ToolWhoisErrorTimeout`.
    TimedOut,
    /// The server could not be reached or the exchange failed, the C# `ToolWhoisErrorFailed`.
    Failed(DownReason),
}

/// Asks `server` about `query` on the WHOIS port, within [`WHOIS_DEADLINE`] for the whole
/// lookup, keeping at most [`MAX_REPLY_BYTES`] of the reply. `query` is one checked by
/// `heimdall_core::tools::whois::whois_query`: it cannot hold a line end.
///
/// # Errors
///
/// [`WhoisFailure`].
pub async fn lookup(server: &str, query: &str) -> Result<WhoisReply, WhoisFailure> {
    lookup_at(server, WHOIS_PORT, query, WHOIS_DEADLINE, MAX_REPLY_BYTES).await
}

/// [`lookup`], on `port`, within `deadline`, keeping at most `limit` bytes.
///
/// # Errors
///
/// [`WhoisFailure`].
pub async fn lookup_at(
    server: &str,
    port: u16,
    query: &str,
    deadline: Duration,
    limit: usize,
) -> Result<WhoisReply, WhoisFailure> {
    let started = Instant::now();
    // One deadline over the lookup, the connection, the query and the whole reply, as the
    // C# links its ten seconds to the lookup's token: a server trickling its reply a byte at
    // a time cannot hold the tool longer.
    answered(
        started,
        tokio::time::timeout(deadline, exchange(server, port, query, deadline, limit)).await,
    )
}

/// [`lookup`] as the gateway reaches the server: the channel it opens onward to `server`,
/// sent as typed for the gateway to resolve, carries the query. Nothing is connected from
/// here.
///
/// # Errors
///
/// [`WhoisFailure`].
pub async fn lookup_via<O: Opener>(
    gateway: &ToolGateway<O>,
    server: &str,
    query: &str,
) -> Result<WhoisReply, WhoisFailure> {
    lookup_via_at(
        gateway,
        server,
        WHOIS_PORT,
        query,
        WHOIS_DEADLINE,
        MAX_REPLY_BYTES,
    )
    .await
}

/// [`lookup_via`], on `port`, within `deadline`, keeping at most `limit` bytes.
///
/// # Errors
///
/// [`WhoisFailure`].
pub async fn lookup_via_at<O: Opener>(
    gateway: &ToolGateway<O>,
    server: &str,
    port: u16,
    query: &str,
    deadline: Duration,
    limit: usize,
) -> Result<WhoisReply, WhoisFailure> {
    let started = Instant::now();
    let work = async {
        let stream = gateway
            .dial_within(server, port, None)
            .await
            .map_err(|error| reachability::reason_of_error(&error))?;
        converse(stream, query, limit).await
    };
    answered(started, tokio::time::timeout(deadline, work).await)
}

/// The lookup's answer from what its deadline-bound exchange gave.
fn answered(
    started: Instant,
    exchanged: Result<Result<(Vec<u8>, bool), DownReason>, tokio::time::error::Elapsed>,
) -> Result<WhoisReply, WhoisFailure> {
    match exchanged {
        Err(_) => Err(WhoisFailure::TimedOut),
        Ok(Err(reason)) => Err(WhoisFailure::Failed(reason)),
        Ok(Ok((reply, truncated))) => Ok(WhoisReply {
            text: server_report_text(&String::from_utf8_lossy(&reply)),
            truncated,
            elapsed: started.elapsed(),
        }),
    }
}

/// The exchange of [`lookup_at`], without its deadline: the reply's bytes, at most `limit`,
/// and whether there were more.
async fn exchange(
    server: &str,
    port: u16,
    query: &str,
    deadline: Duration,
    limit: usize,
) -> Result<(Vec<u8>, bool), DownReason> {
    let targets: Vec<SocketAddr> = super::resolve(server, deadline)
        .await
        .map_err(|_| DownReason::Dns)?
        .into_iter()
        .map(|address| SocketAddr::new(address, port))
        .collect();
    let connected = heimdall_net::connect_addresses(&targets, None)
        .await
        .map_err(|failure| reachability::down_reason(&failure))?;
    converse(connected.stream, query, limit).await
}

/// Sends `query` on `stream` and reads the reply to its end, at most `limit` bytes of it,
/// and whether there were more.
async fn converse<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    query: &str,
    limit: usize,
) -> Result<(Vec<u8>, bool), DownReason> {
    let line = format!("{query}{LINE_END}");
    stream
        .write_all(line.as_bytes())
        .await
        .map_err(|error| reachability::reason_of_error(&error))?;
    let mut reply = Vec::new();
    let mut chunk = [0_u8; CHUNK];
    loop {
        let count = stream
            .read(&mut chunk)
            .await
            .map_err(|error| reachability::reason_of_error(&error))?;
        if count == 0 {
            return Ok((reply, false));
        }
        let room = limit - reply.len();
        if count > room {
            reply.extend_from_slice(&chunk[..room]);
            return Ok((reply, true));
        }
        reply.extend_from_slice(&chunk[..count]);
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use tokio::net::TcpListener;

    use super::*;

    /// A WHOIS server on loopback answering a query with `reply`, written in pieces of
    /// `piece` bytes `pause` apart; its port.
    async fn server(reply: Vec<u8>, piece: usize, pause: Duration) -> u16 {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind");
        let port = listener.local_addr().expect("address").port();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut query = [0_u8; 256];
            let _ = socket.read(&mut query).await;
            for part in reply.chunks(piece) {
                if socket.write_all(part).await.is_err() {
                    return;
                }
                tokio::time::sleep(pause).await;
            }
        });
        port
    }

    #[tokio::test]
    async fn a_reply_is_read_to_its_end_and_made_safe() {
        let port = server(
            // A title-setting escape sequence and a zero-width space (E2 80 8B).
            b"Domain Name: EXAMPLE.COM\r\nRegistrar: \x1b]0;title\x07Test\xe2\x80\x8b\r\n".to_vec(),
            usize::MAX,
            Duration::ZERO,
        )
        .await;
        let reply = lookup_at(
            "127.0.0.1",
            port,
            "example.com",
            Duration::from_secs(5),
            MAX_REPLY_BYTES,
        )
        .await
        .expect("reply");
        assert_eq!(reply.text, "Domain Name: EXAMPLE.COM\nRegistrar: Test\n");
        assert!(!reply.truncated);
    }

    #[tokio::test]
    async fn an_oversized_reply_is_cut_at_the_limit() {
        let limit = 1024;
        let port = server(vec![b'x'; limit * 8], CHUNK, Duration::ZERO).await;
        let reply = lookup_at(
            "127.0.0.1",
            port,
            "example.com",
            Duration::from_secs(5),
            limit,
        )
        .await
        .expect("reply");
        assert_eq!(reply.text.len(), limit);
        assert!(reply.truncated);
    }

    #[tokio::test]
    async fn a_trickling_reply_cannot_outlast_the_deadline() {
        // One byte every 50 ms never trips a per-read timeout; the one deadline stops it.
        let port = server(vec![b'x'; 1000], 1, Duration::from_millis(50)).await;
        let deadline = Duration::from_millis(400);
        let started = Instant::now();
        let reply = lookup_at("127.0.0.1", port, "example.com", deadline, MAX_REPLY_BYTES).await;
        assert_eq!(reply, Err(WhoisFailure::TimedOut));
        assert!(started.elapsed() < deadline * 5, "{:?}", started.elapsed());
    }
}
