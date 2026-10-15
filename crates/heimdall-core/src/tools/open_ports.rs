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

//! Inspection engine for active listening and established TCP and UDP ports.
//!
//! Provides portable parsing of netstat output and native inspection across
//! supported platforms.

use std::process::Command;

/// An open or listening TCP/UDP port entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PortEntry {
    /// Protocol: "TCP" or "UDP".
    pub protocol: String,
    /// Local IP or hostname.
    pub local_address: String,
    /// Local port number.
    pub local_port: u16,
    /// Remote IP or hostname.
    pub remote_address: String,
    /// Remote port number (0 if listening).
    pub remote_port: u16,
    /// Connection state (e.g. "LISTENING", "ESTABLISHED", "TIME_WAIT").
    pub state: String,
    /// Process ID owning the socket.
    pub pid: u32,
    /// Process name or executable.
    pub process_name: String,
}

impl PortEntry {
    /// Formats the port entry as a CSV line.
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{}",
            self.protocol,
            self.local_address,
            self.local_port,
            self.remote_address,
            self.remote_port,
            self.state,
            self.pid,
            self.process_name
        )
    }
}

/// Parser for `netstat -ano` output.
pub struct NetstatParser;

impl NetstatParser {
    /// Parses standard `netstat -ano` output into a list of [`PortEntry`].
    #[must_use]
    pub fn parse(output: &str) -> Vec<PortEntry> {
        let mut entries = Vec::new();

        for raw_line in output.lines() {
            let line = raw_line.trim();
            if line.is_empty()
                || line.starts_with("Active")
                || line.starts_with("Proto")
                || line.starts_with("Connexions")
            {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let proto_upper = parts[0].to_uppercase();
            if proto_upper.starts_with("TCP") {
                // Format: TCP  local_addr:port  foreign_addr:port  STATE  PID
                if parts.len() >= 5 {
                    let (local_addr, local_port) = Self::split_endpoint(parts[1]);
                    let (remote_addr, remote_port) = Self::split_endpoint(parts[2]);
                    let state = parts[3].to_uppercase();
                    let pid = parts[4].parse::<u32>().unwrap_or(0);
                    let process_name = Self::format_pid(pid);

                    entries.push(PortEntry {
                        protocol: "TCP".to_owned(),
                        local_address: local_addr,
                        local_port,
                        remote_address: remote_addr,
                        remote_port,
                        state,
                        pid,
                        process_name,
                    });
                }
            } else if proto_upper.starts_with("UDP") {
                // Format: UDP  local_addr:port  *:*  PID
                if parts.len() >= 4 {
                    let (local_addr, local_port) = Self::split_endpoint(parts[1]);
                    let (remote_addr, remote_port) = Self::split_endpoint(parts[2]);
                    let pid = parts[3].parse::<u32>().unwrap_or(0);
                    let process_name = Self::format_pid(pid);

                    entries.push(PortEntry {
                        protocol: "UDP".to_owned(),
                        local_address: local_addr,
                        local_port,
                        remote_address: remote_addr,
                        remote_port,
                        state: "UDP".to_owned(),
                        pid,
                        process_name,
                    });
                }
            }
        }

        entries
    }

    /// Splits an endpoint string like "127.0.0.1:8080" or "[::1]:80" into address and port.
    #[must_use]
    pub fn split_endpoint(endpoint: &str) -> (String, u16) {
        if let Some(colon_pos) = endpoint.rfind(':') {
            let addr = &endpoint[..colon_pos];
            let port_str = &endpoint[colon_pos + 1..];
            let port = port_str.parse::<u16>().unwrap_or(0);
            (
                addr.trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_owned(),
                port,
            )
        } else {
            (endpoint.to_owned(), 0)
        }
    }

    fn format_pid(pid: u32) -> String {
        if pid == 0 {
            "System Idle".to_owned()
        } else {
            format!("[{pid}]")
        }
    }
}

/// Executes the system `netstat -ano` command and parses the active connections.
#[must_use]
pub fn run_netstat_command() -> Vec<PortEntry> {
    #[cfg(windows)]
    let output = Command::new("netstat").args(["-ano"]).output();

    #[cfg(not(windows))]
    let output = Command::new("netstat").args(["-tulpen"]).output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            NetstatParser::parse(&stdout)
        }
        Err(_) => Vec::new(),
    }
}

/// Builds a CSV string from a slice of [`PortEntry`].
#[must_use]
pub fn build_open_ports_csv(entries: &[PortEntry]) -> String {
    let mut csv = String::from(
        "Protocol,LocalAddress,LocalPort,RemoteAddress,RemotePort,State,PID,ProcessName\n",
    );
    for entry in entries {
        csv.push_str(&entry.to_csv_line());
        csv.push('\n');
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_endpoint_ipv4() {
        let (addr, port) = NetstatParser::split_endpoint("192.168.1.50:443");
        assert_eq!(addr, "192.168.1.50");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_split_endpoint_ipv6() {
        let (addr, port) = NetstatParser::split_endpoint("[::1]:8080");
        assert_eq!(addr, "::1");
        assert_eq!(port, 8080);
    }

    #[test]
    fn test_netstat_parser_sample() {
        let sample = "\
Active Connections

  Proto  Local Address          Foreign Address        State           PID
  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1124
  TCP    127.0.0.1:49670        127.0.0.1:49671        ESTABLISHED     4520
  UDP    0.0.0.0:5353           *:*                                    2048
";
        let entries = NetstatParser::parse(sample);
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].protocol, "TCP");
        assert_eq!(entries[0].local_address, "0.0.0.0");
        assert_eq!(entries[0].local_port, 135);
        assert_eq!(entries[0].state, "LISTENING");
        assert_eq!(entries[0].pid, 1124);

        assert_eq!(entries[1].protocol, "TCP");
        assert_eq!(entries[1].local_port, 49670);
        assert_eq!(entries[1].remote_port, 49671);
        assert_eq!(entries[1].state, "ESTABLISHED");
        assert_eq!(entries[1].pid, 4520);

        assert_eq!(entries[2].protocol, "UDP");
        assert_eq!(entries[2].local_port, 5353);
        assert_eq!(entries[2].pid, 2048);
    }
}
