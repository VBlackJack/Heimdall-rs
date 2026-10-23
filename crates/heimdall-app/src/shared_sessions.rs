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
//! `SharedProfileSanitizer` lets through and no more: SSH, SFTP, RDP, Telnet and VNC sessions
//! with a valid host, a name, a folder, a port, an account (SSH, SFTP and RDP) and the SSH
//! gateway the file names (SSH and RDP), as a host, a port and an account and nothing else.
//! A session that names a route Heimdall cannot take for its protocol (a jump host on Telnet or
//! VNC, a proxy on SSH, SFTP, Telnet or VNC) is refused, never taken as a direct connection. A key path, a saved password or an agent of the file never
//! become a session's, and the file's folder never gives a session the defaults of a local
//! folder. The command a file asks an SSH session to type after connecting is kept as a
//! step nobody approved: it is typed only once the user approved it, and asked again when the
//! file changes it (`app::shared`). A gateway of the file lives in the shared
//! namespace, read-only and in memory: it is never added to the user's saved gateways, and
//! its host key is asked about like any server's. A session whose gateway is not usable is
//! refused rather than taken as a direct connection. Nothing here opens a
//! file for writing, and a session handed out is a copy.
//!
//! A source that cannot be read, or that no longer parses, loads nothing the first time and
//! keeps the previous sessions after that, with the reason, as the C# does. Changes are found
//! by looking at the file's size and modification time on a timer; a file modified a moment
//! ago is left to settle first, as the C# reload debounce, so half a file written by a sync
//! client is not read.
//!
//! Each read that gives sessions leaves a private offline copy of them (`shared_cache`); a
//! source whose file cannot be reached, and that holds nothing in memory, falls back on that
//! copy and says so, as the C# `LoadOfflineCopyAsync`.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use heimdall_core::import::foreign::{FileWarning, GATEWAY_ID_KEY, Parsed};
use heimdall_core::import::mobaxterm;
use heimdall_core::import::mobaxterm::Limits;
use heimdall_core::post_connect::{PostConnect, PostConnectStep};
use heimdall_core::profile::{
    DEFAULT_RDP_PORT, DEFAULT_SSH_PORT, DEFAULT_TELNET_PORT, DEFAULT_VNC_PORT, Forwards, ProfileId,
    RdpExtras, RdpOptions, RdpProfile, SshGateway, SshMode, SshProfile, TelnetProfile, VncProfile,
};
use heimdall_core::shared_sessions::{
    MAX_SOURCE_FILE_BYTES, SharedSource, gateway_id, is_valid_host, is_valid_source_id, session_id,
};
use serde_json::Value;

use crate::shared_cache::{CachedGateway, CachedSession, CachedSource, SourceCache};
use crate::shared_report::{ImportReport, Refused, SkipReason, Taken};

/// The user's `MobaXterm` variables, the C# `MobaXtermUserVariables`: the shared provider fills
/// in the `[NAME]` of a login with them as the file import does (`SharedSessionProvider.cs`).
pub type Variables = std::collections::BTreeMap<String, String>;

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

/// The most post-connect steps a shared SSH session keeps. `MobaXterm` gives one command per
/// session; the bound is for a file that gives more.
const MAX_STEPS_PER_SESSION: usize = 16;

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
    /// Set when what is held is the offline copy: the file cannot be reached.
    pub offline: Option<Offline>,
}

/// The offline copy a source is held from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offline {
    /// When the copy was taken, that is when the file was last read.
    pub taken_at: SystemTime,
    /// Why the file is not used.
    pub reason: SourceProblem,
}

/// What a shared session connects with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedTarget {
    /// A shell.
    Ssh(SshProfile),
    /// The files of a server over SFTP: an SSH profile that opens its files.
    Sftp(SshProfile),
    /// A remote desktop.
    Rdp(RdpProfile),
    /// A Telnet terminal.
    Telnet(TelnetProfile),
    /// A VNC remote desktop.
    Vnc(VncProfile),
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
    /// The SSH gateway it goes through, in the shared namespace; none for a direct session.
    pub gateway: Option<SshGateway>,
    /// What it connects with.
    pub target: SharedTarget,
}

