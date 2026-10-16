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

//! What the OPENPORTS, NETIF, ROUTES and WIFI tools share once a table is read: how long
//! a listing may take, where `ip` is, how its JSON and a CSV copy are read and written.
//!
//! The C# reads these tables through Win32 and .NET calls (`GetExtendedTcpTable`,
//! `NetworkInterface.GetAllNetworkInterfaces`); without FFI the same facts come from a
//! structured source instead: Windows `PowerShell` writing JSON, `ip -j`, `ss`, `nmcli -t`.
//! Never the text a console program words in the user's language, except `netsh`, which
//! has nothing structured for the visible Wi-Fi networks. They are run by
//! [`super::system_program`], which says how they fail.

use std::time::Duration;

use serde::{Deserialize, Deserializer};

use super::csv_cell;
use super::system_program::ProgramError;

/// How long a listing may take, as the C# services list's `CancelAfter(30s)`: Windows
/// `PowerShell` takes a few seconds to start on a busy machine.
pub const LIST_TIME_LIMIT: Duration = Duration::from_secs(30);

/// How long the Wi-Fi scan may take, as the C# `WifiScanService.ProcessTimeoutMs`.
pub const WIFI_TIME_LIMIT: Duration = Duration::from_secs(10);

/// Where `ip` is looked for: by its full path, never on the `PATH`.
#[cfg(target_os = "linux")]
pub const IP_CANDIDATES: [&str; 4] = ["/usr/sbin/ip", "/sbin/ip", "/usr/bin/ip", "/bin/ip"];

/// `json` read as `T`.
///
/// # Errors
///
/// [`ProgramError::Unreadable`], with why.
pub fn parse_json<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, ProgramError> {
    serde_json::from_str(json.trim()).map_err(|error| ProgramError::Unreadable(error.to_string()))
}

/// A list as `ConvertTo-Json` writes one: an array, a lone object when the list has one
/// element, nothing (`null`) when it is empty.
#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    Many(Vec<T>),
    One(T),
}

/// Reads a list written by `ConvertTo-Json`, which writes a list of one as its element and
/// an empty one as `null`, into a `Vec`.
///
/// # Errors
///
/// The value is neither a list, nor an element, nor `null`.
pub fn one_or_many<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(match Option::<OneOrMany<T>>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(OneOrMany::Many(items)) => items,
        Some(OneOrMany::One(item)) => vec![item],
    })
}

/// Reads a text `ConvertTo-Json` may write as `null` into a `String`, empty for `null`.
///
/// # Errors
///
/// The value is neither a text nor `null`.
pub fn text_or_null<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

/// A CSV document: `headers`, then each of `rows`, every cell quoted and made unable to
/// start a spreadsheet formula, as the C# `InputValidator.SanitizeCsvCell`: an SSID, a
/// process or an interface name is chosen by someone else.
#[must_use]
pub fn csv_document<I>(headers: &[String], rows: I) -> String
where
    I: IntoIterator<Item = Vec<String>>,
{
    let line = |cells: &[String]| {
        cells
            .iter()
            .map(|cell| csv_cell::quoted(cell))
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut csv = line(headers);
    csv.push('\n');
    for row in rows {
        csv.push_str(&line(&row));
        csv.push('\n');
    }
    csv
}

/// The IPv4 mask of a prefix of `length` bits, dotted (`24` is `255.255.255.0`); `None`
/// past 32.
#[must_use]
pub fn ipv4_mask(length: u8) -> Option<std::net::Ipv4Addr> {
    const IPV4_BITS: u8 = 32;
    if length > IPV4_BITS {
        return None;
    }
    let bits = u32::MAX
        .checked_shl(u32::from(IPV4_BITS - length))
        .unwrap_or(0);
    Some(std::net::Ipv4Addr::from(bits))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Listed {
        #[serde(default, deserialize_with = "one_or_many")]
        rows: Vec<u32>,
    }

    #[test]
    fn a_list_reads_whether_powershell_wrote_it_whole_alone_or_null() {
        let rows = |json: &str| parse_json::<Listed>(json).map(|listed| listed.rows);
        assert_eq!(rows(r#"{"rows":[1,2]}"#), Ok(vec![1, 2]));
        assert_eq!(rows(r#"{"rows":7}"#), Ok(vec![7]));
        assert_eq!(rows(r#"{"rows":null}"#), Ok(vec![]));
        assert_eq!(rows("{}"), Ok(vec![]));
        assert!(matches!(rows("not json"), Err(ProgramError::Unreadable(_))));
    }

    #[test]
    fn every_csv_cell_is_quoted_and_a_formula_defused() {
        let csv = csv_document(
            &["SSID".to_owned(), "Signal".to_owned()],
            [vec!["=HYPERLINK(\"x\")".to_owned(), "80%".to_owned()]],
        );
        assert_eq!(
            csv,
            "\"SSID\",\"Signal\"\n\"'=HYPERLINK(\"\"x\"\")\",\"80%\"\n"
        );
    }

    #[test]
    fn a_prefix_length_gives_its_dotted_mask() {
        assert_eq!(ipv4_mask(0), Some(std::net::Ipv4Addr::new(0, 0, 0, 0)));
        assert_eq!(
            ipv4_mask(20),
            Some(std::net::Ipv4Addr::new(255, 255, 240, 0))
        );
        assert_eq!(
            ipv4_mask(32),
            Some(std::net::Ipv4Addr::new(255, 255, 255, 255))
        );
        assert_eq!(ipv4_mask(33), None);
    }
}
