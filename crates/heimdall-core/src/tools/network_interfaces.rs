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

//! The local network interfaces, as the C# `NetworkInterfacesService` and `NicFormatter`.
//!
//! The C# reads `NetworkInterface.GetAllNetworkInterfaces`: on Windows, Windows
//! `PowerShell` reads the very same .NET call and writes it as JSON, so the link speed and
//! the operational status are the system's, never guessed; a speed the system does not know
//! stays unknown. On Linux, `ip -j` and `/sys/class/net` say the same facts.

use serde::Deserialize;

use super::system_listing::{self, one_or_many, text_or_null};
use super::system_program::{self, ProgramError};

/// Reads every interface through the call the C# makes, written as JSON: the interface type
/// and the operational status as their .NET numbers, the speed in bits per second (-1 when
/// unknown), the first IPv4 address, its mask and the first IPv4 gateway, as the C#
/// `ToSnapshot`. No double quote: see [`system_program::run_powershell`].
#[cfg(windows)]
const LISTING_SCRIPT: &str = "$rows = @(foreach ($nic in \
     [System.Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces()) { \
     $properties = $nic.GetIPProperties(); \
     $v4 = @($properties.UnicastAddresses | Where-Object { $_.Address.AddressFamily -eq 'InterNetwork' })[0]; \
     $gateway = @($properties.GatewayAddresses | Where-Object { $_.Address.AddressFamily -eq 'InterNetwork' })[0]; \
     New-Object PSObject -Property @{ Name = $nic.Name; Type = [int]$nic.NetworkInterfaceType; \
     Status = [int]$nic.OperationalStatus; Speed = $nic.Speed; \
     Mac = $nic.GetPhysicalAddress().ToString(); IPv4 = [string]$v4.Address; \
     Mask = [string]$v4.IPv4Mask; Gateway = [string]$gateway.Address; \
     Dhcp = ($properties.DhcpServerAddresses.Count -gt 0) } }); \
     ConvertTo-Json -Compress -Depth 3 -InputObject @{ interfaces = $rows }";

/// The folder where Linux says each interface's link facts.
#[cfg(target_os = "linux")]
const SYS_CLASS_NET: &str = "/sys/class/net";

/// The file of an interface's link speed, in megabits per second.
#[cfg(target_os = "linux")]
const SPEED_FILE: &str = "speed";

/// The folder an interface has when it is a Wi-Fi one.
#[cfg(target_os = "linux")]
const WIRELESS_FOLDER: &str = "wireless";

/// Bits per second in a megabit per second, the unit of a Linux link speed.
const BITS_PER_MEGABIT: u64 = 1_000_000;

/// Bits per second in a kilobit, a megabit and a gigabit per second, as the C#
/// `NicFormatter.FormatSpeed` divides.
const KILO: u64 = 1_000;
const MEGA: u64 = 1_000_000;
const GIGA: u64 = 1_000_000_000;

/// The kind of an interface, as .NET's `NetworkInterfaceType` sorts them: the view names
/// each in the user's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InterfaceKind {
    /// Wired Ethernet, whatever its speed.
    Ethernet,
    /// Wi-Fi (IEEE 802.11).
    Wireless,
    /// The loopback interface.
    Loopback,
    /// A tunnel (VPN, IPv6 transition).
    Tunnel,
    /// A point-to-point (PPP) link.
    Ppp,
    /// Mobile broadband.
    MobileBroadband,
    /// The system does not say.
    Unknown,
    /// Another IANA interface type, by its number.
    Other(u32),
}

impl InterfaceKind {
    /// The kind of .NET's `NetworkInterfaceType` numbered `code`, the IANA `ifType`.
    #[must_use]
    pub fn from_dotnet(code: u32) -> Self {
        match code {
            1 => Self::Unknown,
            // Ethernet, Ethernet3Megabit, FastEthernetT, FastEthernetFx, GigabitEthernet.
            6 | 26 | 62 | 69 | 117 => Self::Ethernet,
            23 => Self::Ppp,
            24 => Self::Loopback,
            71 => Self::Wireless,
            131 => Self::Tunnel,
            // Wman, Wwanpp, Wwanpp2.
            237 | 243 | 244 => Self::MobileBroadband,
            other => Self::Other(other),
        }
    }

