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

//! IP addresses as .NET's `IPAddress.TryParse` reads them, which the C# subnet calculator,
//! IP converter and network calculator all go through: an address holding a colon is IPv6;
//! any other is IPv4 in the forms `inet_aton` takes, one to four parts, each decimal, octal
//! after a leading 0, or hexadecimal after `0x`, the last part filling the bytes left, so
//! that `3232235777`, `0xC0A80101` and `192.168.257` are all addresses.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// The most parts of an IPv4 address.
const MAX_PARTS: usize = 4;

/// The largest value of a part that is not the last.
const BYTE_MAX: u64 = 0xFF;

/// The largest value of an address.
const ADDRESS_MAX: u64 = 0xFFFF_FFFF;

/// The bases a part is written in.
const DECIMAL: u64 = 10;
const OCTAL: u64 = 8;
const HEXADECIMAL: u64 = 16;

/// Bits in a byte.
const BYTE_BITS: u32 = 8;

/// The address `text` writes, as .NET's `IPAddress.TryParse`; `None` when it is none.
#[must_use]
pub fn parse(text: &str) -> Option<IpAddr> {
    if text.contains(':') {
        parse_v6(text).map(IpAddr::V6)
    } else {
        parse_v4(text).map(IpAddr::V4)
    }
}

/// An IPv6 address, in brackets or not, its scope after `%` dropped.
fn parse_v6(text: &str) -> Option<Ipv6Addr> {
    let text = text
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(text);
    let address = text.split_once('%').map_or(
        text,
        |(address, scope)| {
            if scope.is_empty() { "" } else { address }
        },
    );
    address.parse().ok()
}

/// An IPv4 address in the forms .NET's `IPv4AddressHelper.ParseNonCanonical` reads.
fn parse_v4(text: &str) -> Option<Ipv4Addr> {
    let pieces: Vec<&str> = text.split('.').collect();
    if pieces.len() > MAX_PARTS {
        return None;
    }
    let mut parts = Vec::with_capacity(MAX_PARTS);
    for piece in &pieces {
        parts.push(parse_part(piece)?);
    }
    let (last, leading) = parts.split_last()?;
    if leading.iter().any(|part| *part > BYTE_MAX) {
        return None;
    }
    // The last part fills the bytes the others leave.
    let free_bits = BYTE_BITS * u32::try_from(MAX_PARTS - leading.len()).ok()?;
    if *last > ADDRESS_MAX >> (u32::BITS - free_bits) {
        return None;
    }
    let value = leading
        .iter()
        .enumerate()
        .fold(*last, |value, (index, part)| {
            let shift = BYTE_BITS * u32::try_from(MAX_PARTS - 1 - index).unwrap_or(0);
            value | (part << shift)
        });
    Some(Ipv4Addr::from(u32::try_from(value).ok()?))
}

/// A part: decimal, octal after a leading 0, hexadecimal after `0x`; `None` when empty or
/// past an address.
fn parse_part(piece: &str) -> Option<u64> {
    let (digits, radix, empty_ok) = if let Some(hex) = piece
        .strip_prefix("0x")
        .or_else(|| piece.strip_prefix("0X"))
    {
        (hex, HEXADECIMAL, false)
    } else if let Some(octal) = piece.strip_prefix('0') {
        // "0" alone is a part, octal and empty.
        (octal, OCTAL, true)
    } else {
        (piece, DECIMAL, false)
    };
    if digits.is_empty() {
        return empty_ok.then_some(0);
    }
    let mut value: u64 = 0;
    for c in digits.chars() {
        let digit = u64::from(c.to_digit(u32::try_from(radix).ok()?)?);
        value = value * radix + digit;
        if value > ADDRESS_MAX {
            return None;
        }
    }
    Some(value)
}

/// The first four bytes of `address` as a number, as the C# network calculator's
/// `IpToUint` takes them of either family (`NetworkCalculatorView.xaml.cs:347-351`).
#[must_use]
pub fn first_four_bytes(address: IpAddr) -> u32 {
    match address {
        IpAddr::V4(v4) => u32::from(v4),
        IpAddr::V6(v6) => {
            let octets = v6.octets();
            u32::from_be_bytes([octets[0], octets[1], octets[2], octets[3]])
        }
    }
}
