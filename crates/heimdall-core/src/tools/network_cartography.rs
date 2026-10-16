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
//! Provides subnet host expansion within the C# scope (`/16` to `/30`), host discovery a
//! bounded number of probes at a time with a stop (a ping sweep through the system's `ping`
//! program, the hosts the ARP table already knows, port probes), the MAC address of each
//! host from the ARP table read again after the sweep and its vendor, its name by reverse
//! DNS, role classification, and CSV export.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::csv_cell::quoted;
use super::mac_vendor::{self, MacVendor};
use super::{arp_monitor, dns_lookup, net_host, ping, probe_pool, system_program};

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
/// How long an echo request waits for its reply, as the C# `PingSweepAsync`
/// (`Math.Min(profile.TimeoutMs, 1000)`).
pub const PING_TIMEOUT_MS: u64 = 1000;
/// How long a reverse lookup of a host's name may take, as a DNS tool's lookup.
pub const REVERSE_DNS_DEADLINE_MS: u64 = dns_lookup::LOOKUP_TIMEOUT_MS;
/// What ends the name a Unix `nslookup` gives an address: `5.0.0.10.in-addr.arpa
/// name = host.lan.`.
const UNIX_NAME_MARK: &str = "name = ";
/// The zone of the reverse names, never a host's own.
const REVERSE_ZONE: &str = ".in-addr.arpa";

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
    /// Its name, by reverse DNS, as the C# `Dns.GetHostEntryAsync`.
    pub hostname: Option<String>,
    /// Its MAC address, from the local ARP table.
    pub mac: Option<String>,
    /// The vendor its MAC address names.
    pub vendor: Option<MacVendor>,
    pub open_ports: Vec<u16>,
    pub role: NodeRole,
    /// Its echo reply's round trip, else its fastest open port's connection.
    pub response_time_ms: u64,
}

/// What a discovery is told of a host before it starts, from the knowledge base, as the C#
/// `ScanAsync` reads its `NetworkKnowledgeBase`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prior {
    /// Everything about it is fresh: shown as it is, never probed, as the C# cache hit
    /// "all".
    pub reuse: Option<DiscoveredNode>,
    /// It answered recently, with this round trip: not pinged again, as the C# cache hit
    /// "ping".
    pub alive_ms: Option<u64>,
    /// Its name, when the scan finds none, as the C# backfills it.
    pub hostname: Option<String>,
    /// Its MAC address, when the ARP table has none.
    pub mac: Option<String>,
}

/// How a discovery runs, as the C# `ScanProfile` options it keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoverOptions {
    /// Look up the name of every host up, as the C# `ReverseDns`, on by default.
    pub reverse_dns: bool,
}

impl Default for DiscoverOptions {
    fn default() -> Self {
        Self { reverse_dns: true }
    }
}

/// What a discovery tells as it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryEvent {
    /// A host was probed: the node found, `None` when it is not up.
    Host(Option<DiscoveredNode>),
    /// The sweep is over: the ARP table is read again and the names of `.0` hosts looked up.
    Naming(usize),
    /// What the sweep's end found of a host told before: its name and its MAC address.
    Details {
        ip: String,
        hostname: Option<String>,
        mac: Option<String>,
        vendor: Option<MacVendor>,
    },
}

/// How a discovery reaches the network, apart from what it makes of the answers, so the
/// tests can answer for it.
pub trait Prober: Sync {
    /// The round trip of an echo request to `ip`, `None` when it got no reply.
    fn ping(&self, ip: Ipv4Addr, stop: &AtomicBool) -> Option<u64>;
    /// How fast `port` of `ip` accepted a connection, `None` when it did not.
    fn connect(&self, ip: Ipv4Addr, port: u16) -> Option<u64>;
    /// The local ARP table, IP and MAC address pairs; empty when it cannot be read.
    fn arp_table(&self, stop: &AtomicBool) -> Vec<(String, String)>;
    /// The name `ip` resolves back to, `None` when it has none.
    fn reverse_name(&self, ip: Ipv4Addr, stop: &AtomicBool) -> Option<String>;
}

