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

//! Shared sessions, as the C# `SharedSessionProvider` (`Services/SharedSessions`): the
//! `MobaXterm` files of the settings' shared sources, read for their sessions and kept in
//! memory, read-only.
//!
//! A shared file is written by other people, so what comes out of it is what the C#
//! `SharedProfileSanitizer` lets through and no more: SSH and RDP sessions with a valid host,
//! a name, a folder, a port and an account. A key path, a saved password, a command to run
//! after connecting, an agent or a gateway of the file never become a session's, and the
//! file's folder never gives a session the defaults of a local folder. Nothing here opens a
//! file for writing, and a session handed out is a copy.
//!
//! A source that cannot be read, or that no longer parses, loads nothing the first time and
//! keeps the previous sessions after that, with the reason, as the C# does. Changes are found
//! by looking at the file's size and modification time on a timer; a file modified a moment
//! ago is left to settle first, as the C# reload debounce, so half a file written by a sync
//! client is not read.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::time::{Duration, SystemTime};

use heimdall_core::import::foreign::{FileWarning, Parsed};
use heimdall_core::import::mobaxterm;
use heimdall_core::import::mobaxterm::Limits;
use heimdall_core::profile::{
    DEFAULT_RDP_PORT, DEFAULT_SSH_PORT, Forwards, ProfileId, RdpExtras, RdpOptions, RdpProfile,
    SshMode, SshProfile,
};
use heimdall_core::shared_sessions::{
    MAX_SOURCE_FILE_BYTES, SharedSource, is_valid_host, is_valid_source_id, session_id,
};
use serde_json::Value;

use crate::text_codec::decode_windows_1252_fallback;

/// How often the files are looked at for a change.
pub const POLL_INTERVAL: Duration = Duration::from_secs(3);

/// How long a file must have stayed as it is to be read after a change, as the C#
/// `SharedSourceReloadDebounceMs` default.
const SETTLE: Duration = Duration::from_secs(1);

/// The most sources read; the settings file is not trusted to be short.
const MAX_SOURCES: usize = 64;

/// The most sessions kept from one source.
const MAX_SESSIONS_PER_SOURCE: usize = 5000;

/// The longest name, folder or session line kept, in bytes. A folder is copied into each of
/// its sessions, so it bounds what a crafted file can make the reader allocate.
const MAX_TEXT_LEN: usize = 2048;

/// The most sections and entries kept from one file.
const MAX_ENTRIES: usize = 10_000;

/// What a shared file may hold.
const LIMITS: Limits = Limits {
    max_text_len: MAX_TEXT_LEN,
    max_entries: MAX_ENTRIES,
};

/// Why a source holds nothing, or holds what it held before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceProblem {
    /// Its identifier is not one a source can have.
    InvalidId,
    /// Another source has the same identifier.
    DuplicateId,
    /// It has no path.
    NoPath,
    /// The file is not there.
    Missing,
    /// The file is larger than the limit.
    TooLarge,
    /// The file could not be read; carries the system's reason.
    Unreadable(String),
    /// The file is not a `MobaXterm` file; carries the reader's reason.
    Unparsable(String),
    /// The file holds no session Heimdall can use.
    Empty,
}

/// What happened to a source on its last read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceStatus {
    /// It is turned off; nothing was read.
    Disabled,
    /// Its sessions are held.
    Loaded,
    /// Nothing is held for it.
    Failed(SourceProblem),
}

/// One shared source as of its last read, as the C# `SharedSessionSourceState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceState {
    /// Its identifier.
    pub id: String,
    /// The name the user gave it.
    pub name: String,
    /// The path read, variables expanded.
    pub path: String,
    /// What happened.
    pub status: SourceStatus,
    /// How many sessions it holds.
    pub sessions: usize,
    /// How many sessions of the file were refused: another protocol, a host that is not
    /// one, a repeated session.
    pub refused: usize,
    /// How many passwords the file stores, never read.
    pub ignored_passwords: usize,
    /// How many entries of the file were not taken: malformed, too long, or no session.
    pub skipped_entries: usize,
    /// The file held more than a source may, and the rest was not read.
    pub truncated: bool,
    /// Why what is held is not what the file holds now: the last change could not be taken
    /// and the previous sessions are kept.
    pub kept_previous: Option<SourceProblem>,
}

/// What a shared session connects with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedTarget {
    /// A shell.
    Ssh(SshProfile),
    /// A remote desktop.
    Rdp(RdpProfile),
}

