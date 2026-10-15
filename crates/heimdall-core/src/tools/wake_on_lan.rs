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

//! Wake-on-LAN magic packet generator and UDP broadcast transmitter.

use std::net::UdpSocket;

/// Magic packet length: 6 bytes synchronization prefix (0xFF) + 16 repetitions of 6-byte MAC.
pub const MAGIC_PACKET_LENGTH: usize = 6 + (16 * 6);

/// Default UDP broadcast port for Wake-on-LAN (standard is 9, alternate is 7).
pub const DEFAULT_WOL_PORT: u16 = 9;

/// Default broadcast destination address.
pub const DEFAULT_BROADCAST_ADDRESS: &str = "255.255.255.255";

/// Parses a MAC address from common representation formats into 6 raw bytes.
///
/// Supports:
/// - Colon separated: `00:11:22:33:44:55`
/// - Hyphen separated: `00-11-22-33-44-55`
/// - Dot separated: `0011.2233.4455`
/// - Raw hexadecimal: `001122334455`
#[must_use]
pub fn parse_mac_address(input: &str) -> Option<[u8; 6]> {
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ':' && *c != '-' && *c != '.')
        .collect();

    if cleaned.len() != 12 {
        return None;
    }

    let mut bytes = [0u8; 6];
    for i in 0..6 {
        let chunk = &cleaned[i * 2..(i * 2) + 2];
        bytes[i] = u8::from_str_radix(chunk, 16).ok()?;
    }

    Some(bytes)
}

/// Normalizes a parsed or string MAC address into canonical colon-separated uppercase format.
#[must_use]
pub fn format_mac_address(bytes: &[u8; 6]) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
    )
}

/// Constructs a 102-byte Wake-on-LAN magic packet for the specified MAC address.
#[must_use]
pub fn build_magic_packet(mac: &[u8; 6]) -> [u8; MAGIC_PACKET_LENGTH] {
    let mut packet = [0xFFu8; MAGIC_PACKET_LENGTH];

    for i in 0..16 {
        let start = 6 + (i * 6);
        packet[start..start + 6].copy_from_slice(mac);
    }

    packet
}

/// Sends a Wake-on-LAN magic packet over UDP broadcast.
pub fn send_magic_packet(mac: &[u8; 6], broadcast_addr: &str, port: u16) -> Result<(), String> {
    let packet = build_magic_packet(mac);
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    socket.set_broadcast(true).map_err(|e| e.to_string())?;

    let destination = format!("{broadcast_addr}:{port}");
    socket
        .send_to(&packet, &destination)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mac_address_formats() {
        let expected = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];

        assert_eq!(parse_mac_address("00:11:22:33:44:55"), Some(expected));
        assert_eq!(parse_mac_address("00-11-22-33-44-55"), Some(expected));
        assert_eq!(parse_mac_address("0011.2233.4455"), Some(expected));
        assert_eq!(parse_mac_address("001122334455"), Some(expected));
        assert_eq!(parse_mac_address("  00:11:22:33:44:55  "), Some(expected));

        assert_eq!(parse_mac_address("00:11:22"), None);
        assert_eq!(parse_mac_address("invalid-mac"), None);
    }

    #[test]
    fn test_format_mac_address() {
        let bytes = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        assert_eq!(format_mac_address(&bytes), "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn test_build_magic_packet() {
        let mac = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];
        let packet = build_magic_packet(&mac);

        assert_eq!(packet.len(), MAGIC_PACKET_LENGTH);
        // First 6 bytes must be 0xFF
        assert_eq!(&packet[0..6], &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);

        // Next 16 * 6 bytes must repeat the MAC
        for i in 0..16 {
            let offset = 6 + (i * 6);
            assert_eq!(&packet[offset..offset + 6], &mac);
        }
    }
}
