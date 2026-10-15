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

//! DNS record lookup engine, as the C# `DnsLookupService` and `NslookupOutputParser`: record
//! types, server presets, input validation, the `nslookup` program's arguments and output.
//!
//! An A or AAAA query to the system's resolver goes through the platform resolver, as the
//! C# `Dns.GetHostEntryAsync`; any other runs the system's `nslookup` by its full path, as
//! the C# `SystemExecutablePath.InSystemDirectory`, on a host checked first so it can never
//! be read as an option (`TryValidateLookupInputs`). The lookup itself, bounded in time and
//! stoppable, runs in `heimdall_app::net_tools`.

use std::fmt;
use std::net::IpAddr;
use std::path::PathBuf;

use super::net_host;

/// How long a lookup may take, as the C# `DnsLookupViewModel.LookupTimeout`.
pub const LOOKUP_TIMEOUT_MS: u64 = 5000;

/// The `nslookup` program of Windows, in its system folder, as the C#
/// `NslookupExecutableName`.
#[cfg(windows)]
pub const NSLOOKUP_PROGRAM: &str = "nslookup.exe";

/// Where the `nslookup` program lives on the other systems, tried in order.
#[cfg(not(windows))]
pub const NSLOOKUP_PROGRAMS: [&str; 2] = ["/usr/bin/nslookup", "/bin/nslookup"];

/// Supported DNS query record types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DnsRecordType {
    /// IPv4 address.
    A,
    /// IPv6 address.
    Aaaa,
    /// Mail exchange server.
    Mx,
    /// Canonical name alias.
    Cname,
    /// Text annotations / verification records.
    Txt,
    /// Authoritative name server.
    Ns,
    /// Pointer for reverse DNS lookup.
    Ptr,
    /// Start of authority zone info.
    Soa,
    /// Service location record.
    Srv,
    /// Any available records.
    Any,
}

impl DnsRecordType {
    /// Every supported record type in UI display order.
    pub const ALL: [Self; 10] = [
        Self::A,
        Self::Aaaa,
        Self::Mx,
        Self::Cname,
        Self::Txt,
        Self::Ns,
        Self::Ptr,
        Self::Soa,
        Self::Srv,
        Self::Any,
    ];

    /// The wire / command-line token for this record type.
    #[must_use]
    pub const fn wire_format(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::Aaaa => "AAAA",
            Self::Mx => "MX",
            Self::Cname => "CNAME",
            Self::Txt => "TXT",
            Self::Ns => "NS",
            Self::Ptr => "PTR",
            Self::Soa => "SOA",
            Self::Srv => "SRV",
            Self::Any => "ANY",
        }
    }
}

impl fmt::Display for DnsRecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire_format())
    }
}

/// Popular public DNS server presets; the view names them in the user's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DnsServerPreset {
    /// System default resolver.
    DefaultSystem,
    /// Cloudflare DNS (1.1.1.1).
    Cloudflare,
    /// Google Public DNS (8.8.8.8).
    Google,
    /// Quad9 DNS (9.9.9.9).
    Quad9,
    /// OpenDNS (208.67.222.222).
    OpenDns,
}

impl DnsServerPreset {
    /// All presets in order.
    pub const ALL: [Self; 5] = [
        Self::DefaultSystem,
        Self::Cloudflare,
        Self::Google,
        Self::Quad9,
        Self::OpenDns,
    ];

    /// IP address of the server, or `None` for system default.
    #[must_use]
    pub const fn server_ip(self) -> Option<&'static str> {
        match self {
            Self::DefaultSystem => None,
            Self::Cloudflare => Some("1.1.1.1"),
            Self::Google => Some("8.8.8.8"),
            Self::Quad9 => Some("9.9.9.9"),
            Self::OpenDns => Some("208.67.222.222"),
        }
    }

    /// The provider's name, a brand never translated; `None` for the system's resolver,
    /// which the view names in the user's language.
    #[must_use]
    pub const fn provider_name(self) -> Option<&'static str> {
        match self {
            Self::DefaultSystem => None,
            Self::Cloudflare => Some("Cloudflare"),
            Self::Google => Some("Google"),
            Self::Quad9 => Some("Quad9"),
            Self::OpenDns => Some("OpenDNS"),
        }
    }
}

/// A request to query DNS records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsLookupRequest {
    /// Target host name or domain, checked by [`validate_request`].
    pub host: String,
    /// The record type to query.
    pub record_type: DnsRecordType,
    /// Optional custom DNS server IP address.
    pub server: Option<String>,
}

