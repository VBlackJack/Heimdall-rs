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

//! Import of another `known_hosts` file into Heimdall-rs's own, as the C# Heimdall's
//! "Trusted SSH hosts..." (`KnownHostsParser`, `KnownHostsImporter`).
//!
//! A plainly named host's key is read; what cannot be trusted as written is said with its
//! line: a hashed host name (it cannot be checked against the keys already trusted, and
//! would trust a key for a host nobody could name), a wildcard or negation, a certificate
//! authority, a revoked key, a key that cannot be read. A key already trusted is said so; a
//! key that contradicts one trusted, or another of the same file for the same server, is a
//! conflict and is never imported: the file cannot overrule what the user accepted. A
//! server trusted by a pinned fingerprint alone is trusted for that key and no other: the
//! key with that fingerprint is recorded in full and the pin dropped, any other key is a
//! conflict.

use std::collections::HashMap;
use std::net::Ipv6Addr;

use russh::keys::{Algorithm, PublicKey};

use crate::known_hosts::{
    KnownHosts, KnownHostsError, Verdict, fingerprint, validate_host, verdict,
};
use crate::pins::{self, PinVerdict, Pins, pin_verdict};
use crate::trust_files::{self, TrustLock};

/// Longest line read, as the C# `MaxLineLength`: a longer one is malformed.
pub const MAX_LINE: usize = 65_536;

/// Largest file read, as the C# `MaxFileSizeBytes`: a larger one is refused whole.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// Port a `known_hosts` host without brackets is on.
const DEFAULT_PORT: u16 = 22;

/// A key for a server, from the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyCandidate {
    /// Host, as the file names it.
    pub host: String,
    /// Port.
    pub port: u16,
    /// The key.
    pub key: PublicKey,
    /// Its line, from 1.
    pub line: usize,
}

/// What a diagnostic says, as the C# `KnownHostsDiagnosticCode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyNote {
    /// `@cert-authority`: not imported.
    CertAuthority,
    /// `@revoked`: not imported.
    Revoked,
    /// A line that is not `hosts type key`; carries why.
    Malformed(Malformed),
    /// A key type not read; carries it.
    UnsupportedKey(String),
    /// A hashed host name: not imported.
    HashedHost,
    /// A wildcard, negation or unreadable host pattern; carries it.
    HostPattern(String),
}

/// Why a line could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Malformed {
    /// Longer than [`MAX_LINE`].
    TooLong,
    /// Fewer than the three fields `hosts type key`; carries how many.
    Fields(usize),
    /// A key that cannot be read.
    BadKey,
    /// A marker other than `@cert-authority` or `@revoked`; carries it, `@` included.
    Marker(String),
}

impl HostKeyNote {
    /// Whether the C# counts it as a warning: a line it could not read. What it read and
    /// chose not to import is information.
    #[must_use]
    pub fn is_warning(&self) -> bool {
        matches!(self, Self::Malformed(_))
    }
}

/// Something the import says about a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyDiagnostic {
    /// Its line, from 1.
    pub line: usize,
    /// What it says.
    pub note: HostKeyNote,
}

/// What a file gives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostKeysParsed {
    /// The keys for plainly named servers, in the file's order.
    pub candidates: Vec<HostKeyCandidate>,
    /// What was left out.
    pub diagnostics: Vec<HostKeyDiagnostic>,
}

/// Reads a `known_hosts` file.
#[must_use]
pub fn parse(text: &str) -> HostKeysParsed {
    let mut parsed = HostKeysParsed::default();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let mut say = |note| {
            parsed
                .diagnostics
                .push(HostKeyDiagnostic { line: number, note });
        };
        if line.len() > MAX_LINE {
            say(HostKeyNote::Malformed(Malformed::TooLong));
            continue;
        }
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('@') {
            let marker = rest.split(char::is_whitespace).next().unwrap_or_default();
            say(match marker {
                "cert-authority" => HostKeyNote::CertAuthority,
                "revoked" => HostKeyNote::Revoked,
                other => HostKeyNote::Malformed(Malformed::Marker(format!("@{other}"))),
            });
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [hosts, kind, blob, ..] = fields.as_slice() else {
            say(HostKeyNote::Malformed(Malformed::Fields(fields.len())));
            continue;
        };
        let Ok(key) = PublicKey::from_openssh(&format!("{kind} {blob}")) else {
            say(if kind.starts_with("ssh-") || kind.starts_with("ecdsa-") {
                HostKeyNote::UnsupportedKey((*kind).to_owned())
            } else {
                HostKeyNote::Malformed(Malformed::BadKey)
            });
            continue;
        };
        for token in hosts
            .split(',')
            .map(str::trim)
            .filter(|token| !token.is_empty())
        {
            match host_of(token) {
                Ok((host, port)) => parsed.candidates.push(HostKeyCandidate {
                    host,
                    port,
                    key: key.clone(),
                    line: number,
                }),
                Err(note) => parsed
                    .diagnostics
                    .push(HostKeyDiagnostic { line: number, note }),
            }
        }
    }
    parsed
}

