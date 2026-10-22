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

//! The passwords a user chose to remember for shared sessions, as the C#
//! `SharedSessionSecretStore` files them: per source, under a key built from what the password
//! opens, never from the shared session's identifier.
//!
//! The identifier of a shared session is minted from its folder and its name, so a file that
//! points a session at another host would keep it and the password would follow. The key here
//! is the protocol, the host (in any case), the port and the login: a shared file that moves a
//! session to another host, port or login names another key, and the user is asked again.
//!
//! The secrets themselves go where every other saved password goes (the system's credential
//! store, or the vault once a master password is set). This module only names the entries and
//! writes the small indexes that let a source's passwords be forgotten, since the system's
//! store cannot be listed. An index holds digests and source identifiers, never a password,
//! a host or a login.

use crate::credentials::{CredentialProtocol, Endpoint};
use crate::shared_sessions::is_valid_source_id;

/// Most passwords remembered for one source; past it the oldest is replaced. An index must
/// fit the smallest secret the system's store keeps (Windows Credential Manager: 2560 bytes).
pub const MAX_PER_SOURCE: usize = 64;

/// Name of the entry listing the sources that have remembered passwords.
pub const SOURCES_ENTRY: &str = "shared-password-sources";

/// Smallest secret, in bytes, every system store keeps (Windows Credential Manager's limit).
#[cfg(test)]
const SMALLEST_STORE_SECRET_BYTES: usize = 2560;

/// Prefix of the entries holding one remembered password.
const ENTRY_PREFIX: &str = "shared-password/";

/// Prefix of the entries listing a source's remembered passwords.
const INDEX_PREFIX: &str = "shared-password-index/";

/// Version line of an index entry.
const INDEX_VERSION: &str = "1";

/// Bytes of the digest a remembered password is named by.
const DIGEST_BYTES: usize = 16;

/// Characters of that digest, in hexadecimal.
const DIGEST_CHARS: usize = DIGEST_BYTES * 2;

/// The endpoint as keys compare it: the host trimmed and without case.
#[must_use]
pub fn normalized(endpoint: &Endpoint) -> Endpoint {
    Endpoint {
        protocol: endpoint.protocol,
        host: endpoint.host.trim().to_ascii_lowercase(),
        port: endpoint.port,
        username: endpoint.username.clone(),
    }
}

/// The bytes that name `endpoint`: each text with its length, so no host and login pair can
/// be read as another.
fn canonical(endpoint: &Endpoint) -> Vec<u8> {
    let endpoint = normalized(endpoint);
    let mut bytes = vec![protocol_code(endpoint.protocol)];
    bytes.extend_from_slice(&endpoint.port.to_be_bytes());
    for part in [
        endpoint.host.as_str(),
        endpoint.username.as_deref().unwrap_or_default(),
    ] {
        let length = u32::try_from(part.len()).unwrap_or(u32::MAX);
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(part.as_bytes());
    }
    bytes
}

fn protocol_code(protocol: CredentialProtocol) -> u8 {
    match protocol {
        CredentialProtocol::Ssh => 1,
        CredentialProtocol::Rdp => 2,
        CredentialProtocol::Vnc => 3,
        CredentialProtocol::Ftp => 4,
        CredentialProtocol::WinRm => 5,
    }
}

