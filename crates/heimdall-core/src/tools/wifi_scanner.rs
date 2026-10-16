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

//! The visible Wi-Fi networks, as the C# `WifiScanService` and `NetshWifiParser`.
//!
//! Windows has no structured listing of the visible networks short of the WLAN API (FFI):
//! `netsh wlan show networks mode=bssid` is read, as in the C#, but not by its labels, which
//! Windows translates (the C# knows the English and French ones only). Its layout is the
//! same in every language: `SSID n : name` and `BSSID n : address` lines, the network's
//! details indented under the first (type, authentication, encryption, in that order), the
//! access point's under the second, a signal written as a percentage, a radio type starting
//! with `802.11`, a channel as a bare number. Its output is decoded as written, in UTF-8 or
//! the system's OEM code page. On Linux, `nmcli -t` lists them, untranslated.
//!
//! A `netsh` that fails (the WLAN service stopped, no Wi-Fi adapter, the location
//! permission refused) is a failure with `netsh`'s words, never "no network".

use std::cmp::Reverse;

use super::system_listing;
use super::system_program::{self, ProgramError};

/// `netsh` inside the system folder.
#[cfg(windows)]
const NETSH_PROGRAM: &str = "netsh.exe";

/// List the networks with per-BSSID detail, as the C# `NetshWifiScanArguments`.
#[cfg(windows)]
const NETSH_ARGUMENTS: [&str; 4] = ["wlan", "show", "networks", "mode=bssid"];

/// Where `nmcli` is looked for: by its full path, never on the `PATH`.
#[cfg(target_os = "linux")]
const NMCLI_CANDIDATES: [&str; 2] = ["/usr/bin/nmcli", "/bin/nmcli"];

/// `nmcli` switches: terse, separators escaped, the fields [`parse_nmcli`] reads.
#[cfg(target_os = "linux")]
const NMCLI_ARGUMENTS: [&str; 8] = [
    "-t",
    "-e",
    "yes",
    "-f",
    "SSID,BSSID,SIGNAL,CHAN,SECURITY",
    "device",
    "wifi",
    "list",
];

/// The word `netsh` starts a network's line with, in every language.
const SSID_WORD: &str = "SSID";

/// The word `netsh` starts an access point's line with, in every language.
const BSSID_WORD: &str = "BSSID";

/// What a radio type starts with.
const RADIO_PREFIX: &str = "802.11";

/// The position of the authentication among a network's details, after its type.
const AUTH_POSITION: usize = 2;

/// The position of the encryption among a network's details.
const ENCRYPTION_POSITION: usize = 3;

/// The separator of `nmcli -t` fields, and its escape.
const NMCLI_SEPARATOR: char = ':';
const NMCLI_ESCAPE: char = '\\';

/// The number of fields asked of `nmcli`.
const NMCLI_FIELDS: usize = 5;

/// A single visible wireless network access point entry, as the C# `WifiEntry`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WifiEntry {
    /// Network SSID name: chosen by whoever runs the access point.
    pub ssid: String,
    /// Hardware BSSID MAC address; empty when the system named the network only.
    pub bssid: String,
    /// Signal percentage as written (e.g. "85%"); empty when not said.
    pub signal: String,
    /// Numerical signal level (0..=100).
    pub signal_value: i32,
    /// Radio channel.
    pub channel: String,
    /// Authentication, as the system words it (e.g. "WPA2-Personal").
    pub auth: String,
    /// Encryption cipher, as the system words it (e.g. "CCMP").
    pub encryption: String,
    /// Radio standard (e.g. "802.11ax").
    pub radio_type: String,
}

