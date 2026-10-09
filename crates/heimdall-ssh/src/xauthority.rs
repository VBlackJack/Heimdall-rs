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

//! The Xauthority file, as `xauth` writes it: the cookies X servers expect, by display.
//!
//! Each entry is five fields, all big-endian: a family (`u16`), then the address, the
//! display number, the authorization name and its data, each a `u16` length and its bytes.
//! Only `MIT-MAGIC-COOKIE-1` entries are used: the others are computed per connection, which
//! forwarding cannot do.

use std::io::Read as _;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

/// The variable naming the file.
const VARIABLE: &str = "XAUTHORITY";

/// The file's name in the home folder when the variable names none.
const FILE_NAME: &str = ".Xauthority";

/// Bytes read of the file at most: a few entries a display, never megabytes.
const FILE_LIMIT: u64 = 1024 * 1024;

/// An IPv4 address.
const FAMILY_INTERNET: u16 = 0;

/// An IPv6 address.
const FAMILY_INTERNET6: u16 = 6;

/// This computer, by its host name: the family `xauth` writes for a local display.
const FAMILY_LOCAL: u16 = 256;

/// Any address.
const FAMILY_WILD: u16 = 65535;

/// The only authorization name kept.
const MIT_MAGIC_COOKIE: &[u8] = b"MIT-MAGIC-COOKIE-1";

/// Bytes of a field's length.
const LENGTH_BYTES: usize = 2;

/// Where Linux says the host name.
#[cfg(unix)]
const HOSTNAME_FILE: &str = "/proc/sys/kernel/hostname";

/// The host name the shell exports, elsewhere.
#[cfg(unix)]
const HOSTNAME_VARIABLE: &str = "HOSTNAME";

/// The host name on Windows.
#[cfg(windows)]
const HOSTNAME_VARIABLE: &str = "COMPUTERNAME";

/// One entry of the file.
pub(crate) struct Entry {
    family: u16,
    address: Vec<u8>,
    /// The display number, in decimal digits; empty for any display.
    number: Vec<u8>,
    name: Vec<u8>,
    data: Zeroizing<Vec<u8>>,
}

/// The display a cookie is looked for.
pub(crate) struct Wanted<'a> {
    /// The display's number.
    pub(crate) number: u16,
    /// This computer's own display: its socket or its loopback address.
    pub(crate) local: bool,
    /// The display's address, when given as one.
    pub(crate) address: Option<IpAddr>,
    /// This computer's host name, when known; unknown, no local entry is taken.
    pub(crate) hostname: Option<&'a str>,
}

/// A cookie found: its authorization name and data.
#[derive(Default)]
pub(crate) struct Cookie {
    pub(crate) name: Vec<u8>,
    pub(crate) data: Zeroizing<Vec<u8>>,
}

/// The file the cookies are read from: the one `XAUTHORITY` names, else `.Xauthority` in the
/// home folder the platform says, on Windows its known folder rather than `HOME` or
/// `USERPROFILE`, which whoever starts Heimdall sets.
pub(crate) fn default_path() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os(VARIABLE).filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(named));
    }
    heimdall_core::paths::home_dir().map(|home| home.join(FILE_NAME))
}

/// The cookie `path` holds for `wanted`; none when the file is not there, unreadable, or has
/// no entry for it.
pub(crate) fn cookie_in(path: &Path, wanted: &Wanted<'_>) -> Option<Cookie> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(FILE_LIMIT).read_to_end(&mut bytes).ok()?;
    cookie(&parse(&bytes), wanted)
}

/// The entries of `bytes`, up to the first one cut short.
pub(crate) fn parse(bytes: &[u8]) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut rest = bytes;
    while let Some(entry) = next_entry(&mut rest) {
        entries.push(entry);
    }
    entries
}

fn next_entry(rest: &mut &[u8]) -> Option<Entry> {
    let family = u16::from_be_bytes(take(rest, LENGTH_BYTES)?.try_into().ok()?);
    let address = field(rest)?.to_vec();
    let number = field(rest)?.to_vec();
    let name = field(rest)?.to_vec();
    let data = Zeroizing::new(field(rest)?.to_vec());
    Some(Entry {
        family,
        address,
        number,
        name,
        data,
    })
}

/// A `u16` length, then that many bytes.
fn field<'a>(rest: &mut &'a [u8]) -> Option<&'a [u8]> {
    let length = u16::from_be_bytes(take(rest, LENGTH_BYTES)?.try_into().ok()?);
    take(rest, usize::from(length))
}

fn take<'a>(rest: &mut &'a [u8], count: usize) -> Option<&'a [u8]> {
    if rest.len() < count {
        return None;
    }
    let (taken, after) = rest.split_at(count);
    *rest = after;
    Some(taken)
}

/// The first `MIT-MAGIC-COOKIE-1` cookie of `entries` for `wanted`.
pub(crate) fn cookie(entries: &[Entry], wanted: &Wanted<'_>) -> Option<Cookie> {
    let number = wanted.number.to_string();
    entries
        .iter()
        .filter(|entry| entry.name == MIT_MAGIC_COOKIE)
        .filter(|entry| entry.number.is_empty() || entry.number == number.as_bytes())
        .find(|entry| matches_address(entry, wanted))
        .map(|entry| Cookie {
            name: entry.name.clone(),
            data: entry.data.clone(),
        })
}