/// The host and port a pattern names: `host`, `[host]:port`, or an IPv6 address without
/// brackets, as the C# reads them.
fn host_of(token: &str) -> Result<(String, u16), HostKeyNote> {
    let pattern = || HostKeyNote::HostPattern(token.to_owned());
    if token.starts_with("|1|") {
        return Err(HostKeyNote::HashedHost);
    }
    if token.contains(['*', '?', '!', '%']) {
        return Err(pattern());
    }
    let (host, port) = if let Some(rest) = token.strip_prefix('[') {
        let (host, after) = rest.split_once(']').ok_or_else(pattern)?;
        let port = match after {
            "" => DEFAULT_PORT,
            _ => after
                .strip_prefix(':')
                .and_then(|port| port.parse::<u16>().ok())
                .filter(|port| *port != 0)
                .ok_or_else(pattern)?,
        };
        (host, port)
    } else {
        match token.matches(':').count() {
            0 => (token, DEFAULT_PORT),
            1 => return Err(pattern()),
            _ if token.parse::<Ipv6Addr>().is_ok() => (token, DEFAULT_PORT),
            _ => return Err(pattern()),
        }
    };
    let host = validate_host(host).map_err(|_| pattern())?;
    Ok((host, port))
}

/// Whether a key is new, already trusted, or contradicts one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyStatus {
    /// Nothing contradicts it: it can be trusted.
    New,
    /// This very key is trusted already.
    Existing,
    /// A different key of its kind is trusted for the server, or given by the file too.
    Conflict,
}

/// What a key of another algorithm than the ones recorded for its server is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtherAlgorithm {
    /// Recorded beside them: it contradicts none. The import of a file the user picked.
    Adds,
    /// A conflict, as the C# `KnownHostsImporter` makes it: its store keeps one key per
    /// server, and any other fingerprint contradicts it.
    Conflicts,
}

/// What contradicts a key, said as a connection says it: a changed key, a key of another
/// algorithm, or another fingerprint pinned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Contradiction {
    /// Another key of its algorithm is recorded; carries that key's fingerprint.
    Changed(String),
    /// Keys of other algorithms only are recorded, and the rule refuses another; carries
    /// their algorithms.
    OtherAlgorithm(Vec<Algorithm>),
    /// Another fingerprint is pinned for the server; carries it.
    Pinned(String),
}

/// What trusting a key does for its server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trusting {
    /// Nothing recorded or pinned for the server contradicts it: recorded.
    Learn,
    /// Its fingerprint is the one pinned for the server: recorded in full and the pin
    /// dropped, as a connection does when it first meets that key.
    LearnPinned,
    /// This very key is recorded already.
    Recorded,
    /// A key recorded, or a pin, contradicts it: never recorded.
    Conflict(Contradiction),
}

/// The one rule every import follows for a key, against the keys `recorded` for its server
/// and the fingerprints `pinned` for it, as the C# `KnownHostsImporter.Import` decides
/// against the server's entry: none, the import; the same fingerprint, a match; another,
/// a conflict, never written over the trust the user gave.
///
/// The key recorded already is [`Trusting::Recorded`]. Another key of its algorithm
/// recorded is a conflict; one of another algorithm only as `other` says. A pin, the
/// algorithm of its key unknown, trusts the key with its fingerprint and no other: that
/// key is [`Trusting::LearnPinned`], any other key a conflict.
#[must_use]
pub fn trusting(
    recorded: &[PublicKey],
    pinned: &[String],
    key: &PublicKey,
    other: OtherAlgorithm,
) -> Trusting {
    match verdict(recorded, key) {
        Verdict::Trusted => return Trusting::Recorded,
        Verdict::Changed { recorded } => {
            return Trusting::Conflict(Contradiction::Changed(fingerprint(&recorded)));
        }
        Verdict::OtherAlgorithm { recorded } if other == OtherAlgorithm::Conflicts => {
            return Trusting::Conflict(Contradiction::OtherAlgorithm(recorded));
        }
        Verdict::Unknown | Verdict::OtherAlgorithm { .. } => {}
    }
    match pin_verdict(pinned, key) {
        PinVerdict::None => Trusting::Learn,
        PinVerdict::Matches => Trusting::LearnPinned,
        PinVerdict::Differs { pinned } => Trusting::Conflict(Contradiction::Pinned(pinned)),
    }
}

