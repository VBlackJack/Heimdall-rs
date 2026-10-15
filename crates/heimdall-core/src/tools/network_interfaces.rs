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

//! Network interface enumeration and snapshot inspection.
//!
//! Provides inspection of system network interfaces, IP configuration, MAC
//! addresses, and operational status.

use std::process::Command;

/// Snapshot of a single network interface.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NicSnapshot {
    /// Interface or adapter name.
    pub name: String,
    /// Interface type (e.g. "Ethernet", "Wireless80211", "Loopback").
    pub interface_type: String,
    /// Operational status ("Up" or "Down").
    pub status: String,
    /// Link speed (e.g. "1 Gbps", "100 Mbps", "-").
    pub speed: String,
    /// Hardware physical MAC address.
    pub mac: String,
    /// Primary IPv4 address.
    pub ipv4: String,
    /// Subnet mask.
    pub subnet: String,
    /// Default gateway.
    pub gateway: String,
    /// DHCP status ("DHCP" or "Static").
    pub dhcp: String,
}

impl NicSnapshot {
    /// Formats the network interface snapshot as a CSV line.
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{}",
            self.name,
            self.interface_type,
            self.status,
            self.speed,
            self.mac,
            self.ipv4,
            self.subnet,
            self.gateway,
            self.dhcp
        )
    }
}

/// Helper formatters for network interface attributes.
pub struct NicFormatter;

impl NicFormatter {
    /// Formats link speed in bits per second to human-readable units.
    #[must_use]
    pub fn format_speed(bits_per_sec: i64) -> String {
        if bits_per_sec <= 0 {
            "-".to_owned()
        } else if bits_per_sec < 1_000_000 {
            format!("{} Kbps", bits_per_sec / 1_000)
        } else if bits_per_sec < 1_000_000_000 {
            format!("{} Mbps", bits_per_sec / 1_000_000)
        } else {
            format!("{} Gbps", bits_per_sec / 1_000_000_000)
        }
    }

    /// Formats a MAC address slice into standard colon-delimited hex string.
    #[must_use]
    pub fn format_mac(bytes: &[u8]) -> String {
        if bytes.is_empty() {
            String::new()
        } else {
            bytes
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(":")
        }
    }

    /// Formats DHCP configuration flag into "DHCP" or "Static".
    #[must_use]
    pub fn format_dhcp(enabled: bool) -> String {
        if enabled {
            "DHCP".to_owned()
        } else {
            "Static".to_owned()
        }
    }
}

/// Parser for Windows `ipconfig /all` output.
pub struct IpconfigParser;

impl IpconfigParser {
    /// Parses Windows `ipconfig /all` text into [`NicSnapshot`] entries.
    #[must_use]
    pub fn parse(output: &str) -> Vec<NicSnapshot> {
        let mut results = Vec::new();
        let mut current_name = String::new();
        let mut current_type = "Ethernet".to_owned();
        let mut current_mac = String::new();
        let mut current_ipv4 = String::new();
        let mut current_subnet = String::new();
        let mut current_gateway = String::new();
        let mut current_dhcp = "Static".to_owned();
        let mut in_adapter = false;

        for line in output.lines() {
            let trimmed = line.trim();

            if (line.starts_with("Ethernet adapter ")
                || line.starts_with("Wireless LAN adapter ")
                || line.starts_with("Carte Ethernet ")
                || line.starts_with("Carte réseau sans fil "))
                && line.contains(':')
            {
                if in_adapter && !current_name.is_empty() {
                    results.push(Self::build_snapshot(
                        &current_name,
                        &current_type,
                        &current_mac,
                        &current_ipv4,
                        &current_subnet,
                        &current_gateway,
                        &current_dhcp,
                    ));
                }

                in_adapter = true;
                let colon_idx = line.find(':').unwrap_or(line.len());
                let raw_name = &line[..colon_idx];
                current_name = raw_name
                    .trim_start_matches("Ethernet adapter ")
                    .trim_start_matches("Wireless LAN adapter ")
                    .trim_start_matches("Carte Ethernet ")
                    .trim_start_matches("Carte réseau sans fil ")
                    .trim()
                    .to_owned();

                current_type = if line.contains("Wireless") || line.contains("sans fil") {
                    "Wireless80211".to_owned()
                } else {
                    "Ethernet".to_owned()
                };

                current_mac.clear();
                current_ipv4.clear();
                current_subnet.clear();
                current_gateway.clear();
                current_dhcp = "Static".to_owned();
                continue;
            }

            if !in_adapter {
                continue;
            }

            if let Some((key, val)) = Self::split_key_val(trimmed) {
                let key_lower = key.to_lowercase();
                if key_lower.contains("physical address") || key_lower.contains("adresse physique")
                {
                    current_mac = val.replace('-', ":").to_uppercase();
                } else if key_lower.contains("ipv4") {
                    let ip_clean = val.split('(').next().unwrap_or(&val).trim();
                    current_ipv4 = ip_clean.to_owned();
                } else if key_lower.contains("subnet mask")
                    || key_lower.contains("masque de sous-réseau")
                {
                    current_subnet = val.to_owned();
                } else if key_lower.contains("default gateway")
                    || key_lower.contains("passerelle par défaut")
                {
                    if !val.is_empty() && current_gateway.is_empty() {
                        current_gateway = val.to_owned();
                    }
                } else if key_lower.contains("dhcp enabled") || key_lower.contains("dhcp activé") {
                    let val_lower = val.to_lowercase();
                    if val_lower.starts_with("yes") || val_lower.starts_with("oui") {
                        current_dhcp = "DHCP".to_owned();
                    } else {
                        current_dhcp = "Static".to_owned();
                    }
                }
            }
        }

        if in_adapter && !current_name.is_empty() {
            results.push(Self::build_snapshot(
                &current_name,
                &current_type,
                &current_mac,
                &current_ipv4,
                &current_subnet,
                &current_gateway,
                &current_dhcp,
            ));
        }

        results
    }

