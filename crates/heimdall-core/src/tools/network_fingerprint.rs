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

//! What network cartography learns of a host beyond its ports, as the C#
//! `CartographyEngine.EnrichHostAsync` collects it: the NetBIOS names (`UdpProbeEngine
//! .QueryNetBiosAsync`), the SNMP system group (`QuerySnmpAsync`), the NTLM and SMB2
//! negotiate data (`NtlmProbe.ProbeWithSmbInfoAsync`), the SSH banner and HASSH
//! (`SshFingerprinter.ComputeHashAsync`), the TLS certificate subject, and the operating
//! system guessed from all of it (`OsFingerprinter`).
//!
//! Every probe only reads: nothing is authenticated, no credential is sent. Every probe is
//! bounded in time and in bytes, and every reply is read as hostile: a length is checked
//! before it is used, and a reply that does not decode is set aside, never a panic.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use data_encoding::HEXLOWER;
use regex::Regex;
use sealvault::hash::{Algorithm, digest};

use super::smb::{self, NtlmInfo, SmbNegotiateInfo};
use super::snmp_walker;

/// The NetBIOS name service port, as the C# `NetBiosPort`.
pub const NETBIOS_PORT: u16 = 137;
/// How long the NBSTAT query waits for its answer, as the C# `QueryNetBiosAsync(ip, 1500)`.
pub const NETBIOS_TIMEOUT_MS: u64 = 1500;
/// How long the SNMP query waits for each answer, as the C# `QuerySnmpAsync(ip, 2000)`.
pub const SNMP_TIMEOUT_MS: u64 = 2000;
/// The budget of each TCP probe, as the C# `Math.Min(profile.TimeoutMs, 3000)`.
pub const TCP_PROBE_TIMEOUT_MS: u64 = 3000;
/// The SMB port.
pub const SMB_PORT: u16 = 445;
/// The SSH port.
pub const SSH_PORT: u16 = 22;
/// The HTTPS port whose certificate is read.
pub const TLS_PORT: u16 = 443;
/// The most bytes of a datagram read: an NBSTAT answer is far shorter.
const MAX_DATAGRAM: usize = 1500;
/// How often a wait for a datagram looks at the stop.
const POLL_INTERVAL_MS: u64 = 100;
/// The community tried, as the C# `QuerySnmpAsync`.
const SNMP_COMMUNITY: &str = "public";
/// The system group, whose entries are sysDescr, sysObjectID, sysUpTime, sysContact,
/// sysName and sysLocation.
const SNMP_SYSTEM_SUBTREE: &str = "1.3.6.1.2.1.1";
/// The entries of the system group read.
const SNMP_SYSTEM_ENTRIES: usize = 6;
const SNMP_SYS_DESCR: &str = "1.3.6.1.2.1.1.1.0";
const SNMP_SYS_NAME: &str = "1.3.6.1.2.1.1.5.0";
const SNMP_SYS_LOCATION: &str = "1.3.6.1.2.1.1.6.0";
/// The identification line sent, as the C# `SshFingerprinter`.
const SSH_CLIENT_BANNER: &[u8] = b"SSH-2.0-Heimdall_Scanner\r\n";
/// The most bytes read before the server's identification line, which RFC 4253 lets be
/// preceded by other lines.
const MAX_SSH_PREAMBLE_BYTES: usize = 4096;
/// The longest identification line, RFC 4253 section 4.2.
const MAX_SSH_BANNER_BYTES: usize = 255;
/// The longest SSH packet read, as the C# `ReadSshPacketAsync`.
const MAX_SSH_PACKET_BYTES: usize = 65_536;
/// The SSH message number of a key exchange initialisation.
const SSH_MSG_KEXINIT: u8 = 20;
/// The message number byte and the cookie before the name lists of a KEXINIT.
const SSH_KEXINIT_HEADER_BYTES: usize = 17;
/// The SMB2 negotiate reply's smallest size, as the C# probe checks it.
const SMB_NEGOTIATE_MIN_BYTES: usize = 68;
/// Where the security buffer's offset and length sit in a session setup reply.
const SMB_SECURITY_FIELDS_AT: usize = 68;
/// The most characters of any text kept from a reply.
pub const MAX_FIELD_CHARS: usize = 256;
/// The confidence an agreement of several sources cannot exceed, as the C#
/// `Math.Min(95, ...)`.
const MAX_CONFIDENCE: u8 = 95;

/// What a NetBIOS NBSTAT answer names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetbiosReply {
    pub name: Option<String>,
    pub domain: Option<String>,
    pub mac: Option<String>,
}

/// What the SNMP system group tells, as the C# `SnmpInfo`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnmpSystem {
    pub descr: Option<String>,
    pub name: Option<String>,
    pub location: Option<String>,
}

