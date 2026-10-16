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

//! Wake-on-LAN, as the C# `WakeOnLan` sends it: the magic packet broadcast over UDP to the
//! local network, on the discard port.

use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use heimdall_core::metadata::MacAddress;
use tokio::net::UdpSocket;

/// The port the magic packet is sent to, as the C# `WolPort`.
pub const WAKE_ON_LAN_PORT: u16 = 9;

/// Time a send has, as the C# `WakeOnLanService.SendTimeoutMs`.
pub const SEND_TIMEOUT: Duration = Duration::from_secs(2);

/// Broadcasts the magic packet waking the card at `mac` to the local network.
///
/// # Errors
///
/// What the system said when the packet could not be sent.
pub async fn send(mac: MacAddress) -> io::Result<()> {
    send_to(
        mac,
        SocketAddr::from((Ipv4Addr::BROADCAST, WAKE_ON_LAN_PORT)),
    )
    .await
}

/// Sends the magic packet waking the card at `mac` to `destination`, as the C# Wake-on-LAN
/// tool: broadcast is allowed to an IPv4 destination; an IPv6 one (IPv6 has no broadcast: a
/// multicast group such as `ff02::1`) is sent from an IPv6 socket.
///
/// # Errors
///
/// What the system said when the packet could not be sent, or a time-out after
/// [`SEND_TIMEOUT`].
pub async fn send_to(mac: MacAddress, destination: SocketAddr) -> io::Result<()> {
    let local = if destination.is_ipv4() {
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))
    } else {
        SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))
    };
    let socket = UdpSocket::bind(local).await?;
    if destination.is_ipv4() {
        socket.set_broadcast(true)?;
    }
    match tokio::time::timeout(
        SEND_TIMEOUT,
        socket.send_to(&mac.magic_packet(), destination),
    )
    .await
    {
        Ok(sent) => sent.map(drop),
        Err(_) => Err(io::Error::from(io::ErrorKind::TimedOut)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_magic_packet_reaches_the_address_and_port_given() {
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind");
        let to = receiver.local_addr().expect("address");
        let mac = MacAddress([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
        send_to(mac, to).await.expect("sent");
        let mut packet = [0_u8; 256];
        let (count, _) = tokio::time::timeout(SEND_TIMEOUT, receiver.recv_from(&mut packet))
            .await
            .expect("in time")
            .expect("received");
        assert_eq!(&packet[..count], mac.magic_packet().as_slice());
    }
}
