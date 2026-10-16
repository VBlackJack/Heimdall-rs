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

//! The open TCP and UDP ports with their owning process, as the C# `OpenPortsService` and
//! `PortMapper`.
//!
//! The C# reads `GetExtendedTcpTable` and `GetExtendedUdpTable` and names each process with
//! `Process.GetProcessById`. Without FFI, Windows `PowerShell` reads the same tables
//! (`Get-NetTCPConnection`, `Get-NetUDPEndpoint`) and the processes (`Get-Process`) as
//! JSON: numbers and addresses, never words of the user's language. On Linux, `ss` lists
//! the sockets, untranslated.

use serde::Deserialize;

use super::system_listing::{self, ListingError, one_or_many, text_or_null};

/// Lists the TCP connections, the UDP endpoints and the processes as JSON, the TCP state as
/// its number. A table with no row is not a failure (`Get-NetTCPConnection` reports an
/// `ObjectNotFound` error then); any other error is. No double quote: see
/// [`system_listing::run_powershell`].
#[cfg(windows)]
const LISTING_SCRIPT: &str = "$tcpErrors = $null; $udpErrors = $null; \
     $tcp = @(Get-NetTCPConnection -ErrorAction SilentlyContinue -ErrorVariable tcpErrors | \
     Select-Object LocalAddress,LocalPort,RemoteAddress,RemotePort,State,OwningProcess); \
     $udp = @(Get-NetUDPEndpoint -ErrorAction SilentlyContinue -ErrorVariable udpErrors | \
     Select-Object LocalAddress,LocalPort,OwningProcess); \
     foreach ($failure in @($tcpErrors) + @($udpErrors)) { \
     if ($failure.CategoryInfo.Category -ne 'ObjectNotFound') { throw $failure } }; \
     $processes = @(Get-Process | Select-Object Id,ProcessName); \
     ConvertTo-Json -Compress -Depth 3 -InputObject @{ tcp = $tcp; udp = $udp; processes = $processes }";

/// Where `ss` is looked for: by its full path, never on the `PATH`.
#[cfg(target_os = "linux")]
const SS_CANDIDATES: [&str; 4] = ["/usr/sbin/ss", "/sbin/ss", "/usr/bin/ss", "/bin/ss"];

/// `ss` switches: TCP and UDP, listening and connected, numeric, with the process, no
/// header.
#[cfg(target_os = "linux")]
const SS_ARGUMENTS: [&str; 6] = ["-t", "-u", "-a", "-n", "-p", "-H"];

/// The TCP state of a socket only bound, which `Get-NetTCPConnection` lists and the C#
/// `GetExtendedTcpTable` does not: left out.
const BOUND_STATE: u32 = 100;

/// The remote end of a UDP endpoint, which has none, as the C# `PortMapper`.
pub const UDP_REMOTE_ADDRESS: &str = "*";

/// The process that owns the sockets of no process: Windows' idle process.
pub const IDLE_PID: u32 = 0;

/// The protocol of a socket, as the C# `PortMapper` names them, in the order it lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Protocol {
    /// TCP over IPv4.
    Tcp,
    /// UDP over IPv4.
    Udp,
    /// TCP over IPv6.
    Tcp6,
    /// UDP over IPv6.
    Udp6,
}

impl Protocol {
    /// The protocol's name, as the C# and `netstat` write it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Tcp => "TCP",
            Self::Udp => "UDP",
            Self::Tcp6 => "TCP6",
            Self::Udp6 => "UDP6",
        }
    }

    /// TCP or UDP on the family of `local_address`: IPv6 when it holds a colon.
    fn of(tcp: bool, local_address: &str) -> Self {
        match (tcp, local_address.contains(':')) {
            (true, false) => Self::Tcp,
            (true, true) => Self::Tcp6,
            (false, false) => Self::Udp,
            (false, true) => Self::Udp6,
        }
    }
}