/// The operating system a host is guessed to run, as the C# `OsFingerprint`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsGuess {
    pub os: String,
    /// Where the guess comes from: `Banner`, `Ports`, `SNMP`, `NTLM`, `TLS`, joined by `+`
    /// when several agree.
    pub source: String,
    /// From 0 to 100.
    pub confidence: u8,
}

/// What the probes of one host gave.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawProbes {
    pub netbios: Option<NetbiosReply>,
    pub snmp: Option<SnmpSystem>,
    pub ntlm: Option<NtlmInfo>,
    pub smb: Option<SmbNegotiateInfo>,
    pub ssh_banner: Option<String>,
    pub ssh_hassh: Option<String>,
    pub tls_subject: Option<String>,
}

/// Everything learned of a host, cleaned for display.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fingerprint {
    pub netbios_name: Option<String>,
    pub netbios_domain: Option<String>,
    /// The MAC address the NBSTAT answer gives, a fallback to the ARP table's.
    pub netbios_mac: Option<String>,
    pub snmp: Option<SnmpSystem>,
    pub ntlm: Option<NtlmInfo>,
    pub smb: Option<SmbNegotiateInfo>,
    pub ssh_banner: Option<String>,
    /// The HASSH of the server's key exchange initialisation, lower-case hexadecimal.
    pub ssh_hassh: Option<String>,
    pub tls_subject: Option<String>,
    pub os: Option<OsGuess>,
}

impl Fingerprint {
    /// Whether nothing was learned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// `text` without control characters, at most [`MAX_FIELD_CHARS`] long; `None` when
/// nothing is left.
#[must_use]
pub fn clean(text: &str) -> Option<String> {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_FIELD_CHARS)
        .collect();
    let cleaned = cleaned.trim();
    (!cleaned.is_empty()).then(|| cleaned.to_owned())
}

fn clean_option(text: Option<&String>) -> Option<String> {
    text.and_then(|text| clean(text))
}

// ── Operating system guesses ───────────────────────────────────────────────

/// Banner patterns, as the C# `BannerPatterns` that an SSH banner or a certificate subject
/// can match: pattern, system, confidence.
const BANNER_PATTERNS: [(&str, &str, u8); 24] = [
    (r"SSH-.*Ubuntu", "Ubuntu Linux", 85),
    (r"SSH-.*Debian", "Debian Linux", 85),
    (r"SSH-.*FreeBSD", "FreeBSD", 85),
    (r"SSH-.*CentOS", "CentOS Linux", 85),
    (r"SSH-.*Red\s?Hat", "Red Hat Linux", 85),
    (r"SSH-.*Fedora", "Fedora Linux", 80),
    (r"SSH-.*SUSE", "SUSE Linux", 80),
    (r"SSH-.*Arch", "Arch Linux", 80),
    (r"SSH-.*Raspbian", "Raspbian (Raspberry Pi)", 85),
    (r"SSH-.*OpenWrt", "OpenWrt", 90),
    (r"SSH-.*Cisco", "Cisco IOS", 90),
    (r"SSH-.*dropbear", "Linux (Embedded)", 70),
    (r"SSH-.*libssh", "Linux", 55),
    (r"RouterOS", "MikroTik RouterOS", 90),
    (r"FortiOS", "Fortinet FortiOS", 90),
    (r"pfSense", "FreeBSD (pfSense)", 90),
    (r"OPNsense", "FreeBSD (OPNsense)", 90),
    (r"DD-WRT", "Linux (DD-WRT)", 90),
    (r"Synology", "Linux (Synology DSM)", 85),
    (r"QNAP", "Linux (QNAP QTS)", 85),
    (r"FreeNAS|TrueNAS", "FreeBSD (TrueNAS)", 85),
    (r"ESXi", "VMware ESXi", 90),
    (r"Proxmox", "Debian Linux (Proxmox)", 90),
    (r"Microsoft-IIS", "Windows Server", 80),
];

/// The compiled [`BANNER_PATTERNS`], case-insensitive as the C#.
static BANNERS: LazyLock<Vec<(Regex, &'static str, u8)>> = LazyLock::new(|| {
    BANNER_PATTERNS
        .iter()
        .map(|(pattern, os, confidence)| {
            (
                Regex::new(&format!("(?i){pattern}")).expect("valid regex"),
                *os,
                *confidence,
            )
        })
        .collect()
});

/// SNMP system description patterns, as the C# `SnmpOsPatterns`.
const SNMP_PATTERNS: [(&str, &str, u8); 19] = [
    ("VMware ESXi", "VMware ESXi", 90),
    ("Cisco IOS", "Cisco IOS", 90),
    ("Cisco Adaptive", "Cisco ASA", 90),
    ("Juniper", "Juniper JUNOS", 85),
    ("FortiOS", "Fortinet FortiOS", 90),
    ("RouterOS", "MikroTik RouterOS", 90),
    ("Ubuntu", "Ubuntu Linux", 85),
    ("Debian", "Debian Linux", 85),
    ("CentOS", "CentOS Linux", 85),
    ("Red Hat", "Red Hat Linux", 85),
    ("SUSE", "SUSE Linux", 80),
    ("Linux", "Linux", 75),
    ("Microsoft Windows", "Windows", 85),
    ("Windows", "Windows", 80),
    ("FreeBSD", "FreeBSD", 80),
    ("HP ETHERNET", "HP Printer Firmware", 75),
    ("RICOH", "Ricoh Printer Firmware", 75),
    ("APC", "APC UPS Firmware", 75),
    ("Eaton", "Eaton UPS Firmware", 75),
];

