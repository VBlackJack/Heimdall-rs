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

//! Import of `PuTTY`'s saved sessions, as the C# Heimdall reads them (`PuttySessionParser`,
//! `PuttySessionImporter`): the SSH sessions, their host, port, user and key.
//!
//! `PuTTY` keeps them in the registry on Windows (`HKCU\Software\SimonTatham\PuTTY\Sessions`)
//! and as files in `~/.putty/sessions` elsewhere; both give the same names and values, read
//! here from either. A session becomes the same candidate as an OpenSSH host, without a
//! gateway chain, so both imports share their preview and their plan.

use std::collections::HashMap;

use crate::import::openssh::Candidate;
use crate::profile::DEFAULT_SSH_PORT;

/// The folder of `PuTTY`'s session files, under the home folder, outside Windows.
pub const SESSIONS_FOLDER: &str = ".putty/sessions";

/// The registry key of `PuTTY`'s sessions, under `HKEY_CURRENT_USER`, on Windows.
pub const SESSIONS_KEY: &str = r"Software\SimonTatham\PuTTY\Sessions";

/// The session `PuTTY` keeps its defaults in: not a server.
const DEFAULT_SETTINGS: &str = "Default Settings";

/// Longest part of a remote command said in a diagnostic.
const COMMAND_SHOWN: usize = 80;

/// A value of a session, as the registry or a file holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Text.
    Text(String),
    /// A number.
    Number(u32),
}

impl Value {
    fn text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Number(number) => number.to_string(),
        }
    }
}

/// A session as `PuTTY` stores it: its name, `%`-encoded, and its values by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawSession {
    /// The name as stored.
    pub encoded_name: String,
    /// Its values, names lowercase.
    pub values: HashMap<String, Value>,
}

impl RawSession {
    /// A session named `encoded_name`, with `values` whatever their names' case.
    #[must_use]
    pub fn new(encoded_name: String, values: impl IntoIterator<Item = (String, Value)>) -> Self {
        Self {
            encoded_name,
            values: values
                .into_iter()
                .map(|(name, value)| (name.to_lowercase(), value))
                .collect(),
        }
    }

    /// A session file of `~/.putty/sessions`: `Key=value` lines, every value text.
    #[must_use]
    pub fn from_file(encoded_name: String, text: &str) -> Self {
        Self::new(
            encoded_name,
            text.lines().filter_map(|line| {
                let (name, value) = line.split_once('=')?;
                Some((name.trim().to_owned(), Value::Text(value.to_owned())))
            }),
        )
    }

    fn text(&self, name: &str) -> Option<String> {
        self.values
            .get(&name.to_lowercase())
            .map(Value::text)
            .filter(|value| !value.trim().is_empty())
    }
}

/// How much a diagnostic matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Read differently, as intended.
    Info,
    /// Left out, or to check.
    Warning,
}

/// What a diagnostic says, as the C# `PuttyDiagnosticCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// "Default Settings", not a server.
    DefaultSettingsSkipped,
    /// A session of another protocol than SSH; carries it.
    NotSsh,
    /// A port that is not 1 to 65535: 22 is used.
    InvalidPort,
    /// A `PuTTY` key (`.ppk`), kept as it is.
    PpkKey,
    /// A proxy, not imported.
    ProxyNotMapped,
    /// Port forwardings, not imported; carries how many.
    ForwardingsNotMapped,
    /// A remote command, not imported.
    RemoteCommandNotMapped,
    /// No host: the session cannot be imported.
    MissingHost,
}

/// Something the import says about a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// How much it matters.
    pub level: Level,
    /// The session, by its name.
    pub session: String,
    /// What it says.
    pub code: Code,
    /// The value concerned, when there is one.
    pub context: Option<String>,
}

/// What `PuTTY`'s sessions give.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    /// The SSH sessions; one with no host has an empty host and cannot be imported.
    pub candidates: Vec<Candidate>,
    /// What was read differently or left out.
    pub diagnostics: Vec<Diagnostic>,
}

/// Reads `sessions`, in the order given.
#[must_use]
pub fn parse(sessions: &[RawSession]) -> Parsed {
    let mut parsed = Parsed::default();
    for session in sessions {
        let name = decode_name(&session.encoded_name);
        let mut say = |level, code, context: Option<String>| {
            parsed.diagnostics.push(Diagnostic {
                level,
                session: name.clone(),
                code,
                context,
            });
        };
        if name.eq_ignore_ascii_case(DEFAULT_SETTINGS) {
            say(Level::Info, Code::DefaultSettingsSkipped, None);
            continue;
        }
        let protocol = session.text("Protocol");
        if !protocol
            .as_deref()
            .is_some_and(|protocol| protocol.eq_ignore_ascii_case("ssh"))
        {
            say(
                Level::Info,
                Code::NotSsh,
                Some(protocol.unwrap_or_default()),
            );
            continue;
        }
        let port = match session.values.get("portnumber") {
            None => DEFAULT_SSH_PORT,
            Some(value) => match value.text().trim().parse::<u16>() {
                Ok(port) if port != 0 => port,
                _ => {
                    say(Level::Warning, Code::InvalidPort, Some(value.text()));
                    DEFAULT_SSH_PORT
                }
            },
        };
        let key = session.text("PublicKeyFile");
        if let Some(key) = key
            .as_ref()
            .filter(|key| key.to_lowercase().ends_with(".ppk"))
        {
            say(Level::Warning, Code::PpkKey, Some(key.clone()));
        }
        let proxy_method = session.text("ProxyMethod");
        let proxy_host = session.text("ProxyHost");
        let proxy_port = session.text("ProxyPort");
        let proxied = proxy_host.is_some()
            || proxy_port.is_some()
            || proxy_method
                .as_deref()
                .and_then(|method| method.trim().parse::<i64>().ok())
                .is_some_and(|method| method != 0);
        if proxied {
            say(
                Level::Info,
                Code::ProxyNotMapped,
                Some(proxy_host.unwrap_or_default()),
            );
        }
        if let Some(forwards) = session.text("PortForwardings") {
            let count = forwards
                .split(['\0', ',', '\t'])
                .filter(|forward| !forward.is_empty())
                .count();
            say(
                Level::Info,
                Code::ForwardingsNotMapped,
                Some(count.to_string()),
            );
        }
        if let Some(command) = session.text("RemoteCommand") {
            let shown = if command.chars().count() > COMMAND_SHOWN {
                format!(
                    "{}...",
                    command.chars().take(COMMAND_SHOWN).collect::<String>()
                )
            } else {
                command
            };
            say(Level::Info, Code::RemoteCommandNotMapped, Some(shown));
        }
        let host = session.text("HostName").unwrap_or_default();
        if host.trim().is_empty() {
            say(Level::Warning, Code::MissingHost, None);
        }
        parsed.candidates.push(Candidate {
            alias: name.clone(),
            host_name: host.trim().to_owned(),
            port,
            user: session.text("UserName"),
            identity_file: key,
            line: 0,
            proxy_jump: Vec::new(),
        });
    }
    parsed
}

/// A session name as `PuTTY` stores it decoded: `%XX` is the byte it stands for.
#[must_use]
pub fn decode_name(encoded: &str) -> String {
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(hex) = encoded.get(index + 1..index + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            decoded.push(byte);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}
