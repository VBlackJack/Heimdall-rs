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

//! Servers trusted by the fingerprint of their key alone, as the C# Heimdall kept most of
//! them: its store holds `SHA256:...` for a host, often without the key itself. A pin is
//! kept beside `known_hosts`, in a file of its own that OpenSSH never reads; the first key
//! the server presents with that fingerprint is trusted and recorded in full, and the pin
//! goes. A key with another fingerprint is a changed key, never a first contact.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use heimdall_core::import::csharp::TrustedHostKey;
use russh::keys::PublicKey;

use crate::known_hosts::{KnownHosts, KnownHostsError, fingerprint, validate_host};
use crate::trust_files::{self, TrustLock};

/// Added to the `known_hosts` file name for the file of pins.
const PINS_SUFFIX: &str = ".pins";

/// Start of an OpenSSH SHA-256 fingerprint.
const SHA256_PREFIX: &str = "SHA256:";

/// Characters of a SHA-256 digest in unpadded base64: 32 bytes.
const SHA256_BASE64_LENGTH: usize = 43;

/// Port a pattern leaves out of the host name.
const DEFAULT_SSH_PORT: u16 = 22;

/// Whether `text` is an OpenSSH SHA-256 fingerprint, `SHA256:` and 43 base64 characters.
#[must_use]
pub fn is_fingerprint(text: &str) -> bool {
    text.strip_prefix(SHA256_PREFIX).is_some_and(|digest| {
        digest.len() == SHA256_BASE64_LENGTH
            && digest
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/')
    })
}

/// What the pins say of a key a server presents, when `known_hosts` has nothing for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinVerdict {
    /// No pin for this server: a first contact.
    None,
    /// The key has a pinned fingerprint.
    Matches,
    /// Pins exist and the key has none of their fingerprints.
    Differs {
        /// A pinned fingerprint.
        pinned: String,
    },
}

/// Compares a presented key with the fingerprints pinned for its server.
#[must_use]
pub fn pin_verdict(pinned: &[String], offered: &PublicKey) -> PinVerdict {
    let Some(first) = pinned.first() else {
        return PinVerdict::None;
    };
    let offered = fingerprint(offered);
    if pinned.contains(&offered) {
        PinVerdict::Matches
    } else {
        PinVerdict::Differs {
            pinned: first.clone(),
        }
    }
}

/// The pins kept beside a `known_hosts` file.
#[derive(Debug, Clone)]
pub struct Pins {
    path: PathBuf,
}

impl Pins {
    /// The pins kept beside `known_hosts`.
    #[must_use]
    pub fn beside(known_hosts: &Path) -> Self {
        let mut name = known_hosts.as_os_str().to_owned();
        name.push(PINS_SUFFIX);
        Self {
            path: PathBuf::from(name),
        }
    }