/// The best guess `text` gives against the banner patterns, `source` naming where it was
/// read.
#[must_use]
pub fn guess_from_banner(text: &str, source: &str) -> Option<OsGuess> {
    BANNERS
        .iter()
        .filter(|(pattern, ..)| pattern.is_match(text))
        .max_by_key(|(_, _, confidence)| *confidence)
        .map(|(_, os, confidence)| OsGuess {
            os: (*os).to_owned(),
            source: source.to_owned(),
            confidence: *confidence,
        })
}

/// The guess an SNMP system description gives, as the C# `GuessFromSnmp`.
#[must_use]
pub fn guess_from_snmp(descr: &str) -> Option<OsGuess> {
    let lower = descr.to_lowercase();
    SNMP_PATTERNS
        .iter()
        .filter(|(pattern, ..)| lower.contains(&pattern.to_lowercase()))
        .max_by_key(|(_, _, confidence)| *confidence)
        .map(|(_, os, confidence)| OsGuess {
            os: (*os).to_owned(),
            source: "SNMP".to_owned(),
            confidence: *confidence,
        })
}

/// The guess the open ports give, as the C# `GuessFromPorts`.
#[must_use]
pub fn guess_from_ports(open_ports: &[u16]) -> Option<OsGuess> {
    let has = |port: u16| open_ports.contains(&port);
    let (os, confidence) = if has(88) && has(389) {
        ("Windows Server", 70)
    } else if has(5985) || has(5986) {
        ("Windows", 65)
    } else if has(3389) {
        ("Windows", 60)
    } else if has(445) && has(135) {
        ("Windows", 55)
    } else if has(22) && !has(3389) && !has(445) && !has(135) && !has(5985) {
        ("Linux", 40)
    } else {
        return None;
    };
    Some(OsGuess {
        os: os.to_owned(),
        source: "Ports".to_owned(),
        confidence,
    })
}

/// The guess an NTLM version (`major.minor.build`) gives, as the C# `GuessFromNtlm`.
#[must_use]
pub fn guess_from_ntlm(os_build: &str) -> Option<OsGuess> {
    let mut parts = os_build.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let _minor = parts.next()?;
    let build: u32 = parts.next()?.parse().ok()?;
    let os = match (major, build) {
        (10, 22000..) => "Windows 11 / Server 2022+",
        (10, 20348..) => "Windows Server 2022",
        (10, 19041..) => "Windows 10 / Server 2019",
        (10, 17763..) => "Windows 10 1809 / Server 2019",
        (10, 14393..) => "Windows 10 / Server 2016",
        (10, _) => "Windows 10",
        (6, 9600..) => "Windows 8.1 / Server 2012 R2",
        (6, 9200..) => "Windows 8 / Server 2012",
        (6, 7601..) => "Windows 7 SP1 / Server 2008 R2",
        (6, _) => "Windows Vista / Server 2008",
        (5, _) => "Windows XP / Server 2003",
        _ => return None,
    };
    Some(OsGuess {
        os: format!("{os} (Build {build})"),
        source: "NTLM".to_owned(),
        confidence: 90,
    })
}

/// The family a system belongs to, so agreeing sources can be told, as the C#
/// `IsSameOsFamily`.
fn family(os: &str) -> Option<u8> {
    let lower = os.to_lowercase();
    let any = |names: &[&str]| names.iter().any(|name| lower.contains(name));
    if any(&["windows"]) {
        Some(0)
    } else if any(&[
        "linux", "ubuntu", "debian", "centos", "red hat", "fedora", "raspbian", "macos",
        "synology", "qnap", "proxmox", "openwrt", "dd-wrt",
    ]) {
        Some(1)
    } else if any(&["freebsd", "pfsense", "opnsense", "truenas"]) {
        Some(2)
    } else if any(&[
        "network equipment",
        "cisco",
        "mikrotik",
        "fortinet",
        "juniper",
        "routeros",
    ]) {
        Some(3)
    } else if any(&["printer", "ups", "firmware", "embedded"]) {
        Some(4)
    } else {
        None
    }
}

