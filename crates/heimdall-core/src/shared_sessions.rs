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

//! Shared session sources, as the C# `SharedSessionSourceDto` and `SharedSessionIds`
//! (`Heimdall.Core/Configuration`): a `MobaXterm` file kept by a team, read and never
//! written. The sessions it holds live in memory under identifiers of their own namespace,
//! so a shared session can never stand in for a saved one.

use sealvault::hash;
use std::fmt::Write;

use serde::{Deserialize, Serialize};

/// Prefix of every shared identifier, as the C# `SharedSessionIds.Prefix`.
pub const SHARED_PREFIX: &str = "shared-";

/// The largest shared file read, as the C# `SharedSessionProvider.MaxSourceFileBytes`.
pub const MAX_SOURCE_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// The longest identifier of a source, as the C# `^[a-z0-9]{1,64}$`.
const MAX_SOURCE_ID_LEN: usize = 64;

/// Bytes of the digest an identifier keeps, as the C# `IdHashBytes`.
const ID_HASH_BYTES: usize = 16;

/// The longest host name, as DNS.
const MAX_HOST_LEN: usize = 253;

/// The longest label of a host name, as DNS.
const MAX_LABEL_LEN: usize = 63;

/// One shared source of the settings, as the C# `SharedSessionSourceDto`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedSource {
    /// Its identifier: lower-case letters and digits.
    pub id: String,
    /// The name the user gave it.
    #[serde(default)]
    pub name: String,
    /// The file read.
    #[serde(default)]
    pub path: String,
    /// Off, nothing is read.
    #[serde(default = "enabled")]
    pub enabled: bool,
}

fn enabled() -> bool {
    true
}

/// Whether `id` names a source, as the C# `IsValidSourceId`.
#[must_use]
pub fn is_valid_source_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_SOURCE_ID_LEN
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

/// Whether `id` is in the shared namespace, as the C# `IsShared`.
#[must_use]
pub fn is_shared(id: &str) -> bool {
    id.starts_with(SHARED_PREFIX)
}

/// The prefix of the identifiers of source `source_id`: `shared-<source>-`.
#[must_use]
pub fn prefix_for(source_id: &str) -> String {
    format!("{SHARED_PREFIX}{source_id}-")
}

/// The identifier of the session `name` filed in `group` of source `source_id`: the prefix,
/// then a hash of what names the entry, so the same file read twice gives the same
/// identifiers, as the C# `MobaXtermImporter.BuildId`.
#[must_use]
pub fn session_id(source_id: &str, group: &str, name: &str) -> String {
    let input = format!("session\u{1f}{group}\u{1f}{name}");
    let digest = hash::sha256(input.as_bytes());
    let mut hex = String::with_capacity(ID_HASH_BYTES * 2);
    for byte in digest.iter().take(ID_HASH_BYTES) {
        let _ = write!(hex, "{byte:02x}");
    }
    format!("{}{hex}", prefix_for(source_id))
}

/// Whether `host` is a host name, an IPv4 address or an IPv6 address, as the C#
/// `SchemaValidator.IsValidHost`: a file other people write may not slip anything else
/// into a connection.
#[must_use]
pub fn is_valid_host(host: &str) -> bool {
    if host.is_empty() || host.len() > MAX_HOST_LEN {
        return false;
    }
    let bare = host
        .strip_prefix('[')
        .map_or(host, |inner| inner.strip_suffix(']').unwrap_or(inner));
    let ipv6_like = !bare.is_empty()
        && bare.contains(':')
        && bare
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b':');
    ipv6_like
        || host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= MAX_LABEL_LEN
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_ids_are_lower_case_letters_and_digits() {
        assert!(is_valid_source_id("team01"));
        assert!(!is_valid_source_id(""));
        assert!(!is_valid_source_id("Team"));
        assert!(!is_valid_source_id("a-b"));
        assert!(!is_valid_source_id(&"a".repeat(65)));
    }

    #[test]
    fn a_session_identifier_is_stable_and_in_its_sources_namespace() {
        let id = session_id("team", "Prod", "web01");
        assert!(id.starts_with("shared-team-"));
        assert!(is_shared(&id));
        assert_eq!(id, session_id("team", "Prod", "web01"));
        assert_ne!(id, session_id("team", "Prod", "web02"));
        assert_ne!(id, session_id("other", "Prod", "web01"));
    }

    #[test]
    fn hosts_are_names_or_addresses_and_nothing_a_shell_would_read() {
        for good in ["web01", "web01.example.com", "10.0.0.1", "[::1]", "fe80::1"] {
            assert!(is_valid_host(good), "{good}");
        }
        for bad in [
            "",
            "-a",
            "a b",
            "a;b",
            "a$(b)",
            "a/b",
            "a..b",
            "host\nname",
            "-oProxyCommand=x",
        ] {
            assert!(!is_valid_host(bad), "{bad:?}");
        }
    }
}
