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

//! What the Wake-on-LAN tool sends and where, as the C# `WakeOnLanViewModel` checks it: the
//! card's MAC address, the broadcast address and the UDP port. The packet is
//! [`MacAddress::magic_packet`], sent by `heimdall_app::wake_on_lan`.

use std::net::{IpAddr, SocketAddr};

use super::net_input;
use crate::metadata::MacAddress;

/// Default UDP port, the C# `WakeOnLanViewModel.DefaultPort` (the discard port).
pub const DEFAULT_WOL_PORT: u16 = 9;

/// Default destination, the C# `WakeOnLanViewModel.DefaultBroadcastAddress`.
pub const DEFAULT_BROADCAST_ADDRESS: &str = "255.255.255.255";

/// Why the typed fields cannot be sent, in the order the C# checks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WolInputError {
    /// Not a MAC address, the C# `ToolWolErrorInvalidMac`.
    InvalidMac,
    /// Not an IP address, the C# `ToolWolErrorInvalidBroadcast`.
    InvalidBroadcast,
    /// Not a port from 1 to 65535.
    InvalidPort,
}

/// The card and the address its packet goes to, from the typed fields: a MAC address
/// ([`MacAddress`]'s reading: twelve ASCII hexadecimal digits, `:`, `-` or `.` between them),
/// an IP address literal (the C# `IPAddress.TryParse`: no name is looked up) and a port.
///
/// # Errors
///
/// The first field that cannot be read, as [`WolInputError`] says.
pub fn destination(
    mac: &str,
    broadcast: &str,
    port: &str,
) -> Result<(MacAddress, SocketAddr), WolInputError> {
    let mac = mac
        .parse::<MacAddress>()
        .map_err(|_| WolInputError::InvalidMac)?;
    let address = broadcast
        .trim()
        .parse::<IpAddr>()
        .map_err(|_| WolInputError::InvalidBroadcast)?;
    let port = net_input::parse_port(port).ok_or(WolInputError::InvalidPort)?;
    Ok((mac, SocketAddr::new(address, port)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: [u8; 6] = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];

    #[test]
    fn the_usual_mac_formats_are_read() {
        for typed in [
            "00:11:22:33:44:55",
            "00-11-22-33-44-55",
            "0011.2233.4455",
            "001122334455",
            "  00:11:22:33:44:55  ",
        ] {
            let (mac, _) = destination(typed, DEFAULT_BROADCAST_ADDRESS, "9").expect(typed);
            assert_eq!(mac.0, MAC, "{typed}");
        }
    }

    #[test]
    fn a_mac_of_other_characters_is_refused_without_a_panic() {
        // Twelve bytes but not twelve ASCII digits: a byte-length check then byte slicing
        // panicked on the first, and `u8::from_str_radix` takes the sign of the second.
        for typed in [
            "0\u{00E9}000000000",
            "+1+1+1+1+1+1",
            "00:11:22",
            "invalid-mac",
            "",
        ] {
            assert_eq!(
                destination(typed, DEFAULT_BROADCAST_ADDRESS, "9"),
                Err(WolInputError::InvalidMac),
                "{typed}"
            );
        }
    }

    #[test]
    fn the_broadcast_must_be_an_ip_literal_and_the_port_in_range() {
        let mac = "00:11:22:33:44:55";
        assert_eq!(
            destination(mac, " 192.168.1.255 ", "7").map(|(_, to)| to),
            Ok("192.168.1.255:7".parse().expect("address"))
        );
        assert_eq!(
            destination(mac, "ff02::1", "9").map(|(_, to)| to),
            Ok("[ff02::1]:9".parse().expect("address"))
        );
        for broadcast in ["broadcast.example", "", "255.255.255.256"] {
            assert_eq!(
                destination(mac, broadcast, "9"),
                Err(WolInputError::InvalidBroadcast),
                "{broadcast}"
            );
        }
        for port in ["", "0", "65536", "nine"] {
            assert_eq!(
                destination(mac, DEFAULT_BROADCAST_ADDRESS, port),
                Err(WolInputError::InvalidPort),
                "{port}"
            );
        }
    }
}