    /// File read and written.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Fingerprints pinned for `host` on `port`.
    ///
    /// # Errors
    ///
    /// An unsafe host name, or a file that exists and cannot be read.
    pub fn pinned(&self, host: &str, port: u16) -> Result<Vec<String>, KnownHostsError> {
        let wanted = pattern(&validate_host(host)?, port);
        Ok(self
            .lines()?
            .into_iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(&wanted))
            .map(|(_, pin)| pin)
            .collect())
    }

    /// Pins `pin` for `host` on `port`; whether it was added: not when it is no
    /// fingerprint, or is there already. One writer of the trust files at a time.
    ///
    /// # Errors
    ///
    /// An unsafe host name, or a file that cannot be read or written.
    pub fn pin(&self, host: &str, port: u16, pin: &str) -> Result<bool, KnownHostsError> {
        self.pin_locked(&trust_files::lock(), host, port, pin)
    }

    /// [`Self::pin`], the lock of the trust files held by the caller.
    pub(crate) fn pin_locked(
        &self,
        _lock: &TrustLock,
        host: &str,
        port: u16,
        pin: &str,
    ) -> Result<bool, KnownHostsError> {
        if !is_fingerprint(pin) || self.pinned(host, port)?.iter().any(|known| known == pin) {
            return Ok(false);
        }
        let wanted = pattern(&validate_host(host)?, port);
        let mut lines = self.lines()?;
        lines.push((wanted, pin.to_owned()));
        self.write(&lines)?;
        Ok(true)
    }

    /// Drops the pins of `host` on `port`; whether there were any. One writer of the trust
    /// files at a time.
    ///
    /// # Errors
    ///
    /// An unsafe host name, or a file that cannot be read or written.
    pub fn unpin(&self, host: &str, port: u16) -> Result<bool, KnownHostsError> {
        self.unpin_locked(&trust_files::lock(), host, port)
    }

    /// [`Self::unpin`], the lock of the trust files held by the caller.
    pub(crate) fn unpin_locked(
        &self,
        _lock: &TrustLock,
        host: &str,
        port: u16,
    ) -> Result<bool, KnownHostsError> {
        let wanted = pattern(&validate_host(host)?, port);
        let lines = self.lines()?;
        let kept: Vec<_> = lines
            .iter()
            .filter(|(name, _)| !name.eq_ignore_ascii_case(&wanted))
            .cloned()
            .collect();
        if kept.len() == lines.len() {
            return Ok(false);
        }
        self.write(&kept)?;
        Ok(true)
    }

    /// Every pin: its host, port and fingerprint, in the order of the file.
    ///
    /// # Errors
    ///
    /// The file exists and cannot be read.
    pub fn all(&self) -> Result<Vec<(String, u16, String)>, KnownHostsError> {
        Ok(self
            .lines()?
            .into_iter()
            .filter_map(|(name, pin)| {
                let (host, port) = match name.strip_prefix('[') {
                    Some(bracketed) => {
                        let (host, port) = bracketed.split_once("]:")?;
                        (host.to_owned(), port.parse().ok()?)
                    }
                    None => (name, DEFAULT_SSH_PORT),
                };
                Some((host, port, pin))
            })
            .collect())
    }

    /// The lines that read: a host pattern and a fingerprint.
    fn lines(&self) -> Result<Vec<(String, String)>, KnownHostsError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(KnownHostsError::Unreadable {
                    path: self.path.clone(),
                    source,
                });
            }
        };
        Ok(text
            .lines()
            .filter_map(|line| {
                let (name, pin) = line.trim().split_once(char::is_whitespace)?;
                let pin = pin.trim();
                is_fingerprint(pin).then(|| (name.to_owned(), pin.to_owned()))
            })
            .collect())
    }

    /// Rewrites the file whole through a file beside it: a failed write leaves it as it
    /// was.
    fn write(&self, lines: &[(String, String)]) -> Result<(), KnownHostsError> {
        let mut text = String::new();
        for (name, pin) in lines {
            text.push_str(name);
            text.push(' ');
            text.push_str(pin);
            text.push('\n');
        }
        trust_files::replace(&self.path, &text).map_err(|_| KnownHostsError::WriteFailed {
            path: self.path.clone(),
        })
    }
}

/// Records in full a key whose fingerprint is pinned for `host` on `port`, and drops the
/// pin: from now on the server is checked against its whole key, as a connection does when
/// it first meets that key. A pin that cannot be dropped is said and stays: the key
/// recorded is the one checked, and the pin trusts that key and no other.
///
/// # Errors
///
/// The key cannot be recorded: the pin stays, and still trusts that key alone.
pub(crate) fn record_in_full(
    lock: &TrustLock,
    known_hosts: &KnownHosts,
    host: &str,
    port: u16,
    key: &PublicKey,
) -> Result<(), KnownHostsError> {
    known_hosts.learn_locked(lock, host, port, key)?;
    if let Err(error) = Pins::beside(known_hosts.path()).unpin_locked(lock, host, port) {
        log::warn!("the pin of a server recorded in full stays: {error}");
    }
    Ok(())
}

