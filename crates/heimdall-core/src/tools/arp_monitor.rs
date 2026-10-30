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

//! Local ARP table parser and anomaly monitor matching the C#
//! `Heimdall.Core.Network.ArpTableParser` and `ArpMonitorViewModel`.
//!
//! Provides platform-specific ARP table parsing (Windows, Linux, macOS), the hardware vendor
//! of each address, change detection between two reads of the table (new hosts, MAC address
//! changes, hosts gone, one address claimed by two MAC addresses), and CSV export.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::csv_cell::quoted;
use super::mac_vendor::{self, MacVendor};
use super::system_program::{self, ProgramError};

/// How long `arp -a` may run, as the C# `WaitForExit(5000)`.
pub const ARP_DEADLINE: Duration = Duration::from_secs(5);
/// The Windows program, in the system folder.
pub const WINDOWS_PROGRAM: &str = "arp.exe";
/// The Unix program, in a system folder.
pub const UNIX_PROGRAM: &str = "arp";
/// The refresh intervals offered, in seconds, as the C# `CmbInterval` items.
pub const MONITOR_INTERVALS_SECS: [u64; 4] = [5, 10, 30, 60];
/// The interval chosen at first, as the C# `SelectedIndex="1"` and its 10000 ms fallback.
pub const DEFAULT_INTERVAL_SECS: u64 = 10;
/// The argument that dumps the whole table, which is what the parsers expect.
const ARP_ARGUMENTS: [&str; 1] = ["-a"];
/// Linux's own view of the table, read when it is there.
const PROC_NET_ARP: &str = "/proc/net/arp";

/// Status of an ARP table entry since the read before, as the C# `ArpEntryState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArpStatus {
    /// First seen in this read.
    New,
    /// Seen again with the same MAC address, as the C# "stable".
    Stable,
    /// Seen again with another MAC address: possible ARP spoofing.
    Changed,
    /// In an earlier read, not in this one; kept with its last sighting, as the C#.
    Gone,
    /// Claimed by more than one MAC address in this very read: one of them is lying.
    Conflict,
}

/// A monitored ARP entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArpEntry {
    pub ip: String,
    pub mac: String,
    /// The MAC address it had before its last change, as the C# `PreviousMac`.
    pub previous_mac: Option<String>,
    /// The other MAC addresses claiming the same IP address, when in [`ArpStatus::Conflict`].
    pub conflicting_macs: Vec<String>,
    /// The hardware vendor its MAC address names; `None` when unknown, which the view says.
    pub vendor: Option<MacVendor>,
    pub status: ArpStatus,
    pub first_seen: String,
    pub last_seen: String,
}

/// Something a read of the table found that deserves the user's eye, as the C# alert
/// banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArpAlert {
    /// The MAC address of `ip` changed, as the C# `ToolArpAlertMacChanged`.
    MacChanged {
        ip: String,
        previous: String,
        current: String,
    },
    /// `ip` is claimed by every one of `macs` at once.
    Conflict { ip: String, macs: Vec<String> },
    /// Hosts that appeared since the read before.
    Appeared(Vec<String>),
    /// Hosts that left the table since the read before.
    Gone(Vec<String>),
}

/// What one read of the table changed: every entry, in the order first seen, and the alerts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArpRefresh {
    pub entries: Vec<ArpEntry>,
    pub alerts: Vec<ArpAlert>,
}

/// Resolves a MAC address to a known vendor based on its OUI; `None` when unknown.
#[must_use]
pub fn lookup_mac_vendor(mac: &str) -> Option<MacVendor> {
    mac_vendor::lookup(mac)
}

/// Octets of a MAC address.
const MAC_OCTETS: usize = 6;