impl SharedSession {
    /// Its name.
    #[must_use]
    pub fn name(&self) -> &str {
        match &self.target {
            SharedTarget::Ssh(profile) | SharedTarget::Sftp(profile) => &profile.name,
            SharedTarget::Rdp(profile) => &profile.name,
            SharedTarget::Telnet(profile) => &profile.name,
            SharedTarget::Vnc(profile) => &profile.name,
        }
    }

    /// Its host and port.
    #[must_use]
    pub fn endpoint(&self) -> (&str, u16) {
        match &self.target {
            SharedTarget::Ssh(profile) | SharedTarget::Sftp(profile) => {
                (&profile.host, profile.port)
            }
            SharedTarget::Rdp(profile) => (&profile.host, profile.port),
            SharedTarget::Telnet(profile) => (&profile.host, profile.port),
            SharedTarget::Vnc(profile) => (&profile.host, profile.port),
        }
    }

    /// Its protocol, as the tree names it.
    #[must_use]
    pub fn protocol(&self) -> &'static str {
        match self.target {
            SharedTarget::Ssh(_) => PROTOCOL_SSH,
            SharedTarget::Sftp(_) => PROTOCOL_SFTP,
            SharedTarget::Rdp(_) => PROTOCOL_RDP,
            SharedTarget::Telnet(_) => PROTOCOL_TELNET,
            SharedTarget::Vnc(_) => PROTOCOL_VNC,
        }
    }
}

/// The protocols of a shared session, as the tree and the report name them.
const PROTOCOL_SSH: &str = "SSH";
const PROTOCOL_SFTP: &str = "SFTP";
const PROTOCOL_RDP: &str = "RDP";

/// Key of the RD Gateway of an RDP session in a session of the file (the profile JSON key).
const RD_GATEWAY_KEY: &str = "rdpGateway";
const PROTOCOL_TELNET: &str = "Telnet";
const PROTOCOL_VNC: &str = "VNC";

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
    /// What the read had to say of the file.
    report: ImportReport,
    /// What the file looked like when it was last read, or failed to be.
    stamp: Stamp,
}

/// The shared sessions held.
#[derive(Debug, Clone, Default)]
pub struct SharedSessions {
    sources: Vec<Loaded>,
    /// Where the offline copies are kept; none, and none is made or used.
    cache: Option<SourceCache>,
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

    /// What the last read of `source_id` had to say of its file, or `None` for a source not
    /// held.
    #[must_use]
    pub fn report_of(&self, source_id: &str) -> Option<&ImportReport> {
        self.sources
            .iter()
            .find(|loaded| loaded.state.id == source_id)
            .map(|loaded| &loaded.report)
    }

    /// Keeps the offline copies in `directory`, or nowhere: the copies are then neither made
    /// nor used.
    pub fn set_cache_directory(&mut self, directory: Option<PathBuf>) {
        self.cache = directory.map(SourceCache::new);
    }

