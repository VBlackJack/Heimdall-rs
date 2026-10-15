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

//! Network cartography and subnet node discovery engine matching the C#
//! `Heimdall.Core.Discovery.CartographyEngine`.
//!
//! Provides subnet host expansion within the C# scope (`/16` to `/30`), port probing a
//! bounded number at a time with a stop, role classification, and CSV export.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use super::default_creds::escape_csv;
use super::net_command;

/// Quick scan ports for network cartography.
pub const QUICK_RECON_PORTS: [u16; 12] =
    [22, 53, 80, 88, 139, 389, 443, 445, 1433, 3306, 3389, 8080];
/// The widest subnet scanned, as the C# `ParseCidr` refuses wider than `/16`.
pub const MIN_PREFIX: u32 = 16;
/// The narrowest subnet scanned, as the C# `ParseCidr`.
pub const MAX_PREFIX: u32 = 30;
/// The prefix given to an address typed without one, as the C# `NormalizeSubnet`.
pub const DEFAULT_PREFIX: u32 = 24;
/// Above this many hosts the C# asks before scanning (`LargeSubnetThreshold`).
pub const LARGE_SUBNET_THRESHOLD: usize = 512;
/// Probes run at the same time, as the C# `DefaultMaxConcurrency`.
pub const MAX_CONCURRENCY: usize = 50;
/// How long a probe waits for its connection, as the C# `DefaultProbeTimeoutMs`.
pub const PROBE_TIMEOUT_MS: u64 = 2000;

/// The role a host's open ports suggest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeRole {
    Directory,
    Database,
    Mail,
    FileStorage,
    Web,
    RemoteManagement,
    NetworkAppliance,
    ActiveNode,
    NoPortsOpen,
}

/// A single discovered network node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredNode {
    pub ip: String,
    pub hostname: Option<String>,
    pub mac: Option<String>,
    pub vendor: Option<String>,
    pub open_ports: Vec<u16>,
    pub role: NodeRole,
    pub response_time_ms: u64,
}

/// Aggregated summary of cartography scan results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CartographySummary {
    pub hosts_scanned: usize,
    pub hosts_up: usize,
    pub open_ports_count: usize,
}

/// Why a subnet was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CidrError {
    /// Not an IPv4 address and a prefix length.
    Invalid,
    /// A prefix wider than [`MIN_PREFIX`] or narrower than [`MAX_PREFIX`].
    PrefixOutOfRange,
}

/// Classifies a host's primary role based on its set of open TCP ports.
#[must_use]
pub fn classify_role(open_ports: &[u16]) -> NodeRole {
    let has = |port: u16| open_ports.contains(&port);

    if has(88) || has(389) || has(636) {
        return NodeRole::Directory;
    }

    if has(1433) || has(3306) || has(5432) || has(1521) || has(27017) || has(6379) {
        return NodeRole::Database;
    }

    if has(25) || has(465) || has(587) || has(110) || has(143) || has(993) || has(995) {
        return NodeRole::Mail;
    }

    if has(445) || has(139) || has(2049) {
        return NodeRole::FileStorage;
    }

    if has(80) || has(443) || has(8080) || has(8443) {
        return NodeRole::Web;
    }

    if has(22) || has(3389) || has(5900) || has(5985) || has(5986) {
        return NodeRole::RemoteManagement;
    }

    if has(53) || has(67) || has(161) {
        return NodeRole::NetworkAppliance;
    }

    if open_ports.is_empty() {
        NodeRole::NoPortsOpen
    } else {
        NodeRole::ActiveNode
    }
}

/// The subnet typed, trimmed, given [`DEFAULT_PREFIX`] when it has none, as the C#
/// `NormalizeSubnet`.
#[must_use]
pub fn normalize_subnet(typed: &str) -> String {
    let trimmed = typed.trim();
    if trimmed.is_empty() || trimmed.contains('/') {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/{DEFAULT_PREFIX}")
    }
}