/// The number of leading whitespace characters of `line`.
fn indent(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

/// `line` as its label and value, split at its first colon, both trimmed; `None` without a
/// colon.
fn key_value(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once(':')?;
    Some((key.trim(), value.trim()))
}

/// The first word of a label, in capitals.
fn first_word(key: &str) -> String {
    key.split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase()
}

/// A signal written as a percentage (`82%`, `82 %` with any space), as its number.
fn signal_percentage(value: &str) -> Option<i32> {
    let compact: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    let digits = compact.strip_suffix('%')?;
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// An access point being read.
#[derive(Default)]
struct AccessPoint {
    bssid: String,
    indent: usize,
    detail_indent: Option<usize>,
    signal: Option<i32>,
    radio: String,
    channel: String,
}

/// A network being read.
#[derive(Default)]
struct Network {
    ssid: String,
    indent: usize,
    detail_indent: Option<usize>,
    details_seen: usize,
    auth: String,
    encryption: String,
    access_points: Vec<AccessPoint>,
}

impl Network {
    /// Its entries: one per access point, or the network alone when `netsh` named no access
    /// point (it does so without the location permission on some builds).
    fn entries(self) -> Vec<WifiEntry> {
        let entry = |point: Option<&AccessPoint>| {
            let signal_value = point.and_then(|point| point.signal);
            WifiEntry {
                ssid: self.ssid.clone(),
                bssid: point.map(|point| point.bssid.clone()).unwrap_or_default(),
                signal: signal_value
                    .map(|value| format!("{value}%"))
                    .unwrap_or_default(),
                signal_value: signal_value.unwrap_or_default(),
                channel: point.map(|point| point.channel.clone()).unwrap_or_default(),
                auth: self.auth.clone(),
                encryption: self.encryption.clone(),
                radio_type: point.map(|point| point.radio.clone()).unwrap_or_default(),
            }
        };
        if self.access_points.is_empty() {
            vec![entry(None)]
        } else {
            self.access_points
                .iter()
                .map(|point| entry(Some(point)))
                .collect()
        }
    }
}

/// Parses `netsh wlan show networks mode=bssid` text into a list of [`WifiEntry`], one per
/// access point, the strongest first, whatever language Windows writes it in.
#[must_use]
pub fn parse_netsh(output: &str) -> Vec<WifiEntry> {
    let mut results = Vec::new();
    let mut network: Option<Network> = None;

    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let depth = indent(line);
        let Some((key, value)) = key_value(line) else {
            continue;
        };
        match first_word(key).as_str() {
            SSID_WORD => {
                results.extend(network.take().map(Network::entries).unwrap_or_default());
                network = Some(Network {
                    ssid: value.to_owned(),
                    indent: depth,
                    ..Network::default()
                });
                continue;
            }
            BSSID_WORD => {
                if let Some(network) = network.as_mut() {
                    network.access_points.push(AccessPoint {
                        bssid: value.to_owned(),
                        indent: depth,
                        ..AccessPoint::default()
                    });
                }
                continue;
            }
            _ => {}
        }
        let Some(current) = network.as_mut() else {
            continue;
        };
        if depth <= current.indent {
            // A line as far out as the network's own (the next interface's name): the
            // network is over.
            results.extend(network.take().map(Network::entries).unwrap_or_default());
            continue;
        }
        if let Some(point) = current.access_points.last_mut() {
            if depth <= point.indent {
                continue;
            }
            // The access point's own details only, not those nested deeper (its load).
            let detail_indent = *point.detail_indent.get_or_insert(depth);
            if depth != detail_indent {
                continue;
            }
            if point.signal.is_none() {
                if let Some(signal) = signal_percentage(value) {
                    point.signal = Some(signal);
                    continue;
                }
            }
            if point.radio.is_empty() && value.starts_with(RADIO_PREFIX) {
                point.radio = value.to_owned();
            } else if point.channel.is_empty()
                && !value.is_empty()
                && value.chars().all(|c| c.is_ascii_digit())
            {
                point.channel = value.to_owned();
            }
        } else {
            let detail_indent = *current.detail_indent.get_or_insert(depth);
            if depth != detail_indent {
                continue;
            }
            current.details_seen += 1;
            match current.details_seen {
                AUTH_POSITION => current.auth = value.to_owned(),
                ENCRYPTION_POSITION => current.encryption = value.to_owned(),
                _ => {}
            }
        }
    }
    results.extend(network.map(Network::entries).unwrap_or_default());
    // The strongest first, as the C# `OrderByDescending(SignalValue)`; equal ones in the
    // order listed.
    results.sort_by_key(|entry| Reverse(entry.signal_value));
    results
}

/// Whether `output` is a listing rather than a message: a listing labels its values (`Name
/// : value`, the label padded before the colon), a message such as "The Wireless
/// AutoConfig Service (wlansvc) is not running." or the location permission's notice does
/// not. A listing of no network still names its interface.
#[must_use]
pub fn is_netsh_listing(output: &str) -> bool {
    output.lines().any(|line| {
        line.split_once(':').is_some_and(|(key, value)| {
            !key.trim().is_empty() && key.ends_with(char::is_whitespace) && !value.trim().is_empty()
        })
    })
}

/// `line` of `nmcli -t -e yes` as its fields, split at the colons not escaped, each
/// unescaped.
fn nmcli_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            NMCLI_ESCAPE => field.extend(chars.next()),
            NMCLI_SEPARATOR => fields.push(std::mem::take(&mut field)),
            other => field.push(other),
        }
    }
    fields.push(field);
    fields
}

