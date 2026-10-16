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

//! MAC address vendors by OUI, as the C# `Heimdall.Core.Discovery.OuiDatabase` (NETMAP) and
//! `Heimdall.App.Services.ArpOuiLookup` (ARPMON): one table for both tools, bundled with
//! Heimdall as `assets/oui/oui_vendors.tsv`, never fetched.

use std::collections::HashMap;
use std::sync::LazyLock;

/// The table: an OUI's six hexadecimal digits, a tab, its vendor; `#` starts a comment.
const OUI_TABLE: &str = include_str!("../../assets/oui/oui_vendors.tsv");
/// Hexadecimal digits of an OUI, the first three octets of a MAC address.
const OUI_DIGITS: usize = 6;
/// What starts a comment line of the table.
const COMMENT: char = '#';
/// The bit of the first octet set in a locally administered address, as the C#
/// `LookupManufacturer` tests it (`firstByte & 0x02`): one an operating system draws at
/// random for privacy, which names no vendor.
const LOCALLY_ADMINISTERED: u8 = 0x02;
/// The bit of the first octet set in a group address, multicast or broadcast: no device's
/// own, so never said randomized as the C# says the broadcast address.
const GROUP_ADDRESS: u8 = 0x01;

/// What a MAC address says of the hardware behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacVendor {
    /// The vendor its OUI is registered to.
    Named(&'static str),
    /// A locally administered address, drawn at random by the device, as the C# says
    /// "Private (Randomized MAC)".
    Randomized,
}

/// The table read once, by OUI in upper case.
static VENDORS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    OUI_TABLE
        .lines()
        .filter_map(table_line)
        .collect::<HashMap<_, _>>()
});

/// The OUI and the vendor of one line of the table; `None` for a comment, a blank line or a
/// line that is not one.
fn table_line(line: &'static str) -> Option<(&'static str, &'static str)> {
    if line.starts_with(COMMENT) {
        return None;
    }
    let (oui, vendor) = line.split_once('\t')?;
    let vendor = vendor.trim();
    (oui.len() == OUI_DIGITS && oui.chars().all(|c| c.is_ascii_hexdigit()) && !vendor.is_empty())
        .then_some((oui, vendor))
}

/// The OUI of `mac`, its first six hexadecimal digits in upper case, its separators (`-`,
/// `:`, `.`) left out; `None` when it has fewer, or holds anything but hexadecimal digits
/// before them, so a MAC address written in another alphabet is never cut inside a
/// character.
fn oui_of(mac: &str) -> Option<String> {
    let oui: String = mac
        .chars()
        .filter(|c| !matches!(c, '-' | ':' | '.'))
        .take(OUI_DIGITS)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    (oui.len() == OUI_DIGITS && oui.chars().all(|c| c.is_ascii_hexdigit())).then_some(oui)
}

/// The vendor of `mac`, as the C# `LookupManufacturer`: the one its OUI is registered to,
/// else [`MacVendor::Randomized`] for a locally administered address that is no group
/// address; `None` when unknown.
#[must_use]
pub fn lookup(mac: &str) -> Option<MacVendor> {
    let oui = oui_of(mac)?;
    if let Some(vendor) = VENDORS.get(oui.as_str()) {
        return Some(MacVendor::Named(vendor));
    }
    let first = u8::from_str_radix(&oui[..2], 16).ok()?;
    (first & LOCALLY_ADMINISTERED != 0 && first & GROUP_ADDRESS == 0)
        .then_some(MacVendor::Randomized)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_line_of_the_table_is_read_and_no_oui_is_twice() {
        let mut seen = HashSet::new();
        for line in OUI_TABLE.lines() {
            if line.is_empty() || line.starts_with(COMMENT) {
                continue;
            }
            let (oui, _) = table_line(line).unwrap_or_else(|| panic!("unread: {line:?}"));
            assert_eq!(oui, oui.to_ascii_uppercase(), "{line:?}");
            assert!(seen.insert(oui), "twice: {oui}");
        }
        assert_eq!(VENDORS.len(), seen.len());
        assert!(VENDORS.len() > 300, "the C# tables: {}", VENDORS.len());
    }

    #[test]
    fn a_vendor_is_found_whatever_the_separators_and_the_case() {
        assert_eq!(
            lookup("00-50-56-11-22-33"),
            Some(MacVendor::Named("VMware"))
        );
        assert_eq!(
            lookup("b8:27:eb:aa:bb:cc"),
            Some(MacVendor::Named("Raspberry Pi"))
        );
        assert_eq!(lookup("0011.32aa.bbcc"), Some(MacVendor::Named("Synology")));
        // The C# OuiDatabase wins over the ARP monitor's own list.
        assert_eq!(
            lookup("08-00-27-00-00-01"),
            Some(MacVendor::Named("Oracle VirtualBox"))
        );
        // Only in the ARP monitor's list.
        assert_eq!(lookup("00-1B-21-00-00-01"), Some(MacVendor::Named("Intel")));
    }

    #[test]
    fn a_locally_administered_address_is_randomized_and_a_group_one_unknown() {
        assert_eq!(lookup("DA-A1-19-00-00-01"), Some(MacVendor::Randomized));
        assert_eq!(lookup("02:00:00:00:00:01"), Some(MacVendor::Randomized));
        assert_eq!(lookup("00-00-01-00-00-01"), None);
        assert_eq!(lookup("ff-ff-ff-ff-ff-ff"), None, "the broadcast address");
        assert_eq!(lookup("01-00-5e-00-00-16"), None, "a multicast one");
    }

    #[test]
    fn a_mac_address_in_another_alphabet_is_unknown_never_a_panic() {
        // Each of these used to be cut at its sixth byte, inside a character.
        for mac in [
            "\u{e9}\u{e9}\u{e9}\u{e9}",
            "00-5\u{e9}-56-11",
            "\u{1f600}\u{1f600}",
            "\u{fffd}\u{fffd}-\u{fffd}",
            "00",
            "",
        ] {
            assert_eq!(lookup(mac), None, "{mac:?}");
        }
    }
}