    /// The kind of a Linux `link_type`, Ethernet being Wi-Fi when the kernel says so.
    #[must_use]
    pub fn from_linux(link_type: &str, wireless: bool) -> Self {
        match link_type {
            "ether" if wireless => Self::Wireless,
            "ether" => Self::Ethernet,
            "loopback" => Self::Loopback,
            "none" | "ipip" | "sit" | "gre" | "ip6tnl" | "tunnel6" => Self::Tunnel,
            "ppp" => Self::Ppp,
            _ => Self::Unknown,
        }
    }
}

/// An interface's operational status, as .NET's `OperationalStatus` and RFC 2863's
/// `ifOperStatus` name them: the view names each in the user's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OperStatus {
    /// Up: it can pass packets.
    Up,
    /// Down.
    Down,
    /// In test mode.
    Testing,
    /// The system does not know.
    Unknown,
    /// Waiting for an external event.
    Dormant,
    /// A component is missing.
    NotPresent,
    /// Down because a layer below it is.
    LowerLayerDown,
}

impl OperStatus {
    /// The status numbered `code` in .NET's `OperationalStatus`.
    #[must_use]
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::Up,
            2 => Self::Down,
            3 => Self::Testing,
            5 => Self::Dormant,
            6 => Self::NotPresent,
            7 => Self::LowerLayerDown,
            _ => Self::Unknown,
        }
    }

    /// The status of a Linux `operstate`.
    #[must_use]
    pub fn from_linux(operstate: &str) -> Self {
        match operstate {
            "UP" => Self::Up,
            "DOWN" => Self::Down,
            "TESTING" => Self::Testing,
            "DORMANT" => Self::Dormant,
            "NOTPRESENT" => Self::NotPresent,
            "LOWERLAYERDOWN" => Self::LowerLayerDown,
            _ => Self::Unknown,
        }
    }
}

/// The unit a link speed is said in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedUnit {
    /// Kilobits per second.
    Kilobits,
    /// Megabits per second.
    Megabits,
    /// Gigabits per second.
    Gigabits,
}

/// `bits_per_second` in the largest unit it reaches, divided down as the C#
/// `NicFormatter.FormatSpeed` does (5.7 Gbit/s is 5).
#[must_use]
pub fn speed_parts(bits_per_second: u64) -> (u64, SpeedUnit) {
    if bits_per_second < MEGA {
        (bits_per_second / KILO, SpeedUnit::Kilobits)
    } else if bits_per_second < GIGA {
        (bits_per_second / MEGA, SpeedUnit::Megabits)
    } else {
        (bits_per_second / GIGA, SpeedUnit::Gigabits)
    }
}

/// A hardware address as the C# `NicFormatter.FormatMac` writes one: its bytes in capital
/// hexadecimal joined by colons, whether it comes as .NET's bare hexadecimal
/// (`08F97ED9D1BB`) or Linux's colon-separated form; empty when there is none.
#[must_use]
pub fn format_mac(address: &str) -> String {
    let digits: Vec<char> = address
        .chars()
        .filter(char::is_ascii_hexdigit)
        .map(|digit| digit.to_ascii_uppercase())
        .collect();
    digits
        .chunks(2)
        .map(|pair| pair.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(":")
}

/// Snapshot of a single network interface, as the C# `NicSnapshot`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NicSnapshot {
    /// Interface name.
    pub name: String,
    /// Interface kind.
    pub kind: InterfaceKind,
    /// Operational status.
    pub status: OperStatus,
    /// Link speed in bits per second; `None` when the system does not know it.
    pub speed_bps: Option<u64>,
    /// Hardware address, see [`format_mac`]; empty when there is none.
    pub mac: String,
    /// First IPv4 address; empty when there is none.
    pub ipv4: String,
    /// Its subnet mask.
    pub subnet: String,
    /// First IPv4 gateway; empty when there is none.
    pub gateway: String,
    /// Whether the address came from DHCP; `None` when the system does not say.
    pub dhcp: Option<bool>,
}