    /// The SSH gateways the sessions go through, each once: held in memory beside the saved
    /// ones, never among them.
    #[must_use]
    pub fn gateways(&self) -> Vec<SshGateway> {
        let mut seen = HashSet::new();
        self.sessions()
            .filter_map(|session| session.gateway.clone())
            .filter(|gateway| seen.insert(gateway.id.clone()))
            .collect()
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
    pub fn load(&mut self, sources: &[SharedSource], variables: &Variables) {
        let previous: HashMap<String, Loaded> = std::mem::take(&mut self.sources)
            .into_iter()
            .map(|loaded| (loaded.state.id.clone(), loaded))
            .collect();
        let mut seen = HashSet::new();
        let cache = self.cache.clone();
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
                read_source(source, &mut seen, kept, variables, cache.as_ref())
            })
            .collect();
        // A source taken out of the settings leaves no copy behind, as the C#.
        if let Some(cache) = &cache {
            let kept: Vec<&str> = sources.iter().map(|source| source.id.as_str()).collect();
            cache.delete_all_except(&kept);
        }
    }

    /// Reads again the sources whose file changed and settled since it was read, appeared
    /// or disappeared; true when something shown changed. Called on a timer.
    pub fn refresh(
        &mut self,
        sources: &[SharedSource],
        variables: &Variables,
        now: SystemTime,
    ) -> bool {
        let wanted: Vec<&SharedSource> = sources.iter().take(MAX_SOURCES).collect();
        let same_list = wanted.len() == self.sources.len()
            && wanted.iter().zip(&self.sources).all(|(source, loaded)| {
                source.id == loaded.state.id
                    && source.name == loaded.state.name
                    && source.enabled == (loaded.state.status != SourceStatus::Disabled)
            });
        if !same_list {
            self.load(sources, variables);
            return true;
        }
        let mut changed = false;
        let cache = self.cache.clone();
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
            let reloaded = read_source(source, &mut seen, kept, variables, cache.as_ref());
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
    variables: &Variables,
    cache: Option<&SourceCache>,
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
        offline: None,
    };
    let failed = |problem| Loaded {
        state: state(SourceStatus::Failed(problem), 0, 0, 0),
        sessions: Vec::new(),
        report: ImportReport::default(),
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
            report: ImportReport::default(),
            stamp,
        };
    }
    if path.is_empty() {
        return failed(SourceProblem::NoPath);
    }
    let had_sessions = previous.is_some_and(|loaded| !loaded.sessions.is_empty());
    let problem = match read_bounded(&path).and_then(|bytes| parse(&source.id, &bytes, variables)) {
        // A file with nothing usable in it is no source of sessions: it reports why, and
        // keeps the previous sessions when there are some.
        // A file that read but whose sessions were all refused (a route Heimdall cannot follow
        // was added to each) loads none: falling back to the previous read or the offline copy
        // would reopen those sessions as direct connections around the route the file asks for.
        Ok(parsed) if !parsed.sessions.is_empty() || parsed.refused > 0 => {
            if let Some(cache) = cache {
                keep_copy(cache, source, &path, &parsed);
            }
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
                report: parsed.report,
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
            // What is held is the offline copy already: it is the reason that changes.
            if let Some(offline) = &mut held.state.offline {
                offline.reason = problem;
            } else {
                held.state.kept_previous = Some(problem);
            }
            held.stamp = stamp;
            held
        }
        // Nothing held, or the file is gone: the offline copy of the last good read.
        _ => cache
            .and_then(|cache| offline_copy(cache, source, &path, &problem, stamp, &state))
            .unwrap_or_else(|| failed(problem)),
    }
}

/// Keeps what `parsed` gave as the offline copy of `source`. A copy that cannot be written is
/// logged and the source loads all the same.
fn keep_copy(cache: &SourceCache, source: &SharedSource, path: &str, parsed: &ParsedSource) {
    let copy = CachedSource {
        original_path: path.to_owned(),
        taken_at: SystemTime::now(),
        sessions: parsed.sessions.iter().map(cached_session).collect(),
        refused: parsed.refused,
        ignored_passwords: parsed.ignored_passwords,
        skipped_entries: parsed.skipped_entries,
        truncated: parsed.truncated,
        report: parsed.report.clone(),
    };
    if let Err(error) = cache.store(&source.id, &copy) {
        log::warn!(
            "shared sessions: the offline copy of source {} could not be written: {error}",
            source.id
        );
    }
}

/// The session as the offline copy keeps it.
fn cached_session(session: &SharedSession) -> CachedSession {
    let (host, port) = session.endpoint();
    let username = match &session.target {
        SharedTarget::Ssh(profile) | SharedTarget::Sftp(profile) => profile.username.clone(),
        SharedTarget::Rdp(profile) => profile.username.clone(),
        SharedTarget::Telnet(_) | SharedTarget::Vnc(_) => None,
    };
    let rd_gateway = match &session.target {
        SharedTarget::Rdp(profile) => profile.extras.rd_gateway().map(str::to_owned),
        _ => None,
    };
    CachedSession {
        rd_gateway,
        folder: session.folder.clone(),
        name: session.name().to_owned(),
        protocol: session.protocol().to_ascii_lowercase(),
        host: host.to_owned(),
        port,
        username,
        gateway: session.gateway.as_ref().map(|gateway| CachedGateway {
            name: gateway.name.clone(),
            host: gateway.host.clone(),
            port: gateway.port,
            username: gateway.username.clone(),
        }),
    }
}

