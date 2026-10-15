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

//! WHOIS client protocol engine and referral server resolver.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Standard port for WHOIS protocol (TCP 43).
pub const WHOIS_PORT: u16 = 43;

/// Default timeout for WHOIS server connection and response.
pub const DEFAULT_WHOIS_TIMEOUT: Duration = Duration::from_secs(10);

/// IANA global referral WHOIS server.
pub const IANA_WHOIS_SERVER: &str = "whois.iana.org";

/// Resolves the canonical WHOIS server hostname for a given domain or IP string.
#[must_use]
pub fn resolve_whois_server(domain: &str) -> &'static str {
    let trimmed = domain.trim();
    let Some(last_dot) = trimmed.rfind('.') else {
        return IANA_WHOIS_SERVER;
    };

    if last_dot == trimmed.len() - 1 {
        return IANA_WHOIS_SERVER;
    }

    let tld = &trimmed[last_dot + 1..].to_ascii_lowercase();
    match tld.as_str() {
        "com" | "net" => "whois.verisign-grs.com",
        "org" => "whois.pir.org",
        "io" => "whois.nic.io",
        "dev" => "whois.nic.google",
        "fr" => "whois.nic.fr",
        "de" => "whois.denic.de",
        "uk" => "whois.nic.uk",
        "eu" => "whois.eu",
        "nl" => "whois.domain-registry.nl",
        "be" => "whois.dns.be",
        "ch" => "whois.nic.ch",
        "au" => "whois.auda.org.au",
        "ca" => "whois.cira.ca",
        "jp" => "whois.jprs.jp",
        _ => IANA_WHOIS_SERVER,
    }
}

/// Request parameters for a WHOIS query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhoisLookupRequest {
    /// Domain name or IP address to lookup.
    pub domain: String,
}

/// Response from a WHOIS lookup attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhoisLookupResult {
    /// Target domain queried.
    pub domain: String,
    /// Raw response output from the WHOIS server.
    pub output: String,
    /// Query round-trip duration in milliseconds.
    pub elapsed_ms: u64,
    /// True if query completed successfully.
    pub success: bool,
    /// Error message if lookup failed.
    pub error: Option<String>,
}

/// Queries a WHOIS server directly over TCP port 43.
#[must_use]
pub fn lookup_whois(request: &WhoisLookupRequest) -> WhoisLookupResult {
    let domain = request.domain.trim();
    if domain.is_empty() {
        return WhoisLookupResult {
            domain: String::new(),
            output: String::new(),
            elapsed_ms: 0,
            success: false,
            error: Some("Domain or IP address cannot be empty".to_string()),
        };
    }

    let server = resolve_whois_server(domain);
    let start = Instant::now();
    let Ok(mut addrs) = (server, WHOIS_PORT).to_socket_addrs() else {
        return WhoisLookupResult {
            domain: domain.to_string(),
            output: String::new(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            success: false,
            error: Some(format!("Could not resolve WHOIS server {server}")),
        };
    };

    let Some(addr) = addrs.next() else {
        return WhoisLookupResult {
            domain: domain.to_string(),
            output: String::new(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            success: false,
            error: Some("No address found for WHOIS server".to_string()),
        };
    };

    let mut stream = match TcpStream::connect_timeout(&addr, DEFAULT_WHOIS_TIMEOUT) {
        Ok(s) => s,
        Err(e) => {
            return WhoisLookupResult {
                domain: domain.to_string(),
                output: String::new(),
                elapsed_ms: start.elapsed().as_millis() as u64,
                success: false,
                error: Some(e.to_string()),
            };
        }
    };

    let _ = stream.set_read_timeout(Some(DEFAULT_WHOIS_TIMEOUT));
    let _ = stream.set_write_timeout(Some(DEFAULT_WHOIS_TIMEOUT));

    let query = format!("{domain}\r\n");
    if let Err(e) = stream.write_all(query.as_bytes()) {
        return WhoisLookupResult {
            domain: domain.to_string(),
            output: String::new(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            success: false,
            error: Some(e.to_string()),
        };
    }

    let mut response = Vec::new();
    if let Err(e) = stream.read_to_end(&mut response) {
        return WhoisLookupResult {
            domain: domain.to_string(),
            output: String::new(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            success: false,
            error: Some(e.to_string()),
        };
    }

    let output = String::from_utf8_lossy(&response).to_string();
    WhoisLookupResult {
        domain: domain.to_string(),
        output,
        elapsed_ms: start.elapsed().as_millis() as u64,
        success: true,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_whois_server_tlds() {
        assert_eq!(
            resolve_whois_server("example.com"),
            "whois.verisign-grs.com"
        );
        assert_eq!(resolve_whois_server("test.org"), "whois.pir.org");
        assert_eq!(resolve_whois_server("infra.fr"), "whois.nic.fr");
        assert_eq!(resolve_whois_server("cloud.io"), "whois.nic.io");
        assert_eq!(resolve_whois_server("app.dev"), "whois.nic.google");
        assert_eq!(resolve_whois_server("unknown.xyz"), IANA_WHOIS_SERVER);
        assert_eq!(resolve_whois_server("192.168.1.1"), IANA_WHOIS_SERVER);
    }

    #[test]
    fn test_empty_domain_result() {
        let req = WhoisLookupRequest { domain: "".into() };
        let res = lookup_whois(&req);
        assert!(!res.success);
    }
}
