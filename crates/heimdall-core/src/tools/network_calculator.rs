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

//! The network calculator's three computations, as the C# `NetworkCalculatorView`
//! (`Views/Tools/NetworkCalculatorView.xaml.cs`) makes them, IPv4 only and by computation
//! alone: the smallest network covering several, the networks covering a range of
//! addresses, and the network a count of hosts needs.

use std::net::Ipv4Addr;

use super::{ip_address, number_text};

/// Bits of an IPv4 address, as the C# `Ipv4Bits`.
const IPV4_BITS: u32 = 32;

/// The widest prefix, as the C# `MaxPrefix`.
const MAX_PREFIX: i32 = 32;

/// Addresses a network spends on itself and its broadcast.
const RESERVED_ADDRESSES: i32 = 2;

/// Percent.
const PERCENT: f64 = 100.0;

/// What went wrong, as the C# `ShowError` calls say it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetCalcError {
    /// No network given, `ToolNetCalcErrorNoCidrs`.
    NoCidrs,
    /// A line that is not a network, `ToolNetCalcErrorInvalidCidr`.
    InvalidCidr(String),
    /// A start or end that is no address, `ToolNetCalcErrorInvalidIpRange`.
    InvalidIpRange,
    /// A start after the end, `ToolNetCalcErrorStartAfterEnd`.
    StartAfterEnd,
    /// No positive count of hosts, `ToolNetCalcErrorInvalidHostCount`.
    InvalidHostCount,
    /// A base network that is no address, `ToolNetCalcErrorInvalidBaseNetwork`.
    InvalidBaseNetwork,
}

/// The mask of `prefix` bits, as the C# `PrefixToMask`.
const fn mask(prefix: u32) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (IPV4_BITS - prefix)
    }
}

/// `value` written as an address, as the C# `UintToIp`.
#[must_use]
pub fn address_text(value: u32) -> String {
    Ipv4Addr::from(value).to_string()
}

/// The smallest network covering others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Supernet {
    /// Its address.
    pub network: u32,
    /// Its prefix.
    pub prefix: u32,
    /// Its broadcast address.
    pub broadcast: u32,
    /// Its usable hosts.
    pub hosts: u64,
}