/// The offline copy of `source` as a source that cannot be reached for `problem`, or `None`
/// when there is no copy of that very file or it holds no session that passes the checks of
/// a file from the share.
fn offline_copy(
    cache: &SourceCache,
    source: &SharedSource,
    path: &str,
    problem: &SourceProblem,
    stamp: Stamp,
    state: &dyn Fn(SourceStatus, usize, usize, usize) -> SourceState,
) -> Option<Loaded> {
    let copy = cache.read(&source.id)?;
    // A copy taken from another file is not this source's.
    if !copy.original_path.eq_ignore_ascii_case(path) {
        log::warn!(
            "shared sessions: the offline copy of source {} was taken from another file; ignored",
            source.id
        );
        return None;
    }
    let mut ids = HashSet::new();
    let sessions: Vec<SharedSession> = copy
        .sessions
        .iter()
        .filter_map(|cached| session_of_copy(&source.id, cached))
        .filter(|session| ids.insert(session.id.clone()))
        .take(MAX_SESSIONS_PER_SOURCE)
        .collect();
    if sessions.is_empty() {
        return None;
    }
    let mut held = state(
        SourceStatus::Loaded,
        sessions.len(),
        copy.refused,
        copy.ignored_passwords,
    );
    held.skipped_entries = copy.skipped_entries;
    held.truncated = copy.truncated;
    held.offline = Some(Offline {
        taken_at: copy.taken_at,
        reason: problem.clone(),
    });
    Some(Loaded {
        state: held,
        sessions,
        report: copy.report,
        stamp,
    })
}

/// A session of an offline copy, through the checks a session of a file goes through.
fn session_of_copy(source_id: &str, cached: &CachedSession) -> Option<SharedSession> {
    let kind = Protocol::of_name(&cached.protocol)?;
    let host = clean(&cached.host).filter(|host| is_valid_host(host))?;
    let name = clean(&cached.name)?;
    let folder = cached.folder.as_deref().and_then(clean);
    // Telnet and VNC have no account: a copy that gives one is not one the reader wrote.
    let username = cached
        .username
        .as_deref()
        .and_then(clean)
        .filter(|_| kind.has_account());
    if cached.port == 0 {
        return None;
    }
    // A gateway the copy cannot vouch for drops the session, never makes it a direct one; a
    // protocol that cannot carry one never gets one.
    if cached.gateway.is_some() && !kind.carries_gateway() {
        return None;
    }
    // Same for an RD Gateway: one the copy holds that cannot be kept drops the session, and a
    // protocol without one never gets one.
    let rd_gateway = match cached.rd_gateway.as_deref().map(str::trim) {
        None => None,
        Some(host) if kind == Protocol::Rdp && is_valid_host(host) => Some(host.to_owned()),
        Some(_) => return None,
    };
    let gateway = match &cached.gateway {
        None => None,
        Some(gateway) => Some(shared_gateway(
            source_id,
            Some(&gateway.name),
            &gateway.host,
            gateway.port,
            gateway.username.as_deref(),
        )?),
    };
    Some(build_session(
        source_id,
        kind,
        folder,
        name,
        host,
        cached.port,
        // The offline copy keeps no command: a copy of a file is not a reason to type one.
        Login {
            username,
            gateway,
            rd_gateway: rd_gateway.as_deref(),
            steps: Vec::new(),
        },
    ))
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
    report: ImportReport,
}

