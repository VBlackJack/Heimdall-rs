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

//! IPv4 routing table inspection engine.
//!
//! Parses system route print outputs into route destination, mask, gateway,
//! interface, and metric entries.

use std::process::Command;

/// A single route table entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RouteEntry {
    /// Destination IP or subnet network address.
    pub destination: String,
    /// Subnet mask.
    pub mask: String,
    /// Next-hop gateway IP.
    pub gateway: String,
    /// Interface IP assigned to this route.
    pub interface: String,
    /// Route metric priority.
    pub metric: String,
}

impl RouteEntry {
    /// Formats the route entry as a CSV line.
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        format!(
            "{},{},{},{},{}",
            self.destination, self.mask, self.gateway, self.interface, self.metric
        )
    }
}

/// Parser for Windows `route print` output.
pub struct RoutePrintParser;

impl RoutePrintParser {
    /// Checks whether `line` indicates the IPv4 section header.
    #[must_use]
    pub fn is_ipv4_section_header(line: &str) -> bool {
        let lower = line.to_lowercase();
        lower.contains("ipv4") && (lower.contains("route") || lower.contains("routage"))
    }

    /// Checks whether `line` indicates active routes header.
    #[must_use]
    pub fn is_active_routes_header(line: &str) -> bool {
        let lower = line.to_lowercase();
        lower.contains("active") || lower.contains("actif") || lower.contains("itin")
    }

    /// Checks whether `line` indicates the end of the active IPv4 routes table.
    #[must_use]
    pub fn is_end_of_section(line: &str) -> bool {
        let lower = line.to_lowercase();
        line.starts_with("==")
            || lower.contains("persistent")
            || lower.contains("persistant")
            || lower.contains("ipv6")
    }

    /// Checks whether `line` is the table column header.
    #[must_use]
    pub fn is_column_header(line: &str) -> bool {
        let lower = line.to_lowercase();
        lower.contains("destination") || lower.contains("masque") || lower.contains("netmask")
    }

    /// Parses the text output of `route print` into a list of [`RouteEntry`].
    #[must_use]
    pub fn parse(output: &str) -> Vec<RouteEntry> {
        let mut entries = Vec::new();
        let mut in_ipv4_section = false;
        let mut in_active_routes = false;

        for raw_line in output.lines() {
            let line = raw_line.trim();

            if !in_ipv4_section && Self::is_ipv4_section_header(line) {
                in_ipv4_section = true;
                continue;
            }

            if in_ipv4_section && !in_active_routes && Self::is_active_routes_header(line) {
                in_active_routes = true;
                continue;
            }

            if in_active_routes && Self::is_column_header(line) {
                continue;
            }

            if in_active_routes && Self::is_end_of_section(line) {
                break;
            }

            if !in_active_routes || line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 5 {
                continue;
            }

            // Must look like an IP address
            if !parts[0].contains('.') {
                continue;
            }

            entries.push(RouteEntry {
                destination: parts[0].to_owned(),
                mask: parts[1].to_owned(),
                gateway: parts[2].to_owned(),
                interface: parts[3].to_owned(),
                metric: parts[4].to_owned(),
            });
        }

        entries
    }
}

/// Executes the system route command (`route print` or `ip route`) and parses entries.
#[must_use]
pub fn run_route_command() -> Vec<RouteEntry> {
    #[cfg(windows)]
    let output = Command::new("route").args(["print"]).output();

    #[cfg(not(windows))]
    let output = Command::new("ip").args(["route"]).output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            RoutePrintParser::parse(&stdout)
        }
        Err(_) => Vec::new(),
    }
}

/// Builds a CSV string from a slice of [`RouteEntry`].
#[must_use]
pub fn build_route_csv(entries: &[RouteEntry]) -> String {
    let mut csv = String::from("Destination,Netmask,Gateway,Interface,Metric\n");
    for rt in entries {
        csv.push_str(&rt.to_csv_line());
        csv.push('\n');
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_print_parser_sample() {
        let sample = "\
===========================================================================
Interface List
 15...00 15 5d 12 34 56 ......Hyper-V Virtual Ethernet Adapter
===========================================================================

IPv4 Route Table
===========================================================================
Active Routes:
Network Destination        Netmask          Gateway       Interface  Metric
          0.0.0.0          0.0.0.0      192.168.1.1    192.168.1.105     25
        127.0.0.0        255.0.0.0         On-link         127.0.0.1    331
        127.0.0.1  255.255.255.255         On-link         127.0.0.1    331
      192.168.1.0    255.255.255.0         On-link     192.168.1.105    281
===========================================================================
Persistent Routes:
  None
";
        let routes = RoutePrintParser::parse(sample);
        assert_eq!(routes.len(), 4);

        assert_eq!(routes[0].destination, "0.0.0.0");
        assert_eq!(routes[0].mask, "0.0.0.0");
        assert_eq!(routes[0].gateway, "192.168.1.1");
        assert_eq!(routes[0].interface, "192.168.1.105");
        assert_eq!(routes[0].metric, "25");

        assert_eq!(routes[3].destination, "192.168.1.0");
        assert_eq!(routes[3].mask, "255.255.255.0");
        assert_eq!(routes[3].gateway, "On-link");
        assert_eq!(routes[3].interface, "192.168.1.105");
        assert_eq!(routes[3].metric, "281");
    }
}
