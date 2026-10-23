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
//! A session is `name= #icon#type%host%port%user%...`: `type` names the protocol (the icon is
//! the user's to change and decides nothing), and the `%` fields after it depend on it. `SubRep` names the folder, `\` separating its levels.
//! Passwords are encrypted by `MobaXterm` with its own algorithm: they are only counted, so
//! the user can be told to enter them again.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Map, Value};

use super::foreign::{FileWarning, GATEWAY_ID_KEY, Parsed, set_text};
use crate::profile::DEFAULT_SSH_PORT;
use crate::shared_sessions::is_valid_host;

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

/// A `MobaXterm` session type and the C# `ConnectionType` it maps to, as the keys of the C#
/// `SessionLayouts` (`MobaXtermImporter.cs:117-165`). FTP is read as well: the C# leaves it out,
/// Heimdall has FTP profiles.
const PROTOCOLS: [(u32, &str); 6] = [
    (0, "SSH"),
    (1, "Telnet"),
    (4, "RDP"),
    (5, "VNC"),
    (6, "FTP"),
    (7, "SFTP"),
];

/// The session type of a WSL distribution (`MobaXtermImporter.cs`, `WslTypeCode`): a Linux system
/// on the computer that runs `MobaXterm`, with nothing to connect to from another one.
const WSL_TYPE_CODE: u32 = 14;

/// Where the fields of an SSH session are, as the C# reads them.
const SSH_KEY_FIELD: usize = 4;
const SSH_COMPRESSION_FIELD: usize = 11;
const SSH_AGENT_FIELD: usize = 12;

/// Where an SSH session keeps the command `MobaXterm` runs on the remote host after login, as
/// the C# `LoginCommandIndex` (`MobaXtermImporter.cs:123`).
const SSH_LOGIN_COMMAND_FIELD: usize = 6;

/// Where an SSH session names its gateway (jump host): host, port and user, as the C#
/// `SessionLayouts` (`MobaXtermImporter.cs:98-106`).
const SSH_GATEWAY_FIELDS: [usize; 3] = [7, 8, 9];

/// Where an RDP session names its SSH gateway: host, port and user
/// (`MobaXtermImporter.cs:107-113`).
const RDP_GATEWAY_FIELDS: [usize; 3] = [12, 13, 14];

/// Where an RDP session names its RD Gateway server, and the packed field holding the gateway
/// settings (`MobaXtermImporter.cs`, `RdGatewaySlot(HostIndex: 19, SettingsIndex: 30)`).
const RD_GATEWAY_HOST_FIELD: usize = 19;
const RD_GATEWAY_SETTINGS_FIELD: usize = 30;

/// The settings field is `flags,mode,server,login,?,auth,`, by position once split on ','.
const RD_GATEWAY_SETTINGS_SEPARATOR: char = ',';
const RD_GATEWAY_MODE_PART: usize = 1;
const RD_GATEWAY_SERVER_PART: usize = 2;
const RD_GATEWAY_AUTH_PART: usize = 5;

/// "Use RDP gateway": Never 0, Detect 1, Always 2. Never and Detect clear the server.
const RD_GATEWAY_MODE_ALWAYS: u32 = 2;

/// The modes that leave the session direct when no server is named: Never 0 and Detect 1.
const RD_GATEWAY_DIRECT_MODES: [u32; 2] = [0, 1];

/// The gateway sign-ins Heimdall reproduces: 0 "Same as end server", 1 "Prompt during
/// connection" and 2 "Specify login/password" all sign in with a password, which the RDP client
/// asks for. 3 "Use smartcard" and any other code are not reproduced.
const PASSWORD_RD_GATEWAY_AUTHS: [u32; 3] = [0, 1, 2];

/// `MobaXterm` writes this in a proxy type field that names no proxy.
const NO_PROXY_TYPE: &str = "0";

/// Where a session keeps the jump host and the proxy its protocol cannot honour, as the C#
/// `SessionLayouts` (`MobaXtermImporter.cs:100-157`): the jump host's host field (Telnet and
/// VNC, which have a slot but are never routed through one), the proxy's type and host fields.
#[derive(Clone, Copy)]
struct UnsupportedRoutes {
    jump_host: Option<usize>,
    proxy: Option<(usize, usize)>,
}

/// The routes of `connection_type` that Heimdall cannot take, as the C# layout table: no jump
/// host slot is read for SSH, SFTP and RDP (SSH and RDP are routed through one, SFTP has no
/// slot), no proxy for RDP.
fn unsupported_routes(connection_type: &str) -> UnsupportedRoutes {
    let (jump_host, proxy) = match connection_type {
        "SSH" => (None, Some((18, 19))),
        "SFTP" => (None, Some((9, 10))),
        "Telnet" => (Some(5), Some((10, 11))),
        "VNC" => (Some(4), Some((12, 13))),
        _ => (None, None),
    };
    UnsupportedRoutes { jump_host, proxy }
}