/// The sessions of a file's `bytes`, sanitized as the C# `SharedProfileSanitizer`.
fn parse(
    source_id: &str,
    bytes: &[u8],
    variables: &Variables,
) -> Result<ParsedSource, SourceProblem> {
    let text = decode_windows_1252_fallback(bytes);
    let resolve = mobaxterm::variable_resolver(variables);
    let (parsed, left_out): (Parsed, _) = mobaxterm::parse_limited(&text, &resolve, &LIMITS);
    if let Some(FileWarning::Unreadable(reason)) = parsed
        .warnings
        .iter()
        .find(|warning| matches!(warning, FileWarning::Unreadable(_)))
    {
        return Err(SourceProblem::Unparsable(reason.clone()));
    }
    let routes = gateways_of(source_id, &parsed.gateways);
    let mut sessions = Vec::new();
    let mut refused = Vec::new();
    let mut ids = HashSet::new();
    for server in &parsed.servers {
        let outcome = sanitize(source_id, server, &routes).and_then(|session| {
            if sessions.len() >= MAX_SESSIONS_PER_SOURCE {
                Err((SkipReason::OverLimit, String::new()))
            } else if !ids.insert(session.id.clone()) {
                Err((SkipReason::DuplicateSession, String::new()))
            } else {
                Ok(session)
            }
        });
        match outcome {
            Ok(session) => sessions.push(session),
            Err((reason, detail)) => {
                let (folder, name) = listed_as(server);
                refused.push(Refused {
                    folder,
                    name,
                    reason,
                    detail,
                });
            }
        }
    }
    // A session the reader left out for its route is said as refused.
    refused.extend(parsed.warnings.iter().filter_map(refused_by_reader));
    // The warning names the session as the file does; its folder is the one of the entry of
    // that name, as the C# looks the folder of a warning's section up.
    let unresolved = parsed
        .warnings
        .iter()
        .filter_map(|warning| match warning {
            FileWarning::UnresolvedVariable { session, name } => {
                let folder = parsed
                    .servers
                    .iter()
                    .find(|server| text_of(server, "displayName") == Some(session.as_str()))
                    .and_then(|server| text_of(server, "group"))
                    .unwrap_or_default();
                // The name is the file's own text: cleaned like every other the report shows.
                Some((folder.to_owned(), display_text(session), name.clone()))
            }
            _ => None,
        })
        .collect();
    let taken: Vec<Taken<'_>> = sessions
        .iter()
        .map(|session| Taken {
            folder: session.folder.as_deref().unwrap_or_default(),
            name: session.name(),
            host: session.endpoint().0,
            protocol: session.protocol(),
            route: session
                .gateway
                .as_ref()
                .map_or("", |gateway| gateway.id.as_str()),
            route_name: session
                .gateway
                .as_ref()
                .map_or("", |gateway| gateway.name.as_str()),
        })
        .collect();
    let refused_count = refused.len();
    let report = ImportReport::build(refused, unresolved, &taken);
    Ok(ParsedSource {
        sessions,
        refused: refused_count,
        ignored_passwords: parsed.stored_credentials,
        skipped_entries: left_out.entries,
        truncated: left_out.truncated,
        report,
    })
}

/// The folder and name the file lists `server` under, for the report.
fn listed_as(server: &Value) -> (String, String) {
    let folder = text_of(server, "group").unwrap_or_default();
    let name = text_of(server, "displayName")
        .or_else(|| text_of(server, "remoteServer"))
        .unwrap_or_default();
    (folder.to_owned(), name.to_owned())
}

/// The post-connect steps a file asks for, as the C# `SharedProfileSanitizer.SanitizeSteps`
/// keeps them: the text typed, on, with the default delay. Stricter than the C#: a command
/// with a control character (an escape sequence, a second line) or longer than
/// [`MAX_TEXT_LEN`] is dropped rather than shown to be approved, since what is displayed is
/// not always what a terminal would act on.
fn steps_of(server: &Value) -> Vec<PostConnectStep> {
    server
        .get("postConnectSteps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|step| step.get("input").and_then(Value::as_str))
        .filter_map(clean)
        .filter(|input| input.len() <= MAX_TEXT_LEN)
        .take(MAX_STEPS_PER_SESSION)
        .map(PostConnectStep::new)
        .collect()
}

/// `text` trimmed, or none when it is empty or holds a control character.
fn clean(text: &str) -> Option<&str> {
    let text = text.trim();
    (!text.is_empty() && !text.chars().any(char::is_control)).then_some(text)
}

/// A text of the file without a control character, or none.
fn text_of<'a>(server: &'a Value, key: &str) -> Option<&'a str> {
    server.get(key).and_then(Value::as_str).and_then(clean)
}

/// What a shared session connects with, before its profile is made.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Protocol {
    Ssh,
    Sftp,
    Rdp,
    Telnet,
    Vnc,
}