/// The best of `candidates`, its confidence raised when several agree, as the C# `MergeAll`:
/// 10 for the first corroboration and 5 for each further source, at most
/// [`MAX_CONFIDENCE`].
#[must_use]
pub fn merge_guesses(candidates: Vec<OsGuess>) -> Option<OsGuess> {
    let best = candidates
        .iter()
        .max_by_key(|candidate| candidate.confidence)?
        .clone();
    if candidates.len() == 1 {
        return Some(best);
    }
    let best_family = family(&best.os);
    let agreeing: Vec<&OsGuess> = candidates
        .iter()
        .filter(|candidate| best_family.is_some() && family(&candidate.os) == best_family)
        .collect();
    if agreeing.len() < 2 {
        return Some(best);
    }
    let boost = 10 + u8::try_from((agreeing.len() - 2) * 5).unwrap_or(u8::MAX);
    let mut sources: Vec<&str> = Vec::new();
    for candidate in &agreeing {
        if !sources.contains(&candidate.source.as_str()) {
            sources.push(candidate.source.as_str());
        }
    }
    Some(OsGuess {
        os: best.os,
        source: sources.join("+"),
        confidence: best.confidence.saturating_add(boost).min(MAX_CONFIDENCE),
    })
}

/// What `raw` and the open ports tell of a host, each text cleaned, the NTLM names filling
/// the NetBIOS ones the NBSTAT answer lacks (NTLM is the more reliable, as the C# notes),
/// and the system guessed from every source.
#[must_use]
pub fn assemble(open_ports: &[u16], raw: RawProbes) -> Fingerprint {
    let ntlm = raw.ntlm.map(|ntlm| NtlmInfo {
        nb_computer: clean_option(ntlm.nb_computer.as_ref()),
        nb_domain: clean_option(ntlm.nb_domain.as_ref()),
        dns_computer: clean_option(ntlm.dns_computer.as_ref()),
        dns_domain: clean_option(ntlm.dns_domain.as_ref()),
        dns_forest: clean_option(ntlm.dns_forest.as_ref()),
        os_build: clean_option(ntlm.os_build.as_ref()),
    });
    let netbios = raw.netbios.unwrap_or_default();
    let netbios_name = clean_option(netbios.name.as_ref())
        .or_else(|| ntlm.as_ref().and_then(|ntlm| ntlm.nb_computer.clone()));
    let netbios_domain = clean_option(netbios.domain.as_ref())
        .or_else(|| ntlm.as_ref().and_then(|ntlm| ntlm.nb_domain.clone()));
    let snmp = raw
        .snmp
        .map(|snmp| SnmpSystem {
            descr: clean_option(snmp.descr.as_ref()),
            name: clean_option(snmp.name.as_ref()),
            location: clean_option(snmp.location.as_ref()),
        })
        .filter(|snmp| *snmp != SnmpSystem::default());
    let ssh_banner = clean_option(raw.ssh_banner.as_ref());
    let tls_subject = clean_option(raw.tls_subject.as_ref());

    let mut guesses = Vec::new();
    guesses.extend(
        ssh_banner
            .as_deref()
            .and_then(|banner| guess_from_banner(banner, "Banner")),
    );
    guesses.extend(
        tls_subject
            .as_deref()
            .and_then(|subject| guess_from_banner(subject, "TLS")),
    );
    guesses.extend(guess_from_ports(open_ports));
    guesses.extend(
        snmp.as_ref()
            .and_then(|snmp| snmp.descr.as_deref())
            .and_then(guess_from_snmp),
    );
    guesses.extend(
        ntlm.as_ref()
            .and_then(|ntlm| ntlm.os_build.as_deref())
            .and_then(guess_from_ntlm),
    );

    Fingerprint {
        netbios_name,
        netbios_domain,
        netbios_mac: clean_option(netbios.mac.as_ref()),
        snmp,
        ntlm,
        smb: raw.smb,
        ssh_banner,
        ssh_hassh: raw.ssh_hassh.filter(|hash| !hash.is_empty()),
        tls_subject,
        os: merge_guesses(guesses),
    }
}

// ── Probes ─────────────────────────────────────────────────────────────────

/// The time left until `deadline`, `None` once it has passed.
fn time_left(deadline: Instant) -> Option<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
}

/// Reads `buf` full from `stream` before `deadline`, `None` when the peer ends, errs, or is
/// too slow: a peer sending a byte at a time cannot hold the probe past it.
fn read_full(stream: &mut TcpStream, buf: &mut [u8], deadline: Instant) -> Option<()> {
    let mut filled = 0;
    while filled < buf.len() {
        stream.set_read_timeout(Some(time_left(deadline)?)).ok()?;
        match stream.read(&mut buf[filled..]) {
            Ok(0) | Err(_) => return None,
            Ok(read) => filled += read,
        }
    }
    Some(())
}

/// A TCP connection to `ip`:`port` before `deadline`, writes bounded too.
fn connect(ip: Ipv4Addr, port: u16, deadline: Instant) -> Option<TcpStream> {
    let stream =
        TcpStream::connect_timeout(&SocketAddr::from((ip, port)), time_left(deadline)?).ok()?;
    stream.set_write_timeout(time_left(deadline)).ok()?;
    Some(stream)
}

