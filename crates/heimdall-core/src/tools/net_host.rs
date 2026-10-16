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

//! The host a network tool is pointed at, checked before any lookup, connection or program
//! runs, as the C# `InputValidator.Validate(value, "Address")` checks it
//! (`InputValidator.cs:71`, `ValidateDns`): an IP address, or a DNS name of letters, digits
//! and hyphens, no label empty, longer than [`MAX_LABEL_LENGTH`] or starting or ending with
//! a hyphen, the whole at most [`MAX_NAME_LENGTH`] long.
//!
//! Additions to the C# rule: an IPv6 address, bare or in brackets, which the C# pattern has
//! no room for though its `Ping` and `nslookup` take one; a label starting with an
//! underscore, the service labels of the SRV records the DNS tool asks for; a label starting
//! with a digit, as RFC 1123 section 2.1 allows (`3com.com`, `1.0.0.127.in-addr.arpa`),
//! where the C# wants a letter first; and the root's trailing dot of a fully qualified name
//! (`example.com.`). A name whose last label is all digits is still refused, as RFC 1123
//! section 2.1 and RFC 3696 section 2 have it, so `1.2.3` is never taken for a name.
//!
//! Whatever is added, a checked host never starts with "-" (no label may) nor holds a space
//! or a control character (none is a letter, a digit, a hyphen, an underscore or a dot), so
//! it is never read as a program's option or as a second argument.

use std::net::{IpAddr, Ipv6Addr};

/// Longest DNS name written out, its root's trailing dot aside: the 255 octets of RFC 1035
/// section 2.3.4, which count a length octet before each label and the root's empty label,
/// leave 253 characters. The C# `MaxFqdnLength` takes 255.
pub const MAX_NAME_LENGTH: usize = 253;

/// Longest label of a DNS name, between two dots, as the C# `MaxDnsLabelLength`.
pub const MAX_LABEL_LENGTH: usize = 63;

/// `typed`, trimmed, when it is an IP address or a host name the rule above takes; an IPv6
/// address in brackets comes back without them. `None` otherwise.
#[must_use]
pub fn checked_host(typed: &str) -> Option<String> {
    let trimmed = typed.trim();
    if let Some(inner) = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        return inner.parse::<Ipv6Addr>().ok().map(|_| inner.to_owned());
    }
    if trimmed.parse::<IpAddr>().is_ok() {
        return Some(trimmed.to_owned());
    }
    is_host_name(trimmed).then(|| trimmed.to_owned())
}

/// Whether `name` is a DNS name as the rule above reads one.
fn is_host_name(name: &str) -> bool {
    let name = name.strip_suffix('.').unwrap_or(name);
    if name.is_empty() || name.len() > MAX_NAME_LENGTH {
        return false;
    }
    let labels_valid = name.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= MAX_LABEL_LENGTH
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    });
    let top_level_numeric = name
        .rsplit('.')
        .next()
        .is_some_and(|label| label.bytes().all(|byte| byte.is_ascii_digit()));
    labels_valid && !top_level_numeric
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_addresses_pass_trimmed() {
        assert_eq!(checked_host(" example.com "), Some("example.com".into()));
        assert_eq!(checked_host("host-1.lan"), Some("host-1.lan".into()));
        assert_eq!(checked_host("localhost"), Some("localhost".into()));
        assert_eq!(checked_host("192.168.1.1"), Some("192.168.1.1".into()));
        assert_eq!(checked_host("2001:db8::1"), Some("2001:db8::1".into()));
        assert_eq!(checked_host("[2001:db8::1]"), Some("2001:db8::1".into()));
        assert_eq!(
            checked_host("_ldap._tcp.example.com"),
            Some("_ldap._tcp.example.com".into())
        );
    }

    #[test]
    fn a_label_starting_with_a_digit_and_the_root_dot_pass() {
        for accepted in [
            "3com.com",
            "1password.com",
            "1.0.0.127.in-addr.arpa",
            "example.com.",
            "9.example",
            "x.y-1.z",
        ] {
            assert_eq!(
                checked_host(accepted),
                Some(accepted.into()),
                "{accepted:?}"
            );
        }
    }

    #[test]
    fn a_name_with_a_numeric_top_label_or_only_a_root_is_refused() {
        for refused in [
            "1.2.3",
            "999.1.1.1",
            "0x7f.1",
            "123",
            ".",
            "..",
            "example.com..",
            ".com",
        ] {
            assert_eq!(checked_host(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn nothing_that_reads_as_an_option_or_a_second_argument_passes() {
        for refused in [
            "",
            "   ",
            "-n",
            "--help",
            "-type=ANY",
            "example.com -n 5",
            "example.com\n-n",
            "exa mple.com",
            "example..com",
            "example.-com",
            "-example.com",
            "example-.com",
            "-1.example.com",
            "1-.example.com",
            "example.com.-",
            "-.",
            "example.com.\n-n",
            "example.com.\t-n",
            "ex\u{e9}mple.com",
            "[example.com]",
            "[192.168.1.1]",
        ] {
            assert_eq!(checked_host(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn a_name_too_long_or_a_label_too_long_is_refused() {
        let label = "a".repeat(MAX_LABEL_LENGTH);
        assert!(checked_host(&format!("{label}.com")).is_some());
        assert!(checked_host(&format!("{label}a.com")).is_none());
        let long = vec!["abc"; MAX_NAME_LENGTH / 4 + 2].join(".");
        assert!(long.len() > MAX_NAME_LENGTH);
        assert!(checked_host(&long).is_none());
        // 253 characters pass, with or without the root's dot; 254 do not.
        let longest = format!("{}.com", "a.".repeat((MAX_NAME_LENGTH - 4) / 2) + "a");
        assert_eq!(longest.len(), MAX_NAME_LENGTH);
        assert!(checked_host(&longest).is_some());
        assert!(checked_host(&format!("{longest}.")).is_some());
        assert!(checked_host(&format!("a{longest}")).is_none());
    }
}
