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

//! The probes of the TCP ping, banner grabber and WHOIS tools, as the C# `TcpPingService`,
//! `BannerGrabService` and `WhoisLookupService`. A name is looked up once, and every address
//! it gives is dialled as every connection is (`heimdall_net`, Happy Eyeballs): a service
//! listening on IPv4 alone behind a name giving `::1` first still answers. What a server
//! sends is made safe ([`crate::text::server_report_text`]) before it leaves here.
//!
//! Every probe is a future: dropping it (the tool's run stopped) stops it at once.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use heimdall_core::tools::banner_grab::{self, BANNER_MAX_BYTES, BannerProbeResult};
use heimdall_core::tools::whois::{MAX_REPLY_BYTES, WHOIS_DEADLINE, WHOIS_PORT};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use crate::reachability::{self, DownReason};
use crate::text::server_report_text;

/// Size of one read of a WHOIS reply.
const WHOIS_CHUNK: usize = 4096;

/// What a WHOIS line ends with (RFC 3912).
const WHOIS_LINE_END: &str = "\r\n";

/// Looks `host` up within `budget`: every address it gives, with `port`.
///
/// # Errors
///
/// [`DownReason::Dns`] when the lookup fails, gives nothing or takes longer.
pub async fn resolve(
    host: &str,
    port: u16,
    budget: Duration,
) -> Result<Vec<SocketAddr>, DownReason> {
    match tokio::time::timeout(budget, tokio::net::lookup_host((host, port))).await {
        Ok(Ok(found)) => {
            let addresses: Vec<SocketAddr> = found.collect();
            if addresses.is_empty() {
                Err(DownReason::Dns)
            } else {
                Ok(addresses)
            }
        }
        Ok(Err(_)) | Err(_) => Err(DownReason::Dns),
    }
}

/// `addresses`, each with `port`.
fn with_port(addresses: &[SocketAddr], port: u16) -> Vec<SocketAddr> {
    addresses
        .iter()
        .map(|address| SocketAddr::new(address.ip(), port))
        .collect()
}

/// One TCP ping probe, as the C# `TcpPingService.ProbeAsync`: how long the handshake with
/// the first of `addresses` to answer took, within `timeout`. The name was looked up before
/// ([`resolve`]): its time is not counted.
///
/// # Errors
///
/// Why no address answered.
pub async fn connect_time(
    addresses: &[SocketAddr],
    timeout: Duration,
) -> Result<Duration, DownReason> {
    let started = Instant::now();
    match heimdall_net::connect_addresses(addresses, Some(timeout)).await {
        Ok(_connected) => Ok(started.elapsed()),
        Err(failure) => Err(reachability::down_reason(&failure)),
    }
}

/// What `port` says first on the first of `addresses` to answer, as the C#
/// `BannerGrabService.ProbePortDirectAsync`: connected within `connect_timeout`, then one read
/// of at most [`BANNER_MAX_BYTES`] within `read_timeout`, made safe and cleaned.
pub async fn grab_banner(
    addresses: &[SocketAddr],
    port: u16,
    connect_timeout: Duration,
    read_timeout: Duration,
) -> BannerProbeResult {
    let addresses = with_port(addresses, port);
    let started = Instant::now();
    let Ok(connected) = heimdall_net::connect_addresses(&addresses, Some(connect_timeout)).await
    else {
        return BannerProbeResult::closed(port);
    };
    let millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut stream = connected.stream;
    let mut buffer = [0_u8; BANNER_MAX_BYTES];
    let banner = match tokio::time::timeout(read_timeout, stream.read(&mut buffer)).await {
        Ok(Ok(count)) if count > 0 => banner_grab::clean_banner(&server_report_text(
            &String::from_utf8_lossy(&buffer[..count]),
        )),
        _ => None,
    };
    BannerProbeResult::open(port, millis, banner)
}

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
pub async fn whois(server: &str, query: &str) -> Result<WhoisReply, WhoisFailure> {
    whois_at(server, WHOIS_PORT, query, WHOIS_DEADLINE, MAX_REPLY_BYTES).await
}