/// A socket's state, as the C# `TcpConnectionStateTable` names the TCP ones: the RFC 793
/// state names, protocol words that `netstat` does not translate either.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PortState {
    /// `CLOSED`.
    Closed,
    /// `LISTEN`.
    Listen,
    /// `SYN_SENT`.
    SynSent,
    /// `SYN_RCVD`.
    SynReceived,
    /// `ESTABLISHED`.
    Established,
    /// `FIN_WAIT1`.
    FinWait1,
    /// `FIN_WAIT2`.
    FinWait2,
    /// `CLOSE_WAIT`.
    CloseWait,
    /// `CLOSING`.
    Closing,
    /// `LAST_ACK`.
    LastAck,
    /// `TIME_WAIT`.
    TimeWait,
    /// `DELETE_TCB`.
    DeleteTcb,
    /// A UDP endpoint, always `LISTENING` as in the C#.
    UdpListening,
    /// A state the tables know and these names do not: as the system gives it.
    Other(String),
}

impl PortState {
    /// The state numbered `code` in the Windows TCP table (`MIB_TCP_STATE`, which
    /// `Get-NetTCPConnection` keeps), as the C# `TcpConnectionStateTable.NameOf`; `None`
    /// for a socket only bound, which the C# table does not hold.
    #[must_use]
    pub fn from_windows_code(code: u32) -> Option<Self> {
        Some(match code {
            1 => Self::Closed,
            2 => Self::Listen,
            3 => Self::SynSent,
            4 => Self::SynReceived,
            5 => Self::Established,
            6 => Self::FinWait1,
            7 => Self::FinWait2,
            8 => Self::CloseWait,
            9 => Self::Closing,
            10 => Self::LastAck,
            11 => Self::TimeWait,
            12 => Self::DeleteTcb,
            BOUND_STATE => return None,
            other => Self::Other(other.to_string()),
        })
    }

    /// The TCP state `ss` writes as `state`.
    #[must_use]
    pub fn from_ss(state: &str) -> Self {
        match state {
            "CLOSED" | "UNCONN" => Self::Closed,
            "LISTEN" => Self::Listen,
            "SYN-SENT" => Self::SynSent,
            "SYN-RECV" => Self::SynReceived,
            "ESTAB" => Self::Established,
            "FIN-WAIT-1" => Self::FinWait1,
            "FIN-WAIT-2" => Self::FinWait2,
            "CLOSE-WAIT" => Self::CloseWait,
            "CLOSING" => Self::Closing,
            "LAST-ACK" => Self::LastAck,
            "TIME-WAIT" => Self::TimeWait,
            other => Self::Other(other.to_owned()),
        }
    }

    /// The state's name, as the C# writes it.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Closed => "CLOSED",
            Self::Listen => "LISTEN",
            Self::SynSent => "SYN_SENT",
            Self::SynReceived => "SYN_RCVD",
            Self::Established => "ESTABLISHED",
            Self::FinWait1 => "FIN_WAIT1",
            Self::FinWait2 => "FIN_WAIT2",
            Self::CloseWait => "CLOSE_WAIT",
            Self::Closing => "CLOSING",
            Self::LastAck => "LAST_ACK",
            Self::TimeWait => "TIME_WAIT",
            Self::DeleteTcb => "DELETE_TCB",
            Self::UdpListening => "LISTENING",
            Self::Other(state) => state,
        }
    }
}

/// An open or listening TCP/UDP port entry, as the C# `PortEntry`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PortEntry {
    /// TCP or UDP, over IPv4 or IPv6.
    pub protocol: Protocol,
    /// Local address.
    pub local_address: String,
    /// Local port number.
    pub local_port: u16,
    /// Remote address; [`UDP_REMOTE_ADDRESS`] for a UDP endpoint.
    pub remote_address: String,
    /// Remote port number (0 if listening).
    pub remote_port: u16,
    /// The socket's state.
    pub state: PortState,
    /// The owning process's identifier, when the system says it (on Linux, only for the
    /// user's own processes unless Heimdall runs as root).
    pub pid: Option<u32>,
    /// The owning process's name, when it could be had: `None` for a process gone since,
    /// which the C# writes as "[pid]" (or "System Idle" for 0).
    pub process_name: Option<String>,
}

/// A row of `Get-NetTCPConnection`.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TcpRow {
    #[serde(default, deserialize_with = "text_or_null")]
    local_address: String,
    local_port: u16,
    #[serde(default, deserialize_with = "text_or_null")]
    remote_address: String,
    remote_port: u16,
    state: u32,
    owning_process: u32,
}

/// A row of `Get-NetUDPEndpoint`.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct UdpRow {
    #[serde(default, deserialize_with = "text_or_null")]
    local_address: String,
    local_port: u16,
    owning_process: u32,
}