/// The digest `endpoint` is remembered under: equal for the same protocol, host (in any
/// case), port and login, different for any other.
#[must_use]
pub fn digest(endpoint: &Endpoint) -> String {
    use std::fmt::Write as _;
    let full = sealvault::hash::sha256(&canonical(endpoint));
    full.iter()
        .take(DIGEST_BYTES)
        .fold(String::with_capacity(DIGEST_CHARS), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// Name of the entry holding the password remembered for `digest` in `source`.
#[must_use]
pub fn entry_name(source: &str, digest: &str) -> String {
    format!("{ENTRY_PREFIX}{source}/{digest}")
}

/// Name of the entry listing the passwords remembered in `source`.
#[must_use]
pub fn index_entry(source: &str) -> String {
    format!("{INDEX_PREFIX}{source}")
}

/// Whether `text` is a digest this module writes.
fn is_digest(text: &str) -> bool {
    text.len() == DIGEST_CHARS
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// `lines` as an index entry: a version line, then one per line.
#[must_use]
pub fn encode_lines(lines: &[String]) -> Vec<u8> {
    let mut text = String::from(INDEX_VERSION);
    for line in lines {
        text.push('\n');
        text.push_str(line);
    }
    text.into_bytes()
}

fn decode_lines(bytes: &[u8], valid: fn(&str) -> bool) -> Option<Vec<String>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.split('\n');
    if lines.next()? != INDEX_VERSION {
        return None;
    }
    let mut kept: Vec<String> = Vec::new();
    for line in lines {
        if !valid(line) {
            return None;
        }
        if !kept.iter().any(|known| known == line) {
            kept.push(line.to_owned());
        }
    }
    Some(kept)
}

/// The digests in a source's index entry; `None` for anything this version did not write.
#[must_use]
pub fn decode_index(bytes: &[u8]) -> Option<Vec<String>> {
    decode_lines(bytes, is_digest)
}

/// The source identifiers in the list of sources with remembered passwords; `None` for
/// anything this version did not write.
#[must_use]
pub fn decode_sources(bytes: &[u8]) -> Option<Vec<String>> {
    decode_lines(bytes, is_valid_source_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ssh(host: &str, port: u16, login: &str) -> Endpoint {
        Endpoint {
            protocol: CredentialProtocol::Ssh,
            host: host.to_owned(),
            port,
            username: Some(login.to_owned()),
        }
    }

    #[test]
    fn the_same_server_and_login_have_one_digest_whatever_the_case_of_the_host() {
        assert_eq!(
            digest(&ssh("Web01.Example.com", 22, "deploy")),
            digest(&ssh(" web01.example.com ", 22, "deploy"))
        );
    }

    #[test]
    fn another_host_port_login_or_protocol_is_another_digest() {
        let base = digest(&ssh("web01.example.com", 22, "deploy"));
        assert_ne!(base, digest(&ssh("web02.example.com", 22, "deploy")));
        assert_ne!(base, digest(&ssh("web01.example.com", 2222, "deploy")));
        assert_ne!(base, digest(&ssh("web01.example.com", 22, "root")));
        // The login is compared as typed.
        assert_ne!(base, digest(&ssh("web01.example.com", 22, "Deploy")));
        let vnc = Endpoint {
            protocol: CredentialProtocol::Vnc,
            host: "web01.example.com".to_owned(),
            port: 22,
            username: Some("deploy".to_owned()),
        };
        assert_ne!(base, digest(&vnc));
    }

    #[test]
    fn a_host_and_login_cannot_be_read_as_another_pair() {
        assert_ne!(
            digest(&ssh("ab", 22, "c")),
            digest(&ssh("a", 22, "bc")),
            "each part is named with its length"
        );
    }

    #[test]
    fn the_entry_names_carry_the_source_and_a_digest_and_no_host_or_login() {
        let endpoint = ssh("web01.example.com", 22, "deploy");
        let name = entry_name("team", &digest(&endpoint));
        assert!(name.starts_with("shared-password/team/"));
        assert!(!name.contains("web01") && !name.contains("deploy"));
        assert_eq!(index_entry("team"), "shared-password-index/team");
    }

    #[test]
    fn an_index_reads_back_and_refuses_anything_else() {
        let digests = vec![
            digest(&ssh("a.example.com", 22, "x")),
            digest(&ssh("b", 1, "y")),
        ];
        assert_eq!(decode_index(&encode_lines(&digests)), Some(digests));
        assert_eq!(decode_index(b"2\nabc"), None);
        assert_eq!(decode_index(b"1\nnot-a-digest"), None);
        assert_eq!(decode_index(&[0xff, 0xfe]), None);
        assert_eq!(decode_index(&encode_lines(&[])), Some(Vec::new()));
        assert_eq!(
            decode_sources(&encode_lines(&["team".to_owned(), "b2".to_owned()])),
            Some(vec!["team".to_owned(), "b2".to_owned()])
        );
        assert_eq!(decode_sources(b"1\nBad-Id"), None);
    }

    #[test]
    fn a_full_index_fits_the_smallest_secret_a_system_store_keeps() {
        let digests: Vec<String> = (0..MAX_PER_SOURCE)
            .map(|n| digest(&ssh("h", 1, &n.to_string())))
            .collect();
        assert!(encode_lines(&digests).len() <= SMALLEST_STORE_SECRET_BYTES);
    }
}