/// One shared session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedSession {
    /// Its identifier, in the shared namespace.
    pub id: ProfileId,
    /// The source it comes from.
    pub source_id: String,
    /// The folder the file files it in, shown in the tree and nothing else.
    pub folder: Option<String>,
    /// What it connects with.
    pub target: SharedTarget,
}

impl SharedSession {
    /// Its name.
    #[must_use]
    pub fn name(&self) -> &str {
        match &self.target {
            SharedTarget::Ssh(profile) => &profile.name,
            SharedTarget::Rdp(profile) => &profile.name,
        }
    }

    /// Its host and port.
    #[must_use]
    pub fn endpoint(&self) -> (&str, u16) {
        match &self.target {
            SharedTarget::Ssh(profile) => (&profile.host, profile.port),
            SharedTarget::Rdp(profile) => (&profile.host, profile.port),
        }
    }

    /// Its protocol, as the tree names it.
    #[must_use]
    pub fn protocol(&self) -> &'static str {
        match self.target {
            SharedTarget::Ssh(_) => "SSH",
            SharedTarget::Rdp(_) => "RDP",
        }
    }
}

/// What a look at a file saw of it, to tell a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: Option<u64>,
}

/// A source and what it holds.
#[derive(Debug, Clone)]
struct Loaded {
    state: SourceState,
    sessions: Vec<SharedSession>,
    /// What the file looked like when it was last read, or failed to be.
    stamp: Stamp,
}

/// The shared sessions held.
#[derive(Debug, Clone, Default)]
pub struct SharedSessions {
    sources: Vec<Loaded>,
}

impl SharedSessions {
    /// The state of every source, in the settings' order.
    #[must_use]
    pub fn sources(&self) -> Vec<&SourceState> {
        self.sources.iter().map(|loaded| &loaded.state).collect()
    }

    /// The sessions of `source_id`, in the file's order.
    #[must_use]
    pub fn sessions_of(&self, source_id: &str) -> &[SharedSession] {
        self.sources
            .iter()
            .find(|loaded| loaded.state.id == source_id)
            .map_or(&[], |loaded| loaded.sessions.as_slice())
    }

    /// Every session of every source.
    pub fn sessions(&self) -> impl Iterator<Item = &SharedSession> {
        self.sources
            .iter()
            .flat_map(|loaded| loaded.sessions.iter())
    }

    /// A copy of the shared session `id`, or `None`: a caller that changes it changes
    /// nothing here.
    #[must_use]
    pub fn find(&self, id: &ProfileId) -> Option<SharedSession> {
        self.sessions().find(|session| session.id == *id).cloned()
    }

    /// Whether no source is configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// Reads every enabled source of `sources` and replaces what is held, as the C#
    /// `LoadAsync`. Never fails over a source: it is reported in its state.
    pub fn load(&mut self, sources: &[SharedSource]) {
        let previous: HashMap<String, Loaded> = std::mem::take(&mut self.sources)
            .into_iter()
            .map(|loaded| (loaded.state.id.clone(), loaded))
            .collect();
        let mut seen = HashSet::new();
        self.sources = sources
            .iter()
            .take(MAX_SOURCES)
            .map(|source| {
                let kept = previous.get(&source.id).filter(|loaded| {
                    loaded
                        .state
                        .path
                        .eq_ignore_ascii_case(&expand(&source.path))
                });
                read_source(source, &mut seen, kept)
            })
            .collect();
    }

    /// Reads again the sources whose file changed and settled since it was read, appeared
    /// or disappeared; true when something shown changed. Called on a timer.
    pub fn refresh(&mut self, sources: &[SharedSource], now: SystemTime) -> bool {
        let wanted: Vec<&SharedSource> = sources.iter().take(MAX_SOURCES).collect();
        let same_list = wanted.len() == self.sources.len()
            && wanted.iter().zip(&self.sources).all(|(source, loaded)| {
                source.id == loaded.state.id
                    && source.name == loaded.state.name
                    && source.enabled == (loaded.state.status != SourceStatus::Disabled)
            });
        if !same_list {
            self.load(sources);
            return true;
        }
        let mut changed = false;
        for (index, source) in wanted.into_iter().enumerate() {
            if !source.enabled {
                continue;
            }
            let path = expand(&source.path);
            let stamp = stamp_of(&path);
            let slot = &self.sources[index];
            if stamp == slot.stamp && path == slot.state.path {
                continue;
            }
            // Written a moment ago: wait for the next look, so a file being written is not
            // read half done.
            let recent = stamp
                .modified
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age < SETTLE);
            if recent {
                continue;
            }
            // Its identifier counts once, as its own: the others' are the ones seen.
            let mut seen: HashSet<String> = self
                .sources
                .iter()
                .enumerate()
                .filter(|(position, _)| *position != index)
                .map(|(_, loaded)| loaded.state.id.clone())
                .collect();
            let kept = (path == slot.state.path).then_some(slot);
            let reloaded = read_source(source, &mut seen, kept);
            if reloaded.state != slot.state || reloaded.sessions != slot.sessions {
                changed = true;
            }
            self.sources[index] = reloaded;
        }
        changed
    }
}