/// What carrying the C# trust over did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Carried {
    /// Keys recorded in full.
    pub keys: usize,
    /// Fingerprints pinned.
    pub pins: usize,
    /// Servers left as they were: already trusted here, or nothing usable in the entry.
    pub left: usize,
}

/// Carries the C# trust over into `known_hosts` and its pins. A server already trusted
/// here is left as it is: never replaced, never added to. A key kept in full is recorded
/// when its fingerprint is the one the C# trusted; otherwise the fingerprint is pinned.
/// Read and written under one lock of the trust files, no other writer between a server's
/// check and its write.
///
/// # Errors
///
/// The file or the pins cannot be read or written; what was carried before stays.
pub fn carry_over(
    known_hosts: &KnownHosts,
    trusted: &[TrustedHostKey],
) -> Result<Carried, KnownHostsError> {
    let pins = Pins::beside(known_hosts.path());
    let lock = trust_files::lock();
    let mut carried = Carried::default();
    for entry in trusted {
        let Ok(host) = validate_host(&entry.host) else {
            carried.left += 1;
            continue;
        };
        if !known_hosts.recorded(&host, entry.port)?.is_empty()
            || !pins.pinned(&host, entry.port)?.is_empty()
        {
            carried.left += 1;
            continue;
        }
        let key = entry
            .key
            .as_deref()
            .and_then(|encoded| data_encoding::BASE64.decode(encoded.trim().as_bytes()).ok())
            .and_then(|bytes| PublicKey::from_bytes(&bytes).ok())
            .filter(|key| fingerprint(key) == entry.fingerprint);
        if let Some(key) = key {
            known_hosts.learn_locked(&lock, &host, entry.port, &key)?;
            carried.keys += 1;
        } else if pins.pin_locked(&lock, &host, entry.port, &entry.fingerprint)? {
            carried.pins += 1;
        } else {
            carried.left += 1;
        }
    }
    Ok(carried)
}

