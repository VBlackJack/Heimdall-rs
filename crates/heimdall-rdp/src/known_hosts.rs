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

//! The RDP servers trusted so far, by the key of their certificate.
//!
//! One line per key trusted: `host:port SHA256:<base64>`, an IPv6 address in brackets, the
//! host in lower case, then optional `name=value` attributes, separated by spaces:
//!
//! - `trusted=<seconds since 1970-01-01 UTC>`: when the key was trusted;
//! - `subject=<text>` and `issuer=<text>`: the names its certificate carried, as UTF-8 in
//!   base64url without padding, so that no space splits them;
//! - `certificate=SHA256:<base64>`: the SHA-256 of the whole certificate, written as a key's
//!   fingerprint, for the FTPS and VNC servers, which are pinned by their whole certificate
//!   as the C# pins them; the RDP servers' lines never carry it;
//! - `valid_from=<seconds>` and `valid_until=<seconds>`: that certificate's validity, since
//!   1970-01-01 UTC, beside it.
//!
//! For example `dc.lab:3389 SHA256:rgJ0... trusted=1767225600 subject=Q049ZGMubGFi`, or
//! `files.lab:990 SHA256:rgJ0... trusted=1767225600 certificate=SHA256:ungWv48Bz...
//! valid_from=1767225600 valid_until=1798761600`.
//!
//! The key stays the second field: a reader of the first two fields alone, an older
//! Heimdall among them, reads every line as before. An attribute not known, or that does
//! not decode, is ignored. Lines starting with `#` and lines that do not parse are kept as
//! they are; forgetting drops whole lines and never rewrites one, and adopting a certificate
//! for a line recorded with its key alone only adds attributes at the end of that line, so
//! every other line keeps its attributes byte for byte, unknown ones included.

use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use data_encoding::BASE64URL_NOPAD;
use heimdall_core::profile::display_address;

use crate::certificate::{CertificateHash, Fingerprint, ServerCertificate, Validity, shown};

/// Attribute of the time a key was trusted, in seconds since the Unix epoch.
const TRUSTED_ATTRIBUTE: &str = "trusted";
/// Attribute of the subject of the certificate trusted.
const SUBJECT_ATTRIBUTE: &str = "subject";
/// Attribute of the issuer of the certificate trusted.
const ISSUER_ATTRIBUTE: &str = "issuer";
/// Attribute of the hash of the whole certificate trusted.
const CERTIFICATE_ATTRIBUTE: &str = "certificate";
/// Attribute of the first moment the certificate trusted holds, in seconds since the Unix
/// epoch.
const VALID_FROM_ATTRIBUTE: &str = "valid_from";
/// Attribute of the last moment the certificate trusted holds, in seconds since the Unix
/// epoch.
const VALID_UNTIL_ATTRIBUTE: &str = "valid_until";
/// What separates an attribute's name from its value.
const ATTRIBUTE_SEPARATOR: char = '=';

/// Where a server stands against the recorded pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Recorded with this key.
    Known,
    /// Never recorded.
    Unknown,
    /// Recorded with another key.
    Changed {
        /// A key recorded for it.
        recorded: Fingerprint,
    },
}

/// Where a server pinned by its whole certificate, an FTPS or VNC one, stands against the
/// recorded pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateVerdict {
    /// Recorded with this whole certificate.
    Known,
    /// Recorded with this key, by a Heimdall that pinned keys alone: no certificate on
    /// record for it, the one presented may be adopted.
    KeyOnly,
    /// Recorded with this key, but with another certificate: renewed on the same key, or
    /// minted again on it by whoever holds it.
    Renewed {
        /// When the certificate on record holds, when recorded.
        recorded: Option<Validity>,
    },
    /// Never recorded.
    Unknown,
    /// Recorded with another key.
    Changed {
        /// A key recorded for it.
        recorded: Fingerprint,
    },
}

/// The file of trusted RDP servers.
#[derive(Debug, Clone)]
pub struct KnownRdpHosts {
    path: PathBuf,
}

/// The address a server is recorded under.
fn address(host: &str, port: u16) -> String {
    display_address(&host.to_ascii_lowercase(), port)
}

/// The host and port of an address as [`address`] writes it: `host:port`, or an IPv6 host
/// in brackets.
fn split_address(address: &str) -> Option<(String, u16)> {
    let (host, port) = match address.strip_prefix('[') {
        Some(bracketed) => bracketed.split_once("]:")?,
        None => address.rsplit_once(':')?,
    };
    if host.is_empty() {
        return None;
    }
    Some((host.to_owned(), port.parse().ok()?))
}

