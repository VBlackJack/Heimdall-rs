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

//! One probe of the TCPPING tool, as the C# `TcpPingService.ProbeAsync`: a TCP connection
//! to every address the host resolved to, before the run and once ([`super::resolve`]), the
//! first that answers winning (`heimdall_net`). Only the handshake is timed.

use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use heimdall_ssh::socks::Opener;
use heimdall_ssh::tool_route::ToolGateway;

use crate::reachability::{self, DownReason};

/// How long the handshake with the first of `addresses` to answer on `port` took, within
/// `timeout`.
///
/// # Errors
///
/// Why no address answered.
pub async fn connect_time(
    addresses: &[IpAddr],
    port: u16,
    timeout: Duration,
) -> Result<Duration, DownReason> {
    let targets: Vec<SocketAddr> = addresses
        .iter()
        .map(|address| SocketAddr::new(*address, port))
        .collect();
    let started = Instant::now();
    match heimdall_net::connect_addresses(&targets, Some(timeout)).await {
        Ok(_connected) => Ok(started.elapsed()),
        Err(failure) => Err(reachability::down_reason(&failure)),
    }
}

/// How long the handshake of the gateway with `host:port` took, within `timeout`, `host`
/// sent as typed for the gateway to resolve. Nothing is connected from here.
///
/// # Errors
///
/// Why the gateway did not connect: a channel it refused is a refusal.
pub async fn connect_time_via<O: Opener>(
    gateway: &ToolGateway<O>,
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<Duration, DownReason> {
    let started = Instant::now();
    match gateway.dial_within(host, port, Some(timeout)).await {
        Ok(_connected) => Ok(started.elapsed()),
        Err(error) => Err(reachability::reason_of_error(&error)),
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use tokio::net::{TcpListener, TcpSocket};

    use super::*;

    #[tokio::test]
    async fn a_listening_port_answers_and_a_closed_one_does_not() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind");
        let port = listener.local_addr().expect("address").port();
        let loopback = [IpAddr::V4(Ipv4Addr::LOCALHOST)];
        let elapsed = connect_time(&loopback, port, Duration::from_secs(5)).await;
        assert!(elapsed.is_ok(), "{elapsed:?}");
        // The closed port is another one, held by a socket bound but not
        // listening: no parallel test can listen on it, and the first port, left in
        // TIME_WAIT by the connection above, is never bound again (Linux refuses that).
        let socket = TcpSocket::new_v4().expect("socket");
        socket
            .bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .expect("bind a closed port");
        let port = socket.local_addr().expect("closed address").port();
        // Windows tries a refused connection again for about two seconds.
        let refused = connect_time(&loopback, port, Duration::from_secs(5)).await;
        assert!(refused.is_err(), "{refused:?}");
        drop(socket);
    }
}