/// A row the Windows listing script writes.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WindowsRow {
    #[serde(default, deserialize_with = "text_or_null")]
    name: String,
    #[serde(rename = "Type")]
    kind: u32,
    status: u32,
    speed: i64,
    #[serde(default, deserialize_with = "text_or_null")]
    mac: String,
    #[serde(rename = "IPv4", default, deserialize_with = "text_or_null")]
    ipv4: String,
    #[serde(default, deserialize_with = "text_or_null")]
    mask: String,
    #[serde(default, deserialize_with = "text_or_null")]
    gateway: String,
    #[serde(default)]
    dhcp: bool,
}

/// What the Windows listing script writes.
#[derive(Deserialize)]
struct WindowsListing {
    #[serde(default, deserialize_with = "one_or_many")]
    interfaces: Vec<WindowsRow>,
}

/// The interfaces of the JSON the Windows listing script writes, in the system's order.
///
/// # Errors
///
/// The JSON is not the listing's.
pub fn parse_windows_listing(json: &str) -> Result<Vec<NicSnapshot>, ProgramError> {
    let listing: WindowsListing = system_listing::parse_json(json)?;
    Ok(listing
        .interfaces
        .into_iter()
        .map(|row| NicSnapshot {
            name: row.name,
            kind: InterfaceKind::from_dotnet(row.kind),
            status: OperStatus::from_code(row.status),
            // .NET says -1 for a speed it does not know; 0 is no more of a speed.
            speed_bps: u64::try_from(row.speed).ok().filter(|&speed| speed > 0),
            mac: format_mac(&row.mac),
            ipv4: row.ipv4,
            subnet: row.mask,
            gateway: row.gateway,
            dhcp: Some(row.dhcp),
        })
        .collect())
}

/// What Linux says of an interface beside `ip`: its speed in megabits per second, when the
/// driver knows it, and whether it is a Wi-Fi one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LinkFacts {
    /// Link speed in megabits per second; `None` when unknown (the kernel says -1).
    pub speed_mbps: Option<u64>,
    /// The interface is a Wi-Fi one.
    pub wireless: bool,
}

/// An interface `ip -j addr` lists.
#[derive(Deserialize)]
struct IpLink {
    #[serde(default)]
    ifname: String,
    #[serde(default)]
    operstate: String,
    #[serde(default)]
    link_type: String,
    #[serde(default)]
    address: String,
    #[serde(default)]
    addr_info: Vec<IpAddressInfo>,
}

/// An address of an interface `ip -j addr` lists.
#[derive(Deserialize)]
struct IpAddressInfo {
    #[serde(default)]
    family: String,
    #[serde(default)]
    local: String,
    #[serde(default)]
    prefixlen: u8,
    /// Set by the DHCP client: the address has a lifetime.
    #[serde(default)]
    dynamic: bool,
}

/// A route `ip -j route` lists, as far as a gateway goes.
#[derive(Deserialize)]
struct IpGatewayRoute {
    #[serde(default)]
    dst: String,
    #[serde(default)]
    gateway: Option<String>,
    #[serde(default)]
    dev: String,
}

/// The destination of the default route in `ip -j route`.
const DEFAULT_DESTINATION: &str = "default";

/// The family of an IPv4 address in `ip -j addr`.
const INET_FAMILY: &str = "inet";