/// A random transaction number; when the generator fails, zero, which the answer check
/// still matches.
fn transaction_id() -> u16 {
    sealvault::random::array::<2>().map_or(0, u16::from_be_bytes)
}

/// Asks UDP `port` of `ip` with `query`, and returns what `accept` makes of the first
/// datagram it accepts, within `timeout_ms`; datagrams from elsewhere are not read, the
/// socket being connected to the host.
fn ask_udp<T>(
    ip: Ipv4Addr,
    port: u16,
    query: &[u8],
    timeout_ms: u64,
    stop: &AtomicBool,
    accept: impl Fn(&[u8]) -> Option<T>,
) -> Option<T> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((ip, port)).ok()?;
    socket
        .set_read_timeout(Some(Duration::from_millis(POLL_INTERVAL_MS)))
        .ok()?;
    socket.send(query).ok()?;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut buffer = vec![0u8; MAX_DATAGRAM];
    while time_left(deadline).is_some() && !stop.load(Ordering::Relaxed) {
        // An error is the poll's own timeout, or an ICMP refusal Windows reports on the
        // next read: waited out as a silence.
        if let Ok(len) = socket.recv(&mut buffer)
            && let Some(found) = buffer.get(..len).and_then(&accept)
        {
            return Some(found);
        }
    }
    None
}

/// The NetBIOS names `ip` gives to a wildcard NBSTAT query, as the C# `QueryNetBiosAsync`.
#[must_use]
pub fn query_netbios(ip: Ipv4Addr, stop: &AtomicBool) -> Option<NetbiosReply> {
    let id = transaction_id();
    let query = smb::build_netbios_nbstat_query(id);
    ask_udp(
        ip,
        NETBIOS_PORT,
        &query,
        NETBIOS_TIMEOUT_MS,
        stop,
        |reply| {
            // An answer to another query is no answer to this one.
            if reply.get(..2)? != id.to_be_bytes() {
                return None;
            }
            let (name, domain, mac) = smb::parse_netbios_response(reply);
            (name.is_some() || domain.is_some() || mac.is_some()).then_some(NetbiosReply {
                name,
                domain,
                mac,
            })
        },
    )
}

/// The system group of the SNMPv2c agent of `ip` under the community `public`, as the C#
/// `QuerySnmpAsync`; `None` when it does not answer.
#[must_use]
pub fn query_snmp(ip: Ipv4Addr, stop: &AtomicBool) -> Option<SnmpSystem> {
    let mut system = SnmpSystem::default();
    let host = ip.to_string();
    // A silence or an error ends the walk; what was read before is kept.
    let _ = snmp_walker::walk(
        &host,
        snmp_walker::DEFAULT_PORT,
        SNMP_COMMUNITY,
        SNMP_SYSTEM_SUBTREE,
        SNMP_SYSTEM_ENTRIES,
        SNMP_TIMEOUT_MS,
        stop,
        |entry| match entry.oid.as_str() {
            SNMP_SYS_DESCR => system.descr = Some(entry.value),
            SNMP_SYS_NAME => system.name = Some(entry.value),
            SNMP_SYS_LOCATION => system.location = Some(entry.value),
            _ => {}
        },
    );
    (system != SnmpSystem::default()).then_some(system)
}

/// One NetBIOS session message read from `stream`, at most what its header bounds.
fn read_netbios_message(stream: &mut TcpStream, deadline: Instant) -> Option<Vec<u8>> {
    let mut header = [0u8; 4];
    read_full(stream, &mut header, deadline)?;
    let mut message = vec![0u8; smb::read_netbios_length(&header)?];
    read_full(stream, &mut message, deadline)?;
    Some(message)
}

/// A NetBIOS session message sent on `stream`.
fn send_netbios_message(stream: &mut TcpStream, body: &[u8]) -> Option<()> {
    stream
        .write_all(&smb::build_netbios_header(body.len()))
        .ok()?;
    stream.write_all(body).ok()?;
    stream.flush().ok()
}