/// A server the file trusts, and its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownRdpHost {
    /// Host, in lower case.
    pub host: String,
    /// Port.
    pub port: u16,
    /// The key of its certificate.
    pub fingerprint: Fingerprint,
    /// The subject of its certificate, made safe to show; `None` when not recorded.
    pub subject: Option<String>,
    /// The issuer of its certificate, made safe to show; `None` when not recorded.
    pub issuer: Option<String>,
    /// When the key was trusted; `None` when not recorded.
    pub trusted: Option<SystemTime>,
    /// The hash of the whole certificate trusted; `None` when only its key is recorded, as
    /// for every RDP server.
    pub certificate: Option<CertificateHash>,
    /// When the certificate trusted holds; `None` when not recorded.
    pub validity: Option<Validity>,
}

/// Seconds since the Unix epoch, as an attribute holds them.
fn moment(value: &str) -> Option<SystemTime> {
    value
        .parse()
        .ok()
        .and_then(|seconds| UNIX_EPOCH.checked_add(Duration::from_secs(seconds)))
}

/// `time` as an attribute holds it, in seconds since the Unix epoch; `None` before it.
fn seconds(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|since| since.as_secs())
}

/// Fills `entry` from a line's attributes, those after its key; the first of a name counts.
fn read_attributes<'a>(entry: &mut KnownRdpHost, attributes: impl Iterator<Item = &'a str>) {
    let (mut valid_from, mut valid_until) = (None, None);
    for attribute in attributes {
        let Some((name, value)) = attribute.split_once(ATTRIBUTE_SEPARATOR) else {
            continue;
        };
        match name {
            TRUSTED_ATTRIBUTE if entry.trusted.is_none() => entry.trusted = moment(value),
            SUBJECT_ATTRIBUTE if entry.subject.is_none() => entry.subject = decoded(value),
            ISSUER_ATTRIBUTE if entry.issuer.is_none() => entry.issuer = decoded(value),
            CERTIFICATE_ATTRIBUTE if entry.certificate.is_none() => {
                entry.certificate = value.parse().ok();
            }
            VALID_FROM_ATTRIBUTE if valid_from.is_none() => valid_from = moment(value),
            VALID_UNTIL_ATTRIBUTE if valid_until.is_none() => valid_until = moment(value),
            _ => {}
        }
    }
    if let (Some(not_before), Some(not_after)) = (valid_from, valid_until) {
        entry.validity = Some(Validity {
            not_before,
            not_after,
        });
    }
}

/// The attributes recording the whole certificate `hash`, and its `validity` when known,
/// each after a space.
fn whole_certificate_attributes(hash: &CertificateHash, validity: Option<&Validity>) -> String {
    let mut attributes = format!(" {CERTIFICATE_ATTRIBUTE}={hash}");
    if let Some(validity) = validity
        && let (Some(from), Some(until)) =
            (seconds(validity.not_before), seconds(validity.not_after))
    {
        let _ = write!(
            attributes,
            " {VALID_FROM_ATTRIBUTE}={from} {VALID_UNTIL_ATTRIBUTE}={until}"
        );
    }
    attributes
}

/// A name as an attribute holds it: UTF-8 in base64url without padding.
fn encoded(text: &str) -> String {
    BASE64URL_NOPAD.encode(text.as_bytes())
}

/// A name read back from an attribute, made safe to show again: the file may have been
/// edited. `None` when it does not decode, or is empty.
fn decoded(value: &str) -> Option<String> {
    let bytes = BASE64URL_NOPAD.decode(value.as_bytes()).ok()?;
    let text = shown(&String::from_utf8(bytes).ok()?);
    (!text.is_empty()).then_some(text)
}

impl KnownRdpHosts {
    /// The file at `path`; a missing file knows no server.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Where `host:port` presenting `presented` stands.
    ///
    /// # Errors
    ///
    /// The file exists and cannot be read.
    pub fn verdict(&self, host: &str, port: u16, presented: &Fingerprint) -> io::Result<Verdict> {
        let keys = self.keys(host, port)?;
        if keys.contains(presented) {
            return Ok(Verdict::Known);
        }
        Ok(keys
            .first()
            .map_or(Verdict::Unknown, |recorded| Verdict::Changed {
                recorded: *recorded,
            }))
    }