/// The interfaces of `ip -j addr` (`addresses`), each with the gateway of its default route
/// in `ip -j -4 route` (`routes`) and what `facts` says of its link.
///
/// # Errors
///
/// Either JSON is not what `ip` writes.
pub fn parse_ip_json(
    addresses: &str,
    routes: &str,
    facts: impl Fn(&str) -> LinkFacts,
) -> Result<Vec<NicSnapshot>, ProgramError> {
    let links: Vec<IpLink> = system_listing::parse_json(addresses)?;
    let routes: Vec<IpGatewayRoute> = system_listing::parse_json(routes)?;
    Ok(links
        .into_iter()
        .map(|link| {
            let facts = facts(&link.ifname);
            let v4 = link
                .addr_info
                .iter()
                .find(|info| info.family == INET_FAMILY);
            let gateway = routes
                .iter()
                .find(|route| route.dst == DEFAULT_DESTINATION && route.dev == link.ifname)
                .and_then(|route| route.gateway.clone())
                .unwrap_or_default();
            NicSnapshot {
                kind: InterfaceKind::from_linux(&link.link_type, facts.wireless),
                status: OperStatus::from_linux(&link.operstate),
                speed_bps: facts
                    .speed_mbps
                    .filter(|&speed| speed > 0)
                    .and_then(|speed| speed.checked_mul(BITS_PER_MEGABIT)),
                mac: format_mac(&link.address),
                ipv4: v4.map(|info| info.local.clone()).unwrap_or_default(),
                subnet: v4
                    .and_then(|info| system_listing::ipv4_mask(info.prefixlen))
                    .map(|mask| mask.to_string())
                    .unwrap_or_default(),
                gateway,
                dhcp: v4.map(|info| info.dynamic),
                name: link.ifname,
            }
        })
        .collect())
}

/// What `/sys/class/net` says of the interface `name`; nothing for a name that is not one
/// folder's.
#[cfg(target_os = "linux")]
fn sysfs_facts(name: &str) -> LinkFacts {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return LinkFacts::default();
    }
    let folder = std::path::Path::new(SYS_CLASS_NET).join(name);
    LinkFacts {
        // The kernel says -1, or fails to read, when the driver does not know.
        speed_mbps: std::fs::read_to_string(folder.join(SPEED_FILE))
            .ok()
            .and_then(|speed| speed.trim().parse::<u64>().ok()),
        wireless: folder.join(WIRELESS_FOLDER).is_dir(),
    }
}

/// Lists the system's network interfaces.
///
/// # Errors
///
/// The system has no way to list them, or the listing could not be started, did not end in
/// time, failed, or wrote something else than asked.
pub fn run_interfaces_command() -> Result<Vec<NicSnapshot>, ProgramError> {
    #[cfg(windows)]
    {
        let json = system_program::run_powershell(LISTING_SCRIPT, system_listing::LIST_TIME_LIMIT)?;
        parse_windows_listing(&json)
    }
    #[cfg(target_os = "linux")]
    {
        let ip = system_program::find_program(&system_listing::IP_CANDIDATES)
            .ok_or(ProgramError::NotAvailable)?;
        let limit = system_listing::LIST_TIME_LIMIT;
        let addresses = system_program::run_linux(&ip, &["-j", "addr"], limit)?;
        let routes =
            system_program::run_linux(&ip, &["-j", "-4", "route", "show", "table", "main"], limit)?;
        parse_ip_json(&addresses, &routes, sysfs_facts)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(ProgramError::NotAvailable)
    }
}