    fn split_key_val(line: &str) -> Option<(String, String)> {
        if let Some(colon) = line.find(':') {
            let key_part = line[..colon].trim().trim_end_matches('.');
            let val_part = line[colon + 1..].trim();
            Some((key_part.to_owned(), val_part.to_owned()))
        } else {
            None
        }
    }

    fn build_snapshot(
        name: &str,
        interface_type: &str,
        mac: &str,
        ipv4: &str,
        subnet: &str,
        gateway: &str,
        dhcp: &str,
    ) -> NicSnapshot {
        let status = if ipv4.is_empty() && mac.is_empty() {
            "Down".to_owned()
        } else {
            "Up".to_owned()
        };

        let speed = if interface_type == "Wireless80211" {
            "866 Mbps".to_owned()
        } else if status == "Up" {
            "1 Gbps".to_owned()
        } else {
            "-".to_owned()
        };

        NicSnapshot {
            name: name.to_owned(),
            interface_type: interface_type.to_owned(),
            status,
            speed,
            mac: mac.to_owned(),
            ipv4: ipv4.to_owned(),
            subnet: subnet.to_owned(),
            gateway: gateway.to_owned(),
            dhcp: dhcp.to_owned(),
        }
    }
}

/// Runs the system network interface listing command.
#[must_use]
pub fn run_interfaces_command() -> Vec<NicSnapshot> {
    #[cfg(windows)]
    let output = Command::new("ipconfig").args(["/all"]).output();

    #[cfg(not(windows))]
    let output = Command::new("ip").args(["addr"]).output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            IpconfigParser::parse(&stdout)
        }
        Err(_) => Vec::new(),
    }
}

/// Builds a CSV string from a slice of [`NicSnapshot`].
#[must_use]
pub fn build_interfaces_csv(entries: &[NicSnapshot]) -> String {
    let mut csv = String::from("Name,Type,Status,Speed,MAC,IPv4,Subnet,Gateway,DHCP\n");
    for nic in entries {
        csv.push_str(&nic.to_csv_line());
        csv.push('\n');
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nic_formatter_speed() {
        assert_eq!(NicFormatter::format_speed(0), "-");
        assert_eq!(NicFormatter::format_speed(54_000_000), "54 Mbps");
        assert_eq!(NicFormatter::format_speed(1_000_000_000), "1 Gbps");
    }

    #[test]
    fn test_nic_formatter_mac() {
        assert_eq!(
            NicFormatter::format_mac(&[0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E]),
            "00:1A:2B:3C:4D:5E"
        );
        assert_eq!(NicFormatter::format_mac(&[]), "");
    }

    #[test]
    fn test_ipconfig_parser_sample() {
        let sample = "\
Windows IP Configuration

Ethernet adapter vEthernet (Default Switch):

   Connection-specific DNS Suffix  . :
   Link-local IPv6 Address . . . . . : fe80::1
   IPv4 Address. . . . . . . . . . . : 172.24.16.1(Preferred)
   Subnet Mask . . . . . . . . . . . : 255.255.240.0
   Default Gateway . . . . . . . . . :
   DHCP Enabled. . . . . . . . . . . : No
   Physical Address. . . . . . . . . : 00-15-5D-12-34-56

Wireless LAN adapter Wi-Fi:

   Physical Address. . . . . . . . . : 3C-52-82-AA-BB-CC
   DHCP Enabled. . . . . . . . . . . : Yes
   IPv4 Address. . . . . . . . . . . : 192.168.1.105(Preferred)
   Subnet Mask . . . . . . . . . . . : 255.255.255.0
   Default Gateway . . . . . . . . . : 192.168.1.1
";
        let list = IpconfigParser::parse(sample);
        assert_eq!(list.len(), 2);

        assert_eq!(list[0].name, "vEthernet (Default Switch)");
        assert_eq!(list[0].interface_type, "Ethernet");
        assert_eq!(list[0].mac, "00:15:5D:12:34:56");
        assert_eq!(list[0].ipv4, "172.24.16.1");
        assert_eq!(list[0].subnet, "255.255.240.0");
        assert_eq!(list[0].dhcp, "Static");

        assert_eq!(list[1].name, "Wi-Fi");
        assert_eq!(list[1].interface_type, "Wireless80211");
        assert_eq!(list[1].mac, "3C:52:82:AA:BB:CC");
        assert_eq!(list[1].ipv4, "192.168.1.105");
        assert_eq!(list[1].gateway, "192.168.1.1");
        assert_eq!(list[1].dhcp, "DHCP");
    }
}