/// `path` with its `%VARIABLE%`s replaced by the environment's, as the C#
/// `Environment.ExpandEnvironmentVariables`; one that is not set stays as written.
#[must_use]
pub fn expand(path: &str) -> String {
    let mut out = String::new();
    let mut rest = path.trim();
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                if let Ok(value) = std::env::var(name) {
                    out.push_str(&value);
                } else {
                    out.push('%');
                    out.push_str(name);
                    out.push('%');
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn stamp_of(path: &str) -> Stamp {
    match std::fs::metadata(path) {
        Ok(meta) => Stamp {
            modified: meta.modified().ok(),
            len: Some(meta.len()),
        },
        Err(_) => Stamp {
            modified: None,
            len: None,
        },
    }
}

/// Reads one source. `previous` is what is held for this very file, kept when the new read
/// is worse.
fn read_source(
    source: &SharedSource,
    seen: &mut HashSet<String>,
    previous: Option<&Loaded>,
) -> Loaded {
    let path = expand(&source.path);
    let stamp = stamp_of(&path);
    let state = |status, sessions, refused, ignored_passwords| SourceState {
        skipped_entries: 0,
        truncated: false,
        id: source.id.clone(),
        name: source.name.clone(),
        path: path.clone(),
        status,
        sessions,
        refused,
        ignored_passwords,
        kept_previous: None,
    };
    let failed = |problem| Loaded {
        state: state(SourceStatus::Failed(problem), 0, 0, 0),
        sessions: Vec::new(),
        stamp,
    };
    if !is_valid_source_id(&source.id) {
        return failed(SourceProblem::InvalidId);
    }
    if !seen.insert(source.id.clone()) {
        return failed(SourceProblem::DuplicateId);
    }
    if !source.enabled {
        return Loaded {
            state: state(SourceStatus::Disabled, 0, 0, 0),
            sessions: Vec::new(),
            stamp,
        };
    }
    if path.is_empty() {
        return failed(SourceProblem::NoPath);
    }
    let had_sessions = previous.is_some_and(|loaded| !loaded.sessions.is_empty());
    let problem = match read_bounded(&path).and_then(|bytes| parse(&source.id, &bytes)) {
        // A file with nothing usable in it is no source of sessions: it reports why, and
        // keeps the previous sessions when there are some.
        Ok(parsed) if !parsed.sessions.is_empty() => {
            return Loaded {
                state: SourceState {
                    skipped_entries: parsed.skipped_entries,
                    truncated: parsed.truncated,
                    ..state(
                        SourceStatus::Loaded,
                        parsed.sessions.len(),
                        parsed.refused,
                        parsed.ignored_passwords,
                    )
                },
                sessions: parsed.sessions,
                stamp,
            };
        }
        Ok(_) => SourceProblem::Empty,
        Err(problem) => problem,
    };
    match previous {
        // A change that cannot be taken keeps what was held, with the reason.
        // A file that is gone, as a source turned off, leaves nothing connectable.
        Some(kept) if had_sessions && problem != SourceProblem::Missing => {
            let mut held = kept.clone();
            held.state.name.clone_from(&source.name);
            held.state.kept_previous = Some(problem);
            held.stamp = stamp;
            held
        }
        _ => failed(problem),
    }
}

/// The bytes of the file at `path`, opened for reading only, at most
/// [`MAX_SOURCE_FILE_BYTES`] of them.
fn read_bounded(path: &str) -> Result<Vec<u8>, SourceProblem> {
    let unreadable = |error: std::io::Error| SourceProblem::Unreadable(error.to_string());
    let mut file = std::fs::File::open(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => SourceProblem::Missing,
        _ => unreadable(error),
    })?;
    let meta = file.metadata().map_err(unreadable)?;
    if !meta.is_file() {
        return Err(SourceProblem::Unreadable(String::new()));
    }
    if meta.len() > MAX_SOURCE_FILE_BYTES {
        return Err(SourceProblem::TooLarge);
    }
    let mut bytes = Vec::new();
    // One byte past the limit tells a file that grew since its size was read.
    (&mut file)
        .take(MAX_SOURCE_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    if bytes.len() as u64 > MAX_SOURCE_FILE_BYTES {
        return Err(SourceProblem::TooLarge);
    }
    Ok(bytes)
}

/// What a file gave.
struct ParsedSource {
    sessions: Vec<SharedSession>,
    refused: usize,
    ignored_passwords: usize,
    skipped_entries: usize,
    truncated: bool,
}

/// The sessions of a file's `bytes`, sanitized as the C# `SharedProfileSanitizer`.
fn parse(source_id: &str, bytes: &[u8]) -> Result<ParsedSource, SourceProblem> {
    let text = decode_windows_1252_fallback(bytes);
    let (parsed, left_out): (Parsed, _) = mobaxterm::parse_limited(&text, &LIMITS);
    if let Some(FileWarning::Unreadable(reason)) = parsed
        .warnings
        .iter()
        .find(|warning| matches!(warning, FileWarning::Unreadable(_)))
    {
        return Err(SourceProblem::Unparsable(reason.clone()));
    }
    let mut sessions = Vec::new();
    let mut refused = 0;
    let mut ids = HashSet::new();
    for server in &parsed.servers {
        let session = sanitize(source_id, server).filter(|session| {
            sessions.len() < MAX_SESSIONS_PER_SOURCE && ids.insert(session.id.clone())
        });
        match session {
            Some(session) => sessions.push(session),
            None => refused += 1,
        }
    }
    Ok(ParsedSource {
        sessions,
        refused,
        ignored_passwords: parsed.stored_credentials,
        skipped_entries: left_out.entries,
        truncated: left_out.truncated,
    })
}

/// A text of the file without a control character, or none.
fn text_of<'a>(server: &'a Value, key: &str) -> Option<&'a str> {
    server
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
}