/// Expands a CIDR subnet notation (e.g. "192.168.1.0/24") into its hosts, network and
/// broadcast addresses left out, as the C# `ParseCidr`.
///
/// # Errors
///
/// [`CidrError`] when it is no IPv4 subnet, or one outside `/16` to `/30`.
pub fn expand_subnet_ips(cidr: &str) -> Result<Vec<Ipv4Addr>, CidrError> {
    let (ip_str, prefix_str) = cidr.trim().split_once('/').ok_or(CidrError::Invalid)?;
    let ip: Ipv4Addr = ip_str.trim().parse().map_err(|_| CidrError::Invalid)?;
    let prefix: u32 = prefix_str.trim().parse().map_err(|_| CidrError::Invalid)?;
    if !(MIN_PREFIX..=MAX_PREFIX).contains(&prefix) {
        return Err(CidrError::PrefixOutOfRange);
    }

    let mask = u32::MAX << (32 - prefix);
    let net = u32::from(ip) & mask;
    let broadcast = net | !mask;
    Ok((net + 1..broadcast).map(Ipv4Addr::from).collect())
}

/// Builds CSV export content for discovered nodes under the column names `header` (IP,
/// hostname, MAC, vendor, open ports, role, response time), each role said by
/// `role_label`; every text cell sanitized against formulas and quoted.
#[must_use]
pub fn build_nodes_csv(
    nodes: &[DiscoveredNode],
    header: [&str; 7],
    role_label: impl Fn(NodeRole) -> String,
) -> String {
    let mut out = header.map(escape_csv).join(",");
    out.push('\n');

    for n in nodes {
        let ports = n
            .open_ports
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(";");

        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            escape_csv(&n.ip),
            escape_csv(n.hostname.as_deref().unwrap_or_default()),
            escape_csv(n.mac.as_deref().unwrap_or_default()),
            escape_csv(n.vendor.as_deref().unwrap_or_default()),
            escape_csv(&ports),
            escape_csv(&role_label(n.role)),
            n.response_time_ms
        ));
    }

    out
}

/// What probing one port of one host found: its open port and how fast, if it is open.
struct PortProbe {
    ip: Ipv4Addr,
    open: Option<(u16, u64)>,
}

/// What is known of a host while its ports are probed.
#[derive(Default)]
struct HostProgress {
    probed: usize,
    open_ports: Vec<u16>,
    fastest_ms: Option<u64>,
}

