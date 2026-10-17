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

//! The address a web profile opens, built from its host, port, scheme and path, as the C#
//! `WebAddress.TryBuild`: derived, never stored, so it cannot disagree with the host the tree,
//! the search and the reachability check read.
//!
//! The scheme is `http` or `https` and nothing else, chosen by a switch, never typed. The
//! host is a name, an IPv4 address or an IPv6 address, and nothing that could carry a scheme,
//! credentials, a path or a second host (`javascript:`, `user@`, `/`, `\`, `?`, `#`). The
//! path is percent-encoded where a browser or the system's protocol handler could read it as
//! something else. No credential ever goes into the address.

use std::fmt::Write as _;
use std::net::{Ipv4Addr, Ipv6Addr};

/// The scheme of a console over TLS.
pub const HTTPS_SCHEME: &str = "https";

/// The scheme of a console in clear, as the C# "Use plain HTTP (unencrypted)".
pub const HTTP_SCHEME: &str = "http";

/// Port of a web console over HTTPS unless a profile says otherwise, as the C#
/// `DefaultPorts.HttpsStd`.
pub const DEFAULT_HTTPS_PORT: u16 = 443;

/// Port of a web console over plain HTTP unless a profile says otherwise, as the C#
/// `DefaultPorts.HttpStd`.
pub const DEFAULT_HTTP_PORT: u16 = 80;

/// The longest host name, as DNS bounds one.
const MAX_HOST_LENGTH: usize = 253;

/// The longest label of a host name, between two dots.
const MAX_LABEL_LENGTH: usize = 63;

/// The scheme a profile uses: plain HTTP only when it asks for it.
#[must_use]
pub const fn scheme_of(use_http: bool) -> &'static str {
    if use_http { HTTP_SCHEME } else { HTTPS_SCHEME }
}

/// The port a scheme uses when the profile names none, as the C# `DefaultPortFor`.
#[must_use]
pub const fn default_port_for(use_http: bool) -> u16 {
    if use_http {
        DEFAULT_HTTP_PORT
    } else {
        DEFAULT_HTTPS_PORT
    }
}

/// Why a web profile's host, port and path form no address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebAddressError {
    /// No host is given.
    HostMissing,
    /// The host is not a name, an IPv4 or an IPv6 address: a URL pasted into it, a scheme,
    /// credentials, a path, a space or a control character.
    HostInvalid,
    /// The port is 0.
    PortInvalid,
}

/// The absolute address of a console on `host` and `port`, over HTTP when `use_http`, else
/// HTTPS, opening `path`: the scheme's own port left out, as the C# address shows it.
///
/// # Errors
///
/// A [`WebAddressError`] when they form none.
pub fn address(
    host: &str,
    port: u16,
    use_http: bool,
    path: Option<&str>,
) -> Result<String, WebAddressError> {
    if port == 0 {
        return Err(WebAddressError::PortInvalid);
    }
    let host = authority_host(host)?;
    let mut url = format!("{}://{host}", scheme_of(use_http));
    if port != default_port_for(use_http) {
        // Writing to a string cannot fail.
        let _ = write!(url, ":{port}");
    }
    url.push_str(&normalized_path(path.unwrap_or_default()));
    Ok(url)
}

/// `typed` as the host part of an address: a name or an IPv4 address as typed, an IPv6
/// address in brackets, written the standard way.
///
/// # Errors
///
/// [`WebAddressError::HostMissing`] for nothing typed, [`WebAddressError::HostInvalid`] for
/// anything but a host.
pub fn authority_host(typed: &str) -> Result<String, WebAddressError> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err(WebAddressError::HostMissing);
    }
    // An IPv6 address, bracketed or not: no zone (`%eth0`), which a browser refuses in an
    // address, and nothing around it.
    let bare = typed
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(typed);
    if bare.contains(':') {
        return bare
            .parse::<Ipv6Addr>()
            .map(|address| format!("[{address}]"))
            .map_err(|_| WebAddressError::HostInvalid);
    }
    if bare.len() != typed.len() {
        // Brackets around anything but an IPv6 address.
        return Err(WebAddressError::HostInvalid);
    }
    if typed.parse::<Ipv4Addr>().is_ok() || is_host_name(typed) {
        Ok(typed.to_owned())
    } else {
        Err(WebAddressError::HostInvalid)
    }
}

