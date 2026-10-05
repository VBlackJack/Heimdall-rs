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
use std::net::Ipv4Addr;

use heimdall_core::metadata::MacAddress;
use tokio::net::UdpSocket;

/// The port the magic packet is sent to, as the C# `WolPort`.
pub const WAKE_ON_LAN_PORT: u16 = 9;

/// Broadcasts the magic packet waking the card at `mac` to the local network.
///
/// # Errors
///
/// What the system said when the packet could not be sent.
pub async fn send(mac: MacAddress) -> io::Result<()> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
    socket.set_broadcast(true)?;
    socket
        .send_to(&mac.magic_packet(), (Ipv4Addr::BROADCAST, WAKE_ON_LAN_PORT))
        .await
        .map(drop)
}