/// The answer of a DNS record lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsLookupResult {
    /// Target host queried.
    pub host: String,
    /// Record type queried.
    pub record_type: DnsRecordType,
    /// DNS server used, if any.
    pub server: Option<String>,
    /// The answer, one line each, made safe to show; empty when nothing came back.
    pub records: Vec<String>,
    /// Execution duration in milliseconds.
    pub elapsed_ms: u64,
}

/// Why a lookup gave no answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsLookupError {
    /// It ran past [`LOOKUP_TIMEOUT_MS`], as the C# `ToolDnsErrorTimeout`.
    TimedOut,
    /// No `nslookup` program where the system keeps it.
    NoProgram,
    /// The resolver or the program failed: why, made safe to show, as the C#
    /// `ToolDnsErrorLookupFailed`.
    Failed(String),
}

/// Validates the typed host, as the C# `ValidateDomain` before a lookup; the server comes
/// from the presets, an address. Returns the request, or an error translation key.
pub fn validate_request(
    host_text: &str,
    record_type: DnsRecordType,
    server: DnsServerPreset,
) -> Result<DnsLookupRequest, &'static str> {
    if host_text.trim().is_empty() {
        return Err("ui-tool-dns-error-host-required");
    }
    let host = net_host::checked_host(host_text).ok_or("ui-tool-dns-error-host-invalid")?;
    Ok(DnsLookupRequest {
        host,
        record_type,
        server: server.server_ip().map(str::to_owned),
    })
}

/// Whether `request` goes to the platform resolver rather than to `nslookup`, as the C#
/// does for an A or AAAA query without a server.
#[must_use]
pub fn uses_system_resolver(request: &DnsLookupRequest) -> bool {
    request.server.is_none()
        && matches!(request.record_type, DnsRecordType::A | DnsRecordType::Aaaa)
}

/// The addresses of `record_type`'s family among `addresses`, each once, in order.
#[must_use]
pub fn addresses_of_type(
    addresses: impl IntoIterator<Item = IpAddr>,
    record_type: DnsRecordType,
) -> Vec<String> {
    let mut records: Vec<String> = Vec::new();
    for address in addresses {
        let wanted = match record_type {
            DnsRecordType::Aaaa => address.is_ipv6(),
            _ => address.is_ipv4(),
        };
        let shown = address.to_string();
        if wanted && !records.contains(&shown) {
            records.push(shown);
        }
    }
    records
}

/// The system's `nslookup` program, by its full path: never one found on the `PATH` or
/// beside Heimdall. `None` when the system has none where it keeps it.
#[must_use]
pub fn nslookup_program() -> Option<PathBuf> {
    #[cfg(windows)]
    return crate::paths::system_program(NSLOOKUP_PROGRAM);
    #[cfg(not(windows))]
    return NSLOOKUP_PROGRAMS
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file());
}

/// The arguments of `nslookup` for `request`, as the C# `CreateLocalNslookupStartInfo`:
/// the type, the host, the server. The host was checked by [`validate_request`], so it never
/// starts with "-".
#[must_use]
pub fn nslookup_arguments(request: &DnsLookupRequest) -> Vec<String> {
    let mut arguments = vec![
        format!("-type={}", request.record_type.wire_format()),
        request.host.clone(),
    ];
    arguments.extend(request.server.clone());
    arguments
}

/// The answer in `nslookup`'s output, one trimmed line each, as the C#
/// `NslookupOutputParser`: the first paragraph, the server that answered, left out, and the
/// blank lines. The paragraph is told by the blank line that ends it rather than by its
/// words, which Windows translates (`Serveur :`).
#[must_use]
pub fn clean_nslookup_output(raw: &str) -> Vec<String> {
    let lines: Vec<&str> = raw
        .lines()
        .map(str::trim)
        .skip_while(|line| line.is_empty())
        .collect();
    let body = match lines.iter().position(|line| line.is_empty()) {
        Some(blank) => &lines[blank..],
        None => &lines[..],
    };
    body.iter()
        .filter(|line| !line.is_empty())
        .map(|line| (*line).to_owned())
        .collect()
}

