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

//! A network's addresses from an address and its prefix, as the C# `SubnetCalculatorView`
//! (`Views/Tools/SubnetCalculatorView.xaml.cs:164-439`) computes them, IPv4 and IPv6, by
//! computation alone.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::{ip_address, number_text};

/// Bits of an IPv4 address.
const IPV4_BITS: u32 = 32;

/// Bits of an IPv6 address.
const IPV6_BITS: u32 = 128;

/// Host bits past which IPv6 hosts are not counted, as the C#'s 64.
const IPV6_COUNTED_HOST_BITS: u32 = 64;

/// What the calculator shows of a network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subnet {
    /// The network address.
    pub network: String,
    /// The broadcast address; IPv4 only.
    pub broadcast: Option<String>,
    /// The subnet mask; IPv4 only.
    pub mask: Option<String>,
    /// The first host.
    pub first_host: String,
    /// The last host.
    pub last_host: String,
    /// How many hosts.
    pub total_hosts: HostCount,
    /// The network and its prefix, `192.168.1.0/24`.
    pub cidr: String,
    /// The wildcard mask; IPv4 only.
    pub wildcard: Option<String>,
}

/// How many hosts a network holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostCount {
    /// This many, which the window writes with its digits grouped, as .NET's "N0".
    Count(u128),
    /// More than the C# counts: an IPv6 network past 64 host bits.
    TooMany,
}

/// The network `input` names, as the C# `Calculate`: an address alone is a single host, an
/// address and `/prefix` the network around it; `None` when it is neither, as
/// `ToolSubnetErrorInvalidCidr` says.
#[must_use]
pub fn calculate(input: &str) -> Option<Subnet> {
    let (address, prefix) = parse_cidr(input.trim())?;
    Some(match address {
        IpAddr::V4(v4) => ipv4(v4, prefix),
        IpAddr::V6(v6) => ipv6(v6, prefix),
    })
}

/// An address and its prefix, as the C# `TryParseCidr` (`SubnetCalculatorView.xaml.cs:371-427`).
fn parse_cidr(input: &str) -> Option<(IpAddr, u32)> {
    let parts: Vec<&str> = input.split('/').collect();
    let address = ip_address::parse(parts.first()?)?;
    let max = match address {
        IpAddr::V4(_) => IPV4_BITS,
        IpAddr::V6(_) => IPV6_BITS,
    };
    match parts.as_slice() {
        [_] => Some((address, max)),
        [_, prefix] => {
            let prefix = number_text::parse_int32(prefix)?;
            let prefix = u32::try_from(prefix).ok().filter(|prefix| *prefix <= max)?;
            Some((address, prefix))
        }
        _ => None,
    }
}

/// The mask of `prefix` bits.
const fn mask_v4(prefix: u32) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (IPV4_BITS - prefix)
    }
}

/// An IPv4 network, as the C# `CalculateIpv4` (`SubnetCalculatorView.xaml.cs:199-264`):
/// its first and last hosts inside the network and broadcast addresses but for /31 and /32,
/// which count every address.
fn ipv4(address: Ipv4Addr, prefix: u32) -> Subnet {
    let mask = mask_v4(prefix);
    let network = u32::from(address) & mask;
    let broadcast = network | !mask;
    let (first, last, total) = match prefix {
        32 => (network, broadcast, 1),
        31 => (network, broadcast, 2),
        // As the C#: the last byte alone moves, which it can at these prefixes.
        _ => (
            network + 1,
            broadcast - 1,
            (1_u128 << (IPV4_BITS - prefix)) - 2,
        ),
    };
    let show = |value: u32| Ipv4Addr::from(value).to_string();
    Subnet {
        network: show(network),
        broadcast: Some(show(broadcast)),
        mask: Some(show(mask)),
        first_host: show(first),
        last_host: show(last),
        total_hosts: HostCount::Count(total),
        cidr: format!("{}/{prefix}", show(network)),
        wildcard: Some(show(!mask)),
    }
}

/// An IPv6 network, as the C# `CalculateIpv6` (`SubnetCalculatorView.xaml.cs:266-343`): its
/// first host one past the network and its last one before its last address, whatever the
/// prefix, as the C#'s; no broadcast, mask nor wildcard.
fn ipv6(address: Ipv6Addr, prefix: u32) -> Subnet {
    let mask = if prefix == 0 {
        0
    } else {
        u128::MAX << (IPV6_BITS - prefix)
    };
    let network = u128::from(address) & mask;
    let last = network | !mask;
    let host_bits = IPV6_BITS - prefix;
    let total = match prefix {
        128 => HostCount::Count(1),
        127 => HostCount::Count(2),
        _ if host_bits > IPV6_COUNTED_HOST_BITS => HostCount::TooMany,
        _ => HostCount::Count((1_u128 << host_bits) - 2),
    };
    let show = |value: u128| Ipv6Addr::from(value).to_string();
    Subnet {
        network: show(network),
        broadcast: None,
        mask: None,
        first_host: show(network.wrapping_add(1)),
        last_host: show(last.wrapping_sub(1)),
        total_hosts: total,
        cidr: format!("{}/{prefix}", show(network)),
        wildcard: None,
    }
}
