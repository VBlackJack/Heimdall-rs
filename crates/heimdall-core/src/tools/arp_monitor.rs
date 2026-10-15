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
//! `Heimdall.Core.Network.ArpTableParser`.
//!
//! Provides platform-specific ARP table parsing (Windows, Linux, macOS),
//! OUI hardware vendor identification, anomaly detection (new devices, MAC changes / spoofing),
//! and CSV export.

use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;

/// Anomaly status of an ARP table entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArpStatus {
    New,
    Known,
    Changed,
    Conflict,
}

impl fmt::Display for ArpStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::New => write!(f, "New"),
            Self::Known => write!(f, "Known"),
            Self::Changed => write!(f, "Changed"),
            Self::Conflict => write!(f, "Conflict"),
        }
    }
}

/// A monitored ARP entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArpEntry {
    pub ip: String,
    pub mac: String,
    pub vendor: String,
    pub status: ArpStatus,
    pub first_seen: String,
    pub last_seen: String,
}

/// Resolves a MAC address to a known vendor based on its OUI (first 3 octets).
#[must_use]
pub fn lookup_mac_vendor(mac: &str) -> &'static str {
    let clean = mac.replace(['-', ':', '.'], "").to_ascii_uppercase();
    if clean.len() < 6 {
        return "Unknown";
    }

    match &clean[..6] {
        "000C29" | "005056" | "000569" => "VMware",
        "00155D" => "Microsoft Hyper-V",
        "B827EB" | "DC2632" | "E45F01" | "28CDC9" => "Raspberry Pi",
        "001A2B" | "002608" | "708105" => "Cisco Systems",
        "F81EDF" | "ACDE48" | "3C15C2" | "A483E7" | "F01898" => "Apple",
        "001E67" | "00216A" | "A4C494" | "ECF451" => "Intel",
        "D4BED9" | "5CF9DD" | "847BEB" | "B04414" => "Dell",
        "001E0B" | "3CD92B" | "705A0F" => "HP",
        "240AC4" | "30AEA4" | "A47B9D" => "Espressif (ESP32/ESP8266)",
        "001132" => "Synology",
        "00089B" => "QNAP",
        "001B67" => "Cisco-Linksys",
        "0014D1" | "D807B6" => "TP-Link",
        "E4F042" | "9055DE" => "Ubiquiti Networks",
        "00180A" => "Cisco Meraki",
        _ => "Unknown",
    }
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
            if ip.parse::<IpAddr>().is_ok()
                && mac.len() == 17
                && mac.chars().filter(|c| *c == '-').count() == 5
            {
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
            if mac != "00:00:00:00:00:00" && ip.parse::<IpAddr>().is_ok() {
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
                        if ip.parse::<IpAddr>().is_ok() {
                            out.push((ip.to_owned(), mac.replace(':', "-").to_ascii_uppercase()));
                        }
                    }
                }
            }
        }
    }

    out
}

/// Compares a newly captured ARP scan against known entries to detect changes and anomalies.
#[must_use]
pub fn detect_changes(
    previous: &[ArpEntry],
    current_raw: &[(String, String)],
    timestamp: &str,
) -> Vec<ArpEntry> {
    let mut prev_map: HashMap<&str, &ArpEntry> = HashMap::new();
    for p in previous {
        prev_map.insert(&p.ip, p);
    }

    let mut result = Vec::new();
    let mut seen_ips = std::collections::HashSet::new();

    for (ip, mac) in current_raw {
        seen_ips.insert(ip.as_str());
        let vendor = lookup_mac_vendor(mac).to_owned();

        if let Some(&prev) = prev_map.get(ip.as_str()) {
            if prev.mac.eq_ignore_ascii_case(mac) {
                result.push(ArpEntry {
                    ip: ip.clone(),
                    mac: mac.clone(),
                    vendor,
                    status: ArpStatus::Known,
                    first_seen: prev.first_seen.clone(),
                    last_seen: timestamp.to_owned(),
                });
            } else {
                // MAC address changed for the same IP (potential ARP spoofing!)
                result.push(ArpEntry {
                    ip: ip.clone(),
                    mac: mac.clone(),
                    vendor,
                    status: ArpStatus::Changed,
                    first_seen: prev.first_seen.clone(),
                    last_seen: timestamp.to_owned(),
                });
            }
        } else {
            // New host observed
            result.push(ArpEntry {
                ip: ip.clone(),
                mac: mac.clone(),
                vendor,
                status: ArpStatus::New,
                first_seen: timestamp.to_owned(),
                last_seen: timestamp.to_owned(),
            });
        }
    }

    // Retain entries that were not seen in this scan as known (historical)
    for p in previous {
        if !seen_ips.contains(p.ip.as_str()) {
            result.push(p.clone());
        }
    }

    result
}

