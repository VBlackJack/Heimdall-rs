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

//! The port a network tool is pointed at, as the C# `InputValidator` (`InputValidator.cs`)
//! reads it. The host is [`super::net_host::checked_host`]'s.

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
    fn a_port_is_digits_from_1_to_65535() {
        assert_eq!(parse_port(" 443 "), Some(443));
        assert_eq!(parse_port("65535"), Some(65_535));
        for bad in ["", "0", "65536", "+443", "-1", "4 43", "http", "44\u{0663}"] {
            assert_eq!(parse_port(bad), None, "{bad}");
        }
    }
}
