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
//! Provides subnet host expansion, port probing, role classification,
//! and topology list / CSV export.

use std::net::Ipv4Addr;

/// Quick scan ports for network cartography.
pub const QUICK_RECON_PORTS: [u16; 12] =
    [22, 53, 80, 88, 139, 389, 443, 445, 1433, 3306, 3389, 8080];

/// A single discovered network node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredNode {
    pub ip: String,
    pub hostname: Option<String>,
    pub mac: Option<String>,
    pub vendor: Option<String>,
    pub open_ports: Vec<u16>,
    pub role: String,
    pub response_time_ms: u64,
}

/// Aggregated summary of cartography scan results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CartographySummary {
    pub hosts_scanned: usize,
    pub hosts_up: usize,
    pub open_ports_count: usize,
}

/// Classifies a host's primary role based on its set of open TCP ports.
#[must_use]
pub fn classify_role(open_ports: &[u16]) -> &'static str {
    let has = |port: u16| open_ports.contains(&port);

    if has(88) || has(389) || has(636) {
        return "Domain Controller / Directory";
    }

    if has(1433) || has(3306) || has(5432) || has(1521) || has(27017) || has(6379) {
        return "Database Server";
    }

    if has(25) || has(465) || has(587) || has(110) || has(143) || has(993) || has(995) {
        return "Mail Server";
    }

    if has(445) || has(139) || has(2049) {
        return "File / Storage Server";
    }

    if has(80) || has(443) || has(8080) || has(8443) {
        return "Web Server";
    }

    if has(22) || has(3389) || has(5900) || has(5985) || has(5986) {
        return "Remote Management Host";
    }

    if has(53) || has(67) || has(161) {
        return "Network Gateway / Appliance";
    }

    if !open_ports.is_empty() {
        "Active Node"
    } else {
        "Host (No Ports Open)"
    }
}

/// Expands a CIDR subnet notation (e.g. "192.168.1.0/24") into a list of usable host IP strings.
pub fn expand_subnet_ips(cidr: &str) -> Result<Vec<String>, String> {
    let (ip_str, prefix_str) = cidr
        .split_once('/')
        .ok_or_else(|| "CIDR must contain a '/' separator".to_owned())?;

    let ip: Ipv4Addr = ip_str
        .trim()
        .parse()
        .map_err(|e| format!("Invalid IPv4 address: {}", e))?;

    let prefix: u32 = prefix_str
        .trim()
        .parse()
        .map_err(|e| format!("Invalid prefix length: {}", e))?;

    if prefix > 32 || prefix < 16 {
        return Err("Prefix length must be between 16 and 32 for subnet cartography".to_owned());
    }

    let ip_u32 = u32::from(ip);
    let mask = if prefix == 0 {
        0
    } else {
        !((1u32 << (32 - prefix)) - 1)
    };
    let net = ip_u32 & mask;
    let broadcast = net | (!mask);

    let start = if prefix >= 31 { net } else { net + 1 };
    let end = if prefix >= 31 {
        broadcast
    } else {
        broadcast - 1
    };

    let mut out = Vec::new();
    for cur in start..=end {
        out.push(Ipv4Addr::from(cur).to_string());
    }

    Ok(out)
}

/// Builds CSV export content for discovered nodes.
#[must_use]
pub fn build_nodes_csv(nodes: &[DiscoveredNode]) -> String {
    let mut out = String::new();
    out.push_str("IP,Hostname,MAC,Vendor,OpenPorts,Role,ResponseTimeMs\n");

    for n in nodes {
        let host = n.hostname.as_deref().unwrap_or("-");
        let mac = n.mac.as_deref().unwrap_or("-");
        let vendor = n.vendor.as_deref().unwrap_or("-");
        let ports = n
            .open_ports
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(";");

        out.push_str(&format!(
            "{},\"{}\",{},\"{}\",\"{}\",\"{}\",{}\n",
            n.ip,
            host.replace('"', "\"\""),
            mac,
            vendor.replace('"', "\"\""),
            ports,
            n.role,
            n.response_time_ms
        ));
    }

    out
}

/// Discovers live nodes on a subnet by checking quick reconnaissance ports.
pub fn discover_subnet(
    cidr: &str,
    timeout_ms: u64,
) -> Result<(Vec<DiscoveredNode>, CartographySummary), String> {
    let ips = expand_subnet_ips(cidr)?;
    let mut nodes = Vec::new();
    let mut total_open = 0;
    let hosts_scanned = ips.len();

    for ip in ips {
        let mut open_ports = Vec::new();
        let mut fastest_ms = u64::MAX;

        for &port in &QUICK_RECON_PORTS {
            let start = std::time::Instant::now();
            let addr_str = format!("{}:{}", ip, port);
            if let Ok(s_addr) = addr_str.parse::<std::net::SocketAddr>() {
                if let Ok(_stream) = std::net::TcpStream::connect_timeout(
                    &s_addr,
                    std::time::Duration::from_millis(timeout_ms.min(300)),
                ) {
                    let elapsed = start.elapsed().as_millis() as u64;
                    if elapsed < fastest_ms {
                        fastest_ms = elapsed;
                    }
                    open_ports.push(port);
                }
            }
        }

        if !open_ports.is_empty() {
            total_open += open_ports.len();
            let role = classify_role(&open_ports).to_string();
            nodes.push(DiscoveredNode {
                ip,
                hostname: None,
                mac: None,
                vendor: None,
                open_ports,
                role,
                response_time_ms: if fastest_ms == u64::MAX {
                    0
                } else {
                    fastest_ms
                },
            });
        }
    }

    let hosts_up = nodes.len();
    Ok((
        nodes,
        CartographySummary {
            hosts_scanned,
            hosts_up,
            open_ports_count: total_open,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_classification_heuristics() {
        assert_eq!(classify_role(&[80, 443]), "Web Server");
        assert_eq!(classify_role(&[389, 88]), "Domain Controller / Directory");
        assert_eq!(classify_role(&[5432]), "Database Server");
        assert_eq!(classify_role(&[22, 3389]), "Remote Management Host");
        assert_eq!(classify_role(&[445]), "File / Storage Server");
        assert_eq!(classify_role(&[]), "Host (No Ports Open)");
    }

    #[test]
    fn expand_subnet_ips_valid_slash_29() {
        let ips = expand_subnet_ips("192.168.1.0/29").unwrap();
        assert_eq!(ips.len(), 6); // 8 - network - broadcast = 6
        assert_eq!(ips[0], "192.168.1.1");
        assert_eq!(ips[5], "192.168.1.6");
    }

    #[test]
    fn nodes_csv_export() {
        let nodes = vec![DiscoveredNode {
            ip: "192.168.1.10".to_owned(),
            hostname: Some("server01".to_owned()),
            mac: Some("00-50-56-11-22-33".to_owned()),
            vendor: Some("VMware".to_owned()),
            open_ports: vec![80, 443],
            role: "Web Server".to_owned(),
            response_time_ms: 3,
        }];

        let csv = build_nodes_csv(&nodes);
        assert!(csv.contains("IP,Hostname,MAC,Vendor,OpenPorts,Role,ResponseTimeMs"));
        assert!(csv.contains(
            "192.168.1.10,\"server01\",00-50-56-11-22-33,\"VMware\",\"80;443\",\"Web Server\",3"
        ));
    }
}