/// The networks `nmcli -t -e yes -f SSID,BSSID,SIGNAL,CHAN,SECURITY device wifi list`
/// lists, the strongest first. `nmcli` says neither the cipher nor the radio type: left
/// empty.
#[must_use]
pub fn parse_nmcli(output: &str) -> Vec<WifiEntry> {
    let mut results: Vec<WifiEntry> = output
        .lines()
        .filter_map(|line| {
            let fields = nmcli_fields(line);
            let [ssid, bssid, signal, channel, security] =
                <[String; NMCLI_FIELDS]>::try_from(fields).ok()?;
            let signal_value: i32 = signal.trim().parse().ok()?;
            Some(WifiEntry {
                ssid,
                bssid: bssid.to_ascii_uppercase(),
                signal: format!("{signal_value}%"),
                signal_value,
                channel,
                auth: security,
                encryption: String::new(),
                radio_type: String::new(),
            })
        })
        .collect();
    results.sort_by_key(|entry| Reverse(entry.signal_value));
    results
}

/// Scans the visible Wi-Fi networks.
///
/// # Errors
///
/// The system has no way to scan them, or the scan could not be started, did not end in
/// time, or failed (on Windows, `netsh`'s message: the WLAN service stopped, no Wi-Fi
/// adapter, the location permission refused).
pub fn run_wifi_command() -> Result<Vec<WifiEntry>, ProgramError> {
    #[cfg(windows)]
    {
        let netsh =
            crate::paths::system_program(NETSH_PROGRAM).ok_or(ProgramError::NotAvailable)?;
        let folder = crate::paths::system_dir().ok_or(ProgramError::NotAvailable)?;
        let mut command = std::process::Command::new(netsh);
        command.args(NETSH_ARGUMENTS).current_dir(folder);
        let finished = system_program::run(command, system_listing::WIFI_TIME_LIMIT)?;
        let output = system_program::decode_text(&finished.stdout);
        if !finished.success || !is_netsh_listing(&output) {
            return Err(finished.failure());
        }
        Ok(parse_netsh(&output))
    }
    #[cfg(target_os = "linux")]
    {
        let nmcli =
            system_program::find_program(&NMCLI_CANDIDATES).ok_or(ProgramError::NotAvailable)?;
        let output =
            system_program::run_linux(&nmcli, &NMCLI_ARGUMENTS, system_listing::WIFI_TIME_LIMIT)?;
        Ok(parse_nmcli(&output))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(ProgramError::NotAvailable)
    }
}