/// The answer of one `nslookup` run: its output's, else what it said on its error stream,
/// as the C#.
#[must_use]
pub fn nslookup_answer(stdout: &str, stderr: &str) -> Vec<String> {
    let answer = clean_nslookup_output(stdout);
    if !answer.is_empty() {
        return answer;
    }
    stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_types_all() {
        assert_eq!(DnsRecordType::ALL.len(), 10);
        assert_eq!(DnsRecordType::A.wire_format(), "A");
        assert_eq!(DnsRecordType::Aaaa.wire_format(), "AAAA");
        assert_eq!(DnsRecordType::Mx.wire_format(), "MX");
    }

    #[test]
    fn test_server_presets() {
        assert_eq!(DnsServerPreset::ALL.len(), 5);
        assert_eq!(DnsServerPreset::Cloudflare.server_ip(), Some("1.1.1.1"));
        assert_eq!(DnsServerPreset::Google.server_ip(), Some("8.8.8.8"));
        assert_eq!(DnsServerPreset::DefaultSystem.server_ip(), None);
    }

    #[test]
    fn a_host_that_reads_as_an_option_never_reaches_nslookup() {
        assert_eq!(
            validate_request("  ", DnsRecordType::Mx, DnsServerPreset::Google),
            Err("ui-tool-dns-error-host-required")
        );
        for host in [
            "-type=ANY",
            "-debug example.com",
            "example.com 8.8.8.8",
            "a;b",
        ] {
            assert_eq!(
                validate_request(host, DnsRecordType::Mx, DnsServerPreset::Google),
                Err("ui-tool-dns-error-host-invalid"),
                "{host}"
            );
        }
        let request = validate_request(" example.com ", DnsRecordType::Mx, DnsServerPreset::Google)
            .expect("valid");
        assert_eq!(
            nslookup_arguments(&request),
            ["-type=MX", "example.com", "8.8.8.8"]
        );
        assert!(!uses_system_resolver(&request));
    }

    #[test]
    fn a_and_aaaa_without_a_server_go_to_the_system_resolver() {
        let request = validate_request(
            "example.com",
            DnsRecordType::Aaaa,
            DnsServerPreset::DefaultSystem,
        )
        .expect("valid");
        assert!(uses_system_resolver(&request));
        let addresses: Vec<IpAddr> = vec![
            "93.184.216.34".parse().unwrap(),
            "2606:2800:220:1::".parse().unwrap(),
            "2606:2800:220:1::".parse().unwrap(),
        ];
        assert_eq!(
            addresses_of_type(addresses.clone(), DnsRecordType::Aaaa),
            ["2606:2800:220:1::"]
        );
        assert_eq!(
            addresses_of_type(addresses, DnsRecordType::A),
            ["93.184.216.34"]
        );
    }

    #[test]
    fn the_server_paragraph_is_left_out_in_english() {
        let raw = "\
Server:  dns.google
Address:  8.8.8.8

Non-authoritative answer:
example.com\tMX preference = 0, mail exchanger = .
";
        assert_eq!(
            clean_nslookup_output(raw),
            [
                "Non-authoritative answer:",
                "example.com\tMX preference = 0, mail exchanger = ."
            ]
        );
    }

    #[test]
    fn the_server_paragraph_is_left_out_in_french_and_on_linux() {
        let french = "Serveur :   UnKnown\r\nAddress:  192.168.1.1\r\n\r\n\
                      R\u{fffd}ponse ne faisant pas autorit\u{fffd} :\r\n\
                      Nom :    example.com\r\nAddresses:  2606:2800:220:1::\r\n          93.184.216.34\r\n\r\n";
        assert_eq!(
            clean_nslookup_output(french),
            [
                "R\u{fffd}ponse ne faisant pas autorit\u{fffd} :",
                "Nom :    example.com",
                "Addresses:  2606:2800:220:1::",
                "93.184.216.34"
            ]
        );
        let linux = "Server:\t\t127.0.0.53\nAddress:\t127.0.0.53#53\n\n\
                     Non-authoritative answer:\nexample.com\ttext = \"v=spf1 -all\"\n\n";
        assert_eq!(
            clean_nslookup_output(linux),
            [
                "Non-authoritative answer:",
                "example.com\ttext = \"v=spf1 -all\""
            ]
        );
    }

    #[test]
    fn an_error_stream_answers_when_the_output_says_nothing() {
        let stdout = "Server:  UnKnown\r\nAddress:  192.168.1.1\r\n\r\n";
        let stderr = "*** UnKnown can't find nope.invalid: Non-existent domain\r\n";
        assert_eq!(
            nslookup_answer(stdout, stderr),
            ["*** UnKnown can't find nope.invalid: Non-existent domain"]
        );
    }
}
