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
use std::collections::HashMap;
use std::fmt::Write;
use std::net::{Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};

/// Prefix of every shared identifier, as the C# `SharedSessionIds.Prefix`.
pub const SHARED_PREFIX: &str = "shared-";

/// The largest shared file read, as the C# `SharedSessionProvider.MaxSourceFileBytes`.
pub const MAX_SOURCE_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// The longest identifier of a source, as the C# `^[a-z0-9]{1,64}$`.
const MAX_SOURCE_ID_LEN: usize = 64;

/// Bytes of the digest an identifier keeps, as the C# `IdHashBytes`.
const ID_HASH_BYTES: usize = 16;

/// Random bytes of a new source's identifier, as a GUID.
const NEW_ID_BYTES: usize = 16;

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

/// The source a shared identifier belongs to, as the C# `SharedSessionIds.SourceIdOf`: what
/// sits between the prefix and the next dash. None for an identifier that is not shared or
/// whose source part is not a valid source identifier.
#[must_use]
pub fn source_id_of(id: &str) -> Option<&str> {
    let rest = id.strip_prefix(SHARED_PREFIX)?;
    let (source, _) = rest.split_once('-')?;
    is_valid_source_id(source).then_some(source)
}

/// The identifier of the gateway of source `source_id` that logs in to `host` on `port` as
/// `user` (without case), so the same file read twice gives the same one, as the C#
/// `ParseContext.GetOrAddGateway` builds it.
#[must_use]
pub fn gateway_id(source_id: &str, host: &str, port: u16, user: &str) -> String {
    let input = format!(
        "gateway\u{1f}{}\u{1f}{port}\u{1f}{}",
        host.to_uppercase(),
        user.to_uppercase()
    );
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
    // An address is parsed, not guessed from its characters; brackets come in pairs.
    if host.starts_with('[') || host.ends_with(']') {
        return host
            .strip_prefix('[')
            .and_then(|inner| inner.strip_suffix(']'))
            .is_some_and(|inner| inner.parse::<Ipv6Addr>().is_ok());
    }
    if host.contains(':') {
        return host.parse::<Ipv6Addr>().is_ok();
    }
    // Four numbers are an address, and must be one that exists.
    if host.split('.').count() == 4 && host.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return host.parse::<Ipv4Addr>().is_ok();
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= MAX_LABEL_LEN
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

/// Why a source's name cannot be saved, as the C# `SettingsSharedSourceName*` errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameProblem {
    /// No name.
    Required,
    /// Another source has this name, in any case.
    Duplicate,
}

/// Why a source's path cannot be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathProblem {
    /// No path.
    Required,
    /// Not an absolute path, nor one that starts with a `%VARIABLE%`, or it holds a control
    /// character: the checks of the C# settings import.
    Invalid,
}

/// Why a source's identifier cannot be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdProblem {
    /// Not lower-case letters and digits, or too long.
    Invalid,
    /// Another source has the same identifier.
    Duplicate,
}

/// What is wrong with one source of a list, each part checked on its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceProblems {
    /// Its identifier.
    pub id: Option<IdProblem>,
    /// Its name.
    pub name: Option<NameProblem>,
    /// Its path.
    pub path: Option<PathProblem>,
}

impl SourceProblems {
    /// Whether the source can be saved.
    #[must_use]
    pub fn is_ok(self) -> bool {
        self.id.is_none() && self.name.is_none() && self.path.is_none()
    }
}

/// Why `path` cannot be a source's file, by the checks of the C# settings import: it must say
/// something, hold no control character, and be absolute or start with a `%VARIABLE%`.
#[must_use]
pub fn path_problem(path: &str) -> Option<PathProblem> {
    let path = path.trim();
    if path.is_empty() {
        Some(PathProblem::Required)
    } else if path.chars().any(char::is_control)
        || !crate::import::csharp_settings::absolute_or_expanded(path)
    {
        Some(PathProblem::Invalid)
    } else {
        None
    }
}

/// What is wrong with each source of `sources`, in order, as the C# settings page checks its
/// rows (a name that is required and not repeated in any case, a path) and its import checks
/// them (a valid identifier of its own, an absolute path).
#[must_use]
pub fn source_problems(sources: &[SharedSource]) -> Vec<SourceProblems> {
    let mut names: HashMap<String, usize> = HashMap::new();
    let mut ids: HashMap<&str, usize> = HashMap::new();
    for source in sources {
        let name = source.name.trim();
        if !name.is_empty() {
            *names.entry(name.to_lowercase()).or_default() += 1;
        }
        *ids.entry(source.id.as_str()).or_default() += 1;
    }
    sources
        .iter()
        .map(|source| {
            let name = source.name.trim();
            SourceProblems {
                id: if !is_valid_source_id(&source.id) {
                    Some(IdProblem::Invalid)
                } else if ids.get(source.id.as_str()).is_some_and(|n| *n > 1) {
                    Some(IdProblem::Duplicate)
                } else {
                    None
                },
                name: if name.is_empty() {
                    Some(NameProblem::Required)
                } else if names.get(&name.to_lowercase()).is_some_and(|n| *n > 1) {
                    Some(NameProblem::Duplicate)
                } else {
                    None
                },
                path: path_problem(&source.path),
            }
        })
        .collect()
}