/// `entries` as CSV under `headers` (name, type, status, speed, MAC, IPv4, subnet, gateway,
/// DHCP), `words` giving the kind, status, speed and DHCP cells in the user's language:
/// every cell quoted and a formula defused, an interface's name being the user's choice.
#[must_use]
pub fn build_interfaces_csv(
    entries: &[NicSnapshot],
    headers: &[String],
    words: impl Fn(&NicSnapshot) -> [String; 4],
) -> String {
    system_listing::csv_document(
        headers,
        entries.iter().map(|nic| {
            let [kind, status, speed, dhcp] = words(nic);
            vec![
                nic.name.clone(),
                kind,
                status,
                speed,
                nic.mac.clone(),
                nic.ipv4.clone(),
                nic.subnet.clone(),
                nic.gateway.clone(),
                dhcp,
            ]
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As the Windows listing script writes it: a Wi-Fi adapter up, one down whose speed
    /// .NET does not know, the loopback, a lone row written as an object would be too.
    const WINDOWS_FIXTURE: &str = r#"{"interfaces":[
        {"Name":"Wi-Fi 4","Type":71,"Status":1,"Speed":5764800000,"Mac":"08F97ED9D1BB",
         "IPv4":"192.168.1.66","Mask":"255.255.255.0","Gateway":"192.168.1.254","Dhcp":true},
        {"Name":"Connexion réseau Bluetooth 4","Type":6,"Status":2,"Speed":-1,"Mac":"08F97ED9D1BF",
         "IPv4":"","Mask":"","Gateway":"","Dhcp":false},
        {"Name":"Loopback Pseudo-Interface 1","Type":24,"Status":1,"Speed":1073741824,"Mac":"",
         "IPv4":"127.0.0.1","Mask":"255.0.0.0","Gateway":null,"Dhcp":false},
        {"Name":"Teredo","Type":131,"Status":7,"Speed":0,"Mac":null,"IPv4":null,"Mask":null,
         "Gateway":null,"Dhcp":false}]}"#;

    #[test]
    fn the_windows_listing_keeps_the_systems_speed_and_status_never_a_guess() {
        let list = parse_windows_listing(WINDOWS_FIXTURE).expect("parses");
        assert_eq!(list.len(), 4);
        assert_eq!(
            list[0],
            NicSnapshot {
                name: "Wi-Fi 4".to_owned(),
                kind: InterfaceKind::Wireless,
                status: OperStatus::Up,
                speed_bps: Some(5_764_800_000),
                mac: "08:F9:7E:D9:D1:BB".to_owned(),
                ipv4: "192.168.1.66".to_owned(),
                subnet: "255.255.255.0".to_owned(),
                gateway: "192.168.1.254".to_owned(),
                dhcp: Some(true),
            }
        );
        // Down, its speed unknown: said so, not 1 Gbit/s because it has a MAC address.
        assert_eq!(list[1].name, "Connexion réseau Bluetooth 4");
        assert_eq!(list[1].kind, InterfaceKind::Ethernet);
        assert_eq!(list[1].status, OperStatus::Down);
        assert_eq!(list[1].speed_bps, None);
        assert_eq!(list[1].dhcp, Some(false));
        assert_eq!(list[2].kind, InterfaceKind::Loopback);
        assert_eq!(list[2].mac, "");
        assert_eq!(list[3].kind, InterfaceKind::Tunnel);
        assert_eq!(list[3].status, OperStatus::LowerLayerDown);
        assert_eq!(list[3].speed_bps, None);
        assert_eq!(list[3].ipv4, "");
    }

    #[test]
    fn a_lone_interface_and_garbage_are_told_apart() {
        let lone = r#"{"interfaces":{"Name":"Ethernet","Type":6,"Status":1,"Speed":1000000000,
            "Mac":"001122334455","IPv4":"10.0.0.2","Mask":"255.0.0.0","Gateway":"10.0.0.1","Dhcp":false}}"#;
        let list = parse_windows_listing(lone).expect("parses");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].speed_bps, Some(1_000_000_000));
        assert!(matches!(
            parse_windows_listing(""),
            Err(ProgramError::Unreadable(_))
        ));
    }

    #[test]
    fn a_speed_is_said_in_its_largest_unit_as_the_csharp_divides() {
        assert_eq!(speed_parts(54_000_000), (54, SpeedUnit::Megabits));
        assert_eq!(speed_parts(1_000_000_000), (1, SpeedUnit::Gigabits));
        assert_eq!(speed_parts(5_764_800_000), (5, SpeedUnit::Gigabits));
        assert_eq!(speed_parts(9_600), (9, SpeedUnit::Kilobits));
    }

    #[test]
    fn a_hardware_address_is_written_in_capitals_with_colons() {
        assert_eq!(format_mac("08F97ED9D1BB"), "08:F9:7E:D9:D1:BB");
        assert_eq!(format_mac("00:15:5d:50:be:ba"), "00:15:5D:50:BE:BA");
        assert_eq!(format_mac(""), "");
    }

    #[test]
    fn unknown_type_and_status_numbers_stay_unknown() {
        assert_eq!(InterfaceKind::from_dotnet(1), InterfaceKind::Unknown);
        assert_eq!(InterfaceKind::from_dotnet(117), InterfaceKind::Ethernet);
        assert_eq!(InterfaceKind::from_dotnet(999), InterfaceKind::Other(999));
        assert_eq!(OperStatus::from_code(4), OperStatus::Unknown);
        assert_eq!(OperStatus::from_code(42), OperStatus::Unknown);
    }

    #[test]
    fn linux_interfaces_read_from_ip_json_and_sysfs() {
        let addresses = r#"[
            {"ifindex":1,"ifname":"lo","operstate":"UNKNOWN","link_type":"loopback",
             "address":"00:00:00:00:00:00","addr_info":[{"family":"inet","local":"127.0.0.1","prefixlen":8},
             {"family":"inet6","local":"::1","prefixlen":128}]},
            {"ifindex":2,"ifname":"eth0","operstate":"UP","link_type":"ether","address":"00:15:5d:50:be:ba",
             "addr_info":[{"family":"inet6","local":"fe80::215:5dff:fe50:beba","prefixlen":64},
             {"family":"inet","local":"172.30.86.176","prefixlen":20,"dynamic":true}]},
            {"ifindex":3,"ifname":"wlan0","operstate":"DOWN","link_type":"ether","address":"3c:52:82:aa:bb:cc",
             "addr_info":[]}]"#;
        let routes = r#"[{"dst":"default","gateway":"172.30.80.1","dev":"eth0","protocol":"kernel","flags":[]},
            {"dst":"172.30.80.0/20","dev":"eth0","scope":"link","prefsrc":"172.30.86.176","flags":[]}]"#;
        let facts = |name: &str| match name {
            "eth0" => LinkFacts {
                speed_mbps: Some(10_000),
                wireless: false,
            },
            "wlan0" => LinkFacts {
                speed_mbps: None,
                wireless: true,
            },
            _ => LinkFacts::default(),
        };
        let list = parse_ip_json(addresses, routes, facts).expect("parses");
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].kind, InterfaceKind::Loopback);
        assert_eq!(list[0].status, OperStatus::Unknown);
        assert_eq!(list[0].subnet, "255.0.0.0");
        assert_eq!(list[0].dhcp, Some(false));
        assert_eq!(
            list[1],
            NicSnapshot {
                name: "eth0".to_owned(),
                kind: InterfaceKind::Ethernet,
                status: OperStatus::Up,
                speed_bps: Some(10_000_000_000),
                mac: "00:15:5D:50:BE:BA".to_owned(),
                ipv4: "172.30.86.176".to_owned(),
                subnet: "255.255.240.0".to_owned(),
                gateway: "172.30.80.1".to_owned(),
                dhcp: Some(true),
            }
        );
        assert_eq!(list[2].kind, InterfaceKind::Wireless);
        assert_eq!(list[2].status, OperStatus::Down);
        assert_eq!(list[2].speed_bps, None);
        assert_eq!(list[2].dhcp, None);
    }

    #[test]
    fn the_csv_quotes_every_cell_and_defuses_an_interface_named_as_a_formula() {
        let mut list = parse_windows_listing(WINDOWS_FIXTURE).expect("parses");
        list[0].name = "+SUM(A1)".to_owned();
        let headers: Vec<String> = ["N", "T", "S", "Sp", "M", "I", "Sub", "G", "D"]
            .map(str::to_owned)
            .to_vec();
        let csv = build_interfaces_csv(&list[..1], &headers, |_| {
            ["Wi-Fi", "Up", "5 Gbit/s", "DHCP"].map(str::to_owned)
        });
        assert_eq!(
            csv.lines().nth(1),
            Some(
                "\"'+SUM(A1)\",\"Wi-Fi\",\"Up\",\"5 Gbit/s\",\"08:F9:7E:D9:D1:BB\",\
                 \"192.168.1.66\",\"255.255.255.0\",\"192.168.1.254\",\"DHCP\""
            )
        );
    }
}
