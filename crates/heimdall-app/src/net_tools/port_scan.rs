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

//! One port of the PORTSCAN tool, as the C# `PortScanService.ProbePortDirectAsync`: a TCP
//! connection to every address the host resolved to, before the scan and once, the first
//! that answers winning (`heimdall_net`), within the timeout. Only the connection is timed.

use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use heimdall_core::tools::port_scanner::{
    self, PortProbeResult, PortStatus, service_name_for_port,
};

/// Port `port` of the host at `addresses`, within `timeout_ms`.
pub async fn probe(addresses: &[IpAddr], port: u16, timeout_ms: u64) -> PortProbeResult {
    let targets: Vec<SocketAddr> = addresses
        .iter()
        .map(|address| SocketAddr::new(*address, port))
        .collect();
    let started = Instant::now();
    let (status, latency_ms) =
        match heimdall_net::connect_addresses(&targets, Some(Duration::from_millis(timeout_ms)))
            .await
        {
            Ok(_connected) => (PortStatus::Open, started.elapsed().as_secs_f64() * 1000.0),
            Err(failure) => (
                port_scanner::status_of_failure(
                    failure.telling_failure().map(|(_, error)| error.kind()),
                ),
                -1.0,
            ),
        };
    PortProbeResult {
        port,
        service: service_name_for_port(port),
        status,
        latency_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn a_listening_port_is_open_and_a_refusing_one_closed() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let open = listener.local_addr().expect("address").port();
        // Bound but not listening: the port stays this test's, and refuses.
        let socket = tokio::net::TcpSocket::new_v4().expect("socket");
        socket
            .bind("127.0.0.1:0".parse().expect("address"))
            .expect("bind");
        let closed = socket.local_addr().expect("address").port();
        let loopback = ["127.0.0.1".parse().expect("address")];

        let result = probe(&loopback, open, 2000).await;
        assert_eq!(result.status, PortStatus::Open);
        assert!(result.latency_ms >= 0.0);
        // Windows tries a refused connection again for about two seconds.
        let result = probe(&loopback, closed, 5000).await;
        assert_eq!(result.status, PortStatus::Closed);
        assert!(result.latency_ms < 0.0);
    }

    #[tokio::test]
    async fn every_address_is_tried() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        // An address that never answers first (TEST-NET-1), then the loopback.
        let addresses = [
            "192.0.2.1".parse().expect("address"),
            "127.0.0.1".parse().expect("address"),
        ];
        assert_eq!(probe(&addresses, port, 3000).await.status, PortStatus::Open);
    }
}