/// [`whois`], on `port`, within `deadline`, keeping at most `limit` bytes.
///
/// # Errors
///
/// [`WhoisFailure`].
pub async fn whois_at(
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
    match tokio::time::timeout(deadline, exchange(server, port, query, limit)).await {
        Err(_) => Err(WhoisFailure::TimedOut),
        Ok(Err(reason)) => Err(WhoisFailure::Failed(reason)),
        Ok(Ok((reply, truncated))) => Ok(WhoisReply {
            text: server_report_text(&String::from_utf8_lossy(&reply)),
            truncated,
            elapsed: started.elapsed(),
        }),
    }
}

/// The exchange of [`whois_at`], without its deadline: the reply's bytes, at most `limit`,
/// and whether there were more.
async fn exchange(
    server: &str,
    port: u16,
    query: &str,
    limit: usize,
) -> Result<(Vec<u8>, bool), DownReason> {
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((server, port))
        .await
        .map_err(|_| DownReason::Dns)?
        .collect();
    let connected = heimdall_net::connect_addresses(&addresses, None)
        .await
        .map_err(|failure| reachability::down_reason(&failure))?;
    let mut stream = connected.stream;
    let line = format!("{query}{WHOIS_LINE_END}");
    stream
        .write_all(line.as_bytes())
        .await
        .map_err(|error| reachability::reason_of_error(&error))?;
    let mut reply = Vec::new();
    let mut chunk = [0_u8; WHOIS_CHUNK];
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

    /// A loopback listener and its address.
    async fn listener() -> (TcpListener, SocketAddr) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        (listener, address)
    }

    #[tokio::test]
    async fn a_listening_port_answers_and_a_closed_one_does_not() {
        let (listener, address) = listener().await;
        let elapsed = connect_time(&[address], Duration::from_secs(5)).await;
        assert!(elapsed.is_ok(), "{elapsed:?}");
        drop(listener);
        let refused = connect_time(&[address], Duration::from_secs(5)).await;
        assert!(refused.is_err(), "{refused:?}");
    }

    #[tokio::test]
    async fn a_banner_is_read_made_safe_and_names_the_service() {
        let (listener, address) = listener().await;
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            socket
                .write_all(b"SSH-2.0-OpenSSH_9.6\x1b[31m\xe2\x80\xae evil\r\n")
                .await
                .expect("write");
        });
        let result = grab_banner(
            &[SocketAddr::new(address.ip(), 0)],
            address.port(),
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(result.banner.as_deref(), Some("SSH-2.0-OpenSSH_9.6 evil"));
        assert_eq!(result.service, "OpenSSH");
        assert!(result.response_ms.is_some());
    }

    #[tokio::test]
    async fn a_closed_port_has_no_time_and_no_banner() {
        let (listener, address) = listener().await;
        drop(listener);
        let result = grab_banner(
            &[address],
            address.port(),
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(result, BannerProbeResult::closed(address.port()));
    }

    /// A WHOIS server on loopback answering every query with `reply`, written in pieces of
    /// `piece` bytes `pause` apart.
    async fn whois_server(reply: Vec<u8>, piece: usize, pause: Duration) -> SocketAddr {
        let (listener, address) = listener().await;
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
        address
    }

    #[tokio::test]
    async fn a_whois_reply_is_read_to_its_end_and_made_safe() {
        let address = whois_server(
            // A title-setting escape sequence and a zero-width space (E2 80 8B).
            b"Domain Name: EXAMPLE.COM\r\nRegistrar: \x1b]0;title\x07Test\xe2\x80\x8b\r\n".to_vec(),
            usize::MAX,
            Duration::ZERO,
        )
        .await;
        let reply = whois_at(
            "127.0.0.1",
            address.port(),
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
    async fn an_oversized_whois_reply_is_cut_at_the_limit() {
        let limit = 1024;
        let address = whois_server(vec![b'x'; limit * 8], WHOIS_CHUNK, Duration::ZERO).await;
        let reply = whois_at(
            "127.0.0.1",
            address.port(),
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
    async fn a_trickling_whois_reply_cannot_outlast_the_deadline() {
        // One byte every 50 ms never trips a per-read timeout; the one deadline stops it.
        let address = whois_server(vec![b'x'; 1000], 1, Duration::from_millis(50)).await;
        let deadline = Duration::from_millis(400);
        let started = Instant::now();
        let reply = whois_at(
            "127.0.0.1",
            address.port(),
            "example.com",
            deadline,
            MAX_REPLY_BYTES,
        )
        .await;
        assert_eq!(reply, Err(WhoisFailure::TimedOut));
        assert!(started.elapsed() < deadline * 5, "{:?}", started.elapsed());
    }
}