impl Protocol {
    /// The protocol a file or an offline copy names, in any case; none for another.
    fn of_name(name: &str) -> Option<Self> {
        [
            (PROTOCOL_SSH, Self::Ssh),
            (PROTOCOL_SFTP, Self::Sftp),
            (PROTOCOL_RDP, Self::Rdp),
            (PROTOCOL_TELNET, Self::Telnet),
            (PROTOCOL_VNC, Self::Vnc),
        ]
        .into_iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(name))
        .map(|(_, protocol)| protocol)
    }

    /// Whether it names an account: Telnet asks for its login in the terminal and a VNC
    /// session has no login field (`MobaXtermImporter.cs:100-157`).
    fn has_account(self) -> bool {
        matches!(self, Self::Ssh | Self::Sftp | Self::Rdp)
    }

    /// Whether it can go through an SSH gateway: the SSH and RDP sessions of a `MobaXterm` file
    /// are the ones with a jump host slot (`MobaXtermImporter.CanCarryGateway`).
    fn carries_gateway(self) -> bool {
        matches!(self, Self::Ssh | Self::Rdp)
    }

    /// The key a session of the file gives its port under, and the port when it gives none.
    fn port_key(self) -> (&'static str, u16) {
        match self {
            Self::Ssh | Self::Sftp => ("sshPort", DEFAULT_SSH_PORT),
            Self::Rdp => ("remotePort", DEFAULT_RDP_PORT),
            Self::Telnet => ("telnetPort", DEFAULT_TELNET_PORT),
            Self::Vnc => ("vncPort", DEFAULT_VNC_PORT),
        }
    }

    /// The key of the account in a session of the file, for the protocols that name one.
    fn account_key(self) -> Option<&'static str> {
        match self {
            Self::Ssh | Self::Sftp => Some("sshUsername"),
            Self::Rdp => Some("rdpUsername"),
            Self::Telnet | Self::Vnc => None,
        }
    }
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

/// The session `server` of a file as the C# sanitizer keeps it: SSH, SFTP, RDP, Telnet or VNC,
/// a valid host, and the fields of its whitelist; for any other, why it is refused and what the
/// file gives that is the reason.
fn sanitize(
    source_id: &str,
    server: &Value,
    routes: &HashMap<String, Result<SshGateway, String>>,
) -> Result<SharedSession, (SkipReason, String)> {
    let kind = text_of(server, "connectionType")
        .ok_or_else(|| (SkipReason::OtherProtocol, String::new()))?;
    // A jump host or a proxy the protocol cannot honour never gets here: the reader refuses
    // such a session (`refused_by_reader`), as the C# parser does
    // (`MobaXtermImporter.cs:308-326`).
    let protocol =
        Protocol::of_name(kind).ok_or_else(|| (SkipReason::OtherProtocol, display_text(kind)))?;
    let host = text_of(server, "remoteServer")
        .filter(|host| is_valid_host(host))
        .ok_or_else(|| {
            let given = server
                .get("remoteServer")
                .and_then(Value::as_str)
                .unwrap_or_default();
            (SkipReason::InvalidHost, display_text(given))
        })?;
    let folder = text_of(server, "group");
    let name = text_of(server, "displayName").unwrap_or(host);
    // A gateway the file names and the sanitizer cannot keep refuses the session: dropping it
    // would turn a session meant to cross a bastion into a direct one. A protocol that cannot
    // carry one refuses it the same way.
    let gateway = match server.get(GATEWAY_ID_KEY).and_then(Value::as_str) {
        None => None,
        Some(_) if !protocol.carries_gateway() => {
            return Err((SkipReason::InvalidGateway, String::new()));
        }
        Some(placeholder) => match routes.get(placeholder) {
            Some(Ok(gateway)) => Some(gateway.clone()),
            Some(Err(host)) => return Err((SkipReason::InvalidGateway, display_text(host))),
            None => return Err((SkipReason::InvalidGateway, String::new())),
        },
    };
    // The RD Gateway of an RDP session is kept (the RDP client goes through it); one that is not
    // a host refuses the session, as dropping it would make the session a direct one
    // (`SharedProfileSanitizer.cs`, `InvalidRdGateway`).
    let rd_gateway = match (protocol, text_of(server, RD_GATEWAY_KEY)) {
        (Protocol::Rdp, Some(gateway)) if is_valid_host(gateway) => Some(gateway),
        (Protocol::Rdp, Some(gateway)) => {
            return Err((SkipReason::InvalidRdGateway, display_text(gateway)));
        }
        _ => None,
    };
    let (port_key, default_port) = protocol.port_key();
    Ok(build_session(
        source_id,
        protocol,
        folder,
        name,
        host,
        port_of(server, port_key, default_port),
        Login {
            username: protocol.account_key().and_then(|key| text_of(server, key)),
            gateway,
            rd_gateway,
            // Only a shell types commands; the other protocols have no post-connect step.
            steps: if matches!(protocol, Protocol::Ssh) {
                steps_of(server)
            } else {
                Vec::new()
            },
        },
    ))
}

