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

//! The network scanner behind Ctrl+Shift+N, as the C# `NetworkScanner` and
//! `NetworkScannerService`: an IPv4 subnet (or one address) swept with echo requests, the
//! hosts that answer probed on the remote-access ports and named by reverse DNS, then the
//! kind of session each suggests.
//!
//! The sweep reaches the network through [`Prober`], the same probes the NETMAP tool runs,
//! at most [`MAX_CONCURRENCY`] at a time and stoppable. Differences from the C#, all on the
//! safe side: the subnet is bounded by [`MAX_HOSTS`] (the C# accepts down to `/16`, 65534
//! hosts), and a bare name is refused where the C# only knows addresses.

use std::net::Ipv4Addr;
use std::sync::atomic::AtomicBool;

use super::network_cartography::Prober;
use super::{net_host, probe_pool};

/// Echo requests and port probes in flight at once, as the C# `SemaphoreSlim(64)`.
pub const MAX_CONCURRENCY: usize = 64;
/// How long a port probe waits for its connection, in milliseconds, as the C#
/// `PortProbeTimeoutMs`.
pub const PORT_PROBE_TIMEOUT_MS: u64 = 500;
/// The widest subnet scanned, as a prefix length: `/22`, 1022 hosts. The C# goes to `/16`.
pub const MIN_PREFIX: u32 = 22;
/// The narrowest prefix: a single address, as the C#.
pub const MAX_PREFIX: u32 = 32;
/// The prefix of an address typed without one, as the C# (`/32`).
pub const SINGLE_HOST_PREFIX: u32 = 32;
/// The prefix of a point-to-point link, whose two addresses are both hosts, as the C#.
const POINT_TO_POINT_PREFIX: u32 = 31;
/// The most hosts one scan sweeps.
pub const MAX_HOSTS: usize = 1022;
/// The subnet the dialog suggests, as the C# prompt's default.
pub const DEFAULT_SUBNET: &str = "192.168.1.0/24";
/// The folder the sessions added are filed in, as the C# `Group = "Discovered"`.
pub const DISCOVERED_FOLDER: &str = "Discovered";

/// SSH port.
pub const PORT_SSH: u16 = 22;
/// Telnet port.
pub const PORT_TELNET: u16 = 23;
/// HTTP port.
pub const PORT_HTTP: u16 = 80;
/// HTTPS port.
pub const PORT_HTTPS: u16 = 443;
/// RDP port.
pub const PORT_RDP: u16 = 3389;
/// VNC port.
pub const PORT_VNC: u16 = 5900;
/// `WinRM` over HTTP port.
pub const PORT_WINRM: u16 = 5985;

/// The ports probed on each host that answers: the C# five (SSH, RDP, HTTP, HTTPS, VNC)
/// and the two more kinds of session Heimdall-rs opens, Telnet and `WinRM`.
pub const SCAN_PORTS: [u16; 7] = [
    PORT_SSH,
    PORT_TELNET,
    PORT_HTTP,
    PORT_HTTPS,
    PORT_RDP,
    PORT_VNC,
    PORT_WINRM,
];

/// Why a target was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetError {
    /// Nothing typed.
    Empty,
    /// Not an IPv4 address, or an address and a prefix that do not read.
    Invalid,
    /// A prefix outside [`MIN_PREFIX`] to [`MAX_PREFIX`]: the subnet is too wide.
    PrefixOutOfRange,
}