/// Trusts `key` for `host` on `port` as [`trusting`] decides, against what `store` records
/// and pins for the server at that moment: read and written under the lock of the trust
/// files, no other writer between the check and the write. What was decided.
///
/// # Errors
///
/// [`KnownHostsError`] when the store or its pins cannot be read, or the key cannot be
/// recorded.
pub fn trust(
    store: &KnownHosts,
    host: &str,
    port: u16,
    key: &PublicKey,
    other: OtherAlgorithm,
) -> Result<Trusting, KnownHostsError> {
    trust_locked(&trust_files::lock(), store, host, port, key, other)
}

/// [`trust`], the lock of the trust files held by the caller.
pub(crate) fn trust_locked(
    lock: &TrustLock,
    store: &KnownHosts,
    host: &str,
    port: u16,
    key: &PublicKey,
    other: OtherAlgorithm,
) -> Result<Trusting, KnownHostsError> {
    let pinned = Pins::beside(store.path()).pinned(host, port)?;
    let decided = trusting(&store.recorded(host, port)?, &pinned, key, other);
    match decided {
        Trusting::Learn => store.learn_locked(lock, host, port, key)?,
        Trusting::LearnPinned => pins::record_in_full(lock, store, host, port, key)?,
        Trusting::Recorded | Trusting::Conflict(_) => {}
    }
    Ok(decided)
}

/// What the import would do with each key, against the keys `store` trusts and the
/// fingerprints it pins.
///
/// # Errors
///
/// [`KnownHostsError`] when the store or its pins cannot be read.
pub fn assess(
    candidates: &[HostKeyCandidate],
    store: &KnownHosts,
) -> Result<Vec<HostKeyStatus>, KnownHostsError> {
    let pins = Pins::beside(store.path());
    // Two different keys of one kind for one server in the same file: neither is trusted.
    let mut seen: HashMap<(String, u16, String), Vec<&PublicKey>> = HashMap::new();
    for candidate in candidates {
        let keys = seen
            .entry((
                candidate.host.clone(),
                candidate.port,
                candidate.key.algorithm().to_string(),
            ))
            .or_default();
        if !keys
            .iter()
            .any(|key| key.key_data() == candidate.key.key_data())
        {
            keys.push(&candidate.key);
        }
    }
    candidates
        .iter()
        .map(|candidate| {
            let group = &seen[&(
                candidate.host.clone(),
                candidate.port,
                candidate.key.algorithm().to_string(),
            )];
            if group.len() > 1 {
                return Ok(HostKeyStatus::Conflict);
            }
            let recorded = store.recorded(&candidate.host, candidate.port)?;
            let pinned = pins.pinned(&candidate.host, candidate.port)?;
            // A key of another kind for a known server adds, and contradicts nothing.
            Ok(
                match trusting(&recorded, &pinned, &candidate.key, OtherAlgorithm::Adds) {
                    Trusting::Learn | Trusting::LearnPinned => HostKeyStatus::New,
                    Trusting::Recorded => HostKeyStatus::Existing,
                    Trusting::Conflict(_) => HostKeyStatus::Conflict,
                },
            )
        })
        .collect()
}

/// How an import went.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HostKeysImported {
    /// Keys now trusted.
    pub imported: usize,
    /// Keys trusted already.
    pub existing: usize,
    /// Keys left out as conflicts.
    pub conflicts: usize,
}

/// Trusts the `chosen` keys in `store`, each checked again as it is written, under the lock
/// of the trust files: a key that has come to contradict one trusted or pinned meanwhile is
/// a conflict, never written over it; the key with a pinned fingerprint is recorded in full
/// and the pin dropped.
///
/// # Errors
///
/// [`KnownHostsError`] when the store cannot be read or written; the keys written before
/// stay.
pub fn import(
    chosen: &[HostKeyCandidate],
    store: &KnownHosts,
) -> Result<HostKeysImported, KnownHostsError> {
    let statuses = assess(chosen, store)?;
    let lock = trust_files::lock();
    let mut done = HostKeysImported::default();
    for (candidate, status) in chosen.iter().zip(statuses) {
        match status {
            HostKeyStatus::Conflict => done.conflicts += 1,
            HostKeyStatus::Existing => done.existing += 1,
            // The same key twice in the file is written once: recorded the second time.
            HostKeyStatus::New => match trust_locked(
                &lock,
                store,
                &candidate.host,
                candidate.port,
                &candidate.key,
                OtherAlgorithm::Adds,
            )? {
                Trusting::Learn | Trusting::LearnPinned => done.imported += 1,
                Trusting::Recorded => done.existing += 1,
                Trusting::Conflict(_) => done.conflicts += 1,
            },
        }
    }
    Ok(done)
}