/// A row of `Get-Process`.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ProcessRow {
    id: u32,
    #[serde(default, deserialize_with = "text_or_null")]
    process_name: String,
}

/// What [`LISTING_SCRIPT`] writes.
#[derive(Deserialize)]
struct WindowsListing {
    #[serde(default, deserialize_with = "one_or_many")]
    tcp: Vec<TcpRow>,
    #[serde(default, deserialize_with = "one_or_many")]
    udp: Vec<UdpRow>,
    #[serde(default, deserialize_with = "one_or_many")]
    processes: Vec<ProcessRow>,
}

/// The sockets of the JSON the Windows listing script writes, each with its process's name,
/// in the C# order (TCP, UDP, TCP6, UDP6), then by local port.
///
/// # Errors
///
/// The JSON is not the listing's.
pub fn parse_windows_listing(json: &str) -> Result<Vec<PortEntry>, ListingError> {
    let listing: WindowsListing = system_listing::parse_json(json)?;
    let names: std::collections::HashMap<u32, String> = listing
        .processes
        .into_iter()
        .filter(|process| !process.process_name.is_empty())
        .map(|process| (process.id, process.process_name))
        .collect();
    let name = |pid: u32| names.get(&pid).cloned();
    let mut entries: Vec<PortEntry> = listing
        .tcp
        .into_iter()
        .filter_map(|row| {
            Some(PortEntry {
                protocol: Protocol::of(true, &row.local_address),
                state: PortState::from_windows_code(row.state)?,
                pid: Some(row.owning_process),
                process_name: name(row.owning_process),
                local_address: row.local_address,
                local_port: row.local_port,
                remote_address: row.remote_address,
                remote_port: row.remote_port,
            })
        })
        .chain(listing.udp.into_iter().map(|row| PortEntry {
            protocol: Protocol::of(false, &row.local_address),
            state: PortState::UdpListening,
            pid: Some(row.owning_process),
            process_name: name(row.owning_process),
            local_address: row.local_address,
            local_port: row.local_port,
            remote_address: UDP_REMOTE_ADDRESS.to_owned(),
            remote_port: 0,
        }))
        .collect();
    sort(&mut entries);
    Ok(entries)
}

/// The sockets `ss -tuanpH` lists, in the same order as on Windows: `Netid State Recv-Q
/// Send-Q Local Peer [Process]` per line, the process as `users:(("name",pid=N,fd=N))` when
/// it can be seen.
#[must_use]
pub fn parse_ss(output: &str) -> Vec<PortEntry> {
    let mut entries: Vec<PortEntry> = output
        .lines()
        .filter_map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            let [netid, state, _receive, _send, local, peer, process @ ..] = words.as_slice()
            else {
                return None;
            };
            let tcp = match *netid {
                "tcp" => true,
                "udp" => false,
                _ => return None,
            };
            let (local_address, local_port) = split_endpoint(local)?;
            let (remote_address, remote_port) = split_endpoint(peer)?;
            let (process_name, pid) = ss_process(&process.join(" "));
            Some(PortEntry {
                protocol: Protocol::of(tcp, &local_address),
                state: if tcp {
                    PortState::from_ss(state)
                } else {
                    PortState::UdpListening
                },
                pid,
                process_name,
                local_address,
                local_port,
                remote_address: if tcp {
                    remote_address
                } else {
                    UDP_REMOTE_ADDRESS.to_owned()
                },
                remote_port: if tcp { remote_port } else { 0 },
            })
        })
        .collect();
    sort(&mut entries);
    entries
}

/// The C# order: protocol, then local port, then local address.
fn sort(entries: &mut [PortEntry]) {
    entries.sort_by(|a, b| {
        (a.protocol, a.local_port, &a.local_address).cmp(&(
            b.protocol,
            b.local_port,
            &b.local_address,
        ))
    });
}

/// An `ss` endpoint (`10.0.0.1:22`, `[::1]:323`, `*:*`, `127.0.0.53%lo:53`) as its address
/// and port, `*` as port 0.
fn split_endpoint(endpoint: &str) -> Option<(String, u16)> {
    let (address, port) = endpoint.rsplit_once(':')?;
    let port = if port == "*" { 0 } else { port.parse().ok()? };
    let address = address
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(address);
    Some((address.to_owned(), port))
}

