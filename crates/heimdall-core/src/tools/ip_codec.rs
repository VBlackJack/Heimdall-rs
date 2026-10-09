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

//! An IPv4 address in its five forms, as the C# `IpCodec`
//! (`Heimdall.Core/Codecs/IpCodec.cs`) converts it: dotted decimal, an integer,
//! hexadecimal, dotted binary and IPv4-mapped IPv6.

use std::net::IpAddr;

use super::ip_address;

/// The prefix of hexadecimal.
const HEX_PREFIX: &str = "0x";

/// Parts of a dotted binary address.
const BINARY_PARTS: usize = 4;

/// The radix of binary.
const BINARY_RADIX: u32 = 2;

/// The radix of hexadecimal.
const HEX_RADIX: u32 = 16;

/// Hexadecimal digits of an address.
const HEX_DIGITS: usize = 8;

/// The binary digits .NET's `Convert.ToByte(text, 2)` reads before it overflows.
const MAX_BINARY_DIGITS: usize = 32;

/// An address in every form, as the C# `IpConversionResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpConversion {
    /// `192.168.1.1`.
    pub dotted: String,
    /// `3232235777`.
    pub decimal: String,
    /// `0xC0A80101`.
    pub hex: String,
    /// `11000000.10101000.00000001.00000001`.
    pub binary: String,
    /// `::ffff:c0a8:0101`.
    pub mapped_ipv6: String,
}

/// `input` in every form, as the C# `IpCodec.TryConvert` (`IpCodec.cs:32-45`); `None`
/// when it is no IPv4 address in any of them.
#[must_use]
pub fn convert(input: &str) -> Option<IpConversion> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    parse(trimmed).map(format)
}

/// The address `input` writes, as the C# `TryParseToUint32` (`IpCodec.cs:47-83`), in its
/// order: hexadecimal after `0x`; four dotted parts of only 0 and 1 as binary, so that
/// `10.0.0.1` reads as `2.0.0.1`, as the C# tests pin; an IPv4 address as .NET reads one;
/// then a plain integer.
fn parse(input: &str) -> Option<u32> {
    if input
        .get(..HEX_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(HEX_PREFIX))
    {
        let digits = &input[HEX_PREFIX.len()..];
        // .NET's `HexNumber` takes no sign.
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        return u32::from_str_radix(digits, HEX_RADIX).ok();
    }
    let parts: Vec<&str> = input.split('.').collect();
    if input.contains('.')
        && input.bytes().all(|byte| matches!(byte, b'0' | b'1' | b'.'))
        && parts.len() == BINARY_PARTS
    {
        let mut value = 0_u32;
        for part in parts {
            if part.is_empty() || part.len() > MAX_BINARY_DIGITS {
                return None;
            }
            let byte = u32::from_str_radix(part, BINARY_RADIX).ok()?;
            value = (value << u8::BITS) | u32::from(u8::try_from(byte).ok()?);
        }
        return Some(value);
    }
    if let Some(IpAddr::V4(address)) = ip_address::parse(input) {
        return Some(u32::from(address));
    }
    // .NET's `NumberStyles.None`: digits only.
    if !input.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    input.parse().ok()
}

/// `value` in every form, as the C# `Format` (`IpCodec.cs:85-105`).
fn format(value: u32) -> IpConversion {
    let bytes = value.to_be_bytes();
    IpConversion {
        dotted: std::net::Ipv4Addr::from(value).to_string(),
        decimal: value.to_string(),
        hex: format!("0x{value:0HEX_DIGITS$X}"),
        binary: bytes
            .iter()
            .map(|byte| format!("{byte:08b}"))
            .collect::<Vec<_>>()
            .join("."),
        mapped_ipv6: format!(
            "::ffff:{:02x}{:02x}:{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3]
        ),
    }
}