/// The SMB2 negotiate data and the NTLM challenge of the server at `ip`:`445`, as the C#
/// `NtlmProbe.ProbeWithSmbInfoAsync`: a negotiate, then a session setup carrying an NTLMSSP
/// negotiate message, whose challenge names the machine. The exchange stops there: no
/// authenticate message is ever sent.
#[must_use]
pub fn probe_smb(ip: Ipv4Addr, stop: &AtomicBool) -> (Option<NtlmInfo>, Option<SmbNegotiateInfo>) {
    let deadline = Instant::now() + Duration::from_millis(TCP_PROBE_TIMEOUT_MS);
    let Some(mut stream) = connect(ip, SMB_PORT, deadline) else {
        return (None, None);
    };
    if stop.load(Ordering::Relaxed)
        || send_netbios_message(&mut stream, &smb::build_smb2_negotiate()).is_none()
    {
        return (None, None);
    }
    let Some(negotiate) = read_netbios_message(&mut stream, deadline) else {
        return (None, None);
    };
    if negotiate.len() < SMB_NEGOTIATE_MIN_BYTES || negotiate.get(..4) != Some(&smb::SMB2_MAGIC) {
        return (None, None);
    }
    let smb_info = smb::parse_negotiate_response(&negotiate);

    let setup = smb::build_smb2_session_setup(&smb::wrap_in_spnego(&smb::build_ntlmssp_type1()));
    if stop.load(Ordering::Relaxed) || send_netbios_message(&mut stream, &setup).is_none() {
        return (None, smb_info);
    }
    let ntlm = read_netbios_message(&mut stream, deadline).and_then(|reply| {
        security_buffer(&reply)
            .and_then(smb::find_ntlmssp_message)
            .and_then(smb::parse_ntlm_type2)
    });
    (ntlm, smb_info)
}

/// The security buffer of an SMB2 session setup reply, `None` when the offsets it gives
/// leave the reply.
#[must_use]
pub fn security_buffer(reply: &[u8]) -> Option<&[u8]> {
    let fields = reply.get(SMB_SECURITY_FIELDS_AT..SMB_SECURITY_FIELDS_AT + 4)?;
    let offset = usize::from(u16::from_le_bytes([fields[0], fields[1]]));
    let len = usize::from(u16::from_le_bytes([fields[2], fields[3]]));
    reply.get(offset..offset.checked_add(len)?)
}

/// The server's identification line: a line starting with `SSH-`, at most
/// [`MAX_SSH_BANNER_BYTES`] long, within [`MAX_SSH_PREAMBLE_BYTES`] of the start.
fn read_ssh_banner(stream: &mut TcpStream, deadline: Instant) -> Option<String> {
    let mut line: Vec<u8> = Vec::new();
    for _ in 0..MAX_SSH_PREAMBLE_BYTES {
        let mut byte = [0u8; 1];
        read_full(stream, &mut byte, deadline)?;
        if byte[0] != b'\n' {
            line.push(byte[0]);
            if line.len() > MAX_SSH_BANNER_BYTES {
                return None;
            }
            continue;
        }
        if line.starts_with(b"SSH-") {
            return Some(String::from_utf8_lossy(&line).trim_end().to_owned());
        }
        line.clear();
    }
    None
}

/// The payload of an SSH binary packet whose bytes after the length are `packet`: the
/// padding length byte, the payload, the padding.
#[must_use]
pub fn ssh_packet_payload(packet: &[u8]) -> Option<&[u8]> {
    let padding = usize::from(*packet.first()?);
    let end = packet.len().checked_sub(padding)?;
    packet.get(1..end).filter(|payload| !payload.is_empty())
}

/// The name list at `offset` of `data` (a length, then that many ASCII bytes), `offset`
/// moved past it.
fn read_name_list<'a>(data: &'a [u8], offset: &mut usize) -> Option<&'a str> {
    let length_end = offset.checked_add(4)?;
    let length = u32::from_be_bytes(data.get(*offset..length_end)?.try_into().ok()?);
    let end = length_end.checked_add(usize::try_from(length).ok()?)?;
    let list = std::str::from_utf8(data.get(length_end..end)?).ok()?;
    *offset = end;
    Some(list)
}

/// The HASSH of a key exchange initialisation `payload`, the MD5 of the key exchange
/// algorithms and the server to client encryption, MAC and compression, joined by `;`, as
/// the C# `SshFingerprinter`; `None` when it is no well-formed initialisation.
#[must_use]
pub fn hassh(payload: &[u8]) -> Option<String> {
    if payload.first() != Some(&SSH_MSG_KEXINIT) || payload.len() < SSH_KEXINIT_HEADER_BYTES {
        return None;
    }
    let mut offset = SSH_KEXINIT_HEADER_BYTES;
    let kex = read_name_list(payload, &mut offset)?;
    let _host_keys = read_name_list(payload, &mut offset)?;
    let _encryption_to_server = read_name_list(payload, &mut offset)?;
    let encryption = read_name_list(payload, &mut offset)?;
    let _mac_to_server = read_name_list(payload, &mut offset)?;
    let mac = read_name_list(payload, &mut offset)?;
    let _compression_to_server = read_name_list(payload, &mut offset)?;
    let compression = read_name_list(payload, &mut offset)?;
    let joined = format!("{kex};{encryption};{mac};{compression}");
    Some(HEXLOWER.encode(digest(Algorithm::Md5, joined.as_bytes()).as_bytes()))
}