/// Refuses a session that names a jump host or a proxy its protocol cannot honour, read from
/// the session's `fields`: taken as a direct connection it would reach its host around the route
/// the file asked for (`MobaXtermImporter.cs:308-326`).
fn refuse_unsupported_routes(connection_type: &str, fields: &[&str]) -> Result<(), Refusal> {
    let field = |index: usize| fields.get(index).copied().unwrap_or_default().trim();
    let unsupported = unsupported_routes(connection_type);
    if let Some(at) = unsupported.jump_host
        && !field(at).is_empty()
    {
        return Err(Refusal::JumpHost(field(at).to_owned()));
    }
    if let Some((type_at, host_at)) = unsupported.proxy {
        let (proxy_type, proxy_host) = (field(type_at), field(host_at));
        if (!proxy_type.is_empty() && proxy_type != NO_PROXY_TYPE) || !proxy_host.is_empty() {
            return Err(Refusal::Proxy(proxy_host.to_owned()));
        }
    }
    Ok(())
}

/// Prefix of the identifier a gateway carries until the caller gives it its own.
const GATEWAY_PLACEHOLDER_ID: &str = "foreign-gateway-";

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

/// Opens and closes a variable token in a login field, as the C# `VariableTokenRegex`.
const TOKEN_OPEN: char = '[';
const TOKEN_CLOSE: char = ']';

/// Delimits an environment variable reference, as .NET `ExpandEnvironmentVariables`.
const ENV_DELIMITER: char = '%';

/// A variable's value: the text it stands for, or `None` when it has none.
pub type VariableResolver<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Reads a `MobaXterm` file, no variable known: every `[NAME]` in a login is kept as written
/// and reported, as the C# `MobaXtermImportOptions.Default`.
#[must_use]
pub fn parse(text: &str) -> Parsed {
    parse_with(text, &|_| None)
}

/// Reads a `MobaXterm` file, each `[NAME]` of a session's login replaced through `resolve`
/// (the C# `ParseContext.ResolveVariables`, `MobaXtermImporter.cs:511`). A name `resolve`
/// gives nothing for stays as written and is reported once for its session.
#[must_use]
pub fn parse_with(text: &str, resolve: VariableResolver<'_>) -> Parsed {
    parse_limited(text, resolve, &Limits::UNLIMITED).0
}

/// Reads a `MobaXterm` file as [`parse_with`] does, within `limits`, and says what it left
/// out. Stored passwords are counted as they are met and never decoded nor kept.
#[must_use]
pub fn parse_limited(
    text: &str,
    resolve: VariableResolver<'_>,
    limits: &Limits,
) -> (Parsed, Skipped) {
    let mut parsed = Parsed::default();
    let mut skipped = Skipped::default();
    let mut gateways = GatewayBook::default();
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
            let mut unresolved = Vec::new();
            let outcome = session(key, value, &folder, resolve, &mut gateways, &mut unresolved);
            if let Err(Refusal::GatewayHost(host)) = &outcome {
                // Never imported as a direct connection: that would reach the host around
                // the bastion the file asked for (`MobaXtermImporter.cs:221-229`).
                parsed.warnings.push(FileWarning::InvalidGatewayHost {
                    session: key.clone(),
                    folder: folder.clone(),
                    host: host.clone(),
                });
            }
            match &outcome {
                Err(Refusal::Wsl) => parsed.warnings.push(FileWarning::WslNotSupported {
                    session: key.clone(),
                    folder: folder.clone(),
                }),
                Err(Refusal::UnsupportedType(type_code)) => {
                    parsed.warnings.push(FileWarning::UnsupportedSessionType {
                        session: key.clone(),
                        folder: folder.clone(),
                        type_code: type_code.clone(),
                    });
                }
                Err(Refusal::JumpHost(host)) => {
                    parsed.warnings.push(FileWarning::JumpHostNotSupported {
                        session: key.clone(),
                        folder: folder.clone(),
                        host: host.clone(),
                    });
                }
                Err(Refusal::Proxy(host)) => {
                    parsed.warnings.push(FileWarning::ProxyNotSupported {
                        session: key.clone(),
                        folder: folder.clone(),
                        host: host.clone(),
                    });
                }
                Err(Refusal::RdGatewayHost(host)) => {
                    parsed.warnings.push(FileWarning::InvalidRdGatewayHost {
                        session: key.clone(),
                        folder: folder.clone(),
                        host: host.clone(),
                    });
                }
                Err(Refusal::RdGatewayRoute(host)) => {
                    // Never imported as a direct connection: that would go around the RD
                    // Gateway the file asked for (`MobaXtermImporter.cs:330-335`).
                    parsed.warnings.push(FileWarning::RdGatewayNotSupported {
                        session: key.clone(),
                        folder: folder.clone(),
                        host: host.clone(),
                    });
                }
                _ => {}
            }
            if let Ok(fields) = outcome {
                parsed.push(fields);
                // Once per session, whichever fields carry the token: the user has one
                // value to define (`MobaXtermImporter.cs:527`).
                let mut reported = HashSet::new();
                parsed.warnings.extend(
                    unresolved
                        .into_iter()
                        .filter(|variable| reported.insert(variable.to_uppercase()))
                        .map(|variable| FileWarning::UnresolvedVariable {
                            session: key.clone(),
                            name: variable,
                        }),
                );
            } else {
                skipped.entries += 1;
            }
        }
    }
    parsed.gateways = gateways.list;
    (parsed, skipped)
}

