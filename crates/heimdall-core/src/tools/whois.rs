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
//! itself is `heimdall_app::net_tools`'.

use std::time::Duration;

use super::net_host;

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
/// (`InputValidator.ValidateDomain` or `IPAddress.TryParse`): once a leading wildcard and
/// dots are stripped, as the C# `TryCanonicalizeDomain`, a host [`net_host::checked_host`]
/// takes, which is what is sent, without the root's trailing dot a registry would not know.
/// `None` for anything else: nothing that could add a line to the query is ever sent.
#[must_use]
pub fn whois_query(typed: &str) -> Option<String> {
    net_host::checked_host(typed.trim().trim_start_matches(['*', '.'])).map(|host| {
        match host.strip_suffix('.') {
            Some(name) => name.to_owned(),
            None => host,
        }
    })
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
        assert_eq!(whois_query("example.com."), Some("example.com".to_owned()));
        assert_eq!(whois_query("3com.com"), Some("3com.com".to_owned()));
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
