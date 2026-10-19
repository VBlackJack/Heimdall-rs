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

//! One port of the BANNER tool, as the C# `BannerGrabService.ProbePortDirectAsync`: a TCP
//! connection to every address the host resolved to, before the scan and once
//! ([`super::resolve`]), the first that answers winning (`heimdall_net`), then one read of
//! what the server says first, made safe ([`crate::text::server_report_text`]).

use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use heimdall_core::tools::banner_grab::{self, BANNER_MAX_BYTES, BannerProbeResult};
use heimdall_ssh::socks::Opener;
use heimdall_ssh::tool_route::ToolGateway;
use tokio::io::{AsyncRead, AsyncReadExt as _};

use crate::text::server_report_text;

/// What `port` says first on the first of `addresses` to answer: connected within
/// `connect_timeout`, then one read of at most [`BANNER_MAX_BYTES`] within `read_timeout`,
/// made safe and cleaned.
pub async fn grab(
    addresses: &[IpAddr],
    port: u16,
    connect_timeout: Duration,
    read_timeout: Duration,
) -> BannerProbeResult {
    let targets: Vec<SocketAddr> = addresses
        .iter()
        .map(|address| SocketAddr::new(*address, port))
        .collect();
    let started = Instant::now();
    let Ok(connected) = heimdall_net::connect_addresses(&targets, Some(connect_timeout)).await
    else {
        return BannerProbeResult::closed(port);
    };
    let millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    read_banner(connected.stream, port, millis, read_timeout).await
}

/// What `port` of `host` says first as the gateway reaches it: connected from the gateway
/// within `connect_timeout`, `host` sent as typed for the gateway to resolve, then one read
/// as [`grab`] does. Nothing is connected from here.
pub async fn grab_via<O: Opener>(
    gateway: &ToolGateway<O>,
    host: &str,
    port: u16,
    connect_timeout: Duration,
    read_timeout: Duration,
) -> BannerProbeResult {
    let started = Instant::now();
    let Ok(stream) = gateway.dial_within(host, port, Some(connect_timeout)).await else {
        return BannerProbeResult::closed(port);
    };
    let millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    read_banner(stream, port, millis, read_timeout).await
}

/// The one read of what a connected port says first, within `read_timeout`.
async fn read_banner<S: AsyncRead + Unpin>(
    mut stream: S,
    port: u16,
    millis: u64,
    read_timeout: Duration,
) -> BannerProbeResult {
    let mut buffer = [0_u8; BANNER_MAX_BYTES];
    let banner = match tokio::time::timeout(read_timeout, stream.read(&mut buffer)).await {
        Ok(Ok(count)) if count > 0 => banner_grab::clean_banner(&server_report_text(
            &String::from_utf8_lossy(&buffer[..count]),
        )),
        _ => None,
    };
    BannerProbeResult::open(port, millis, banner)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use tokio::io::AsyncWriteExt as _;
    use tokio::net::TcpListener;

    use super::*;

    const LOOPBACK: [IpAddr; 1] = [IpAddr::V4(Ipv4Addr::LOCALHOST)];

    #[tokio::test]
    async fn a_banner_is_read_made_safe_and_names_the_service() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind");
        let port = listener.local_addr().expect("address").port();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            // A colour escape sequence and a right-to-left override (E2 80 AE).
            socket
                .write_all(b"SSH-2.0-OpenSSH_9.6\x1b[31m\xe2\x80\xae evil\r\n")
                .await
                .expect("write");
        });
        let result = grab(
            &LOOPBACK,
            port,
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
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind");
        let port = listener.local_addr().expect("address").port();
        drop(listener);
        let result = grab(
            &LOOPBACK,
            port,
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(result, BannerProbeResult::closed(port));
    }
}
