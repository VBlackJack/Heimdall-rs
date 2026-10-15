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

//! What the network tools accept as a host and a port, as the C# `InputValidator`
//! (`Heimdall.Core/Security/InputValidator.cs`) reads them.

use std::net::Ipv4Addr;

/// Longest name accepted, the RFC limit of the C# `MaxFqdnLength`.
pub const MAX_NAME_LENGTH: usize = 255;

/// Longest DNS label accepted, the C# `MaxDnsLabelLength`.
pub const MAX_DNS_LABEL_LENGTH: usize = 63;

/// Whether `typed` passes the C# `InputValidator.Validate(value, "Address")` rule
/// (`InputValidator.cs:65-67`, `106-137`, `167-190`): an IPv4 address, or a DNS name
/// starting with a letter, of letters, digits and hyphens, no label empty, longer than
/// [`MAX_DNS_LABEL_LENGTH`] or starting or ending with a hyphen, the whole at most
/// [`MAX_NAME_LENGTH`] long. An IPv6 address is not one, as in the C#. The caller trims.
#[must_use]
pub fn is_address(typed: &str) -> bool {
    if typed.len() > MAX_NAME_LENGTH {
        return false;
    }
    if typed.parse::<Ipv4Addr>().is_ok() {
        return true;
    }
    typed.starts_with(|c: char| c.is_ascii_alphabetic())
        && typed.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= MAX_DNS_LABEL_LENGTH
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

/// The port `typed` names, as the C# `int.TryParse` then `InputValidator.ValidatePortRange`
/// (`InputValidator.cs:143-146`) accept it: decimal digits only, from 1 to 65535.
#[must_use]
pub fn parse_port(typed: &str) -> Option<u16> {
    let typed = typed.trim();
    if typed.is_empty() || !typed.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    typed.parse::<u16>().ok().filter(|port| *port != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_an_ipv4_or_a_name_as_the_csharp_rule() {
        for good in ["192.168.1.1", "example.com", "a", "web-01.corp.example"] {
            assert!(is_address(good), "{good}");
        }
        let long_label = format!("a{}", "b".repeat(MAX_DNS_LABEL_LENGTH));
        let long_name = format!("a{}", ".b".repeat(MAX_NAME_LENGTH / 2 + 1));
        for bad in [
            "",
            "1example.com",
            "-a.com",
            "a-.com",
            "a..com",
            "a.-b.com",
            "a.com.",
            "a b.com",
            "a_b.com",
            "h\u{00F4}te.fr",
            "::1",
            "example.com:22",
            &long_label,
            &long_name,
        ] {
            assert!(!is_address(bad), "{bad}");
        }
    }

    #[test]
    fn a_port_is_digits_from_1_to_65535() {
        assert_eq!(parse_port(" 443 "), Some(443));
        assert_eq!(parse_port("65535"), Some(65_535));
        for bad in ["", "0", "65536", "+443", "-1", "4 43", "http", "44\u{0663}"] {
            assert_eq!(parse_port(bad), None, "{bad}");
        }
    }
}