/// The first process of an `ss` process column, `users:(("name",pid=N,fd=N),...)`: its name
/// and identifier, `None` for what is not there.
fn ss_process(column: &str) -> (Option<String>, Option<u32>) {
    const NAME_START: &str = "((\"";
    const NAME_END: &str = "\",";
    const PID_KEY: &str = "pid=";
    let Some(start) = column.find(NAME_START) else {
        return (None, None);
    };
    let rest = &column[start + NAME_START.len()..];
    let Some(end) = rest.find(NAME_END) else {
        return (None, None);
    };
    let name = &rest[..end];
    let pid = rest[end..].find(PID_KEY).and_then(|at| {
        let digits: String = rest[end + at + PID_KEY.len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok()
    });
    ((!name.is_empty()).then(|| name.to_owned()), pid)
}

/// Lists the system's open ports.
///
/// # Errors
///
/// The system has no way to list them, or the listing could not be started, did not end in
/// time, failed, or wrote something else than asked.
pub fn run_open_ports() -> Result<Vec<PortEntry>, ListingError> {
    #[cfg(windows)]
    {
        let json = system_listing::run_powershell(LISTING_SCRIPT, system_listing::LIST_TIME_LIMIT)?;
        parse_windows_listing(&json)
    }
    #[cfg(target_os = "linux")]
    {
        let ss = system_listing::find_program(&SS_CANDIDATES).ok_or(ListingError::NotAvailable)?;
        let output =
            system_listing::run_linux(&ss, &SS_ARGUMENTS, system_listing::LIST_TIME_LIMIT)?;
        Ok(parse_ss(&output))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(ListingError::NotAvailable)
    }
}

/// `entries` as CSV under `headers` (protocol, local address, local port, remote address,
/// remote port, state, PID, process), the process as `process` words it: every cell quoted
/// and a formula defused, a process name being chosen by whoever started it.
#[must_use]
pub fn build_open_ports_csv(
    entries: &[PortEntry],
    headers: &[String],
    process: impl Fn(&PortEntry) -> String,
) -> String {
    system_listing::csv_document(
        headers,
        entries.iter().map(|entry| {
            vec![
                entry.protocol.name().to_owned(),
                entry.local_address.clone(),
                entry.local_port.to_string(),
                entry.remote_address.clone(),
                entry.remote_port.to_string(),
                entry.state.name().to_owned(),
                entry.pid.map(|pid| pid.to_string()).unwrap_or_default(),
                process(entry),
            ]
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As the Windows listing script writes it on a French Windows: names and numbers only.
    const WINDOWS_FIXTURE: &str = r#"{"tcp":[
        {"LocalAddress":"::","LocalPort":65074,"RemoteAddress":"::","RemotePort":0,"State":100,"OwningProcess":32108},
        {"LocalAddress":"0.0.0.0","LocalPort":135,"RemoteAddress":"0.0.0.0","RemotePort":0,"State":2,"OwningProcess":1124},
        {"LocalAddress":"192.168.1.66","LocalPort":50432,"RemoteAddress":"140.82.112.4","RemotePort":443,"State":5,"OwningProcess":4520},
        {"LocalAddress":"::1","LocalPort":49670,"RemoteAddress":"::1","RemotePort":49671,"State":11,"OwningProcess":0}],
      "udp":{"LocalAddress":"fe80::1%67","LocalPort":5353,"OwningProcess":9999},
      "processes":[{"Id":0,"ProcessName":"Idle"},{"Id":1124,"ProcessName":"svchost"},
        {"Id":4520,"ProcessName":"=cmd|' /C calc'!A0"}]}"#;

    #[test]
    fn the_windows_listing_names_each_process_and_leaves_bound_sockets_out() {
        let entries = parse_windows_listing(WINDOWS_FIXTURE).expect("parses");
        let seen: Vec<(Protocol, u16, &str, Option<u32>, Option<&str>)> = entries
            .iter()
            .map(|entry| {
                (
                    entry.protocol,
                    entry.local_port,
                    entry.state.name(),
                    entry.pid,
                    entry.process_name.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                (Protocol::Tcp, 135, "LISTEN", Some(1124), Some("svchost")),
                (
                    Protocol::Tcp,
                    50432,
                    "ESTABLISHED",
                    Some(4520),
                    Some("=cmd|' /C calc'!A0")
                ),
                (Protocol::Tcp6, 49670, "TIME_WAIT", Some(0), Some("Idle")),
                // A process gone since the sockets were read has no name: the view says
                // its identifier.
                (Protocol::Udp6, 5353, "LISTENING", Some(9999), None),
            ]
        );
        assert_eq!(entries[1].remote_address, "140.82.112.4");
        assert_eq!(entries[1].remote_port, 443);
        assert_eq!(entries[3].remote_address, UDP_REMOTE_ADDRESS);
        assert_eq!(entries[3].local_address, "fe80::1%67");
    }

    #[test]
    fn an_empty_windows_listing_is_no_socket_and_garbage_is_a_failure() {
        assert_eq!(
            parse_windows_listing(r#"{"tcp":[],"udp":null,"processes":[]}"#),
            Ok(vec![])
        );
        assert!(matches!(
            parse_windows_listing("Get-NetTCPConnection : Accès refusé"),
            Err(ListingError::Unreadable(_))
        ));
    }

    #[test]
    fn a_tcp_state_without_a_name_is_kept_as_its_number() {
        assert_eq!(
            PortState::from_windows_code(42),
            Some(PortState::Other("42".to_owned()))
        );
        assert_eq!(PortState::from_windows_code(BOUND_STATE), None);
    }

    #[test]
    fn ss_output_is_read_with_its_processes_when_visible() {
        let output = "\
udp   UNCONN 0      0      10.255.255.254:53   0.0.0.0:*
udp   UNCONN 0      0               [::1]:323     [::]:*    users:((\"chronyd\",pid=612,fd=6))
tcp   LISTEN 0      4096        127.0.0.1:2222 0.0.0.0:*    users:((\"sshd\",pid=901,fd=3),(\"sshd\",pid=902,fd=3))
tcp   ESTAB  0      0      172.30.86.176:22  172.30.80.1:51514 users:((\"sshd: me [priv]\",pid=1200,fd=4))
tcp   TIME-WAIT 0   0      127.0.0.53%lo:53  127.0.0.1:40000
nl    UNCONN 0      0      rtnl:kernel         *
";
        let entries = parse_ss(output);
        let seen: Vec<(Protocol, &str, u16, &str, Option<u32>, Option<&str>)> = entries
            .iter()
            .map(|entry| {
                (
                    entry.protocol,
                    entry.local_address.as_str(),
                    entry.local_port,
                    entry.state.name(),
                    entry.pid,
                    entry.process_name.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                (
                    Protocol::Tcp,
                    "172.30.86.176",
                    22,
                    "ESTABLISHED",
                    Some(1200),
                    Some("sshd: me [priv]")
                ),
                (Protocol::Tcp, "127.0.0.53%lo", 53, "TIME_WAIT", None, None),
                (
                    Protocol::Tcp,
                    "127.0.0.1",
                    2222,
                    "LISTEN",
                    Some(901),
                    Some("sshd")
                ),
                (Protocol::Udp, "10.255.255.254", 53, "LISTENING", None, None),
                (
                    Protocol::Udp6,
                    "::1",
                    323,
                    "LISTENING",
                    Some(612),
                    Some("chronyd")
                ),
            ]
        );
        assert_eq!(entries[0].remote_address, "172.30.80.1");
        assert_eq!(entries[0].remote_port, 51514);
        assert_eq!(entries[3].remote_address, UDP_REMOTE_ADDRESS);
    }

    #[test]
    fn the_csv_quotes_every_cell_and_defuses_a_process_named_as_a_formula() {
        let entries = parse_windows_listing(WINDOWS_FIXTURE).expect("parses");
        let headers: Vec<String> = ["P", "LA", "LP", "RA", "RP", "S", "PID", "Process"]
            .map(str::to_owned)
            .to_vec();
        let csv = build_open_ports_csv(&entries[1..2], &headers, |entry| {
            entry.process_name.clone().unwrap_or_default()
        });
        assert_eq!(
            csv,
            "\"P\",\"LA\",\"LP\",\"RA\",\"RP\",\"S\",\"PID\",\"Process\"\n\
             \"TCP\",\"192.168.1.66\",\"50432\",\"140.82.112.4\",\"443\",\"ESTABLISHED\",\"4520\",\"'=cmd|' /C calc'!A0\"\n"
        );
    }
}