/// The hosts of the subnet or address typed, as the C# `ParseCidr` and `GenerateAddresses`:
/// network and broadcast addresses left out, `/31` both of its addresses, a bare address
/// itself. The text goes through [`net_host::checked_host`] first, as every tool's host.
///
/// # Errors
///
/// [`TargetError`] when it is empty, no IPv4 address or subnet, or wider than [`MIN_PREFIX`].
pub fn parse_target(typed: &str) -> Result<Vec<Ipv4Addr>, TargetError> {
    let trimmed = typed.trim();
    if trimmed.is_empty() {
        return Err(TargetError::Empty);
    }
    let (address, prefix) = match trimmed.split_once('/') {
        Some((address, prefix)) => (
            address.trim(),
            prefix
                .trim()
                .parse::<u32>()
                .map_err(|_| TargetError::Invalid)?,
        ),
        None => (trimmed, SINGLE_HOST_PREFIX),
    };
    let address = net_host::checked_host(address).ok_or(TargetError::Invalid)?;
    let address: Ipv4Addr = address.parse().map_err(|_| TargetError::Invalid)?;
    if !(MIN_PREFIX..=MAX_PREFIX).contains(&prefix) {
        return Err(TargetError::PrefixOutOfRange);
    }
    let mask = u32::MAX.checked_shl(32 - prefix).unwrap_or(0);
    let network = u32::from(address) & mask;
    let broadcast = network | !mask;
    if prefix >= POINT_TO_POINT_PREFIX {
        return Ok((network..=broadcast).map(Ipv4Addr::from).collect());
    }
    Ok((network + 1..broadcast).map(Ipv4Addr::from).collect())
}

/// The kind of session a host's open ports suggest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteKind {
    /// Remote desktop.
    Rdp,
    /// SSH terminal.
    Ssh,
    /// VNC desktop.
    Vnc,
    /// Remote `PowerShell`.
    WinRm,
    /// Telnet terminal.
    Telnet,
    /// A web console.
    Web,
}

/// A kind of session and the port it dials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Suggestion {
    /// The kind of session.
    pub kind: RemoteKind,
    /// The open port it dials.
    pub port: u16,
}

/// What an open port suggests, the most telling first: the C# order (RDP, SSH, VNC), then
/// the kinds it has no row for. HTTPS before HTTP.
const SUGGESTIONS: [(u16, RemoteKind); 7] = [
    (PORT_RDP, RemoteKind::Rdp),
    (PORT_SSH, RemoteKind::Ssh),
    (PORT_VNC, RemoteKind::Vnc),
    (PORT_WINRM, RemoteKind::WinRm),
    (PORT_TELNET, RemoteKind::Telnet),
    (PORT_HTTPS, RemoteKind::Web),
    (PORT_HTTP, RemoteKind::Web),
];

/// The session `open_ports` suggest; `None` when none is a remote-access port. The C#
/// falls back to SSH for a host with none, which would file a session that cannot connect.
#[must_use]
pub fn suggest(open_ports: &[u16]) -> Option<Suggestion> {
    SUGGESTIONS
        .iter()
        .find(|(port, _)| open_ports.contains(port))
        .map(|&(port, kind)| Suggestion { kind, port })
}

/// A host that answered the sweep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedHost {
    /// Its address.
    pub ip: Ipv4Addr,
    /// Its name, by reverse DNS.
    pub hostname: Option<String>,
    /// The echo reply's round trip, in milliseconds.
    pub round_trip_ms: u64,
    /// Which of [`SCAN_PORTS`] accepted a connection, in port order.
    pub open_ports: Vec<u16>,
}

impl ScannedHost {
    /// The session this host suggests.
    #[must_use]
    pub fn suggestion(&self) -> Option<Suggestion> {
        suggest(&self.open_ports)
    }

    /// The name a session of this host takes: its DNS name, else its address, as the C#
    /// `DisplayName`.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.hostname.clone().unwrap_or_else(|| self.ip.to_string())
    }
}

/// What a scan tells as it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanEvent {
    /// `done` of `total` addresses have been pinged.
    Progress {
        /// Addresses pinged.
        done: usize,
        /// Addresses to ping.
        total: usize,
    },
    /// A host that answered, with its open ports and its name; sent once the sweep is over,
    /// in address order.
    Host(ScannedHost),
}