/// Whether `text` is six groups of hexadecimal digits (`min_digits` to two each) joined by
/// `separator`: a table cell is only a MAC address when it reads as one, whatever the length
/// of the text (C# `MacAddressPattern`).
fn is_mac_address(text: &str, separator: char, min_digits: usize) -> bool {
    let groups: Vec<&str> = text.split(separator).collect();
    groups.len() == MAC_OCTETS
        && groups.iter().all(|group| {
            (min_digits..=2).contains(&group.len()) && group.bytes().all(|b| b.is_ascii_hexdigit())
        })
}

/// Parses Windows `arp -a` output into IP-MAC pairs.
#[must_use]
pub fn parse_windows_arp(output: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let ip = parts[0];
            let mac = parts[1];
            if ip.parse::<IpAddr>().is_ok() && is_mac_address(mac, '-', 2) {
                out.push((ip.to_owned(), mac.to_ascii_uppercase()));
            }
        }
    }

    out
}

/// Parses Linux `/proc/net/arp` content into IP-MAC pairs.
#[must_use]
pub fn parse_linux_proc_net_arp(content: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();

    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            let ip = parts[0];
            let mac = parts[3];
            if mac != "00:00:00:00:00:00"
                && is_mac_address(mac, ':', 2)
                && ip.parse::<IpAddr>().is_ok()
            {
                out.push((ip.to_owned(), mac.replace(':', "-").to_ascii_uppercase()));
            }
        }
    }

    out
}

/// Parses macOS `arp -a` output into IP-MAC pairs.
#[must_use]
pub fn parse_macos_arp(output: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();

    for line in output.lines() {
        if let (Some(open), Some(close)) = (line.find('('), line.find(')')) {
            if open < close {
                let ip = &line[open + 1..close];
                if let Some(at_idx) = line.find(" at ") {
                    let remainder = line[at_idx + 4..].trim();
                    if let Some(space_idx) = remainder.find(' ') {
                        let mac = &remainder[..space_idx];
                        // macOS drops the leading zero of an octet: `0:50:56:...`.
                        if ip.parse::<IpAddr>().is_ok() && is_mac_address(mac, ':', 1) {
                            out.push((ip.to_owned(), mac.replace(':', "-").to_ascii_uppercase()));
                        }
                    }
                }
            }
        }
    }

    out
}

/// The MAC addresses of each IP address of `current_raw`, each once whatever its case, and
/// the addresses in the order read: a table lists an address once per interface.
fn macs_by_ip(current_raw: &[(String, String)]) -> (Vec<&str>, HashMap<&str, Vec<&str>>) {
    let mut order = Vec::new();
    let mut macs: HashMap<&str, Vec<&str>> = HashMap::new();
    for (ip, mac) in current_raw {
        let known = macs.entry(ip.as_str()).or_insert_with(|| {
            order.push(ip.as_str());
            Vec::new()
        });
        if !known.iter().any(|seen| seen.eq_ignore_ascii_case(mac)) {
            known.push(mac.as_str());
        }
    }
    (order, macs)
}

/// `entry` seen in this read with `macs` at `timestamp`: stable, changed or in conflict, its
/// alert pushed on `alerts`.
fn seen_again(entry: &mut ArpEntry, macs: &[&str], timestamp: &str, alerts: &mut Vec<ArpAlert>) {
    entry.last_seen = timestamp.to_owned();
    if let [mac] = macs {
        entry.conflicting_macs.clear();
        if entry.mac.eq_ignore_ascii_case(mac) {
            entry.status = ArpStatus::Stable;
        } else {
            alerts.push(ArpAlert::MacChanged {
                ip: entry.ip.clone(),
                previous: entry.mac.clone(),
                current: (*mac).to_owned(),
            });
            entry.previous_mac = Some(std::mem::replace(&mut entry.mac, (*mac).to_owned()));
            entry.status = ArpStatus::Changed;
        }
    } else {
        // The MAC address shown stays the one it had, when it is among them.
        let kept = macs
            .iter()
            .find(|mac| mac.eq_ignore_ascii_case(&entry.mac))
            .or_else(|| macs.first())
            .map_or_else(|| entry.mac.clone(), |mac| (*mac).to_owned());
        entry.conflicting_macs = macs
            .iter()
            .filter(|mac| !mac.eq_ignore_ascii_case(&kept))
            .map(|mac| (*mac).to_owned())
            .collect();
        entry.mac = kept;
        // Alerted when it begins, not at every read while it lasts.
        if entry.status != ArpStatus::Conflict {
            alerts.push(ArpAlert::Conflict {
                ip: entry.ip.clone(),
                macs: macs.iter().map(|mac| (*mac).to_owned()).collect(),
            });
        }
        entry.status = ArpStatus::Conflict;
    }
    entry.vendor = lookup_mac_vendor(&entry.mac);
}