/// Builds CSV export content for ARP entries.
#[must_use]
pub fn build_arp_csv(entries: &[ArpEntry]) -> String {
    let mut out = String::new();
    out.push_str("IP,MAC,Vendor,Status,FirstSeen,LastSeen\n");

    for e in entries {
        let safe_vendor = e.vendor.replace('"', "\"\"");
        out.push_str(&format!(
            "{},{},\"{}\",{},{},{}\n",
            e.ip, e.mac, safe_vendor, e.status, e.first_seen, e.last_seen
        ));
    }

    out
}

/// Reads the current system ARP cache table.
pub fn read_system_arp_table() -> Vec<(String, String)> {
    use std::process::Command;

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = Command::new("arp");
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.arg("-a");
        if let Ok(output) = cmd.output() {
            let text = String::from_utf8_lossy(&output.stdout);
            return parse_windows_arp(&text);
        }
        Vec::new()
    }
    #[cfg(not(windows))]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/net/arp") {
            return parse_linux_proc_net_arp(&content);
        }
        let mut cmd = Command::new("arp");
        cmd.arg("-a");
        if let Ok(output) = cmd.output() {
            let text = String::from_utf8_lossy(&output.stdout);
            return parse_windows_arp(&text);
        }
        Vec::new()
    }
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
        assert_eq!(lookup_mac_vendor("00-50-56-11-22-33"), "VMware");
        assert_eq!(lookup_mac_vendor("b8:27:eb:aa:bb:cc"), "Raspberry Pi");
        assert_eq!(lookup_mac_vendor("ff-ff-ff-ff-ff-ff"), "Unknown");
    }

    #[test]
    fn change_detection_new_and_changed() {
        let prev = vec![ArpEntry {
            ip: "10.0.0.1".to_owned(),
            mac: "00-50-56-01-02-03".to_owned(),
            vendor: "VMware".to_owned(),
            status: ArpStatus::Known,
            first_seen: "10:00".to_owned(),
            last_seen: "10:00".to_owned(),
        }];

        // Case 1: same MAC -> Known
        let current_same = vec![("10.0.0.1".to_owned(), "00-50-56-01-02-03".to_owned())];
        let res_same = detect_changes(&prev, &current_same, "10:05");
        assert_eq!(res_same[0].status, ArpStatus::Known);
        assert_eq!(res_same[0].last_seen, "10:05");

        // Case 2: different MAC -> Changed
        let current_changed = vec![("10.0.0.1".to_owned(), "B8-27-EB-01-02-03".to_owned())];
        let res_changed = detect_changes(&prev, &current_changed, "10:06");
        assert_eq!(res_changed[0].status, ArpStatus::Changed);

        // Case 3: new IP -> New
        let current_new = vec![("10.0.0.2".to_owned(), "B8-27-EB-01-02-03".to_owned())];
        let res_new = detect_changes(&prev, &current_new, "10:07");
        assert_eq!(res_new[0].status, ArpStatus::New);
    }
}