/// `entries` as CSV under `headers` (SSID, BSSID, signal, channel, authentication,
/// encryption, radio type), as the C# copies them: every cell quoted and a formula defused,
/// an SSID being chosen by whoever runs the access point.
#[must_use]
pub fn build_wifi_csv(entries: &[WifiEntry], headers: &[String]) -> String {
    system_listing::csv_document(
        headers,
        entries.iter().map(|w| {
            vec![
                w.ssid.clone(),
                w.bssid.clone(),
                w.signal.clone(),
                w.channel.clone(),
                w.auth.clone(),
                w.encryption.clone(),
                w.radio_type.clone(),
            ]
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Windows 11 listing in English: an access point's load nested deeper, its band
    /// before its channel.
    const ENGLISH: &str = "
Interface name : Wi-Fi
There are 2 networks currently visible.

SSID 1 : HomeNetwork
    Network type            : Infrastructure
    Authentication          : WPA2-Personal
    Encryption              : CCMP
    BSSID 1                 : 10:20:30:40:50:60
         Signal             : 82%
         Radio type         : 802.11ax
         Band               : 5 GHz
         Channel            : 36
         Bss Load:
             Connected Stations:        4
             Channel Utilization:       12 (4 %)
         Basic rates (Mbps) : 6 12 24
         Other rates (Mbps) : 9 18 36 48 54
    BSSID 2                 : 10:20:30:40:50:61
         Signal             : 91%
         Radio type         : 802.11ac
         Channel            : 1

SSID 2 : GuestWiFi
    Network type            : Infrastructure
    Authentication          : Open
    Encryption              : None
    BSSID 1                 : aa:bb:cc:dd:ee:ff
         Signal             : 45%
         Radio type         : 802.11n
         Channel            : 6
";

    /// The same in French, as Windows writes it: translated labels, a no-break space before
    /// the colons and the percent sign, a typographic apostrophe.
    const FRENCH: &str = "
Nom de l\u{2019}interface\u{a0}: Wi-Fi 4
Actuellement 2\u{a0}réseaux sont visibles.

SSID 1\u{a0}: Maison
    Type de réseau\u{a0}         : Infrastructure
    Authentification\u{a0}       : WPA3-Personnel
    Chiffrement\u{a0}            : CCMP
    BSSID 1\u{a0}                : 10:20:30:40:50:60
         Signal\u{a0}            : 82\u{a0}%
         Type de radio\u{a0}     : 802.11ax
         Bande\u{a0}             : 5\u{a0}GHz
         Canal\u{a0}             : 36

SSID 2\u{a0}: =cmd|' /C calc'!A0
    Type de réseau\u{a0}         : Infrastructure
    Authentification\u{a0}       : Ouvrir
    Chiffrement\u{a0}            : Aucun
";

    #[test]
    fn an_english_listing_reads_one_entry_per_access_point_strongest_first() {
        let list = parse_netsh(ENGLISH);
        let seen: Vec<(&str, &str, &str, i32, &str, &str, &str, &str)> = list
            .iter()
            .map(|w| {
                (
                    w.ssid.as_str(),
                    w.bssid.as_str(),
                    w.signal.as_str(),
                    w.signal_value,
                    w.channel.as_str(),
                    w.auth.as_str(),
                    w.encryption.as_str(),
                    w.radio_type.as_str(),
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                (
                    "HomeNetwork",
                    "10:20:30:40:50:61",
                    "91%",
                    91,
                    "1",
                    "WPA2-Personal",
                    "CCMP",
                    "802.11ac"
                ),
                (
                    "HomeNetwork",
                    "10:20:30:40:50:60",
                    "82%",
                    82,
                    "36",
                    "WPA2-Personal",
                    "CCMP",
                    "802.11ax"
                ),
                (
                    "GuestWiFi",
                    "aa:bb:cc:dd:ee:ff",
                    "45%",
                    45,
                    "6",
                    "Open",
                    "None",
                    "802.11n"
                ),
            ]
        );
    }

    #[test]
    fn a_french_listing_reads_the_same_without_knowing_its_words() {
        let list = parse_netsh(FRENCH);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].ssid, "Maison");
        assert_eq!(list[0].bssid, "10:20:30:40:50:60");
        assert_eq!(list[0].signal, "82%");
        assert_eq!(list[0].signal_value, 82);
        assert_eq!(list[0].channel, "36");
        assert_eq!(list[0].auth, "WPA3-Personnel");
        assert_eq!(list[0].encryption, "CCMP");
        assert_eq!(list[0].radio_type, "802.11ax");
        // A network named without its access points (no location permission on some
        // builds) is still listed, not dropped.
        assert_eq!(list[1].ssid, "=cmd|' /C calc'!A0");
        assert_eq!(list[1].bssid, "");
        assert_eq!(list[1].signal, "");
        assert_eq!(list[1].auth, "Ouvrir");
        assert_eq!(list[1].encryption, "Aucun");
        assert!(is_netsh_listing(FRENCH));
    }

    #[test]
    fn a_french_listing_written_in_cp850_reads_once_decoded() {
        // "réseau" and the no-break space as a French console writes them: not UTF-8.
        let bytes = b"\r\nNom de l'interface\xff: Wi-Fi\r\n\r\nSSID 1\xff: Caf\x82\r\n    Type de r\x82seau          : Infrastructure\r\n    Authentification        : WPA2-Personnel\r\n    Chiffrement             : CCMP\r\n    BSSID 1                 : 10:20:30:40:50:60\r\n         Signal             : 70%\r\n         Canal              : 11\r\n";
        let text = system_program::decode_text(bytes);
        let list = parse_netsh(&text);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].bssid, "10:20:30:40:50:60");
        assert_eq!(list[0].signal_value, 70);
        assert_eq!(list[0].channel, "11");
        assert_eq!(list[0].auth, "WPA2-Personnel");
        assert_eq!(list[0].ssid, "Caf\u{e9}");
    }

    #[test]
    fn a_netsh_message_is_no_listing_and_no_network_still_is_one() {
        assert!(!is_netsh_listing(
            "\r\nThe Wireless AutoConfig Service (wlansvc) is not running.\r\n"
        ));
        assert!(!is_netsh_listing(
            "There is no wireless interface on the system."
        ));
        assert!(!is_netsh_listing(
            "Network shell commands need location permission to access WLAN information. \
             Turn on Location services on the Location page in Privacy & security settings.\r\n\
             \r\nHere is the URI for the Location page in the Settings app:\r\n\
             ms-settings:privacy-location\r\n\
             Function WlanGetAvailableNetworkList returns error 5:\r\n\
             The requested operation requires elevation (Run as administrator).\r\n"
        ));
        let none = "\r\nInterface name : Wi-Fi\r\nThere are 0 networks currently visible.\r\n";
        assert!(is_netsh_listing(none));
        assert_eq!(parse_netsh(none), vec![]);
    }

    #[test]
    fn nmcli_terse_output_reads_with_its_escaped_colons() {
        let output = "\
Caf\\: Wi-Fi:AA\\:BB\\:CC\\:DD\\:EE\\:FF:54:6:WPA2
Open:11\\:22\\:33\\:44\\:55\\:66:88:36:
broken line
";
        let list = parse_nmcli(output);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].ssid, "Open");
        assert_eq!(list[0].bssid, "11:22:33:44:55:66");
        assert_eq!(list[0].signal, "88%");
        assert_eq!(list[0].channel, "36");
        assert_eq!(list[0].auth, "");
        assert_eq!(list[1].ssid, "Caf: Wi-Fi");
        assert_eq!(list[1].bssid, "AA:BB:CC:DD:EE:FF");
        assert_eq!(list[1].signal_value, 54);
        assert_eq!(list[1].auth, "WPA2");
    }

    #[test]
    fn the_csv_quotes_every_cell_and_defuses_an_ssid_written_as_a_formula() {
        let list = parse_netsh(FRENCH);
        let headers: Vec<String> = [
            "SSID",
            "BSSID",
            "Signal",
            "Canal",
            "Auth",
            "Chiffrement",
            "Radio",
        ]
        .map(str::to_owned)
        .to_vec();
        let csv = build_wifi_csv(&list[1..], &headers);
        assert_eq!(
            csv.lines().nth(1),
            Some("\"'=cmd|' /C calc'!A0\",\"\",\"\",\"\",\"Ouvrir\",\"Aucun\",\"\"")
        );
    }
}