/// `host` on `port` as `known_hosts` writes it: `host`, or `[host]:port`.
fn pattern(host: &str, port: u16) -> String {
    if port == DEFAULT_SSH_PORT {
        host.to_owned()
    } else {
        format!("[{host}]:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = include_str!("../tests/fixtures/hostkeys/host-ed25519.pub");
    const OTHER: &str = include_str!("../tests/fixtures/hostkeys/host-ed25519-other.pub");

    fn key(text: &str) -> PublicKey {
        PublicKey::from_openssh(text.trim()).expect("key")
    }

    fn wire(key: &PublicKey) -> String {
        data_encoding::BASE64.encode(&key.to_bytes().expect("encoded"))
    }

    fn trusted(host: &str, port: u16, fingerprint: &str, key: Option<String>) -> TrustedHostKey {
        TrustedHostKey {
            host: host.to_owned(),
            port,
            fingerprint: fingerprint.to_owned(),
            key,
        }
    }

    #[test]
    fn a_fingerprint_is_sha256_and_43_base64_characters() {
        assert!(is_fingerprint(&fingerprint(&key(KEY))));
        for refused in [
            "",
            "SHA256:",
            "MD5:aa:bb",
            "SHA256:short",
            "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ] {
            assert!(!is_fingerprint(refused), "{refused:?}");
        }
    }

    #[test]
    fn pins_are_kept_per_server_and_dropped_with_it() {
        let dir = tempfile::tempdir().expect("dir");
        let pins = Pins::beside(&dir.path().join("known_hosts"));
        let print = fingerprint(&key(KEY));
        assert!(
            pins.pinned("web.lab", 22).expect("read").is_empty(),
            "no file"
        );
        assert!(pins.pin("Web.Lab", 22, &print).expect("pinned"));
        assert!(!pins.pin("web.lab", 22, &print).expect("pinned"), "once");
        assert!(
            !pins
                .pin("web.lab", 22, "not a fingerprint")
                .expect("refused")
        );
        assert!(pins.pin("web.lab", 2222, &print).expect("pinned"));
        assert_eq!(
            pins.pinned("web.lab", 22).expect("read"),
            std::slice::from_ref(&print)
        );
        assert_eq!(
            pins.all().expect("all"),
            [
                ("web.lab".to_owned(), 22, print.clone()),
                ("web.lab".to_owned(), 2222, print.clone())
            ]
        );
        assert!(pins.unpin("web.lab", 22).expect("unpinned"));
        assert!(
            !pins.unpin("web.lab", 22).expect("unpinned"),
            "gone already"
        );
        assert_eq!(pins.pinned("web.lab", 2222).expect("read"), [print]);
        assert!(pins.pin("bad host", 22, &fingerprint(&key(KEY))).is_err());
    }

    #[test]
    fn a_key_matches_its_pin_and_differs_from_another() {
        let pinned = [fingerprint(&key(OTHER))];
        assert_eq!(pin_verdict(&[], &key(KEY)), PinVerdict::None);
        assert_eq!(pin_verdict(&pinned, &key(OTHER)), PinVerdict::Matches);
        assert_eq!(
            pin_verdict(&pinned, &key(KEY)),
            PinVerdict::Differs {
                pinned: pinned[0].clone()
            }
        );
    }

    #[test]
    fn the_csharp_trust_is_carried_over_without_replacing_anything() {
        let dir = tempfile::tempdir().expect("dir");
        let known = KnownHosts::new(dir.path().join("known_hosts"));
        let pins = Pins::beside(known.path());
        known.learn("mine.lab", 22, &key(OTHER)).expect("own entry");
        let print = fingerprint(&key(KEY));
        let carried = carry_over(
            &known,
            &[
                // The key kept, and its fingerprint the one trusted: recorded.
                trusted("full.lab", 22, &print, Some(wire(&key(KEY)))),
                // The key kept is not the one trusted: the fingerprint is what counts.
                trusted("odd.lab", 22, &print, Some(wire(&key(OTHER)))),
                // A fingerprint alone, on another port: pinned.
                trusted("bare.lab", 2222, &print, None),
                // Already trusted here: left as it is.
                trusted("mine.lab", 22, &print, Some(wire(&key(KEY)))),
                // Nothing usable.
                trusted("junk.lab", 22, "MD5:aa:bb", Some("%%".to_owned())),
                trusted("bad host", 22, &print, None),
            ],
        )
        .expect("carried");
        assert_eq!(
            carried,
            Carried {
                keys: 1,
                pins: 2,
                left: 3
            }
        );
        let data = |keys: Vec<PublicKey>| -> Vec<_> {
            keys.iter().map(|key| key.key_data().clone()).collect()
        };
        assert_eq!(
            data(known.recorded("full.lab", 22).expect("read")),
            [key(KEY).key_data().clone()]
        );
        assert!(known.recorded("odd.lab", 22).expect("read").is_empty());
        assert_eq!(
            pins.pinned("odd.lab", 22).expect("read"),
            std::slice::from_ref(&print)
        );
        assert_eq!(pins.pinned("bare.lab", 2222).expect("read"), [print]);
        assert_eq!(
            data(known.recorded("mine.lab", 22).expect("read")),
            [key(OTHER).key_data().clone()],
            "never replaced"
        );
        let again =
            carry_over(&known, &[trusted("bare.lab", 2222, "SHA256:x", None)]).expect("carried");
        assert_eq!(again.left, 1, "a pinned server is left as it is");

        let listed = known.entries().expect("listed");
        assert!(
            listed
                .iter()
                .any(|entry| entry.host == "bare.lab" && entry.algorithm.is_empty()),
            "pins are listed: {listed:?}"
        );
    }
}