/// A new identifier for a source, as the C# `SharedSessionIds.NewSourceId` (a GUID without
/// dashes): 32 lower-case hexadecimal digits, so it is one [`is_valid_source_id`] accepts.
///
/// # Errors
///
/// The system's random source failed.
pub fn new_source_id() -> Result<String, sealvault::Error> {
    let mut bytes = [0_u8; NEW_ID_BYTES];
    sealvault::random::fill(&mut bytes)?;
    let mut id = String::with_capacity(NEW_ID_BYTES * 2);
    for byte in bytes {
        let _ = write!(id, "{byte:02x}");
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_source_of_a_shared_identifier_is_read_back_from_it() {
        let id = session_id("team01", "Prod", "web01");
        assert_eq!(source_id_of(&id), Some("team01"));
        assert_eq!(
            source_id_of(&gateway_id("team01", "h", 22, "u")),
            Some("team01")
        );
        assert_eq!(source_id_of("local-profile"), None);
        assert_eq!(source_id_of("shared-nodash"), None);
        assert_eq!(source_id_of("shared-Bad-abc"), None);
        assert_eq!(source_id_of("shared--abc"), None);
    }

    #[test]
    fn source_ids_are_lower_case_letters_and_digits() {
        assert!(is_valid_source_id("team01"));
        assert!(!is_valid_source_id(""));
        assert!(!is_valid_source_id("Team"));
        assert!(!is_valid_source_id("a-b"));
        assert!(!is_valid_source_id(&"a".repeat(65)));
    }

    fn source(id: &str, name: &str, path: &str) -> SharedSource {
        SharedSource {
            id: id.to_owned(),
            name: name.to_owned(),
            path: path.to_owned(),
            enabled: true,
        }
    }

    #[test]
    fn a_new_source_identifier_is_valid_and_new_each_time() {
        let first = new_source_id().expect("random");
        assert!(is_valid_source_id(&first));
        assert_eq!(first.len(), 32);
        assert_ne!(first, new_source_id().expect("random"));
    }

    #[test]
    fn a_source_path_is_absolute_or_starts_with_a_variable() {
        assert_eq!(path_problem(""), Some(PathProblem::Required));
        assert_eq!(path_problem("  "), Some(PathProblem::Required));
        assert_eq!(path_problem("team.mxtsessions"), Some(PathProblem::Invalid));
        assert_eq!(path_problem("/srv/a\u{7}b"), Some(PathProblem::Invalid));
        assert_eq!(path_problem("C:\\share\\team.mxtsessions"), None);
        assert_eq!(path_problem("\\\\server\\share\\team.mxtsessions"), None);
        assert_eq!(path_problem("/srv/team.mxtsessions"), None);
        assert_eq!(path_problem("%USERPROFILE%\\team.mxtsessions"), None);
    }

    #[test]
    fn source_rows_need_a_unique_id_a_unique_name_and_a_path() {
        let rows = [
            source("one", "Team", "/srv/a"),
            source("two", "team", "/srv/b"),
            source("one", "Other", "/srv/c"),
            source("Bad", "", "relative"),
        ];
        let problems = source_problems(&rows);
        assert_eq!(problems[0].id, Some(IdProblem::Duplicate));
        assert_eq!(problems[0].name, Some(NameProblem::Duplicate));
        assert_eq!(problems[1].name, Some(NameProblem::Duplicate));
        assert_eq!(problems[1].id, None);
        assert!(problems[1].path.is_none());
        assert_eq!(problems[2].name, None);
        assert_eq!(problems[3].id, Some(IdProblem::Invalid));
        assert_eq!(problems[3].name, Some(NameProblem::Required));
        assert_eq!(problems[3].path, Some(PathProblem::Invalid));
        assert!(source_problems(&[source("one", "Team", "/srv/a")])[0].is_ok());
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
        for good in [
            "web01",
            "web01.example.com",
            "10.0.0.1",
            "[::1]",
            "fe80::1",
            "::ffff:192.0.2.1",
            "[::ffff:192.0.2.1]",
        ] {
            assert!(is_valid_host(good), "{good}");
        }
        for bad in [
            ":",
            "1:2:3",
            "[::1",
            "::1]",
            "[]",
            "[web01]",
            "999.1.1.1",
            "1:2:3:4:5:6:7:8:9",
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