/// Probes the [`QUICK_RECON_PORTS`] of every host of `ips`, [`MAX_CONCURRENCY`] probes at a
/// time, each waiting `timeout_ms` at most; `on_host` is told of every host once all its
/// ports are probed, with the node found or `None` when no port answered. Once `stop` is set
/// no probe starts, and hosts not fully probed are not told.
pub fn discover(
    ips: &[Ipv4Addr],
    timeout_ms: u64,
    stop: &AtomicBool,
    mut on_host: impl FnMut(Option<DiscoveredNode>),
) {
    let timeout = Duration::from_millis(timeout_ms);
    // Host after host, so each host's ports end close together and it is shown soon.
    let jobs: Vec<(Ipv4Addr, u16)> = ips
        .iter()
        .flat_map(|ip| QUICK_RECON_PORTS.iter().map(move |port| (*ip, *port)))
        .collect();
    let mut progress: HashMap<Ipv4Addr, HostProgress> = HashMap::new();
    net_command::for_each_bounded(
        jobs,
        MAX_CONCURRENCY,
        stop,
        |(ip, port)| {
            let start = Instant::now();
            let open = TcpStream::connect_timeout(&SocketAddr::from((ip, port)), timeout)
                .is_ok()
                .then(|| {
                    let elapsed = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                    (port, elapsed)
                });
            PortProbe { ip, open }
        },
        |probe| {
            let host = progress.entry(probe.ip).or_default();
            host.probed += 1;
            if let Some((port, elapsed)) = probe.open {
                host.open_ports.push(port);
                host.fastest_ms = Some(host.fastest_ms.map_or(elapsed, |ms| ms.min(elapsed)));
            }
            if host.probed < QUICK_RECON_PORTS.len() {
                return;
            }
            let Some(mut done) = progress.remove(&probe.ip) else {
                return;
            };
            if done.open_ports.is_empty() {
                on_host(None);
                return;
            }
            done.open_ports.sort_unstable();
            on_host(Some(DiscoveredNode {
                ip: probe.ip.to_string(),
                hostname: None,
                mac: None,
                vendor: None,
                role: classify_role(&done.open_ports),
                open_ports: done.open_ports,
                response_time_ms: done.fastest_ms.unwrap_or_default(),
            }));
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_classification_heuristics() {
        assert_eq!(classify_role(&[80, 443]), NodeRole::Web);
        assert_eq!(classify_role(&[389, 88]), NodeRole::Directory);
        assert_eq!(classify_role(&[5432]), NodeRole::Database);
        assert_eq!(classify_role(&[22, 3389]), NodeRole::RemoteManagement);
        assert_eq!(classify_role(&[445]), NodeRole::FileStorage);
        assert_eq!(classify_role(&[9999]), NodeRole::ActiveNode);
        assert_eq!(classify_role(&[]), NodeRole::NoPortsOpen);
    }

    #[test]
    fn expand_subnet_ips_valid_slash_29() {
        let ips = expand_subnet_ips("192.168.1.0/29").unwrap();
        assert_eq!(ips.len(), 6); // 8 - network - broadcast = 6
        assert_eq!(ips[0], Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(ips[5], Ipv4Addr::new(192, 168, 1, 6));
    }

    #[test]
    fn the_scope_is_the_csharp_one_from_slash_16_to_slash_30() {
        assert_eq!(expand_subnet_ips("10.0.0.0/16").expect("16").len(), 65_534);
        assert_eq!(expand_subnet_ips("10.0.0.0/30").expect("30").len(), 2);
        assert_eq!(
            expand_subnet_ips("10.0.0.0/15"),
            Err(CidrError::PrefixOutOfRange)
        );
        assert_eq!(
            expand_subnet_ips("10.0.0.0/31"),
            Err(CidrError::PrefixOutOfRange)
        );
        assert_eq!(
            expand_subnet_ips("10.0.0.0/0"),
            Err(CidrError::PrefixOutOfRange)
        );
        assert_eq!(
            expand_subnet_ips("10.0.0.0/99"),
            Err(CidrError::PrefixOutOfRange)
        );
        assert_eq!(expand_subnet_ips("10.0.0.0"), Err(CidrError::Invalid));
        assert_eq!(expand_subnet_ips("host/24"), Err(CidrError::Invalid));
        assert_eq!(expand_subnet_ips("10.0.0.0/x"), Err(CidrError::Invalid));
        assert_eq!(normalize_subnet(" 10.1.2.0 "), "10.1.2.0/24");
        assert_eq!(normalize_subnet("10.1.0.0/16"), "10.1.0.0/16");
        assert_eq!(normalize_subnet("  "), "");
        assert!(
            expand_subnet_ips("10.0.0.0/22").expect("22").len() > LARGE_SUBNET_THRESHOLD,
            "a /22 asks first"
        );
        assert!(expand_subnet_ips("10.0.0.0/23").expect("23").len() <= LARGE_SUBNET_THRESHOLD);
    }

    #[test]
    fn a_host_is_told_once_all_its_ports_are_probed_and_a_stop_starts_no_more() {
        // Port 22 or 80 of the loopback may be open on the machine running the tests: only
        // that each host is told once is checked here.
        let ips = [Ipv4Addr::LOCALHOST, Ipv4Addr::new(127, 0, 0, 2)];
        let stop = AtomicBool::new(false);
        let mut told = 0;
        discover(&ips, 500, &stop, |_| told += 1);
        assert_eq!(told, 2, "every host told once");
        let stop = AtomicBool::new(true);
        let mut told = 0;
        discover(&ips, 500, &stop, |_| told += 1);
        assert_eq!(told, 0);
    }

    #[test]
    fn nodes_csv_export() {
        let nodes = vec![DiscoveredNode {
            ip: "192.168.1.10".to_owned(),
            hostname: Some("server01".to_owned()),
            mac: Some("00-50-56-11-22-33".to_owned()),
            vendor: Some("VMware".to_owned()),
            open_ports: vec![80, 443],
            role: NodeRole::Web,
            response_time_ms: 3,
        }];

        let csv = build_nodes_csv(&nodes, ["IP", "H", "M", "V", "P", "R", "T"], |role| {
            format!("{role:?}")
        });
        assert!(csv.starts_with("\"IP\",\"H\",\"M\",\"V\",\"P\",\"R\",\"T\"\n"));
        assert!(csv.contains(
            "\"192.168.1.10\",\"server01\",\"00-50-56-11-22-33\",\"VMware\",\"80;443\",\"Web\",3"
        ));
    }
}