/// The RD Gateway an RDP session goes through, as the C# `ReadRdGateway`
/// (`MobaXtermImporter.cs:462-480`), and whether Heimdall can take that route.
///
/// Measured: "Never" and "Detect" leave the server empty in both fields, "Always" writes it in
/// both, and a session never given gateway settings has no settings field at all. A session that
/// names no server stays direct, as in `MobaXterm`. One that names a server is reproduced only
/// when the settings confirm "Always", the same server and a password sign-in; anything else
/// (smart card, a code not measured, a missing settings field, another server) is a route that
/// cannot be taken as written, so the session is refused rather than imported as a direct
/// connection that would go around the gateway.
struct RdGatewayRoute {
    server: String,
    reproducible: bool,
}

fn read_rd_gateway<'a>(field: &dyn Fn(usize) -> &'a str) -> RdGatewayRoute {
    let server = field(RD_GATEWAY_HOST_FIELD).trim();
    let settings: Vec<&str> = field(RD_GATEWAY_SETTINGS_FIELD)
        .split(RD_GATEWAY_SETTINGS_SEPARATOR)
        .collect();
    let part = |index: usize| settings.get(index).copied().unwrap_or_default().trim();
    let settings_server = part(RD_GATEWAY_SERVER_PART);
    let code = |index: usize| {
        let text = part(index);
        // Digits only, as the C# `NumberStyles.None`.
        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        text.parse::<u32>().ok()
    };
    if server.is_empty() && settings_server.is_empty() {
        // No gateway named: direct, but only when the settings are absent or say Never or
        // Detect. Anything else (Always without a server, a mode not measured, garbage) is a
        // gateway setting that cannot be followed, not a license to go direct.
        let absent = settings.iter().all(|setting| setting.trim().is_empty());
        let direct = absent
            || code(RD_GATEWAY_MODE_PART)
                .is_some_and(|mode| RD_GATEWAY_DIRECT_MODES.contains(&mode));
        return RdGatewayRoute {
            server: String::new(),
            reproducible: direct,
        };
    }
    let reproducible = server.eq_ignore_ascii_case(settings_server)
        && code(RD_GATEWAY_MODE_PART) == Some(RD_GATEWAY_MODE_ALWAYS)
        && code(RD_GATEWAY_AUTH_PART).is_some_and(|auth| PASSWORD_RD_GATEWAY_AUTHS.contains(&auth));
    let named = if server.is_empty() {
        settings_server
    } else {
        server
    };
    RdGatewayRoute {
        server: named.to_owned(),
        reproducible,
    }
}

/// Why a session line gives no session.
enum Refusal {
    /// No host, or a line of another shape.
    NotASession,
    /// A WSL distribution.
    Wsl,
    /// A session type Heimdall does not take; the type code as written.
    UnsupportedType(String),
    /// A jump host on a protocol that is never routed through one; the host as written.
    JumpHost(String),
    /// A proxy; the proxy host as written, possibly empty.
    Proxy(String),
    /// The session names a gateway whose host is not usable.
    GatewayHost(String),
    /// The RDP session names an RD Gateway whose host is not usable.
    RdGatewayHost(String),
    /// The RDP session goes through an RD Gateway Heimdall cannot take as written (a smart card
    /// sign-in, or a setting not recognised).
    RdGatewayRoute(String),
}

/// The gateways the sessions of a file name, each once: two sessions through the same host,
/// port and user (compared without case) share it, as the C# `ParseContext.GetOrAddGateway`.
#[derive(Default)]
struct GatewayBook {
    ids: HashMap<(String, u16, String), String>,
    /// The gateways, as the C# `SshGatewayDto`.
    list: Vec<Value>,
}