/// The session a warning of the reader says it left out, as a refused one: its SSH gateway has
/// no usable host, its RD Gateway has none or is signed in to in a way Heimdall does not
/// reproduce (a smart card), it goes through a jump host or a proxy its protocol cannot follow
/// (taking any of these direct would go around the route), it is a WSL distribution, or it is of
/// a type Heimdall does not take.
fn refused_by_reader(warning: &FileWarning) -> Option<Refused> {
    let no_detail = String::new();
    let (session, folder, host, reason) = match warning {
        FileWarning::InvalidGatewayHost {
            session,
            folder,
            host,
        } => (session, folder, host, SkipReason::InvalidGateway),
        FileWarning::InvalidRdGatewayHost {
            session,
            folder,
            host,
        } => (session, folder, host, SkipReason::InvalidRdGateway),
        FileWarning::RdGatewayNotSupported {
            session,
            folder,
            host,
        } => (session, folder, host, SkipReason::RdGatewayNotSupported),
        FileWarning::JumpHostNotSupported {
            session,
            folder,
            host,
        } => (session, folder, host, SkipReason::JumpHostNotSupported),
        FileWarning::ProxyNotSupported {
            session,
            folder,
            host,
        } => (session, folder, host, SkipReason::ProxyNotSupported),
        FileWarning::UnsupportedSessionType {
            session,
            folder,
            type_code,
        } => (session, folder, type_code, SkipReason::OtherProtocol),
        FileWarning::FtpNotSupported { session, folder } => {
            (session, folder, &no_detail, SkipReason::FtpNotSupported)
        }
        FileWarning::WslNotSupported { session, folder } => {
            (session, folder, &no_detail, SkipReason::WslNotSupported)
        }
        _ => return None,
    };
    Some(Refused {
        folder: display_text(folder),
        name: display_text(session),
        reason,
        detail: display_text(host),
    })
}

/// The gateways of a file as the shared namespace keeps them, by the identifier the reader gave
/// them: each with the host, port and account the file says and nothing else, or the host the
/// file gives when it is not one.
fn gateways_of(source_id: &str, gateways: &[Value]) -> HashMap<String, Result<SshGateway, String>> {
    gateways
        .iter()
        .filter_map(|gateway| {
            let placeholder = gateway.get("id").and_then(Value::as_str)?;
            let host = gateway
                .get("host")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let kept = shared_gateway(
                source_id,
                text_of(gateway, "name"),
                host,
                port_of(gateway, "port", DEFAULT_SSH_PORT),
                text_of(gateway, "user"),
            )
            .ok_or_else(|| host.to_owned());
            Some((placeholder.to_owned(), kept))
        })
        .collect()
}

/// A gateway of a shared source: its identifier follows its host, port and account, so the
/// same file read twice gives the same one, and it holds no key, no password and no parent.
/// None when its host is not one or its port is zero.
fn shared_gateway(
    source_id: &str,
    name: Option<&str>,
    host: &str,
    port: u16,
    username: Option<&str>,
) -> Option<SshGateway> {
    let host = clean(host).filter(|host| is_valid_host(host))?;
    if port == 0 {
        return None;
    }
    let username = username.and_then(clean);
    let id = ProfileId::new(gateway_id(
        source_id,
        host,
        port,
        username.unwrap_or_default(),
    ));
    Some(SshGateway {
        id,
        name: name.and_then(clean).unwrap_or(host).to_owned(),
        host: host.to_owned(),
        port,
        username: username.map(str::to_owned),
        key_path: None,
        parent: None,
    })
}

/// The longest text of a file shown back in a report.
const MAX_REPORT_TEXT: usize = 120;

/// `text` made safe to show: no control character, and short.
fn display_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(MAX_REPORT_TEXT)
        .collect()
}