/// Sweeps `ips`, as the C# `ScanSubnetAsync`: an echo request to each, [`MAX_CONCURRENCY`]
/// at a time; the ports of [`SCAN_PORTS`] probed on each that answered; the name of each
/// looked up. Once `stop` is set no probe starts. Returns the hosts that answered, by
/// address.
pub fn scan(
    ips: &[Ipv4Addr],
    prober: &impl Prober,
    stop: &AtomicBool,
    mut on_event: impl FnMut(ScanEvent),
) -> Vec<ScannedHost> {
    let total = ips.len();
    let mut done = 0;
    let mut alive: Vec<(Ipv4Addr, u64)> = Vec::new();
    probe_pool::for_each_bounded(
        ips.to_vec(),
        MAX_CONCURRENCY,
        stop,
        |ip| (ip, prober.ping(ip, stop)),
        |(ip, round_trip)| {
            done += 1;
            on_event(ScanEvent::Progress { done, total });
            if let Some(round_trip) = round_trip {
                alive.push((ip, round_trip));
            }
        },
    );
    alive.sort_by_key(|(ip, _)| *ip);

    let jobs: Vec<(Ipv4Addr, u16)> = alive
        .iter()
        .flat_map(|(ip, _)| SCAN_PORTS.iter().map(move |port| (*ip, *port)))
        .collect();
    let mut hosts: Vec<ScannedHost> = alive
        .iter()
        .map(|&(ip, round_trip_ms)| ScannedHost {
            ip,
            hostname: None,
            round_trip_ms,
            open_ports: Vec::new(),
        })
        .collect();
    probe_pool::for_each_bounded(
        jobs,
        MAX_CONCURRENCY,
        stop,
        |(ip, port)| (ip, port, prober.connect(ip, port).is_some()),
        |(ip, port, open)| {
            if open && let Some(host) = hosts.iter_mut().find(|host| host.ip == ip) {
                host.open_ports.push(port);
            }
        },
    );
    for host in &mut hosts {
        host.open_ports.sort_unstable();
    }

    let indexes: Vec<usize> = (0..hosts.len()).collect();
    let mut names: Vec<(usize, Option<String>)> = Vec::new();
    probe_pool::for_each_bounded(
        indexes,
        MAX_CONCURRENCY,
        stop,
        |at| (at, prober.reverse_name(hosts[at].ip, stop)),
        |named| names.push(named),
    );
    for (at, name) in names {
        hosts[at].hostname = name;
    }
    for host in &hosts {
        on_event(ScanEvent::Host(host.clone()));
    }
    hosts
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    #[test]
    fn a_slash_24_gives_its_254_hosts_without_network_and_broadcast() {
        let hosts = parse_target("192.168.1.0/24").expect("a subnet");
        assert_eq!(hosts.len(), 254);
        assert_eq!(hosts.first(), Some(&Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(hosts.last(), Some(&Ipv4Addr::new(192, 168, 1, 254)));
    }

    #[test]
    fn an_address_inside_the_subnet_is_masked_to_its_network() {
        let hosts = parse_target(" 10.0.0.77 / 30 ").expect("a subnet");
        assert_eq!(
            hosts,
            [Ipv4Addr::new(10, 0, 0, 77), Ipv4Addr::new(10, 0, 0, 78)]
        );
    }

    #[test]
    fn a_bare_address_is_one_host_and_a_slash_31_is_two() {
        assert_eq!(
            parse_target("10.1.2.3"),
            Ok(vec![Ipv4Addr::new(10, 1, 2, 3)])
        );
        assert_eq!(
            parse_target("10.1.2.3/32"),
            Ok(vec![Ipv4Addr::new(10, 1, 2, 3)])
        );
        assert_eq!(
            parse_target("10.1.2.3/31"),
            Ok(vec![Ipv4Addr::new(10, 1, 2, 2), Ipv4Addr::new(10, 1, 2, 3)])
        );
    }

    #[test]
    fn the_widest_subnet_stays_within_the_host_limit() {
        let hosts = parse_target("172.16.0.0/22").expect("the widest");
        assert_eq!(hosts.len(), MAX_HOSTS);
        for wider in ["172.16.0.0/21", "10.0.0.0/16", "10.0.0.0/0", "10.0.0.0/33"] {
            assert_eq!(
                parse_target(wider),
                Err(TargetError::PrefixOutOfRange),
                "{wider}"
            );
        }
    }

    #[test]
    fn what_is_no_ipv4_subnet_is_refused() {
        assert_eq!(parse_target("   "), Err(TargetError::Empty));
        for typed in [
            "example.com",
            "example.com/24",
            "2001:db8::1",
            "300.1.1.1/24",
            "10.0.0.0/x",
            "10.0.0.0/",
            "-oProxyCommand=x",
            "10.0.0.0/24/1",
        ] {
            assert_eq!(parse_target(typed), Err(TargetError::Invalid), "{typed}");
        }
    }

    #[test]
    fn open_ports_suggest_the_most_telling_kind_first() {
        let kind = |ports: &[u16]| suggest(ports).map(|s| (s.kind, s.port));
        assert_eq!(kind(&[22, 80, 3389]), Some((RemoteKind::Rdp, PORT_RDP)));
        assert_eq!(kind(&[22, 443]), Some((RemoteKind::Ssh, PORT_SSH)));
        assert_eq!(kind(&[80, 5900]), Some((RemoteKind::Vnc, PORT_VNC)));
        assert_eq!(kind(&[23, 5985]), Some((RemoteKind::WinRm, PORT_WINRM)));
        assert_eq!(kind(&[23, 80]), Some((RemoteKind::Telnet, PORT_TELNET)));
        assert_eq!(kind(&[80, 443]), Some((RemoteKind::Web, PORT_HTTPS)));
        assert_eq!(kind(&[80]), Some((RemoteKind::Web, PORT_HTTP)));
        assert_eq!(kind(&[]), None);
        assert_eq!(kind(&[8080, 445]), None);
    }

    /// Answers for a fixed set of hosts and ports.
    struct Fake {
        up: HashMap<Ipv4Addr, u64>,
        open: HashSet<(Ipv4Addr, u16)>,
        names: HashMap<Ipv4Addr, String>,
    }

    impl Prober for Fake {
        fn ping(&self, ip: Ipv4Addr, _: &AtomicBool) -> Option<u64> {
            self.up.get(&ip).copied()
        }
        fn connect(&self, ip: Ipv4Addr, port: u16) -> Option<u64> {
            self.open.contains(&(ip, port)).then_some(1)
        }
        fn arp_table(&self, _: &AtomicBool) -> Vec<(String, String)> {
            Vec::new()
        }
        fn reverse_name(&self, ip: Ipv4Addr, _: &AtomicBool) -> Option<String> {
            self.names.get(&ip).cloned()
        }
    }

    #[test]
    fn a_scan_finds_the_hosts_that_answer_with_their_ports_and_names() {
        let a = Ipv4Addr::new(10, 0, 0, 1);
        let b = Ipv4Addr::new(10, 0, 0, 2);
        let c = Ipv4Addr::new(10, 0, 0, 3);
        let prober = Fake {
            up: HashMap::from([(b, 4), (a, 2)]),
            // A port open on a host that does not answer the sweep is never probed.
            open: HashSet::from([(a, 22), (a, 443), (b, 3389), (c, 22)]),
            names: HashMap::from([(a, "gw.lan".to_owned())]),
        };
        let mut events = Vec::new();
        let hosts = scan(&[a, b, c], &prober, &AtomicBool::new(false), |event| {
            events.push(event);
        });
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].ip, a);
        assert_eq!(hosts[0].hostname.as_deref(), Some("gw.lan"));
        assert_eq!(hosts[0].open_ports, [22, 443]);
        assert_eq!(hosts[0].round_trip_ms, 2);
        assert_eq!(hosts[1].ip, b);
        assert_eq!(hosts[1].display_name(), "10.0.0.2");
        assert_eq!(hosts[1].open_ports, [3389]);
        let progress = events
            .iter()
            .filter(|event| matches!(event, ScanEvent::Progress { total: 3, .. }))
            .count();
        assert_eq!(progress, 3);
        let told: Vec<_> = events
            .into_iter()
            .filter_map(|event| match event {
                ScanEvent::Host(host) => Some(host),
                ScanEvent::Progress { .. } => None,
            })
            .collect();
        assert_eq!(told, hosts);
    }

    #[test]
    fn a_stopped_scan_probes_nothing() {
        let a = Ipv4Addr::new(10, 0, 0, 1);
        let prober = Fake {
            up: HashMap::from([(a, 2)]),
            open: HashSet::new(),
            names: HashMap::new(),
        };
        let hosts = scan(&[a], &prober, &AtomicBool::new(true), |_| {});
        assert!(hosts.is_empty());
    }
}