/// Compares a newly captured ARP table with the entries shown, as the C# `ApplySnapshot`:
/// an address seen again is stable with the same MAC address and changed with another, which
/// raises an alert; one not seen again is gone, kept with its last sighting; one not shown
/// before is new, after those shown. Better than the C#, which keys the table by address and
/// so keeps one MAC address of each: an address claimed by two MAC addresses in the same
/// table is a conflict, and alerted. Hosts appearing or leaving are alerted too, but not on
/// the first read, when every host is new.
#[must_use]
pub fn detect_changes(
    previous: &[ArpEntry],
    current_raw: &[(String, String)],
    timestamp: &str,
) -> ArpRefresh {
    let (order, current) = macs_by_ip(current_raw);
    let mut alerts = Vec::new();
    let mut entries = Vec::with_capacity(previous.len() + order.len());
    let mut gone = Vec::new();

    for shown in previous {
        let mut entry = shown.clone();
        match current.get(shown.ip.as_str()) {
            Some(macs) => seen_again(&mut entry, macs, timestamp, &mut alerts),
            None => {
                if entry.status != ArpStatus::Gone {
                    gone.push(entry.ip.clone());
                }
                entry.status = ArpStatus::Gone;
                entry.conflicting_macs.clear();
            }
        }
        entries.push(entry);
    }

    let mut appeared = Vec::new();
    for ip in order {
        if previous.iter().any(|shown| shown.ip == ip) {
            continue;
        }
        let Some(macs) = current.get(ip) else {
            continue;
        };
        let mut entry = ArpEntry {
            ip: ip.to_owned(),
            mac: macs
                .first()
                .map(|mac| (*mac).to_owned())
                .unwrap_or_default(),
            previous_mac: None,
            conflicting_macs: Vec::new(),
            vendor: None,
            status: ArpStatus::New,
            first_seen: timestamp.to_owned(),
            last_seen: timestamp.to_owned(),
        };
        if macs.len() > 1 {
            seen_again(&mut entry, macs, timestamp, &mut alerts);
        } else {
            entry.vendor = lookup_mac_vendor(&entry.mac);
        }
        appeared.push(entry.ip.clone());
        entries.push(entry);
    }

    if !previous.is_empty() {
        if !appeared.is_empty() {
            alerts.push(ArpAlert::Appeared(appeared));
        }
        if !gone.is_empty() {
            alerts.push(ArpAlert::Gone(gone));
        }
    }
    ArpRefresh { entries, alerts }
}

/// Builds CSV export content for ARP entries under the column names `header` (IP, MAC,
/// vendor, status, first seen, last seen), each vendor said by `vendor_label` and each
/// status by `status_label`; every text cell sanitized against formulas and quoted.
#[must_use]
pub fn build_arp_csv(
    entries: &[ArpEntry],
    header: [&str; 6],
    vendor_label: impl Fn(Option<MacVendor>) -> String,
    status_label: impl Fn(ArpStatus) -> String,
) -> String {
    let mut out = header.map(quoted).join(",");
    out.push('\n');

    for e in entries {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            quoted(&e.ip),
            quoted(&e.mac),
            quoted(&vendor_label(e.vendor)),
            quoted(&status_label(e.status)),
            quoted(&e.first_seen),
            quoted(&e.last_seen)
        ));
    }

    out
}