/// How a shared session logs in: its account and the gateway it goes through.
struct Login<'a> {
    username: Option<&'a str>,
    gateway: Option<SshGateway>,
    /// The RD Gateway of an RDP session, a host already checked. Its own login is never carried.
    rd_gateway: Option<&'a str>,
    /// The commands the file asks a shell to type once it is ready, SSH only. Never approved
    /// by the file: they run only once the user approved them (see `app::shared`).
    steps: Vec<PostConnectStep>,
}

/// The shared session of a source: its identifier follows its folder and name, as the C#
/// `MobaXtermImporter.BuildId`.
fn build_session(
    source_id: &str,
    kind: Protocol,
    folder: Option<&str>,
    name: &str,
    host: &str,
    port: u16,
    login: Login<'_>,
) -> SharedSession {
    let Login {
        username,
        gateway,
        rd_gateway,
        steps,
    } = login;
    let id = ProfileId::new(session_id(source_id, folder.unwrap_or_default(), name));
    let route = gateway.as_ref().map(|gateway| gateway.id.clone());
    let target = match kind {
        Protocol::Ssh => {
            SharedTarget::Ssh(ssh_profile(&id, name, host, port, username, route, steps))
        }
        Protocol::Sftp => {
            let mut files = ssh_profile(&id, name, host, port, username, route, steps);
            // Opens its files, as the C# SFTP connection type.
            files.sftp = true;
            SharedTarget::Sftp(files)
        }
        Protocol::Rdp => SharedTarget::Rdp(rdp_profile(
            &id, name, host, port, username, route, rd_gateway,
        )),
        Protocol::Telnet => SharedTarget::Telnet(TelnetProfile {
            id: id.clone(),
            name: name.to_owned(),
            group: None,
            host: host.to_owned(),
            port,
            session_logging: None,
        }),
        Protocol::Vnc => SharedTarget::Vnc(vnc_profile(&id, name, host, port)),
    };
    SharedSession {
        id,
        source_id: source_id.to_owned(),
        folder: folder.map(str::to_owned),
        gateway,
        target,
    }
}

/// An SSH session of a shared file: its own fields and nothing else. No key, no agent, no
/// folder: a local folder's defaults never reach it. Its gateway is one of the shared
/// namespace. The commands the file asks to type after connecting are kept as steps the user
/// has not approved: they are typed only once approved.
fn ssh_profile(
    id: &ProfileId,
    name: &str,
    host: &str,
    port: u16,
    username: Option<&str>,
    gateway: Option<ProfileId>,
    steps: Vec<PostConnectStep>,
) -> SshProfile {
    SshProfile {
        id: id.clone(),
        name: name.to_owned(),
        group: None,
        host: host.to_owned(),
        port,
        username: username.map(str::to_owned),
        key_path: None,
        gateway,
        local_tunnel_port: None,
        vault_entry: None,
        forwards: Forwards::default(),
        post_connect: PostConnect {
            // From a file other people write: each step waits for its own Run.
            steps: steps
                .into_iter()
                .map(PostConnectStep::from_outside)
                .collect(),
            approved: None,
        },
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
        ssh_mode: SshMode::Embedded,
        x11_forwarding: false,
    }
}

/// A VNC session of a shared file: the C# sanitizer keeps a name, a host and a port and leaves
/// every other option at the default of a new profile. No account, no stored password, and
/// the safe defaults: a server asking for no password or offering no encryption is not
/// accepted on the file's word.
fn vnc_profile(id: &ProfileId, name: &str, host: &str, port: u16) -> VncProfile {
    VncProfile {
        id: id.clone(),
        name: name.to_owned(),
        group: None,
        host: host.to_owned(),
        port,
        view_only: false,
        allow_no_password: false,
        require_tls: false,
        username: None,
        vault_entry: None,
    }
}

/// An RDP session of a shared file, on the application's RDP options.
fn rdp_profile(
    id: &ProfileId,
    name: &str,
    host: &str,
    port: u16,
    username: Option<&str>,
    gateway: Option<ProfileId>,
    rd_gateway: Option<&str>,
) -> RdpProfile {
    RdpProfile {
        extras: RdpExtras {
            rd_gateway: rd_gateway.map(str::to_owned),
            ..RdpExtras::default()
        },
        id: id.clone(),
        name: name.to_owned(),
        group: None,
        host: host.to_owned(),
        port,
        username: username.map(str::to_owned),
        domain: None,
        allow_tls_only: false,
        gateway,
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
