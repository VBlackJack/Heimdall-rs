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

//! `MobaXterm` sessions (`.mxtsessions`, `.mobaconf`, or the `[Bookmarks]` sections of
//! `MobaXterm.ini`), read as the C# `MobaXtermImporter` reads them.
//!
//! A session is `name= #code#flags%host%port%user%...`: `code` names the protocol, and the
//! `%` fields after it depend on it. `SubRep` names the folder, `\` separating its levels.
//! Passwords are encrypted by `MobaXterm` with its own algorithm: they are only counted, so
//! the user can be told to enter them again.

use serde_json::{Map, Value};

use super::foreign::{Parsed, set_text};

/// Sections whose entries are stored passwords, counted and never read.
const PASSWORD_SECTIONS: [&str; 2] = ["Passwords", "Credentials"];

/// Prefix of the sections that hold sessions: `Bookmarks`, `Bookmarks_1`, ...
const BOOKMARKS_PREFIX: &str = "Bookmarks";

/// Keys of a bookmarks section that are not sessions.
const FOLDER_KEY: &str = "SubRep";
const ICON_KEY: &str = "ImgNum";

/// A key path `MobaXterm` writes when there is none.
const NO_KEY_PATH: &str = "-1";

/// What a key path may not hold: a shell's metacharacters, as the C# `SanitizeFilePath`.
const UNSAFE_PATH_CHARACTERS: [char; 8] = [';', '|', '&', '`', '$', '>', '<', '!'];

/// What a folder name may not hold, as .NET's `Path.GetInvalidFileNameChars()` on Windows,
/// the separators aside.
const UNSAFE_FOLDER_CHARACTERS: [char; 7] = ['<', '>', ':', '"', '|', '?', '*'];

/// A `MobaXterm` protocol code and the C# `ConnectionType` it maps to.
const PROTOCOLS: [(u32, &str); 6] = [
    (109, "SSH"),
    (91, "RDP"),
    (140, "SFTP"),
    (130, "FTP"),
    (128, "VNC"),
    (98, "Telnet"),
];

/// Where the fields of an SSH session are, as the C# reads them.
const SSH_KEY_FIELD: usize = 4;
const SSH_COMPRESSION_FIELD: usize = 11;
const SSH_AGENT_FIELD: usize = 12;

/// Where the fields of an FTP session are.
const FTP_PASSIVE_FIELD: usize = 3;
const FTP_TLS_FIELD: usize = 4;

/// What a read of a file may hold, for a file other people write: nothing larger is kept,
/// whatever the file says, so a crafted one cannot make the reader allocate without bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The longest section name, key or value kept, in bytes.
    pub max_text_len: usize,
    /// The most sections and entries kept in all.
    pub max_entries: usize,
}

impl Limits {
    /// No limit: the file of the person reading it.
    pub const UNLIMITED: Self = Self {
        max_text_len: usize::MAX,
        max_entries: usize::MAX,
    };
}

/// What a read left out, beyond the sessions it gave.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Skipped {
    /// Entries and sections that were not taken: too long, malformed, or of another kind
    /// than a session.
    pub entries: usize,
    /// The read stopped at [`Limits::max_entries`].
    pub truncated: bool,
}

/// Reads a `MobaXterm` file.
#[must_use]
pub fn parse(text: &str) -> Parsed {
    parse_limited(text, &Limits::UNLIMITED).0
}

/// Reads a `MobaXterm` file within `limits`, and says what it left out. Stored passwords
/// are counted as they are met and never decoded nor kept.
#[must_use]
pub fn parse_limited(text: &str, limits: &Limits) -> (Parsed, Skipped) {
    let mut parsed = Parsed::default();
    let mut skipped = Skipped::default();
    let sections = sections(text, limits, &mut parsed.stored_credentials, &mut skipped);
    for (name, entries) in &sections {
        if !starts_with_ignore_case(name, BOOKMARKS_PREFIX) {
            continue;
        }
        let folder = entries
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(FOLDER_KEY))
            .map(|(_, value)| folder_path(value))
            .unwrap_or_default();
        for (key, value) in entries {
            if key.eq_ignore_ascii_case(FOLDER_KEY) || key.eq_ignore_ascii_case(ICON_KEY) {
                continue;
            }
            if let Some(fields) = session(key, value, &folder) {
                parsed.push(fields);
            } else {
                skipped.entries += 1;
            }
        }
    }
    (parsed, skipped)
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