/// The smallest network covering the networks of `input`, one a line, as the C#
/// `OnSupernetComputeClick` (`NetworkCalculatorView.xaml.cs:139-208`).
///
/// # Errors
///
/// [`NetCalcError::NoCidrs`] for no line, [`NetCalcError::InvalidCidr`] with the first
/// line that is not a network.
pub fn supernet(input: &str) -> Result<Supernet, NetCalcError> {
    let lines: Vec<&str> = input
        .split(['\r', '\n'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return Err(NetCalcError::NoCidrs);
    }
    let mut networks = Vec::with_capacity(lines.len());
    for line in lines {
        networks.push(parse_cidr(line).ok_or_else(|| NetCalcError::InvalidCidr(line.to_owned()))?);
    }
    let (mut lowest, mut highest) = (u32::MAX, u32::MIN);
    for (address, prefix) in networks {
        let network = address & mask(prefix);
        lowest = lowest.min(network);
        highest = highest.max(network | !mask(prefix));
    }
    let mut prefix = (lowest ^ highest).leading_zeros();
    let mut network = lowest & mask(prefix);
    let mut broadcast = network | !mask(prefix);
    while broadcast < highest && prefix > 0 {
        prefix -= 1;
        network = lowest & mask(prefix);
        broadcast = network | !mask(prefix);
    }
    Ok(Supernet {
        network,
        prefix,
        broadcast,
        hosts: usable_hosts(IPV4_BITS - prefix),
    })
}

/// Usable hosts of `host_bits`, as the C#'s `(1L << bits) - 2`, never below 0.
fn usable_hosts(host_bits: u32) -> u64 {
    (1_u64 << host_bits).saturating_sub(2)
}

/// A network and its prefix, as the C# `TryParseCidr` (`NetworkCalculatorView.xaml.cs:331-345`):
/// an address of either family, its first four bytes taken, and a prefix of 0 to 32.
fn parse_cidr(line: &str) -> Option<(u32, u32)> {
    let (address, prefix) = line.split_once('/')?;
    if prefix.contains('/') {
        return None;
    }
    let address = ip_address::parse(address.trim())?;
    let prefix =
        number_text::parse_int32(prefix).filter(|prefix| (0..=MAX_PREFIX).contains(prefix))?;
    Some((
        ip_address::first_four_bytes(address),
        u32::try_from(prefix).ok()?,
    ))
}

/// The networks covering `start` to `end`, written `a.b.c.d/n`, as the C#
/// `OnRangeComputeClick` and `RangeToCidrs` (`NetworkCalculatorView.xaml.cs:212-241, 381-420`):
/// from the start, the largest block it is aligned on that fits, in turn.
///
/// # Errors
///
/// [`NetCalcError::InvalidIpRange`] when either is no address,
/// [`NetCalcError::StartAfterEnd`] when the start is after the end.
pub fn range_to_cidrs(start: &str, end: &str) -> Result<Vec<String>, NetCalcError> {
    let read = |text: &str| ip_address::parse(text.trim()).map(ip_address::first_four_bytes);
    let (Some(mut start), Some(end)) = (read(start), read(end)) else {
        return Err(NetCalcError::InvalidIpRange);
    };
    if start > end {
        return Err(NetCalcError::StartAfterEnd);
    }
    let mut blocks = Vec::new();
    while start <= end {
        let mut bits = if start == 0 {
            IPV4_BITS
        } else {
            start.trailing_zeros()
        };
        let size = u64::from(end) - u64::from(start) + 1;
        while bits > 0 && (1_u64 << bits) > size {
            bits -= 1;
        }
        blocks.push(format!("{}/{}", address_text(start), IPV4_BITS - bits));
        start = start.wrapping_add(u32::try_from(1_u64 << bits).unwrap_or(0));
        if start == 0 {
            break;
        }
    }
    Ok(blocks)
}

/// The network a count of hosts needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VlanPlan {
    /// Its address.
    pub network: u32,
    /// Its prefix.
    pub prefix: u32,
    /// Its mask.
    pub mask: u32,
    /// Its broadcast address.
    pub broadcast: u32,
    /// Its first usable address.
    pub first_usable: u32,
    /// Its last usable address.
    pub last_usable: u32,
    /// Its usable hosts.
    pub usable_hosts: u64,
    /// The hosts asked for.
    pub requested: u32,
    /// What share of the usable hosts they take, in percent.
    pub utilization: f64,
}

/// The network of `base` that `hosts` hosts need, as the C# `OnVlanComputeClick`
/// (`NetworkCalculatorView.xaml.cs:245-297`): the fewest host bits holding the hosts and
/// the network's own two addresses, with the C#'s 32-bit arithmetic.
///
/// # Errors
///
/// [`NetCalcError::InvalidHostCount`] for a count that is not a positive 32-bit number,
/// [`NetCalcError::InvalidBaseNetwork`] for a base that is no address.
pub fn vlan(hosts: &str, base: &str) -> Result<VlanPlan, NetCalcError> {
    let requested = number_text::parse_int32(hosts)
        .filter(|hosts| *hosts > 0)
        .ok_or(NetCalcError::InvalidHostCount)?;
    let base = ip_address::parse(base.trim())
        .map(ip_address::first_four_bytes)
        .ok_or(NetCalcError::InvalidBaseNetwork)?;
    // As the C#'s `int` arithmetic, which wraps past its largest value.
    let required = requested.wrapping_add(RESERVED_ADDRESSES);
    let mut host_bits = 0;
    while host_bits < IPV4_BITS && 1_i32.wrapping_shl(host_bits) < required {
        host_bits += 1;
    }
    let prefix = IPV4_BITS - host_bits;
    let mask = mask(prefix);
    let network = base & mask;
    let broadcast = network | !mask;
    let usable_hosts = usable_hosts(host_bits);
    let requested = requested.unsigned_abs();
    #[expect(
        clippy::cast_precision_loss,
        reason = "as the C#'s double, exact below 2^53"
    )]
    let utilization = f64::from(requested) * PERCENT / usable_hosts.max(1) as f64;
    Ok(VlanPlan {
        network,
        prefix,
        mask,
        broadcast,
        first_usable: network.wrapping_add(1),
        last_usable: broadcast.wrapping_sub(1),
        usable_hosts,
        requested,
        utilization,
    })
}
