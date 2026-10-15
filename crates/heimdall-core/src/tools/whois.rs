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

//! The WHOIS tool's query and server, as the C# `WhoisLookupViewModel` and
//! `WhoisServerResolver`: what is asked, of which server, within which bounds. The exchange
//! itself is `heimdall_app::network_tools`'.

use std::net::IpAddr;
use std::time::Duration;

use super::net_input;

/// Port of the WHOIS protocol (RFC 3912).
pub const WHOIS_PORT: u16 = 43;

/// One deadline for the whole lookup (name, connection, query and reply), as the C#
/// `WhoisLookupService.QueryTimeout` linked to the caller's token.
pub const WHOIS_DEADLINE: Duration = Duration::from_secs(10);

/// Most bytes of a reply kept: a registry's answer is a few KiB, a server sending more is
/// cut there rather than read without end.
pub const MAX_REPLY_BYTES: usize = 64 * 1024;

/// IANA's WHOIS server, asked for a TLD the table does not name, as the C#.
pub const IANA_WHOIS_SERVER: &str = "whois.iana.org";

/// The WHOIS server of `domain`'s TLD, as the C# `WhoisServerResolver.GetWhoisServer`.
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

/// What is asked for `typed`, as the C# `WhoisLookupViewModel.LookupAsync` accepts it
/// (`InputValidator.ValidateDomain` or `IPAddress.TryParse`): an IP address, IPv4 or IPv6, as
/// typed; else a name the [`net_input::is_address`] rule accepts once a leading wildcard and
/// dots are stripped, the form the C# `TryCanonicalizeDomain` approves, which is what is sent.
/// `None` for anything else: nothing that could add a line to the query is ever sent.
#[must_use]
pub fn whois_query(typed: &str) -> Option<String> {
    let typed = typed.trim();
    if typed.parse::<IpAddr>().is_ok() {
        return Some(typed.to_owned());
    }
    let canonical = typed.trim_start_matches(['*', '.']);
    net_input::is_address(canonical).then(|| canonical.to_owned())
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
    fn the_query_is_a_checked_name_or_an_ip_address() {
        assert_eq!(whois_query(" example.com "), Some("example.com".to_owned()));
        assert_eq!(whois_query("*.example.com"), Some("example.com".to_owned()));
        assert_eq!(whois_query("1.1.1.1"), Some("1.1.1.1".to_owned()));
        assert_eq!(whois_query("2001:db8::1"), Some("2001:db8::1".to_owned()));
        for bad in [
            "",
            "example.com\r\nevil",
            "a..com",
            "-a.com",
            "exa mple.com",
        ] {
            assert_eq!(whois_query(bad), None, "{bad:?}");
        }
    }
}