    /// Whether a key is recorded for `host:port`: the server was trusted before.
    ///
    /// # Errors
    ///
    /// The file exists and cannot be read.
    pub fn knows(&self, host: &str, port: u16) -> io::Result<bool> {
        Ok(!self.keys(host, port)?.is_empty())
    }

    /// The keys recorded for `host:port`, in the order of the file.
    fn keys(&self, host: &str, port: u16) -> io::Result<Vec<Fingerprint>> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let wanted = address(host, port);
        Ok(text
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let (Some(line_address), Some(key)) = (fields.next(), fields.next()) else {
                    return None;
                };
                if line_address != wanted {
                    return None;
                }
                key.parse::<Fingerprint>().ok()
            })
            .collect())
    }

    /// The servers trusted and their keys, in the order of the file; comments and lines that
    /// do not parse are left out, as they trust nothing.
    ///
    /// # Errors
    ///
    /// The file exists and cannot be read.
    pub fn entries(&self) -> io::Result<Vec<KnownRdpHost>> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        Ok(text
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let (host, port) = split_address(fields.next()?)?;
                let fingerprint = fields.next()?.parse().ok()?;
                let mut entry = KnownRdpHost {
                    host,
                    port,
                    fingerprint,
                    subject: None,
                    issuer: None,
                    trusted: None,
                    certificate: None,
                    validity: None,
                };
                read_attributes(&mut entry, fields);
                Some(entry)
            })
            .collect())
    }

    /// Forgets `key` for `host:port` only: another key trusted for the same server stays,
    /// and so does every other line. Whether it was there.
    ///
    /// # Errors
    ///
    /// The file exists and cannot be read or written.
    pub fn forget_key(&self, host: &str, port: u16, key: &Fingerprint) -> io::Result<bool> {
        let wanted = address(host, port);
        self.rewrite_without(|line| {
            let mut fields = line.split_whitespace();
            fields.next() == Some(wanted.as_str())
                && fields
                    .next()
                    .and_then(|text| text.parse::<Fingerprint>().ok())
                    .is_some_and(|recorded| recorded == *key)
        })
    }

    /// Where `host:port`, pinned by its whole certificate, stands when it presents the
    /// certificate of hash `certificate` on `key`.
    ///
    /// # Errors
    ///
    /// The file exists and cannot be read.
    pub fn certificate_verdict(
        &self,
        host: &str,
        port: u16,
        key: &Fingerprint,
        certificate: &CertificateHash,
    ) -> io::Result<CertificateVerdict> {
        let host = host.to_ascii_lowercase();
        let entries: Vec<KnownRdpHost> = self
            .entries()?
            .into_iter()
            .filter(|entry| entry.host == host && entry.port == port)
            .collect();
        let same_key: Vec<&KnownRdpHost> = entries
            .iter()
            .filter(|entry| entry.fingerprint == *key)
            .collect();
        if same_key
            .iter()
            .any(|entry| entry.certificate == Some(*certificate))
        {
            return Ok(CertificateVerdict::Known);
        }
        if same_key.iter().any(|entry| entry.certificate.is_none()) {
            return Ok(CertificateVerdict::KeyOnly);
        }
        if !same_key.is_empty() {
            return Ok(CertificateVerdict::Renewed {
                recorded: same_key.iter().find_map(|entry| entry.validity),
            });
        }
        Ok(entries
            .first()
            .map_or(CertificateVerdict::Unknown, |recorded| {
                CertificateVerdict::Changed {
                    recorded: recorded.fingerprint,
                }
            }))
    }

    /// Records for `host:port` the whole `certificate`, of hash `hash` and holding during
    /// `validity` when known, with its key, the time, its subject and its issuer, creating
    /// the file and its folder if needed. It replaces the lines of the same key for the
    /// server, a renewed certificate or one recorded by its key alone, as the C# keeps one
    /// certificate per server: the certificate it replaces is no longer trusted.
    ///
    /// # Errors
    ///
    /// The file cannot be read or written.
    pub fn record_whole_certificate(
        &self,
        host: &str,
        port: u16,
        certificate: &ServerCertificate,
        (hash, validity): (&CertificateHash, Option<&Validity>),
    ) -> io::Result<()> {
        self.forget_key(host, port, &certificate.fingerprint)?;
        self.append(
            host,
            port,
            &certificate.fingerprint,
            Some(certificate),
            &whole_certificate_attributes(hash, validity),
        )
    }

    /// Adopts the whole certificate of hash `hash`, holding during `validity` when known,
    /// for the lines of `host:port` that record `key` alone, as a Heimdall that pinned keys
    /// wrote them: its attributes are added at the end of those lines, every other line kept
    /// byte for byte. Whether a line took it.
    ///
    /// # Errors
    ///
    /// The file exists and cannot be read or written.
    pub fn adopt_certificate(
        &self,
        host: &str,
        port: u16,
        key: &Fingerprint,
        (hash, validity): (&CertificateHash, Option<&Validity>),
    ) -> io::Result<bool> {
        let wanted = address(host, port);
        let added = whole_certificate_attributes(hash, validity);
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        let mut adopted = false;
        let lines: Vec<String> = text
            .lines()
            .map(|line| {
                let mut fields = line.split_whitespace();
                let recorded_alone = fields.next() == Some(wanted.as_str())
                    && fields
                        .next()
                        .and_then(|text| text.parse::<Fingerprint>().ok())
                        .is_some_and(|recorded| recorded == *key)
                    && !fields.any(|attribute| {
                        attribute
                            .split_once(ATTRIBUTE_SEPARATOR)
                            .is_some_and(|(name, _)| name == CERTIFICATE_ATTRIBUTE)
                    });
                if recorded_alone {
                    adopted = true;
                    format!("{}{added}", line.trim_end())
                } else {
                    line.to_owned()
                }
            })
            .collect();
        if adopted {
            let mut rewritten = lines.join("\n");
            rewritten.push('\n');
            fs::write(&self.path, rewritten)?;
        }
        Ok(adopted)
    }

    /// Rewrites the file without the lines `drop` picks; whether one was.
    fn rewrite_without(&self, drop: impl Fn(&str) -> bool) -> io::Result<bool> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        let kept: Vec<&str> = text.lines().filter(|line| !drop(line)).collect();
        if kept.len() == text.lines().count() {
            return Ok(false);
        }
        let mut rewritten = kept.join("\n");
        if !rewritten.is_empty() {
            rewritten.push('\n');
        }
        fs::write(&self.path, rewritten)?;
        Ok(true)
    }

    /// Forgets every key recorded for `host:port`, keeping the other lines as they are.
    /// Whether one was there. The next connection asks about the server again.
    ///
    /// # Errors
    ///
    /// The file exists and cannot be read or written.
    pub fn forget(&self, host: &str, port: u16) -> io::Result<bool> {
        let wanted = address(host, port);
        self.rewrite_without(|line| line.split_whitespace().next() == Some(wanted.as_str()))
    }

    /// Records `host:port` with `key` and the time, creating the file and its folder if
    /// needed.
    ///
    /// # Errors
    ///
    /// The file cannot be written.
    pub fn record(&self, host: &str, port: u16, key: &Fingerprint) -> io::Result<()> {
        self.append(host, port, key, None, "")
    }

    /// Records `host:port` with the key of `certificate`, the time, and the subject and
    /// issuer of the certificate, creating the file and its folder if needed.
    ///
    /// # Errors
    ///
    /// The file cannot be written.
    pub fn record_certificate(
        &self,
        host: &str,
        port: u16,
        certificate: &ServerCertificate,
    ) -> io::Result<()> {
        self.append(host, port, &certificate.fingerprint, Some(certificate), "")
    }

    /// Appends the line of `host:port` and `key`, with the names of `certificate` if given,
    /// then `more`, attributes each after a space.
    fn append(
        &self,
        host: &str,
        port: u16,
        key: &Fingerprint,
        certificate: Option<&ServerCertificate>,
        more: &str,
    ) -> io::Result<()> {
        let mut line = format!("{} {key}", address(host, port));
        if let Ok(since) = SystemTime::now().duration_since(UNIX_EPOCH) {
            let _ = write!(line, " {TRUSTED_ATTRIBUTE}={}", since.as_secs());
        }
        if let Some(certificate) = certificate {
            for (name, value) in [
                (SUBJECT_ATTRIBUTE, &certificate.subject),
                (ISSUER_ATTRIBUTE, &certificate.issuer),
            ] {
                if !value.is_empty() {
                    let _ = write!(line, " {name}={}", encoded(value));
                }
            }
        }
        line.push_str(more);
        if let Some(dir) = self.path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            fs::create_dir_all(dir)?;
        }
        // A file that does not end with a line break gets one first, so the new line stays
        // its own.
        let needs_break =
            fs::read(&self.path).is_ok_and(|bytes| bytes.last().is_some_and(|last| *last != b'\n'));
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let separator = if needs_break { "\n" } else { "" };
        writeln!(file, "{separator}{line}")?;
        file.sync_all()
    }
}