/// The sections of an INI file, in order: a section named twice is the last one, a key
/// given twice in a section its last value, names compared without case, as the C#
/// dictionaries.
fn sections(
    text: &str,
    limits: &Limits,
    credentials: &mut usize,
    skipped: &mut Skipped,
) -> Vec<(String, Vec<(String, String)>)> {
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut current: Option<usize> = None;
    // Sections whose folder was too long to keep: none of their sessions is taken.
    let mut poisoned: Vec<bool> = Vec::new();
    let mut kept = 0_usize;
    // Inside a section of stored passwords: its lines are counted, nothing else.
    let mut in_credentials = false;
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[')
            && let Some((name, _)) = rest.split_once(']')
        {
            in_credentials = PASSWORD_SECTIONS
                .iter()
                .any(|section| name.eq_ignore_ascii_case(section));
            if in_credentials {
                current = None;
                continue;
            }
            // A section named again replaces the first, in its place, as the C# dictionary.
            if let Some(index) = sections
                .iter()
                .position(|(known, _)| known.eq_ignore_ascii_case(name))
            {
                kept = kept.saturating_sub(sections[index].1.len());
                sections[index].1.clear();
                poisoned[index] = false;
                current = Some(index);
            } else if name.len() > limits.max_text_len || kept >= limits.max_entries {
                skipped.entries += 1;
                skipped.truncated |= kept >= limits.max_entries;
                current = None;
            } else {
                kept += 1;
                sections.push((name.to_owned(), Vec::new()));
                poisoned.push(false);
                current = Some(sections.len() - 1);
            }
            continue;
        }
        if in_credentials {
            if line.split_once('=').is_some_and(|(key, _)| !key.is_empty()) {
                *credentials += 1;
            }
            continue;
        }
        let Some(index) = current else {
            continue;
        };
        if poisoned[index] {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && !key.is_empty()
        {
            let key = key.trim();
            if key.len() > limits.max_text_len || value.len() > limits.max_text_len {
                skipped.entries += 1;
                if key.eq_ignore_ascii_case(FOLDER_KEY) {
                    kept = kept.saturating_sub(sections[index].1.len());
                    sections[index].1.clear();
                    poisoned[index] = true;
                }
                continue;
            }
            let entries = &mut sections[index].1;
            match entries
                .iter_mut()
                .find(|(known, _)| known.eq_ignore_ascii_case(key))
            {
                Some(entry) => value.clone_into(&mut entry.1),
                None if kept >= limits.max_entries => {
                    skipped.entries += 1;
                    skipped.truncated = true;
                }
                None => {
                    kept += 1;
                    entries.push((key.to_owned(), value.to_owned()));
                }
            }
        }
    }
    sections
}

/// A session line as the C# `ParseSession`: `None` when it names no host or a protocol
/// Heimdall does not map.
fn session(name: &str, value: &str, folder: &str) -> Option<Map<String, Value>> {
    let value = value.trim();
    let code = protocol_code(value)?;
    let (_, connection_type) = PROTOCOLS.iter().find(|(known, _)| *known == code)?;
    let fields: Vec<&str> = fields_part(value).split('%').collect();
    let field = |index: usize| fields.get(index).copied().unwrap_or_default();
    let host = field(0);
    if host.trim().is_empty() {
        return None;
    }
    let mut dto = Map::new();
    set_text(&mut dto, "displayName", name);
    set_text(&mut dto, "remoteServer", host);
    dto.insert(
        "connectionType".to_owned(),
        Value::String((*connection_type).to_owned()),
    );
    set_text(&mut dto, "group", folder);
    // A port that is not a positive number is the protocol's default, as the C#.
    let port = field(1).trim().parse::<i64>().ok().filter(|port| *port > 0);
    let user = field(2);
    match *connection_type {
        "SSH" | "SFTP" => {
            set_port(&mut dto, "sshPort", port);
            set_text(&mut dto, "sshUsername", user);
            if *connection_type == "SSH" {
                if let Some(path) = key_path(field(SSH_KEY_FIELD)) {
                    dto.insert("sshKeyPath".to_owned(), Value::String(path));
                }
                if fields.len() > SSH_COMPRESSION_FIELD {
                    dto.insert(
                        "sshCompression".to_owned(),
                        Value::Bool(field(SSH_COMPRESSION_FIELD) == "1"),
                    );
                }
                if fields.len() > SSH_AGENT_FIELD {
                    dto.insert(
                        "sshAgentForwarding".to_owned(),
                        Value::Bool(field(SSH_AGENT_FIELD) == "1"),
                    );
                }
            }
        }
        "RDP" => {
            set_port(&mut dto, "remotePort", port);
            set_text(&mut dto, "rdpUsername", user);
        }
        "FTP" => {
            set_port(&mut dto, "ftpPort", port);
            set_text(&mut dto, "ftpUsername", user);
            if fields.len() > FTP_PASSIVE_FIELD {
                dto.insert(
                    "ftpPassiveMode".to_owned(),
                    Value::Bool(field(FTP_PASSIVE_FIELD) != "0"),
                );
            }
            if fields.len() > FTP_TLS_FIELD {
                dto.insert(
                    "ftpUseSsl".to_owned(),
                    Value::Bool(field(FTP_TLS_FIELD) == "1"),
                );
            }
        }
        "VNC" => set_port(&mut dto, "vncPort", port),
        _ => {
            set_port(&mut dto, "telnetPort", port);
        }
    }
    Some(dto)
}

fn set_port(dto: &mut Map<String, Value>, key: &str, port: Option<i64>) {
    if let Some(port) = port {
        dto.insert(key.to_owned(), Value::from(port));
    }
}

/// The number between the first two `#`, as the C# `#(\d+)#`.
fn protocol_code(value: &str) -> Option<u32> {
    let mut search = value;
    while let Some(start) = search.find('#') {
        let rest = &search[start + 1..];
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits > 0 && rest[digits..].starts_with('#') {
            return rest[..digits].parse().ok();
        }
        search = rest;
    }
    None
}

/// What follows the first `%` after the second `#`, as the C# `ExtractFieldsPart`.
fn fields_part(value: &str) -> &str {
    let Some(first) = value.find('#') else {
        return "";
    };
    let Some(second) = value[first + 1..].find('#').map(|at| first + 1 + at) else {
        return "";
    };
    value[second..]
        .find('%')
        .map_or("", |percent| &value[second + percent + 1..])
}

/// A folder as a Heimdall group, as the C# `SanitizeGroupName`: `\` becomes `/`, `../` and
/// `./` go, empty levels go, and what a file name may not hold goes.
fn folder_path(folder: &str) -> String {
    let mut path = folder
        .replace('\\', "/")
        .replace("../", "")
        .replace("./", "");
    while path.contains("//") {
        path = path.replace("//", "/");
    }
    path.trim_matches('/')
        .trim()
        .chars()
        .filter(|c| !c.is_control() && !UNSAFE_FOLDER_CHARACTERS.contains(c))
        .collect()
}

/// A key path as the C# `SanitizeFilePath` keeps it: none when it is empty, `MobaXterm`'s
/// "none", holds a shell metacharacter or climbs out with `..`.
fn key_path(path: &str) -> Option<String> {
    let path = path.trim();
    (!path.is_empty()
        && path != NO_KEY_PATH
        && !path.contains(UNSAFE_PATH_CHARACTERS)
        && !path.contains(".."))
    .then(|| path.to_owned())
}