/// The identification line and the HASSH of the SSH server at `ip`:`port`, as the C#
/// `SshFingerprinter.ComputeHashAsync`: the server's line is read, ours sent, and the key
/// exchange initialisation the server answers with is read. The exchange stops there: no key
/// is exchanged and nothing is authenticated.
#[must_use]
pub fn probe_ssh(ip: Ipv4Addr, port: u16, stop: &AtomicBool) -> (Option<String>, Option<String>) {
    let deadline = Instant::now() + Duration::from_millis(TCP_PROBE_TIMEOUT_MS);
    let Some(mut stream) = connect(ip, port, deadline) else {
        return (None, None);
    };
    let Some(banner) = read_ssh_banner(&mut stream, deadline) else {
        return (None, None);
    };
    if stop.load(Ordering::Relaxed) || stream.write_all(SSH_CLIENT_BANNER).is_err() {
        return (Some(banner), None);
    }
    let mut length = [0u8; 4];
    let hash = read_full(&mut stream, &mut length, deadline)
        .and_then(|()| usize::try_from(u32::from_be_bytes(length)).ok())
        .filter(|length| (1..=MAX_SSH_PACKET_BYTES).contains(length))
        .and_then(|length| {
            let mut packet = vec![0u8; length];
            read_full(&mut stream, &mut packet, deadline)?;
            hassh(ssh_packet_payload(&packet)?)
        });
    (Some(banner), hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name_list(out: &mut Vec<u8>, list: &str) {
        out.extend_from_slice(&u32::try_from(list.len()).unwrap().to_be_bytes());
        out.extend_from_slice(list.as_bytes());
    }

    fn kex_init() -> Vec<u8> {
        let mut payload = vec![SSH_MSG_KEXINIT];
        payload.extend_from_slice(&[0u8; 16]);
        for list in [
            "curve25519-sha256",
            "ssh-ed25519",
            "aes128-ctr",
            "aes256-ctr",
            "hmac-sha2-256",
            "hmac-sha2-512",
            "none",
            "zlib",
        ] {
            name_list(&mut payload, list);
        }
        payload
    }

    #[test]
    fn hassh_is_the_md5_of_the_server_to_client_lists() {
        let expected = HEXLOWER.encode(
            digest(
                Algorithm::Md5,
                b"curve25519-sha256;aes256-ctr;hmac-sha2-512;zlib",
            )
            .as_bytes(),
        );
        assert_eq!(hassh(&kex_init()), Some(expected));
    }

    #[test]
    fn a_malformed_key_exchange_initialisation_gives_nothing() {
        let good = kex_init();
        assert_eq!(hassh(&[]), None);
        assert_eq!(hassh(&[SSH_MSG_KEXINIT]), None);
        // Every truncation is refused, none panics.
        for end in 0..good.len() {
            assert_eq!(hassh(&good[..end]), None, "{end}");
        }
        // A name list longer than the packet.
        let mut lying = vec![SSH_MSG_KEXINIT];
        lying.extend_from_slice(&[0u8; 16]);
        lying.extend_from_slice(&u32::MAX.to_be_bytes());
        lying.extend_from_slice(b"abc");
        assert_eq!(hassh(&lying), None);
        // Another message.
        let mut other = good;
        other[0] = 21;
        assert_eq!(hassh(&other), None);
    }

    #[test]
    fn an_ssh_packet_gives_its_payload_without_the_padding() {
        assert_eq!(ssh_packet_payload(&[2, 20, 1, 9, 9]), Some(&[20, 1][..]));
        assert_eq!(ssh_packet_payload(&[]), None);
        // Padding longer than the packet, or nothing but padding.
        assert_eq!(ssh_packet_payload(&[9, 1, 2]), None);
        assert_eq!(ssh_packet_payload(&[2, 9, 9]), None);
    }

    #[test]
    fn a_security_buffer_must_lie_inside_the_reply() {
        let mut reply = vec![0u8; 80];
        reply[68..70].copy_from_slice(&72u16.to_le_bytes());
        reply[70..72].copy_from_slice(&8u16.to_le_bytes());
        assert_eq!(security_buffer(&reply).map(<[u8]>::len), Some(8));
        reply[70..72].copy_from_slice(&9u16.to_le_bytes());
        assert_eq!(security_buffer(&reply), None);
        reply[68..70].copy_from_slice(&u16::MAX.to_le_bytes());
        reply[70..72].copy_from_slice(&u16::MAX.to_le_bytes());
        assert_eq!(security_buffer(&reply), None);
        assert_eq!(security_buffer(&reply[..60]), None);
    }

    #[test]
    fn the_nbstat_query_carries_the_transaction_id_the_answer_is_checked_against() {
        let query = smb::build_netbios_nbstat_query(0xABCD);
        assert_eq!(query[..2], 0xABCDu16.to_be_bytes());
    }

    #[test]
    fn hostile_netbios_answers_never_panic() {
        for len in 0..80 {
            for fill in [0x00u8, 0x7F, 0xFF, 0xC0] {
                let _ = smb::parse_netbios_response(&vec![fill; len]);
            }
        }
    }

    #[test]
    fn text_from_the_network_is_cleaned_and_bounded() {
        assert_eq!(
            clean("  host\u{0}\u{7}name\n "),
            Some("hostname".to_owned())
        );
        assert_eq!(clean("\u{0}\u{1}  "), None);
        let long = "a".repeat(MAX_FIELD_CHARS * 2);
        assert_eq!(
            clean(&long).map(|text| text.chars().count()),
            Some(MAX_FIELD_CHARS)
        );
    }

    #[test]
    fn systems_are_guessed_from_banners_ports_snmp_and_ntlm() {
        let ubuntu = guess_from_banner("SSH-2.0-OpenSSH_8.9p1 Ubuntu-3", "Banner").unwrap();
        assert_eq!(
            (ubuntu.os.as_str(), ubuntu.confidence),
            ("Ubuntu Linux", 85)
        );
        assert_eq!(guess_from_banner("SSH-2.0-OpenSSH_9.0", "Banner"), None);
        let appliance = guess_from_banner("CN=pfsense.lab, O=pfSense", "TLS").unwrap();
        assert_eq!(appliance.os, "FreeBSD (pfSense)");

        assert_eq!(guess_from_ports(&[88, 389]).unwrap().os, "Windows Server");
        assert_eq!(guess_from_ports(&[3389]).unwrap().confidence, 60);
        assert_eq!(guess_from_ports(&[22]).unwrap().os, "Linux");
        assert_eq!(guess_from_ports(&[22, 445, 135]).unwrap().os, "Windows");
        assert_eq!(guess_from_ports(&[]), None);

        assert_eq!(
            guess_from_snmp("Linux router 5.10 Debian").unwrap().os,
            "Debian Linux"
        );
        assert_eq!(guess_from_snmp(""), None);

        assert_eq!(
            guess_from_ntlm("10.0.20348").unwrap().os,
            "Windows Server 2022 (Build 20348)"
        );
        assert_eq!(guess_from_ntlm("10.0"), None);
        assert_eq!(guess_from_ntlm("x.y.z"), None);
        assert_eq!(guess_from_ntlm("4.0.1"), None);
    }

    #[test]
    fn agreeing_sources_raise_the_confidence_up_to_a_ceiling() {
        let guess = |os: &str, source: &str, confidence| OsGuess {
            os: os.to_owned(),
            source: source.to_owned(),
            confidence,
        };
        assert_eq!(merge_guesses(Vec::new()), None);
        let alone = guess("Linux", "Ports", 40);
        assert_eq!(merge_guesses(vec![alone.clone()]), Some(alone));
        let merged = merge_guesses(vec![
            guess("Linux", "Ports", 40),
            guess("Ubuntu Linux", "Banner", 85),
        ])
        .unwrap();
        assert_eq!(
            (
                merged.os.as_str(),
                merged.source.as_str(),
                merged.confidence
            ),
            ("Ubuntu Linux", "Ports+Banner", 95)
        );
        let disagreeing = merge_guesses(vec![
            guess("Windows", "Ports", 60),
            guess("Ubuntu Linux", "Banner", 85),
        ])
        .unwrap();
        assert_eq!(
            (disagreeing.os.as_str(), disagreeing.confidence),
            ("Ubuntu Linux", 85)
        );
    }

    #[test]
    fn a_fingerprint_is_assembled_from_every_probe() {
        let raw = RawProbes {
            netbios: Some(NetbiosReply {
                name: Some("SRV\u{7}01".to_owned()),
                domain: None,
                mac: Some("00-50-56-00-00-10".to_owned()),
            }),
            snmp: Some(SnmpSystem {
                descr: Some("Linux nas 5.10".to_owned()),
                ..SnmpSystem::default()
            }),
            ntlm: Some(NtlmInfo {
                nb_computer: Some("OTHER".to_owned()),
                nb_domain: Some("LAB".to_owned()),
                os_build: Some("10.0.17763".to_owned()),
                ..NtlmInfo::default()
            }),
            ssh_banner: Some("SSH-2.0-OpenSSH_8.9p1 Ubuntu-3\r".to_owned()),
            ssh_hassh: Some("abc".to_owned()),
            ..RawProbes::default()
        };
        let fingerprint = assemble(&[22, 445], raw);
        // The NBSTAT name wins, the NTLM domain fills the one it lacks.
        assert_eq!(fingerprint.netbios_name.as_deref(), Some("SRV01"));
        assert_eq!(fingerprint.netbios_domain.as_deref(), Some("LAB"));
        assert_eq!(
            fingerprint.ssh_banner.as_deref(),
            Some("SSH-2.0-OpenSSH_8.9p1 Ubuntu-3")
        );
        assert!(fingerprint.os.is_some());
        assert!(!fingerprint.is_empty());
        assert!(assemble(&[], RawProbes::default()).is_empty());
    }
}