fn matches_address(entry: &Entry, wanted: &Wanted<'_>) -> bool {
    match entry.family {
        FAMILY_WILD => true,
        FAMILY_LOCAL => {
            wanted.local
                && wanted
                    .hostname
                    .is_some_and(|hostname| entry.address.eq_ignore_ascii_case(hostname.as_bytes()))
        }
        FAMILY_INTERNET => {
            matches!(wanted.address, Some(IpAddr::V4(ip)) if entry.address == ip.octets())
        }
        FAMILY_INTERNET6 => {
            matches!(wanted.address, Some(IpAddr::V6(ip)) if entry.address == ip.octets())
        }
        _ => false,
    }
}

/// This computer's host name, as `xauth` writes it for a local display.
pub(crate) fn hostname() -> Option<String> {
    #[cfg(unix)]
    let name = std::fs::read_to_string(HOSTNAME_FILE)
        .ok()
        .or_else(|| std::env::var(HOSTNAME_VARIABLE).ok());
    #[cfg(windows)]
    let name = std::env::var(HOSTNAME_VARIABLE).ok();
    name.map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::{
        FAMILY_INTERNET, FAMILY_LOCAL, FAMILY_WILD, MIT_MAGIC_COOKIE, Wanted, cookie, parse,
    };

    fn entry(family: u16, address: &[u8], number: &str, name: &[u8], data: &[u8]) -> Vec<u8> {
        let mut bytes = family.to_be_bytes().to_vec();
        for field in [address, number.as_bytes(), name, data] {
            bytes.extend(u16::try_from(field.len()).expect("short").to_be_bytes());
            bytes.extend(field);
        }
        bytes
    }

    fn local(number: u16, hostname: Option<&str>) -> Wanted<'_> {
        Wanted {
            number,
            local: true,
            address: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            hostname,
        }
    }

    fn found(bytes: &[u8], wanted: &Wanted<'_>) -> Option<Vec<u8>> {
        cookie(&parse(bytes), wanted).map(|cookie| cookie.data.to_vec())
    }

    #[test]
    fn the_local_entry_of_the_display_s_number_and_host_is_taken() {
        let mut file = entry(FAMILY_LOCAL, b"other", "0", MIT_MAGIC_COOKIE, &[1; 16]);
        file.extend(entry(
            FAMILY_LOCAL,
            b"desk",
            "1",
            MIT_MAGIC_COOKIE,
            &[2; 16],
        ));
        file.extend(entry(
            FAMILY_LOCAL,
            b"DESK",
            "0",
            MIT_MAGIC_COOKIE,
            &[3; 16],
        ));
        assert_eq!(found(&file, &local(0, Some("desk"))), Some(vec![3; 16]));
        assert_eq!(found(&file, &local(1, Some("desk"))), Some(vec![2; 16]));
        assert_eq!(
            found(&file, &local(2, Some("desk"))),
            None,
            "no such display"
        );
        // The host name unknown: no local entry, which may be another computer's.
        assert_eq!(found(&file, &local(0, None)), None);
    }

    #[test]
    fn a_local_entry_of_another_computer_is_never_taken() {
        let file = entry(FAMILY_LOCAL, b"elsewhere", "0", MIT_MAGIC_COOKIE, &[1; 16]);
        assert_eq!(found(&file, &local(0, Some("desk"))), None);
        assert_eq!(found(&file, &local(0, None)), None);
    }

    #[test]
    fn a_wild_or_internet_entry_matches_and_another_protocol_is_skipped() {
        let mut file = entry(FAMILY_WILD, b"", "5", b"XDM-AUTHORIZATION-1", &[9; 8]);
        file.extend(entry(
            FAMILY_INTERNET,
            &[10, 0, 0, 1],
            "5",
            MIT_MAGIC_COOKIE,
            &[4; 16],
        ));
        file.extend(entry(
            FAMILY_INTERNET,
            &[127, 0, 0, 1],
            "5",
            MIT_MAGIC_COOKIE,
            &[5; 16],
        ));
        file.extend(entry(FAMILY_WILD, b"", "", MIT_MAGIC_COOKIE, &[6; 16]));
        assert_eq!(found(&file, &local(5, Some("desk"))), Some(vec![5; 16]));
        assert_eq!(
            found(&file, &local(7, Some("desk"))),
            Some(vec![6; 16]),
            "an empty number is any display"
        );
        let remote = Wanted {
            number: 5,
            local: false,
            address: None,
            hostname: Some("desk"),
        };
        assert_eq!(found(&file, &remote), Some(vec![6; 16]));
    }

    #[test]
    fn an_entry_cut_short_ends_the_file() {
        let mut file = entry(FAMILY_WILD, b"", "0", MIT_MAGIC_COOKIE, &[7; 16]);
        let second = entry(FAMILY_WILD, b"", "1", MIT_MAGIC_COOKIE, &[8; 16]);
        file.extend(&second[..second.len() - 1]);
        assert_eq!(parse(&file).len(), 1);
        assert_eq!(found(&file, &local(1, None)), None);
        assert!(parse(&[]).is_empty());
        assert!(parse(&[0xFF]).is_empty());
    }
}