/// The network as the system reaches it: the system's `ping` and `nslookup` programs by
/// their whole path, as the PING and DNS tools run them, the ARP table as the ARP monitor
/// reads it, a TCP connection for a port. Nothing needs to be run as administrator.
#[derive(Debug, Clone)]
pub struct SystemProber {
    ping: Option<PathBuf>,
    nslookup: Option<PathBuf>,
    connect_timeout: Duration,
}

impl SystemProber {
    /// The system's probes, a connection waiting `connect_timeout_ms` at most.
    #[must_use]
    pub fn new(connect_timeout_ms: u64) -> Self {
        Self {
            ping: ping::ping_program(),
            nslookup: dns_lookup::nslookup_program(),
            connect_timeout: Duration::from_millis(connect_timeout_ms),
        }
    }
}

/// The milliseconds of `elapsed`, as a whole number.
fn whole_ms(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

/// Every line `program` prints when run with `args`, ended past `deadline` or once `stop` is
/// set.
fn program_output(
    program: &std::path::Path,
    args: &[&str],
    deadline: Duration,
    stop: &AtomicBool,
) -> String {
    let mut output = String::new();
    // How it ended changes nothing: what it printed, if anything, is what is read.
    let _ = system_program::run_lines(program, args, deadline, stop, |line| {
        output.push_str(line);
        output.push('\n');
    });
    output
}

impl Prober for SystemProber {
    fn ping(&self, ip: Ipv4Addr, stop: &AtomicBool) -> Option<u64> {
        let program = self.ping.as_deref()?;
        let arguments = ping::ping_arguments(IpAddr::V4(ip), PING_TIMEOUT_MS);
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        let deadline = Duration::from_millis(PING_TIMEOUT_MS + ping::PROGRAM_GRACE_MS);
        let reply = ping::parse_reply(&program_output(program, &arguments, deadline, stop))?;
        echo_from(&reply, ip)
    }

    fn connect(&self, ip: Ipv4Addr, port: u16) -> Option<u64> {
        let start = Instant::now();
        TcpStream::connect_timeout(&SocketAddr::from((ip, port)), self.connect_timeout)
            .ok()
            .map(|_| whole_ms(start.elapsed()))
    }

    fn arp_table(&self, stop: &AtomicBool) -> Vec<(String, String)> {
        arp_monitor::read_system_arp_table(stop).unwrap_or_default()
    }

    fn reverse_name(&self, ip: Ipv4Addr, stop: &AtomicBool) -> Option<String> {
        let program = self.nslookup.as_deref()?;
        let address = ip.to_string();
        let deadline = Duration::from_millis(REVERSE_DNS_DEADLINE_MS);
        let output = program_output(program, &[address.as_str()], deadline, stop);
        parse_reverse_name(&output, ip, cfg!(windows))
    }
}

/// The round trip of `reply` when `ip` sent it: a router answering for a host it cannot
/// reach is no echo of the host.
#[must_use]
pub fn echo_from(reply: &ping::PingReply, ip: Ipv4Addr) -> Option<u64> {
    if reply
        .address
        .is_some_and(|address| address != IpAddr::V4(ip))
    {
        return None;
    }
    // A round trip below a millisecond (`time<1ms`) is a whole millisecond's at most.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(reply.latency_ms.max(0.0).round() as u64)
}

/// The name `nslookup` gives `ip` in `output`, as the C# reverse lookup keeps it: a real
/// host name, never the address echoed back nor a reverse zone name. Unix `nslookup`
/// writes `... name = host.lan.`; Windows' (`windows`) writes the answer in the paragraph
/// after the server's, its first labelled line holding a name (`Name:`, `Nom :`), whatever
/// the language of its label. Only an RFC 1123 host name is kept, so nothing the DNS
/// server says can be shown as anything but a name.
#[must_use]
pub fn parse_reverse_name(output: &str, ip: Ipv4Addr, windows: bool) -> Option<String> {
    let usable = |value: &str| {
        let value = value.trim().trim_end_matches('.');
        let name = net_host::checked_host(value)?;
        let reverse = name.to_ascii_lowercase().ends_with(REVERSE_ZONE);
        (name.parse::<IpAddr>().is_err() && !reverse && name != ip.to_string()).then_some(name)
    };
    if !windows {
        return output.lines().find_map(|line| {
            let (_, name) = line.split_once(UNIX_NAME_MARK)?;
            usable(name)
        });
    }
    let lines: Vec<&str> = output
        .lines()
        .map(str::trim)
        .skip_while(|line| line.is_empty())
        .collect();
    // The server's paragraph first, up to its blank line: its name is not the answer.
    let answer = lines.iter().position(|line| line.is_empty())?;
    lines[answer..].iter().find_map(|line| {
        let (label, value) = line.split_once(':')?;
        let label = label.trim();
        if label.is_empty() || label.contains(char::is_whitespace) || label.contains('*') {
            return None;
        }
        usable(value)
    })
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

/// The subnet `cidr` written plainly, its network address and its prefix (`10.0.0.0/24` for
/// `10.0.0.9 / 24`), as the scan history names its snapshots; `None` when it is none.
#[must_use]
pub fn canonical_subnet(cidr: &str) -> Option<String> {
    let (ip_str, prefix_str) = cidr.trim().split_once('/')?;
    let ip: Ipv4Addr = ip_str.trim().parse().ok()?;
    let prefix: u32 = prefix_str.trim().parse().ok()?;
    let mask = u32::MAX
        .checked_shl(32_u32.checked_sub(prefix)?)
        .unwrap_or(0);
    Some(format!("{}/{prefix}", Ipv4Addr::from(u32::from(ip) & mask)))
}

/// Builds CSV export content for discovered nodes under the column names `header` (IP,
/// hostname, MAC, vendor, open ports, role, response time), each vendor said by
/// `vendor_label` and each role by `role_label`; every text cell sanitized against formulas
/// and quoted.
#[must_use]
pub fn build_nodes_csv(
    nodes: &[DiscoveredNode],
    header: [&str; 7],
    vendor_label: impl Fn(Option<MacVendor>) -> String,
    role_label: impl Fn(NodeRole) -> String,
) -> String {
    let mut out = header.map(quoted).join(",");
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
            quoted(&n.ip),
            quoted(n.hostname.as_deref().unwrap_or_default()),
            quoted(n.mac.as_deref().unwrap_or_default()),
            quoted(&vendor_label(n.vendor)),
            quoted(&ports),
            quoted(&role_label(n.role)),
            n.response_time_ms
        ));
    }

    out
}