/// `typed` as the host a tunnel's gateway dials: checked as [`authority_host`] checks it,
/// an IPv6 address without its brackets.
///
/// # Errors
///
/// As [`authority_host`].
pub fn forward_host(typed: &str) -> Result<String, WebAddressError> {
    let host = authority_host(typed)?;
    Ok(host
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .map_or_else(|| host.clone(), str::to_owned))
}

/// Whether `typed` is a host name: ASCII letters, digits, hyphens and underscores, in labels
/// of at most [`MAX_LABEL_LENGTH`] between dots, none empty but a last one (a fully
/// qualified name's trailing dot), none starting or ending with a hyphen; and not ending in
/// a number, which a browser reads as an IPv4 address in another form (`2130706433`,
/// `0x7f.1`, `010.0.0.1`): the tree would show one address and the browser open another.
fn is_host_name(typed: &str) -> bool {
    let name = typed.strip_suffix('.').unwrap_or(typed);
    let last = name.rsplit('.').next().unwrap_or_default();
    // The WHATWG URL parser's "ends in a number": decimal digits, or `0x` and hex digits.
    let ends_in_number = last.bytes().all(|byte| byte.is_ascii_digit())
        || last
            .strip_prefix("0x")
            .or_else(|| last.strip_prefix("0X"))
            .is_some_and(|hex| hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
    !name.is_empty()
        && !ends_in_number
        && typed.len() <= MAX_HOST_LENGTH
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= MAX_LABEL_LENGTH
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
}

/// The path, query and fragment of the address, as the C# `NormalizePath`: `/` for none, a
/// `/` put before one typed without it; then every character a browser or the system's
/// protocol handler could read as something else percent-encoded: spaces, quotes, angle
/// brackets, backslashes, control and non-ASCII characters. A `%` typed is kept, the path
/// being taken as already encoded where it is.
#[must_use]
pub fn normalized_path(typed: &str) -> String {
    let trimmed = typed.trim();
    let mut path = String::with_capacity(trimmed.len() + 1);
    if !trimmed.starts_with(['/', '?', '#']) {
        path.push('/');
    }
    let mut utf8 = [0_u8; 4];
    for ch in trimmed.chars() {
        if ch.is_ascii_graphic()
            && !matches!(
                ch,
                '"' | '\'' | '<' | '>' | '\\' | '`' | '^' | '{' | '|' | '}'
            )
        {
            path.push(ch);
        } else {
            for byte in ch.encode_utf8(&mut utf8).bytes() {
                let _ = write!(path, "%{byte:02X}");
            }
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_https_console_on_its_own_port_has_no_port_in_its_address() {
        assert_eq!(
            address("ilo.lab", 443, false, None).as_deref(),
            Ok("https://ilo.lab/")
        );
        assert_eq!(
            address("ilo.lab", 80, true, Some("login.html")).as_deref(),
            Ok("http://ilo.lab/login.html")
        );
    }

    #[test]
    fn another_port_is_written_and_the_scheme_default_moves_with_the_scheme() {
        assert_eq!(
            address("idrac.lab", 8443, false, Some("/start")).as_deref(),
            Ok("https://idrac.lab:8443/start")
        );
        assert_eq!(
            address("nas.lab", 443, true, None).as_deref(),
            Ok("http://nas.lab:443/"),
            "443 is not plain HTTP's port"
        );
        assert_eq!(
            address("nas.lab", 80, false, None).as_deref(),
            Ok("https://nas.lab:80/")
        );
        assert_eq!(
            address("nas.lab", 0, false, None),
            Err(WebAddressError::PortInvalid)
        );
        assert_eq!(
            address("nas.lab", u16::MAX, false, None).as_deref(),
            Ok("https://nas.lab:65535/")
        );
    }

    #[test]
    fn an_ipv6_host_is_bracketed_and_written_the_standard_way() {
        assert_eq!(
            address("fe80::1", 443, false, None).as_deref(),
            Ok("https://[fe80::1]/")
        );
        assert_eq!(
            address("[2001:DB8:0:0:0:0:0:1]", 8080, true, None).as_deref(),
            Ok("http://[2001:db8::1]:8080/")
        );
        for refused in [
            "fe80::1%eth0",
            "[fe80::1",
            "fe80::1]",
            "[ilo.lab]",
            "[10.0.0.1]",
        ] {
            assert_eq!(
                authority_host(refused),
                Err(WebAddressError::HostInvalid),
                "{refused}"
            );
        }
    }

    #[test]
    fn a_host_carrying_a_scheme_credentials_a_port_or_a_path_is_refused() {
        for refused in [
            "javascript:alert(1)",
            "file:///C:/Windows",
            "https://ilo.lab",
            "admin@ilo.lab",
            "admin:secret@ilo.lab",
            "ilo.lab:8443",
            "ilo.lab/path",
            "ilo.lab\\path",
            "ilo.lab?x=1",
            "ilo.lab#top",
            "ilo lab",
            "ilo.lab\u{0}",
            "ilo.lab\nevil",
            "-ilo.lab",
            "ilo-.lab",
            "ilo..lab",
            ".ilo.lab",
            "ilo\"lab",
            "il\u{f6}.lab",
            "%69lo.lab",
            // Numbers a browser reads as IPv4 addresses the profile does not show.
            "2130706433",
            "0x7f.0.0.1",
            "0x7F",
            "010.0.0.1.",
            "1.2.3",
            "10.0.0.256",
            "0",
            "rack.0xbeef",
        ] {
            assert_eq!(
                authority_host(refused),
                Err(WebAddressError::HostInvalid),
                "{refused:?}"
            );
        }
        assert_eq!(authority_host("  "), Err(WebAddressError::HostMissing));
    }

    #[test]
    fn a_tunnel_dials_the_host_checked_an_ipv6_address_without_brackets() {
        assert_eq!(forward_host(" ilo.lab ").as_deref(), Ok("ilo.lab"));
        assert_eq!(forward_host("[fe80::0001]").as_deref(), Ok("fe80::1"));
        assert_eq!(forward_host("fe80::1").as_deref(), Ok("fe80::1"));
        assert_eq!(
            forward_host("ilo.lab:22"),
            Err(WebAddressError::HostInvalid)
        );
    }

    #[test]
    fn names_and_ipv4_addresses_are_taken_as_typed_trimmed() {
        assert_eq!(authority_host(" ilo-01.lab ").as_deref(), Ok("ilo-01.lab"));
        assert_eq!(authority_host("ilo.lab.").as_deref(), Ok("ilo.lab."));
        assert_eq!(authority_host("my_nas").as_deref(), Ok("my_nas"));
        assert_eq!(authority_host("ilo2").as_deref(), Ok("ilo2"));
        assert_eq!(authority_host("10.ilo.lab").as_deref(), Ok("10.ilo.lab"));
        assert_eq!(
            authority_host("rack.0xbeefz").as_deref(),
            Ok("rack.0xbeefz")
        );
        assert_eq!(authority_host("192.0.2.10").as_deref(), Ok("192.0.2.10"));
        assert_eq!(
            authority_host(&"a".repeat(MAX_LABEL_LENGTH + 1)),
            Err(WebAddressError::HostInvalid)
        );
    }

    #[test]
    fn the_path_is_normalized_as_the_csharp_one_and_encoded_where_it_could_mislead() {
        assert_eq!(normalized_path(""), "/");
        assert_eq!(normalized_path("  "), "/");
        assert_eq!(normalized_path("index.html"), "/index.html");
        assert_eq!(normalized_path("?page=1"), "?page=1");
        assert_eq!(normalized_path("#top"), "#top");
        assert_eq!(normalized_path("/a b"), "/a%20b");
        assert_eq!(normalized_path("/a\"b'c<d>"), "/a%22b%27c%3Cd%3E");
        assert_eq!(normalized_path("/a\\b"), "/a%5Cb");
        assert_eq!(normalized_path("/\u{7}\r\n"), "/%07%0D%0A");
        assert_eq!(normalized_path("/caf\u{e9}"), "/caf%C3%A9");
        assert_eq!(normalized_path("/already%20encoded"), "/already%20encoded");
    }

    #[test]
    fn a_path_cannot_move_the_address_to_another_host() {
        // After the host, `//` and `@` are the path's: the host stays the profile's.
        assert_eq!(
            address("ilo.lab", 443, false, Some("//evil.example/x")).as_deref(),
            Ok("https://ilo.lab//evil.example/x")
        );
        assert_eq!(
            address("ilo.lab", 443, false, Some("@evil.example")).as_deref(),
            Ok("https://ilo.lab/@evil.example")
        );
    }
}