/// Reads the current system ARP cache table: `/proc/net/arp` on Linux, else the system's
/// `arp -a` run by its whole path, ended past [`ARP_DEADLINE`] or when `stop` is set.
/// `None` when the table could not be read at all.
#[must_use]
pub fn read_system_arp_table(stop: &AtomicBool) -> Option<Vec<(String, String)>> {
    if cfg!(target_os = "linux")
        && let Ok(content) = std::fs::read_to_string(PROC_NET_ARP)
    {
        return Some(parse_linux_proc_net_arp(&content));
    }
    let program = system_program::system_tool(WINDOWS_PROGRAM, UNIX_PROGRAM)?;
    let mut output = String::new();
    let end = system_program::run_lines(&program, &ARP_ARGUMENTS, ARP_DEADLINE, stop, |line| {
        output.push_str(line);
        output.push('\n');
    });
    if matches!(end, Err(ProgramError::NotStarted(_))) {
        return None;
    }
    Some(if cfg!(target_os = "macos") {
        parse_macos_arp(&output)
    } else {
        parse_windows_arp(&output)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_windows_arp_lines() {
        let sample = "Interface: 192.168.1.50 --- 0x10\n  Internet Address      Physical Address      Type\n  192.168.1.1           00-50-56-c0-00-01     dynamic\n  192.168.1.254         b8-27-eb-12-34-56     dynamic\n";
        let parsed = parse_windows_arp(sample);
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[0],
            ("192.168.1.1".to_owned(), "00-50-56-C0-00-01".to_owned())
        );
        assert_eq!(
            parsed[1],
            ("192.168.1.254".to_owned(), "B8-27-EB-12-34-56".to_owned())
        );
    }

    #[test]
    fn parse_linux_arp_proc() {
        let sample = "IP address       HW type     Flags       HW address            Mask     Device\n192.168.1.1      0x1         0x2         00:50:56:c0:00:01     *        eth0\n";
        let parsed = parse_linux_proc_net_arp(sample);
        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed[0],
            ("192.168.1.1".to_owned(), "00-50-56-C0-00-01".to_owned())
        );
    }

    #[test]
    fn oui_vendor_lookup() {
        assert_eq!(
            lookup_mac_vendor("00-50-56-11-22-33"),
            Some(MacVendor::Named("VMware"))
        );
        assert_eq!(
            lookup_mac_vendor("b8:27:eb:aa:bb:cc"),
            Some(MacVendor::Named("Raspberry Pi"))
        );
        assert_eq!(lookup_mac_vendor("01-00-5e-00-00-16"), None);
    }

    #[test]
    fn a_table_cell_that_is_not_a_mac_address_is_never_taken_for_one() {
        let windows = "  192.168.1.9  zz-zz-zz-zz-zz-zz  dynamic\n  192.168.1.8  00-50-56-c0-00  dynamic\n  192.168.1.7  00-50-56-c0-00-01-02  dynamic\n";
        assert!(parse_windows_arp(windows).is_empty());
        let linux = "header\n192.168.1.9 0x1 0x2 zz:zz:zz:zz:zz:zz * eth0\n192.168.1.8 0x1 0x2 incomplete * eth0\n";
        assert!(parse_linux_proc_net_arp(linux).is_empty());
        let macos = "? (192.168.1.9) at (incomplete) on en0\n? (192.168.1.8) at \u{1b}[2Jzz on en0 ifscope\n";
        assert!(parse_macos_arp(macos).is_empty());
        let valid = "? (192.168.1.1) at 0:50:56:c0:0:1 on en0 ifscope [ethernet]\n";
        assert_eq!(
            parse_macos_arp(valid),
            vec![("192.168.1.1".to_owned(), "0-50-56-C0-0-1".to_owned())]
        );
    }

    #[test]
    fn a_mac_address_in_another_alphabet_is_never_parsed() {
        let lossy = String::from_utf8_lossy(b"  192.168.1.9  00-50-\xff\xfe-11-22-33  dynamic");
        assert!(parse_windows_arp(&lossy).is_empty());
    }

    #[test]
    fn arp_csv_is_sanitized_and_quoted() {
        let entries = vec![entry("10.0.0.1", "00-50-56-01-02-03", ArpStatus::New)];
        let csv = build_arp_csv(
            &entries,
            ["IP", "MAC", "V", "S", "F", "L"],
            |_| "=HYPERLINK(\"x\")".to_owned(),
            |status| format!("{status:?}"),
        );
        assert!(csv.starts_with("\"IP\",\"MAC\",\"V\",\"S\",\"F\",\"L\"\n"));
        assert!(
            csv.contains("\"10.0.0.1\",\"00-50-56-01-02-03\",\"'=HYPERLINK(\"\"x\"\")\",\"New\""),
            "{csv}"
        );
    }

    fn entry(ip: &str, mac: &str, status: ArpStatus) -> ArpEntry {
        ArpEntry {
            ip: ip.to_owned(),
            mac: mac.to_owned(),
            previous_mac: None,
            conflicting_macs: Vec::new(),
            vendor: lookup_mac_vendor(mac),
            status,
            first_seen: "10:00".to_owned(),
            last_seen: "10:00".to_owned(),
        }
    }

    fn table(rows: &[(&str, &str)]) -> Vec<(String, String)> {
        rows.iter()
            .map(|(ip, mac)| ((*ip).to_owned(), (*mac).to_owned()))
            .collect()
    }

    fn statuses(refresh: &ArpRefresh) -> Vec<(&str, ArpStatus)> {
        refresh
            .entries
            .iter()
            .map(|entry| (entry.ip.as_str(), entry.status))
            .collect()
    }

    #[test]
    fn change_detection_new_stable_changed_and_gone() {
        let prev = vec![
            entry("10.0.0.1", "00-50-56-01-02-03", ArpStatus::Stable),
            entry("10.0.0.2", "00-50-56-01-02-04", ArpStatus::New),
        ];

        // Same MAC address, whatever its case: stable, seen now, first seen kept.
        let res = detect_changes(
            &prev,
            &table(&[
                ("10.0.0.1", "00-50-56-01-02-03"),
                ("10.0.0.2", "00-50-56-01-02-04"),
            ]),
            "10:05",
        );
        assert_eq!(
            statuses(&res),
            [
                ("10.0.0.1", ArpStatus::Stable),
                ("10.0.0.2", ArpStatus::Stable)
            ],
            "a new host is stable at the next read, as the C#"
        );
        assert_eq!(res.entries[0].last_seen, "10:05");
        assert_eq!(res.entries[0].first_seen, "10:00");
        assert!(res.alerts.is_empty());

        // Another MAC address: changed, the old one kept and alerted.
        let res = detect_changes(
            &prev,
            &table(&[
                ("10.0.0.1", "B8-27-EB-01-02-03"),
                ("10.0.0.2", "00-50-56-01-02-04"),
            ]),
            "10:06",
        );
        assert_eq!(res.entries[0].status, ArpStatus::Changed);
        assert_eq!(res.entries[0].mac, "B8-27-EB-01-02-03");
        assert_eq!(
            res.entries[0].previous_mac.as_deref(),
            Some("00-50-56-01-02-03")
        );
        assert_eq!(
            res.entries[0].vendor,
            Some(MacVendor::Named("Raspberry Pi")),
            "the vendor follows the MAC address"
        );
        assert_eq!(
            res.alerts,
            [ArpAlert::MacChanged {
                ip: "10.0.0.1".to_owned(),
                previous: "00-50-56-01-02-03".to_owned(),
                current: "B8-27-EB-01-02-03".to_owned(),
            }]
        );

        // A new address after those shown, one missing gone with its last sighting.
        let res = detect_changes(
            &prev,
            &table(&[
                ("10.0.0.3", "B8-27-EB-01-02-05"),
                ("10.0.0.1", "00-50-56-01-02-03"),
            ]),
            "10:07",
        );
        assert_eq!(
            statuses(&res),
            [
                ("10.0.0.1", ArpStatus::Stable),
                ("10.0.0.2", ArpStatus::Gone),
                ("10.0.0.3", ArpStatus::New)
            ]
        );
        assert_eq!(res.entries[1].last_seen, "10:00", "its last sighting kept");
        assert_eq!(
            res.alerts,
            [
                ArpAlert::Appeared(vec!["10.0.0.3".to_owned()]),
                ArpAlert::Gone(vec!["10.0.0.2".to_owned()])
            ]
        );

        // Still gone: said once, not at every read; back again, stable.
        let again = detect_changes(
            &res.entries,
            &table(&[
                ("10.0.0.1", "00-50-56-01-02-03"),
                ("10.0.0.3", "B8-27-EB-01-02-05"),
            ]),
            "10:08",
        );
        assert_eq!(again.entries[1].status, ArpStatus::Gone);
        assert!(again.alerts.is_empty(), "{:?}", again.alerts);
        let back = detect_changes(
            &again.entries,
            &table(&[("10.0.0.2", "00-50-56-01-02-04")]),
            "10:09",
        );
        assert_eq!(back.entries[1].status, ArpStatus::Stable);
        assert_eq!(back.entries[1].last_seen, "10:09");
    }

    #[test]
    fn the_first_read_alerts_nothing_but_conflicts() {
        let res = detect_changes(
            &[],
            &table(&[
                ("10.0.0.1", "00-50-56-01-02-03"),
                ("10.0.0.2", "00-50-56-01-02-04"),
            ]),
            "10:00",
        );
        assert_eq!(
            statuses(&res),
            [("10.0.0.1", ArpStatus::New), ("10.0.0.2", ArpStatus::New)]
        );
        assert!(res.alerts.is_empty(), "every host is new on the first read");
    }

    #[test]
    fn an_address_claimed_by_two_mac_addresses_is_a_conflict_alerted_once() {
        let prev = vec![entry("10.0.0.1", "00-50-56-01-02-03", ArpStatus::Stable)];
        let both = table(&[
            ("10.0.0.1", "B8-27-EB-01-02-03"),
            ("10.0.0.1", "00-50-56-01-02-03"),
            ("10.0.0.1", "00-50-56-01-02-03"),
        ]);
        let res = detect_changes(&prev, &both, "10:05");
        assert_eq!(res.entries.len(), 1, "one entry per address");
        let conflict = &res.entries[0];
        assert_eq!(conflict.status, ArpStatus::Conflict);
        assert_eq!(
            conflict.mac, "00-50-56-01-02-03",
            "the one it had stays shown"
        );
        assert_eq!(conflict.conflicting_macs, ["B8-27-EB-01-02-03"]);
        assert_eq!(
            res.alerts,
            [ArpAlert::Conflict {
                ip: "10.0.0.1".to_owned(),
                macs: vec![
                    "B8-27-EB-01-02-03".to_owned(),
                    "00-50-56-01-02-03".to_owned()
                ],
            }]
        );
        let lasting = detect_changes(&res.entries, &both, "10:06");
        assert_eq!(lasting.entries[0].status, ArpStatus::Conflict);
        assert!(lasting.alerts.is_empty(), "alerted when it begins");
        let over = detect_changes(
            &lasting.entries,
            &table(&[("10.0.0.1", "00-50-56-01-02-03")]),
            "10:07",
        );
        assert_eq!(over.entries[0].status, ArpStatus::Stable);
        assert!(over.entries[0].conflicting_macs.is_empty());

        // A new address in conflict from its first read.
        let first = detect_changes(
            &[],
            &table(&[
                ("10.0.0.9", "00-50-56-01-02-03"),
                ("10.0.0.9", "00-50-56-01-02-09"),
            ]),
            "10:00",
        );
        assert_eq!(first.entries[0].status, ArpStatus::Conflict);
        assert_eq!(first.alerts.len(), 1);
    }
}