/// One probe of a host.
#[derive(Debug, Clone, Copy)]
enum Probe {
    Ping(Ipv4Addr),
    Port(Ipv4Addr, u16),
}

/// What one probe found: an echo's or an open port's time, `None` when nothing answered.
struct Probed {
    probe: Probe,
    answer_ms: Option<u64>,
}

/// What is known of a host while it is probed.
#[derive(Default)]
struct HostProgress {
    probed: usize,
    echo_ms: Option<u64>,
    open_ports: Vec<u16>,
    fastest_ms: Option<u64>,
}

/// The MAC address the ARP table `table` gives each address of `ips`.
fn arp_macs(table: Vec<(String, String)>, ips: &HashSet<Ipv4Addr>) -> HashMap<Ipv4Addr, String> {
    table
        .into_iter()
        .filter_map(|(ip, mac)| {
            let ip: Ipv4Addr = ip.parse().ok()?;
            ips.contains(&ip).then_some((ip, mac))
        })
        .collect()
}

/// A node's MAC address set to `mac`, its vendor with it.
fn set_mac(node: &mut DiscoveredNode, mac: Option<String>) {
    node.vendor = mac.as_deref().and_then(mac_vendor::lookup);
    node.mac = mac;
}

/// Discovers the hosts of `ips`, as the C# `CartographyEngine.ScanAsync` with its quick
/// profile: the ARP table read first, its hosts up whether or not they answer; an echo
/// request to every host and its [`QUICK_RECON_PORTS`] probed, [`MAX_CONCURRENCY`] probes
/// at a time, host after host; a host is up when it answers the echo, is in the ARP table,
/// has an open port, or its `priors` say it answered recently. `on_event` is told of every
/// host once probed. Once the sweep is over, the ARP table is read again for the MAC
/// addresses the probes taught it, and the name of every host up looked up when `options`
/// say so, as many at a time; what they find is told for each host it changes. A host whose
/// `priors` say all is fresh is told as they say, never probed. Once `stop` is set no probe
/// starts and the programs running are ended. Returns the nodes found, by address.
pub fn discover(
    ips: &[Ipv4Addr],
    priors: &HashMap<Ipv4Addr, Prior>,
    options: DiscoverOptions,
    prober: &impl Prober,
    stop: &AtomicBool,
    mut on_event: impl FnMut(DiscoveryEvent),
) -> Vec<DiscoveredNode> {
    let wanted: HashSet<Ipv4Addr> = ips.iter().copied().collect();
    let seeds = arp_macs(prober.arp_table(stop), &wanted);
    let mut found: Vec<DiscoveredNode> = Vec::new();
    let mut reused: HashSet<String> = HashSet::new();
    let mut jobs = Vec::new();
    for ip in ips {
        let prior = priors.get(ip);
        if let Some(node) = prior.and_then(|prior| prior.reuse.clone()) {
            on_event(DiscoveryEvent::Host(Some(node.clone())));
            reused.insert(node.ip.clone());
            found.push(node);
            continue;
        }
        if prior.and_then(|prior| prior.alive_ms).is_none() {
            jobs.push(Probe::Ping(*ip));
        }
        jobs.extend(QUICK_RECON_PORTS.iter().map(|port| Probe::Port(*ip, *port)));
    }
    let mut progress: HashMap<Ipv4Addr, HostProgress> = HashMap::new();
    probe_pool::for_each_bounded(
        jobs,
        MAX_CONCURRENCY,
        stop,
        |probe| {
            let answer_ms = match probe {
                Probe::Ping(ip) => prober.ping(ip, stop),
                Probe::Port(ip, port) => prober.connect(ip, port),
            };
            Probed { probe, answer_ms }
        },
        |probed| {
            let ip = match probed.probe {
                Probe::Ping(ip) | Probe::Port(ip, _) => ip,
            };
            let host = progress.entry(ip).or_default();
            host.probed += 1;
            match (probed.probe, probed.answer_ms) {
                (Probe::Ping(_), answer) => host.echo_ms = answer,
                (Probe::Port(_, port), Some(ms)) => {
                    host.open_ports.push(port);
                    host.fastest_ms = Some(host.fastest_ms.map_or(ms, |fastest| fastest.min(ms)));
                }
                (Probe::Port(..), None) => {}
            }
            let prior = priors.get(&ip);
            let alive_ms = prior.and_then(|prior| prior.alive_ms);
            let expected = QUICK_RECON_PORTS.len() + usize::from(alive_ms.is_none());
            if host.probed < expected {
                return;
            }
            let Some(mut done) = progress.remove(&ip) else {
                return;
            };
            let seed = seeds.get(&ip).cloned();
            let up = done.echo_ms.is_some()
                || alive_ms.is_some()
                || seed.is_some()
                || !done.open_ports.is_empty();
            if !up {
                on_event(DiscoveryEvent::Host(None));
                return;
            }
            done.open_ports.sort_unstable();
            let mut node = DiscoveredNode {
                ip: ip.to_string(),
                hostname: prior.and_then(|prior| prior.hostname.clone()),
                mac: None,
                vendor: None,
                role: classify_role(&done.open_ports),
                open_ports: done.open_ports,
                response_time_ms: done
                    .echo_ms
                    .or(alive_ms)
                    .or(done.fastest_ms)
                    .unwrap_or_default(),
            };
            set_mac(
                &mut node,
                seed.or_else(|| prior.and_then(|prior| prior.mac.clone())),
            );
            on_event(DiscoveryEvent::Host(Some(node.clone())));
            found.push(node);
        },
    );
    found.sort_by_key(|node| node.ip.parse::<Ipv4Addr>().ok());
    if stop.load(Ordering::Relaxed) {
        return found;
    }

    // The probes taught the ARP table the MAC addresses of the hosts that answered, as the
    // C# reads it again after its scan.
    let fresh = arp_macs(prober.arp_table(stop), &wanted);
    // A host told as the knowledge base knows it keeps its name, as the C# cache hit.
    let to_name: Vec<usize> = (0..found.len())
        .filter(|at| options.reverse_dns && !reused.contains(&found[*at].ip))
        .collect();
    on_event(DiscoveryEvent::Naming(to_name.len()));
    let mut names: HashMap<usize, String> = HashMap::new();
    probe_pool::for_each_bounded(
        to_name,
        MAX_CONCURRENCY,
        stop,
        |at| {
            let name = found[at]
                .ip
                .parse::<Ipv4Addr>()
                .ok()
                .and_then(|ip| prober.reverse_name(ip, stop));
            (at, name)
        },
        |(at, name)| {
            if let Some(name) = name {
                names.insert(at, name);
            }
        },
    );
    for (at, node) in found.iter_mut().enumerate() {
        let ip = node.ip.parse::<Ipv4Addr>().ok();
        let mac = ip
            .and_then(|ip| fresh.get(&ip).cloned())
            .or_else(|| node.mac.clone());
        let hostname = names.remove(&at).or_else(|| node.hostname.clone());
        if mac == node.mac && hostname == node.hostname {
            continue;
        }
        node.hostname = hostname;
        set_mac(node, mac);
        on_event(DiscoveryEvent::Details {
            ip: node.ip.clone(),
            hostname: node.hostname.clone(),
            mac: node.mac.clone(),
            vendor: node.vendor,
        });
    }
    found
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
        assert_eq!(
            canonical_subnet(" 10.1.2.9 / 24").as_deref(),
            Some("10.1.2.0/24")
        );
        assert_eq!(
            canonical_subnet("10.1.2.9/32").as_deref(),
            Some("10.1.2.9/32")
        );
        assert_eq!(canonical_subnet("10.1.2.9/0").as_deref(), Some("0.0.0.0/0"));
        assert_eq!(canonical_subnet("10.1.2.9/33"), None);
        assert_eq!(canonical_subnet("x/24"), None);
        assert!(
            expand_subnet_ips("10.0.0.0/22").expect("22").len() > LARGE_SUBNET_THRESHOLD,
            "a /22 asks first"
        );
        assert!(expand_subnet_ips("10.0.0.0/23").expect("23").len() <= LARGE_SUBNET_THRESHOLD);
    }

    /// A network answering as told: the echoes, the open ports and their times, the ARP
    /// tables read one after the other, the names; every probe counted.
    #[derive(Default)]
    struct FakeNetwork {
        echoes: HashMap<Ipv4Addr, u64>,
        open: HashMap<(Ipv4Addr, u16), u64>,
        arp_reads: std::sync::Mutex<Vec<Vec<(String, String)>>>,
        names: HashMap<Ipv4Addr, String>,
        pinged: std::sync::Mutex<Vec<Ipv4Addr>>,
        connects: std::sync::atomic::AtomicUsize,
        named: std::sync::Mutex<Vec<Ipv4Addr>>,
        /// Sets the stop at the first echo request.
        stop_at_ping: Option<std::sync::Arc<AtomicBool>>,
    }

    impl Prober for FakeNetwork {
        fn ping(&self, ip: Ipv4Addr, _: &AtomicBool) -> Option<u64> {
            self.pinged.lock().expect("lock").push(ip);
            if let Some(stop) = &self.stop_at_ping {
                stop.store(true, Ordering::SeqCst);
            }
            self.echoes.get(&ip).copied()
        }

        fn connect(&self, ip: Ipv4Addr, port: u16) -> Option<u64> {
            self.connects.fetch_add(1, Ordering::SeqCst);
            self.open.get(&(ip, port)).copied()
        }

        fn arp_table(&self, _: &AtomicBool) -> Vec<(String, String)> {
            let mut reads = self.arp_reads.lock().expect("lock");
            if reads.is_empty() {
                Vec::new()
            } else {
                reads.remove(0)
            }
        }

        fn reverse_name(&self, ip: Ipv4Addr, _: &AtomicBool) -> Option<String> {
            self.named.lock().expect("lock").push(ip);
            self.names.get(&ip).cloned()
        }
    }

    fn ip(last: u8) -> Ipv4Addr {
        Ipv4Addr::new(10, 0, 0, last)
    }

    fn arp(rows: &[(u8, &str)]) -> Vec<(String, String)> {
        rows.iter()
            .map(|(last, mac)| (ip(*last).to_string(), (*mac).to_owned()))
            .collect()
    }

    #[test]
    fn a_sweep_finds_hosts_by_echo_arp_and_ports_then_names_and_macs_them() {
        let mut network = FakeNetwork::default();
        // .1 answers the echo; .2 only a port; .3 only is in the ARP table; .4 is silent.
        network.echoes.insert(ip(1), 3);
        network.open.insert((ip(2), 443), 7);
        network.open.insert((ip(2), 22), 9);
        network.arp_reads = std::sync::Mutex::new(vec![
            arp(&[(3, "B8-27-EB-00-00-03"), (99, "00-50-56-00-00-99")]),
            arp(&[(1, "00-50-56-00-00-01"), (3, "B8-27-EB-00-00-03")]),
        ]);
        network.names.insert(ip(1), "gw.lab".to_owned());
        let ips = [ip(4), ip(3), ip(2), ip(1)];
        let stop = AtomicBool::new(false);
        let mut events = Vec::new();
        let found = discover(
            &ips,
            &HashMap::new(),
            DiscoverOptions::default(),
            &network,
            &stop,
            |event| events.push(event),
        );

        let told: Vec<Option<String>> = events
            .iter()
            .filter_map(|event| match event {
                DiscoveryEvent::Host(node) => Some(node.as_ref().map(|n| n.ip.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(told.len(), 4, "every host told once");
        assert_eq!(told.iter().filter(|node| node.is_none()).count(), 1);
        assert_eq!(
            network.pinged.lock().expect("lock").len(),
            4,
            "every host pinged"
        );
        assert_eq!(
            network.connects.load(Ordering::SeqCst),
            4 * QUICK_RECON_PORTS.len()
        );
        assert!(events.contains(&DiscoveryEvent::Naming(3)));

        let shown: Vec<&str> = found.iter().map(|n| n.ip.as_str()).collect();
        assert_eq!(shown, ["10.0.0.1", "10.0.0.2", "10.0.0.3"], "by address");
        let (one, two, three) = (&found[0], &found[1], &found[2]);
        assert_eq!(one.response_time_ms, 3, "the echo's round trip");
        assert_eq!(one.role, NodeRole::NoPortsOpen);
        assert_eq!(one.hostname.as_deref(), Some("gw.lab"));
        assert_eq!(
            one.mac.as_deref(),
            Some("00-50-56-00-00-01"),
            "from the ARP table read again"
        );
        assert_eq!(one.vendor, Some(MacVendor::Named("VMware")));
        assert_eq!(two.open_ports, [22, 443]);
        assert_eq!(two.role, NodeRole::Web);
        assert_eq!(two.response_time_ms, 7, "its fastest port without an echo");
        assert_eq!(two.mac, None);
        assert_eq!(three.mac.as_deref(), Some("B8-27-EB-00-00-03"), "seeded");
        assert_eq!(three.vendor, Some(MacVendor::Named("Raspberry Pi")));
        assert!(
            events.contains(&DiscoveryEvent::Details {
                ip: "10.0.0.1".to_owned(),
                hostname: Some("gw.lab".to_owned()),
                mac: Some("00-50-56-00-00-01".to_owned()),
                vendor: Some(MacVendor::Named("VMware")),
            }),
            "{events:?}"
        );
        assert!(
            !events.iter().any(
                |event| matches!(event, DiscoveryEvent::Details { ip, .. } if ip == "10.0.0.3")
            ),
            "nothing new of .3: not told again"
        );
    }

    #[test]
    fn the_knowledge_base_skips_what_is_fresh_and_fills_what_is_missing() {
        let mut network = FakeNetwork::default();
        network.open.insert((ip(2), 80), 4);
        let cached = DiscoveredNode {
            ip: ip(1).to_string(),
            hostname: Some("cached.lab".to_owned()),
            mac: None,
            vendor: None,
            open_ports: vec![22],
            role: NodeRole::RemoteManagement,
            response_time_ms: 2,
        };
        let mut priors = HashMap::new();
        priors.insert(
            ip(1),
            Prior {
                reuse: Some(cached.clone()),
                ..Prior::default()
            },
        );
        priors.insert(
            ip(2),
            Prior {
                alive_ms: Some(5),
                hostname: Some("known.lab".to_owned()),
                mac: Some("00-50-56-00-00-02".to_owned()),
                ..Prior::default()
            },
        );
        let stop = AtomicBool::new(false);
        let found = discover(
            &[ip(1), ip(2)],
            &priors,
            DiscoverOptions { reverse_dns: false },
            &network,
            &stop,
            |_| {},
        );
        assert!(
            network.pinged.lock().expect("lock").is_empty(),
            "both known up"
        );
        assert_eq!(
            network.connects.load(Ordering::SeqCst),
            QUICK_RECON_PORTS.len(),
            ".1 is never probed"
        );
        assert!(network.named.lock().expect("lock").is_empty(), "no lookup");
        assert_eq!(found[0], cached);
        assert_eq!(found[1].open_ports, [80]);
        assert_eq!(found[1].response_time_ms, 5, "the round trip known");
        assert_eq!(found[1].hostname.as_deref(), Some("known.lab"));
        assert_eq!(found[1].mac.as_deref(), Some("00-50-56-00-00-02"));
    }

    #[test]
    fn a_stop_starts_no_more_probe_and_looks_up_no_name() {
        let ips: Vec<Ipv4Addr> = (1..=200).map(ip).collect();
        let network = FakeNetwork::default();
        let stopped = AtomicBool::new(true);
        let found = discover(
            &ips,
            &HashMap::new(),
            DiscoverOptions::default(),
            &network,
            &stopped,
            |_| {},
        );
        assert!(found.is_empty());
        assert!(network.pinged.lock().expect("lock").is_empty());
        assert_eq!(network.connects.load(Ordering::SeqCst), 0);

        // Stopped during the sweep: no name is looked up after it.
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let mut network = FakeNetwork {
            stop_at_ping: Some(std::sync::Arc::clone(&stop)),
            ..FakeNetwork::default()
        };
        network.echoes.insert(ip(1), 1);
        discover(
            &ips,
            &HashMap::new(),
            DiscoverOptions::default(),
            &network,
            &stop,
            |_| {},
        );
        let probes =
            network.pinged.lock().expect("lock").len() + network.connects.load(Ordering::SeqCst);
        assert!(
            probes < ips.len() * (QUICK_RECON_PORTS.len() + 1),
            "{probes}"
        );
        assert!(network.named.lock().expect("lock").is_empty());
    }

    #[test]
    fn an_echo_is_the_host_s_own_not_a_router_s() {
        let reply = |address: Option<IpAddr>, latency_ms| ping::PingReply {
            latency_ms,
            ttl: Some(64),
            address,
        };
        assert_eq!(
            echo_from(&reply(Some(IpAddr::V4(ip(1))), 0.4), ip(1)),
            Some(0)
        );
        assert_eq!(
            echo_from(&reply(Some(IpAddr::V4(ip(1))), 12.6), ip(1)),
            Some(13)
        );
        assert_eq!(echo_from(&reply(None, 3.0), ip(1)), Some(3));
        assert_eq!(
            echo_from(&reply(Some(IpAddr::V4(ip(254))), 1.0), ip(1)),
            None
        );
    }

    #[test]
    fn a_reverse_name_is_read_from_nslookup_in_any_language() {
        let unix = "Server:\t\t127.0.0.53\nAddress:\t127.0.0.53#53\n\nNon-authoritative answer:\n5.0.0.10.in-addr.arpa\tname = host.lab.\n\nAuthoritative answers can be found from:\n";
        assert_eq!(
            parse_reverse_name(unix, ip(5), false).as_deref(),
            Some("host.lab")
        );
        let unix_none = "Server:\t\t127.0.0.53\nAddress:\t127.0.0.53#53\n\n** server can't find 5.0.0.10.in-addr.arpa: NXDOMAIN\n";
        assert_eq!(parse_reverse_name(unix_none, ip(5), false), None);
        let english = "Server:  dns.lab\r\nAddress:  10.0.0.254\r\n\r\nName:    host.lab\r\nAddress:  10.0.0.5\r\n\r\n";
        assert_eq!(
            parse_reverse_name(english, ip(5), true).as_deref(),
            Some("host.lab")
        );
        let french =
            "Serveur :   UnKnown\nAddress:  10.0.0.254\n\nNom :    host.lab\nAddress:  10.0.0.5\n";
        assert_eq!(
            parse_reverse_name(french, ip(5), true).as_deref(),
            Some("host.lab")
        );
        // No answer: only the server's paragraph, whose name is never taken.
        let none = "Server:  dns.lab\nAddress:  10.0.0.254\n\n";
        assert_eq!(parse_reverse_name(none, ip(5), true), None);
        assert_eq!(parse_reverse_name("Server:  dns.lab\n", ip(5), true), None);
        // What is no host name is never shown as one.
        let odd = "Server:  dns.lab\n\nName:    -x \u{202e}evil\nName:    10.0.0.5\nName:    5.0.0.10.in-addr.arpa\n";
        assert_eq!(parse_reverse_name(odd, ip(5), true), None);
        assert_eq!(parse_reverse_name("x name = 10.0.0.5", ip(5), false), None);
    }

    #[test]
    fn nodes_csv_export() {
        let nodes = vec![DiscoveredNode {
            ip: "192.168.1.10".to_owned(),
            hostname: Some("server01".to_owned()),
            mac: Some("00-50-56-11-22-33".to_owned()),
            vendor: Some(MacVendor::Named("VMware")),
            open_ports: vec![80, 443],
            role: NodeRole::Web,
            response_time_ms: 3,
        }];

        let csv = build_nodes_csv(
            &nodes,
            ["IP", "H", "M", "V", "P", "R", "T"],
            |vendor| match vendor {
                Some(MacVendor::Named(name)) => name.to_owned(),
                _ => String::new(),
            },
            |role| format!("{role:?}"),
        );
        assert!(csv.starts_with("\"IP\",\"H\",\"M\",\"V\",\"P\",\"R\",\"T\"\n"));
        assert!(csv.contains(
            "\"192.168.1.10\",\"server01\",\"00-50-56-11-22-33\",\"VMware\",\"80;443\",\"Web\",3"
        ));
    }
}