/// A port of the file, or `default`.
fn port_of(server: &Value, key: &str, default: u16) -> u16 {
    server
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port > 0)
        .unwrap_or(default)
}

/// The session `server` of a file as the C# sanitizer keeps it: SSH or RDP, a valid host,
/// and the fields of its whitelist; `None` for any other.
fn sanitize(source_id: &str, server: &Value) -> Option<SharedSession> {
    let kind = text_of(server, "connectionType")?;
    let host = text_of(server, "remoteServer").filter(|host| is_valid_host(host))?;
    let folder = text_of(server, "group").map(str::to_owned);
    let name = text_of(server, "displayName").unwrap_or(host).to_owned();
    let id = ProfileId::new(session_id(
        source_id,
        folder.as_deref().unwrap_or_default(),
        &name,
    ));
    let target = if kind.eq_ignore_ascii_case("SSH") {
        SharedTarget::Ssh(ssh_profile(
            &id,
            &name,
            host,
            port_of(server, "sshPort", DEFAULT_SSH_PORT),
            text_of(server, "sshUsername"),
        ))
    } else if kind.eq_ignore_ascii_case("RDP") {
        SharedTarget::Rdp(rdp_profile(
            &id,
            &name,
            host,
            port_of(server, "remotePort", DEFAULT_RDP_PORT),
            text_of(server, "rdpUsername"),
        ))
    } else {
        return None;
    };
    Some(SharedSession {
        id,
        source_id: source_id.to_owned(),
        folder,
        target,
    })
}

/// An SSH session of a shared file: its own fields and nothing else. No key, no gateway, no
/// command after connecting, no agent, no folder: a local folder's defaults never reach it.
fn ssh_profile(
    id: &ProfileId,
    name: &str,
    host: &str,
    port: u16,
    username: Option<&str>,
) -> SshProfile {
    SshProfile {
        id: id.clone(),
        name: name.to_owned(),
        group: None,
        host: host.to_owned(),
        port,
        username: username.map(str::to_owned),
        key_path: None,
        gateway: None,
        local_tunnel_port: None,
        vault_entry: None,
        forwards: Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
        ssh_mode: SshMode::Embedded,
        x11_forwarding: false,
    }
}

/// An RDP session of a shared file, on the application's RDP options.
fn rdp_profile(
    id: &ProfileId,
    name: &str,
    host: &str,
    port: u16,
    username: Option<&str>,
) -> RdpProfile {
    RdpProfile {
        extras: RdpExtras::default(),
        id: id.clone(),
        name: name.to_owned(),
        group: None,
        host: host.to_owned(),
        port,
        username: username.map(str::to_owned),
        domain: None,
        allow_tls_only: false,
        gateway: None,
        local_tunnel_port: None,
        redirect_clipboard: true,
        redirect_drives: false,
        options: RdpOptions::default(),
        vault_entry: None,
        forwards: Forwards::default(),
        follow_defaults: true,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
    }
}