impl GatewayBook {
    fn get_or_add(&mut self, host: &str, port: u16, user: &str) -> String {
        let key = (host.to_uppercase(), port, user.to_uppercase());
        if let Some(id) = self.ids.get(&key) {
            return id.clone();
        }
        let id = format!("{GATEWAY_PLACEHOLDER_ID}{}", self.list.len());
        let mut dto = Map::new();
        dto.insert("id".to_owned(), Value::String(id.clone()));
        dto.insert(
            "name".to_owned(),
            Value::String(gateway_name(host, port, user)),
        );
        dto.insert("host".to_owned(), Value::String(host.to_owned()));
        dto.insert("port".to_owned(), Value::from(port));
        set_text(&mut dto, "user", user);
        self.list.push(Value::Object(dto));
        self.ids.insert(key, id.clone());
        id
    }
}

/// A gateway's name: `user@host`, then `:port` unless it is the SSH default, as the C#
/// `BuildGatewayName`.
fn gateway_name(host: &str, port: u16, user: &str) -> String {
    let mut name = if user.is_empty() {
        host.to_owned()
    } else {
        format!("{user}@{host}")
    };
    if port != DEFAULT_SSH_PORT {
        name.push(':');
        name.push_str(&port.to_string());
    }
    name
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

/// A session line as the C# `ParseSession`: refused when it names no host, a protocol
/// Heimdall does not map, or a gateway whose host is not usable.
fn session(
    name: &str,
    value: &str,
    folder: &str,
    resolve: VariableResolver<'_>,
    gateways: &mut GatewayBook,
    unresolved: &mut Vec<String>,
) -> Result<Map<String, Value>, Refusal> {
    let value = value.trim();
    let (type_text, fields_text) = split_session(value).ok_or(Refusal::NotASession)?;
    // Digits only, as the C# `NumberStyles.None`: anything else is a type to report, not a
    // number to read.
    let code = type_text
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| type_text.parse::<u32>().ok())
        .flatten();
    if code == Some(WSL_TYPE_CODE) {
        return Err(Refusal::Wsl);
    }
    let (_, connection_type) = PROTOCOLS
        .iter()
        .find(|(known, _)| Some(*known) == code)
        .ok_or_else(|| Refusal::UnsupportedType(type_text.to_owned()))?;
    let fields: Vec<&str> = fields_text.split('%').collect();
    let field = |index: usize| fields.get(index).copied().unwrap_or_default();
    let host = field(0);
    if host.trim().is_empty() {
        return Err(Refusal::NotASession);
    }
    // Before anything else is read of the session: it is not taken at all, as the C#.
    refuse_unsupported_routes(connection_type, &fields)?;
    let mut dto = Map::new();
    set_text(&mut dto, "displayName", name);
    set_text(&mut dto, "remoteServer", host);
    dto.insert(
        "connectionType".to_owned(),
        Value::String((*connection_type).to_owned()),
    );
    set_text(&mut dto, "group", folder);
    // The gateway first, as the C#: its login's variables are met before the session's own.
    let route = match *connection_type {
        "SSH" => Some(SSH_GATEWAY_FIELDS),
        "RDP" => Some(RDP_GATEWAY_FIELDS),
        _ => None,
    };
    if let Some([host_at, port_at, user_at]) = route {
        let gateway_host = field(host_at).trim();
        if !gateway_host.is_empty() {
            if !is_valid_host(gateway_host) {
                return Err(Refusal::GatewayHost(gateway_host.to_owned()));
            }
            let gateway_port = field(port_at)
                .trim()
                .parse::<u16>()
                .ok()
                .filter(|port| *port > 0)
                .unwrap_or(DEFAULT_SSH_PORT);
            let gateway_user = replace_variables(field(user_at), resolve, unresolved);
            let id = gateways.get_or_add(gateway_host, gateway_port, gateway_user.trim());
            dto.insert(GATEWAY_ID_KEY.to_owned(), Value::String(id));
        }
    }
    if *connection_type == "RDP" {
        let route = read_rd_gateway(&field);
        if !route.reproducible {
            return Err(Refusal::RdGatewayRoute(route.server));
        }
        if !route.server.is_empty() {
            if !is_valid_host(&route.server) {
                return Err(Refusal::RdGatewayHost(route.server));
            }
            // The gateway's own login is not carried over: the RDP client asks for it.
            set_text(&mut dto, "rdpGateway", &route.server);
        }
    }
    // A port that is not a positive number is the protocol's default, as the C#.
    let port = field(1).trim().parse::<i64>().ok().filter(|port| *port > 0);
    let user = replace_variables(field(2), resolve, unresolved);
    let user = user.as_str();
    match *connection_type {
        "SSH" | "SFTP" => {
            set_port(&mut dto, "sshPort", port);
            set_text(&mut dto, "sshUsername", user);
            if *connection_type == "SSH" {
                ssh_options(&mut dto, &fields);
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
    Ok(dto)
}

/// What only an SSH session of the file carries: the command run after login, the key, the
/// compression and the agent forwarding.
fn ssh_options(dto: &mut Map<String, Value>, fields: &[&str]) {
    let field = |index: usize| fields.get(index).copied().unwrap_or_default();
    // The command run after login is one post-connect step, as the C# `ApplyLoginCommand`.
    // Never approved by the file: whoever imports it, or opens it as a shared session, is
    // asked before it is typed.
    let command = field(SSH_LOGIN_COMMAND_FIELD).trim();
    if !command.is_empty() {
        dto.insert(
            "postConnectSteps".to_owned(),
            serde_json::json!([{ "input": command }]),
        );
    }
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

fn set_port(dto: &mut Map<String, Value>, key: &str, port: Option<i64>) {
    if let Some(port) = port {
        dto.insert(key.to_owned(), Value::from(port));
    }
}

/// `value` with each `[NAME]` replaced through `resolve`, as the C# `ResolveVariables`: a name
/// it gives nothing for (or an empty value) stays as written and is added to `unresolved`.
/// The result is not scanned again, so a value naming another variable cannot loop.
fn replace_variables(
    value: &str,
    resolve: VariableResolver<'_>,
    unresolved: &mut Vec<String>,
) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(open) = rest.find(TOKEN_OPEN) {
        out.push_str(&rest[..open]);
        let after = &rest[open + TOKEN_OPEN.len_utf8()..];
        let name = after
            .find(TOKEN_CLOSE)
            .map(|close| &after[..close])
            .filter(|name| is_valid_variable_name(name));
        let Some(name) = name else {
            // Not a token: the bracket is text, the scan goes on after it.
            out.push(TOKEN_OPEN);
            rest = after;
            continue;
        };
        if let Some(resolved) = resolve(name).filter(|resolved| !resolved.is_empty()) {
            out.push_str(&resolved);
        } else {
            unresolved.push(name.to_owned());
            out.push(TOKEN_OPEN);
            out.push_str(name);
            out.push(TOKEN_CLOSE);
        }
        rest = &after[name.len() + TOKEN_CLOSE.len_utf8()..];
    }
    out.push_str(rest);
    out
}

/// Whether `name` can name a variable: letters, digits and underscores, not starting with a
/// digit, as the C# `MobaXtermVariableResolver.IsValidName`. A name outside it could be saved
/// but would never be replaced.
#[must_use]
pub fn is_valid_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The resolver for the user's variables, as the C# `MobaXtermVariableResolver.FromVariables`:
/// a name is looked up without case and its value has the environment variables expanded. A
/// value that expands to nothing, or still names an environment variable the system does not
/// define, gives nothing, so the token is kept and reported rather than imported as a login
/// such as `%USERNAME%` spelled out.
pub fn variable_resolver(
    variables: &BTreeMap<String, String>,
) -> impl Fn(&str) -> Option<String> + use<> {
    // Keyed in lower case; the later of two names that differ only by case wins, as the C#
    // dictionary assignment.
    let lookup: BTreeMap<String, String> = variables
        .iter()
        .filter(|(name, _)| !name.trim().is_empty())
        .map(|(name, value)| (name.trim().to_lowercase(), value.clone()))
        .collect();
    move |name| {
        lookup
            .get(&name.to_lowercase())
            .and_then(|raw| resolve_value(raw))
    }
}

/// The value a variable resolves to: `raw` with environment variables expanded, or `None`
/// when that leaves nothing or an undefined `%NAME%` (the C# `ResolveValue`). The one place a
/// value is resolved, for the import and for the settings preview alike.
#[must_use]
pub fn resolve_value(raw: &str) -> Option<String> {
    resolve_value_with(raw, &environment_variable)
}

/// [`resolve_value`] over the environment `lookup` gives, so that a test needs no real one.
fn resolve_value_with(raw: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    if raw.trim().is_empty() {
        return None;
    }
    let expanded = expand_environment(raw, lookup);
    let expanded = expanded.trim();
    (!expanded.is_empty() && !has_unexpanded_reference(expanded)).then(|| expanded.to_owned())
}

/// `text` with each `%NAME%` replaced through `lookup`, as .NET `ExpandEnvironmentVariables`:
/// a name `lookup` does not know is left as written, and its closing `%` may open the next
/// reference.
fn expand_environment(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find(ENV_DELIMITER) {
        out.push_str(&rest[..open]);
        let after = &rest[open + ENV_DELIMITER.len_utf8()..];
        let Some(close) = after.find(ENV_DELIMITER) else {
            out.push(ENV_DELIMITER);
            rest = after;
            break;
        };
        let name = &after[..close];
        match lookup(name) {
            Some(value) if !name.is_empty() => {
                out.push_str(&value);
                rest = &after[close + ENV_DELIMITER.len_utf8()..];
            }
            _ => {
                out.push(ENV_DELIMITER);
                out.push_str(name);
                rest = &after[close..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Whether `text` still holds a `%NAME%`, the C# `%[^%\s]+%`: a name of one or more
/// characters that are neither `%` nor white space, between two `%`.
fn has_unexpanded_reference(text: &str) -> bool {
    let mut parts = text.split(ENV_DELIMITER).skip(1).peekable();
    // Each part but the last lies between two `%`.
    while let Some(part) = parts.next() {
        if parts.peek().is_some() && !part.is_empty() && !part.contains(char::is_whitespace) {
            return true;
        }
    }
    false
}

/// An environment variable of this process, named without case as Windows names them.
fn environment_variable(name: &str) -> Option<String> {
    std::env::var(name).ok().or_else(|| {
        std::env::vars_os()
            .find(|(known, _)| known.to_string_lossy().eq_ignore_ascii_case(name))
            .and_then(|(_, value)| value.into_string().ok())
    })
}

/// The type and the `%` fields of a session value `#icon#type%host%port%...#font#...`, as the C#
/// `TryReadSessionLine` (`MobaXtermImporter.cs:515-542`): the third `#` section, cut at its first
/// `%`. The icon, the first number, is the user's to change and says nothing of the protocol.
/// `None` for a value of another shape, or with no type.
fn split_session(value: &str) -> Option<(&str, &str)> {
    if !value.starts_with('#') {
        return None;
    }
    let main = value.split('#').nth(MAIN_SECTION)?;
    let (type_text, fields) = main.split_once('%').unwrap_or((main, ""));
    let type_text = type_text.trim();
    (!type_text.is_empty()).then_some((type_text, fields))
}

/// Which `#` section of a session value holds the type and the fields: the value starts with
/// `#`, so the first section is empty and the second is the icon.
const MAIN_SECTION: usize = 2;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::default_mobaxterm_variables;

    /// A file with one SSH session whose login is `login`.
    fn file(login: &str) -> String {
        format!("[Bookmarks]\nweb= #109#0%web.lab%22%{login}\n")
    }

    fn user(parsed: &Parsed) -> Option<&str> {
        parsed.servers[0]["sshUsername"].as_str()
    }

    fn variables(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(name))
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn the_default_variable_is_das_and_it_names_the_user_through_the_environment() {
        assert_eq!(
            default_mobaxterm_variables(),
            variables(&[("DAS", "%USERNAME%")])
        );
        // The C# default value, expanded: `%USERNAME%` is the Windows account.
        assert_eq!(
            resolve_value_with("%USERNAME%", &env(&[("USERNAME", "jbombled")])),
            Some("jbombled".to_owned())
        );
        let resolve = |name: &str| (name == "DAS").then(|| "jbombled".to_owned());
        let parsed = parse_with(&file("[DAS]"), &resolve);
        assert_eq!(user(&parsed), Some("jbombled"));
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn a_custom_variable_is_found_without_case_and_replaces_every_token_of_the_login() {
        let resolve = variable_resolver(&variables(&[("Team", "ops"), ("DAS", "jb")]));
        let parsed = parse_with(&file("[TEAM]-[das]-x"), &resolve);
        assert_eq!(user(&parsed), Some("ops-jb-x"));
        assert!(parsed.warnings.is_empty());
        // Text that is not a token is left as it is.
        let parsed = parse_with(&file("a[1x]b[ c ]d[DAS"), &resolve);
        assert_eq!(user(&parsed), Some("a[1x]b[ c ]d[DAS"));
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn a_value_has_its_environment_variables_expanded_as_net_does() {
        let lookup = env(&[("USERNAME", "jb"), ("DOMAIN", "CORP")]);
        let resolve = |raw: &str| resolve_value_with(raw, &lookup);
        assert_eq!(resolve("%DOMAIN%\\%USERNAME%"), Some("CORP\\jb".to_owned()));
        assert_eq!(resolve("  %username%  "), Some("jb".to_owned()), "any case");
        assert_eq!(resolve("100%"), Some("100%".to_owned()), "a lone percent");
        // Nothing, or a reference the system does not define, is no value.
        assert_eq!(resolve(""), None);
        assert_eq!(resolve("   "), None);
        assert_eq!(resolve("%NOPE%"), None);
        assert_eq!(resolve("%USERNAME%\\%NOPE%"), None);
        assert_eq!(
            resolve("%%"),
            Some("%%".to_owned()),
            "an empty name is text"
        );
        assert_eq!(resolve("a % b % c"), Some("a % b % c".to_owned()));
    }

    #[test]
    fn a_name_without_a_value_keeps_its_token_and_is_reported_once_per_session() {
        let text = "[Bookmarks]\n\
            a= #109#0%a.lab%22%[NOPE]\n\
            b= #109#0%b.lab%22%[nope]-[Nope]\n\
            c= #109#0%c.lab%22%plain\n";
        let resolve = variable_resolver(&variables(&[("EMPTY", "")]));
        let parsed = parse_with(text, &resolve);
        assert_eq!(parsed.servers[0]["sshUsername"], "[NOPE]");
        assert_eq!(parsed.servers[1]["sshUsername"], "[nope]-[Nope]");
        assert_eq!(
            parsed.warnings,
            [
                FileWarning::UnresolvedVariable {
                    session: "a".to_owned(),
                    name: "NOPE".to_owned()
                },
                FileWarning::UnresolvedVariable {
                    session: "b".to_owned(),
                    name: "nope".to_owned()
                },
            ],
            "once for a session, the case of its first token"
        );
        // A variable whose value resolves to nothing is unresolved too.
        let parsed = parse_with(&file("[EMPTY]"), &resolve);
        assert_eq!(user(&parsed), Some("[EMPTY]"));
        assert_eq!(parsed.warnings.len(), 1);
        // With no resolver at all, as the C# default options.
        assert_eq!(parse(&file("[DAS]")).warnings.len(), 1);
    }

    #[test]
    fn a_value_naming_another_variable_is_not_resolved_again() {
        let resolve = variable_resolver(&variables(&[
            ("A", "[B]"),
            ("B", "[A]"),
            ("SELF", "[SELF]"),
        ]));
        let parsed = parse_with(&file("[A]"), &resolve);
        assert_eq!(
            user(&parsed),
            Some("[B]"),
            "the result is not scanned again"
        );
        let parsed = parse_with(&file("[SELF]"), &resolve);
        assert_eq!(user(&parsed), Some("[SELF]"));
        assert!(parsed.warnings.is_empty(), "it resolved, to that text");
    }

    #[test]
    fn a_variable_name_is_letters_digits_and_underscores_not_starting_with_a_digit() {
        for valid in ["DAS", "_x", "a1", "A_b_2"] {
            assert!(is_valid_variable_name(valid), "{valid}");
        }
        for invalid in ["", "1a", "a-b", "a b", "é", "a]"] {
            assert!(!is_valid_variable_name(invalid), "{invalid}");
        }
    }

    /// A session line: the session `kind` (the type of the file) and each `(index, value)` at its
    /// place in the `%` list.
    fn line(name: &str, kind: u32, fields: &[(usize, &str)]) -> String {
        let mut values = vec![""; 32];
        for (at, value) in fields {
            values[*at] = value;
        }
        format!(
            "{name}= #109#{kind}%{}
",
            values.join("%")
        )
    }

    const TELNET: u32 = 1;
    const RDP: u32 = 4;
    const VNC: u32 = 5;
    const SFTP: u32 = 7;
    const SSH: u32 = 0;

    /// The warnings of `text`, as `(session, what)` pairs for the ones that refuse a route.
    fn refused(parsed: &Parsed) -> Vec<(&str, String)> {
        parsed
            .warnings
            .iter()
            .filter_map(|warning| match warning {
                FileWarning::JumpHostNotSupported { session, host, .. } => {
                    Some((session.as_str(), format!("jump:{host}")))
                }
                FileWarning::ProxyNotSupported { session, host, .. } => {
                    Some((session.as_str(), format!("proxy:{host}")))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_route_the_protocol_cannot_take_refuses_the_session_with_its_host() {
        // Telnet: jump host at 5, proxy at 10-11. VNC: jump host at 4, proxy at 12-13.
        // SFTP: no jump host slot, proxy at 9-10. SSH: proxy at 18-19.
        let text = format!(
            "[Bookmarks]
{}{}{}{}{}{}{}",
            line("tj", TELNET, &[(0, "t.lab"), (5, "jump.lab"), (6, "22")]),
            line("tp", TELNET, &[(0, "t.lab"), (10, "1"), (11, "proxy.lab")]),
            line("vj", VNC, &[(0, "v.lab"), (4, "jump.lab"), (5, "22")]),
            line("vp", VNC, &[(0, "v.lab"), (12, "2"), (13, "proxy.lab")]),
            line("sp", SFTP, &[(0, "f.lab"), (9, "1"), (10, "proxy.lab")]),
            line("xp", SSH, &[(0, "s.lab"), (18, "3")]),
            line("ok", TELNET, &[(0, "t.lab"), (10, "0")]),
        );
        let parsed = parse(&text);
        let names: Vec<_> = parsed
            .servers
            .iter()
            .map(|server| server["displayName"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(names, ["ok"], "the others would go around their route");
        assert_eq!(
            refused(&parsed),
            [
                ("tj", "jump:jump.lab".to_owned()),
                ("tp", "proxy:proxy.lab".to_owned()),
                ("vj", "jump:jump.lab".to_owned()),
                ("vp", "proxy:proxy.lab".to_owned()),
                ("sp", "proxy:proxy.lab".to_owned()),
                // A proxy type without a host is still a proxy, with an empty host.
                ("xp", "proxy:".to_owned()),
            ]
        );
    }

    #[test]
    fn an_ssh_or_rdp_jump_host_is_a_gateway_not_an_unsupported_route() {
        let parsed = parse(&format!(
            "[Bookmarks]
{}",
            line(
                "s",
                SSH,
                &[(0, "s.lab"), (7, "jump.lab"), (8, "22"), (9, "ops")]
            )
        ));
        assert!(parsed.warnings.is_empty());
        assert!(parsed.servers[0].get(GATEWAY_ID_KEY).is_some());
    }

    #[test]
    fn the_type_decides_the_protocol_not_the_icon() {
        // Every icon, one type: a custom icon does not make a Telnet session an SSH one.
        let text = "[Bookmarks]
            ssh= #42#0%a.lab%22%root
            telnet= #109#1%b.lab%23
            rdp= #7#4%c.lab%3389%admin
";
        let parsed = parse(text);
        let types: Vec<_> = parsed
            .servers
            .iter()
            .map(|server| server["connectionType"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(types, ["SSH", "Telnet", "RDP"]);
    }

    #[test]
    fn a_wsl_session_and_any_other_type_are_refused_with_their_name() {
        let text = "[Bookmarks]
            wsl= #151#14%AlmaLinux-9%
            odd= #12#99%x.lab%22
            word= #12#abc%x.lab%22
            ok= #109#0%ok.lab%22%root
";
        let parsed = parse(text);
        assert_eq!(parsed.servers.len(), 1);
        assert_eq!(
            parsed.warnings,
            [
                FileWarning::WslNotSupported {
                    session: "wsl".to_owned(),
                    folder: String::new(),
                },
                FileWarning::UnsupportedSessionType {
                    session: "odd".to_owned(),
                    folder: String::new(),
                    type_code: "99".to_owned(),
                },
                FileWarning::UnsupportedSessionType {
                    session: "word".to_owned(),
                    folder: String::new(),
                    type_code: "abc".to_owned(),
                },
            ]
        );
    }

    /// An RDP session with RD Gateway server `at19` and packed settings `at30`.
    fn rdp_gateway(at19: &str, at30: &str) -> Parsed {
        parse(&format!(
            "[Bookmarks]\n{}",
            line("r", RDP, &[(0, "pc.lab"), (19, at19), (30, at30)])
        ))
    }

    #[test]
    fn an_rd_gateway_set_to_never_or_detect_leaves_the_session_direct() {
        for settings in ["0,0,,,,0,", "0,1,,,,0,", ""] {
            let parsed = rdp_gateway("", settings);
            assert_eq!(parsed.servers.len(), 1, "{settings}");
            assert!(parsed.servers[0].get("rdpGateway").is_none(), "{settings}");
            assert!(parsed.warnings.is_empty(), "{settings}");
        }
    }

    #[test]
    fn an_rd_gateway_set_to_always_with_a_password_sign_in_is_kept_without_its_login() {
        for auth in ["0", "1", "2"] {
            let parsed = rdp_gateway("rdg.lab", &format!("0,2,rdg.lab,svc,,{auth},"));
            assert_eq!(parsed.servers.len(), 1, "auth {auth}");
            assert_eq!(parsed.servers[0]["rdpGateway"], "rdg.lab");
            assert!(parsed.warnings.is_empty(), "auth {auth}");
            assert!(!parsed.servers[0].to_string().contains("svc"), "login kept");
        }
    }

    #[test]
    fn an_rd_gateway_with_a_smart_card_or_an_unrecognised_setting_is_refused_not_made_direct() {
        for (at19, at30) in [
            ("rdg.lab", "0,2,rdg.lab,,,3,"),
            ("rdg.lab", "0,2,rdg.lab,,,9,"),
            ("rdg.lab", "0,2,rdg.lab,,,,"),
            ("rdg.lab", "0,7,rdg.lab,,,0,"),
            ("rdg.lab", "0,1,rdg.lab,,,0,"),
            ("rdg.lab", ""),
            ("rdg.lab", "0,2,other.lab,,,0,"),
            ("", "0,2,rdg.lab,,,0,"),
            ("", "0,2,,,,3,"),
            ("", "0,7,,,,0,"),
            ("", "garbage"),
        ] {
            let parsed = rdp_gateway(at19, at30);
            assert!(parsed.servers.is_empty(), "{at19} {at30}");
            assert!(
                matches!(
                    parsed.warnings[..],
                    [FileWarning::RdGatewayNotSupported { .. }]
                ),
                "{at19} {at30}: {:?}",
                parsed.warnings
            );
        }
    }

    #[test]
    fn an_rd_gateway_that_is_not_a_host_is_refused() {
        let parsed = rdp_gateway("-oProxy x", "0,2,-oProxy x,,,0,");
        assert!(parsed.servers.is_empty());
        assert!(matches!(
            parsed.warnings[..],
            [FileWarning::InvalidRdGatewayHost { .. }]
        ));
    }
}
