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

//! Private key files: OpenSSH, `PuTTY` `.ppk` (versions 2 and 3), and PEM.
//!
//! Whether a file is encrypted is read from its text, per format, because russh reports it
//! differently for each: `KeyIsEncrypted` for OpenSSH, a wrapped `ssh-key` error for `.ppk`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use data_encoding::BASE64;
use russh::keys::ssh_key;
use russh::keys::{PrivateKey, PublicKey, decode_secret_key};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::secret::Secret;

const PPK_HEADER: &str = "PuTTY-User-Key-File-";
const PPK_ENCRYPTION_FIELD: &str = "Encryption:";
const PPK_UNENCRYPTED: &str = "none";
const PPK_PUBLIC_LINES_FIELD: &str = "Public-Lines:";
const OPENSSH_HEADER: &str = "-----BEGIN OPENSSH PRIVATE KEY-----";
const PEM_HEADER_PREFIX: &str = "-----BEGIN ";
const PEM_ENCRYPTED_MARKERS: [&str; 2] = ["Proc-Type: 4,ENCRYPTED", "BEGIN ENCRYPTED PRIVATE KEY"];

/// Format of a key file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFormat {
    /// `-----BEGIN OPENSSH PRIVATE KEY-----`.
    OpenSsh,
    /// `PuTTY-User-Key-File-2` or `-3`.
    Ppk,
    /// PKCS#1 or PKCS#8 PEM.
    Pem,
}

/// Why a key file could not be used.
#[derive(Debug, Error)]
pub enum KeyFileError {
    /// The file could not be read.
    #[error("{path}: {source}")]
    Io {
        /// File concerned.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// The file is not a private key in a known format.
    #[error("{path}: not a supported private key")]
    UnknownFormat {
        /// File concerned.
        path: PathBuf,
    },
    /// The key is encrypted and no passphrase was given.
    #[error("{path}: passphrase required")]
    NeedsPassphrase {
        /// File concerned.
        path: PathBuf,
    },
    /// The passphrase did not decrypt the key. A corrupt encrypted file reads the same way.
    #[error("{path}: wrong passphrase")]
    WrongPassphrase {
        /// File concerned.
        path: PathBuf,
    },
    /// The key is not encrypted and still did not load.
    #[error("{path}: {reason}")]
    Invalid {
        /// File concerned.
        path: PathBuf,
        /// What the decoder reported.
        reason: String,
    },
}

/// A private key file, read but not yet decrypted.
pub struct KeyFile {
    path: PathBuf,
    text: Zeroizing<String>,
    format: KeyFormat,
    encrypted: bool,
    public_key: Option<PublicKey>,
}

impl std::fmt::Debug for KeyFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyFile")
            .field("path", &self.path)
            .field("format", &self.format)
            .field("encrypted", &self.encrypted)
            .finish_non_exhaustive()
    }
}

impl KeyFile {
    /// Reads `path` and identifies its format, without decrypting it.
    ///
    /// # Errors
    ///
    /// Returns [`KeyFileError::Io`] or [`KeyFileError::UnknownFormat`].
    pub fn read(path: impl Into<PathBuf>) -> Result<Self, KeyFileError> {
        let path = path.into();
        let text =
            Zeroizing::new(
                fs::read_to_string(&path).map_err(|source| KeyFileError::Io {
                    path: path.clone(),
                    source,
                })?,
            );
        let trimmed = text.trim_start();
        let (format, encrypted, public_key) = if trimmed.starts_with(PPK_HEADER) {
            (
                KeyFormat::Ppk,
                ppk_is_encrypted(&text),
                ppk_public_key(&text),
            )
        } else if trimmed.starts_with(OPENSSH_HEADER) {
            let parsed = ssh_key::PrivateKey::from_openssh(text.as_bytes())
                .map_err(|_| KeyFileError::UnknownFormat { path: path.clone() })?;
            (
                KeyFormat::OpenSsh,
                parsed.is_encrypted(),
                Some(parsed.public_key().clone()),
            )
        } else if trimmed.starts_with(PEM_HEADER_PREFIX) {
            let encrypted = PEM_ENCRYPTED_MARKERS
                .iter()
                .any(|marker| text.contains(marker));
            (KeyFormat::Pem, encrypted, None)
        } else {
            return Err(KeyFileError::UnknownFormat { path });
        };
        Ok(Self {
            path,
            text,
            format,
            encrypted,
            public_key,
        })
    }

    /// File read.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Detected format.
    #[must_use]
    pub fn format(&self) -> KeyFormat {
        self.format
    }

    /// Whether a passphrase is needed.
    #[must_use]
    pub fn is_encrypted(&self) -> bool {
        self.encrypted
    }

    /// Public key, when the format exposes it without decryption (OpenSSH and `.ppk`).
    #[must_use]
    pub fn public_key(&self) -> Option<&PublicKey> {
        self.public_key.as_ref()
    }

    /// Decrypts the key.
    ///
    /// # Errors
    ///
    /// [`KeyFileError::NeedsPassphrase`] when encrypted and `passphrase` is `None`,
    /// [`KeyFileError::WrongPassphrase`] when it does not decrypt, and
    /// [`KeyFileError::Invalid`] when an unencrypted key does not load.
    pub fn decrypt(&self, passphrase: Option<&Secret>) -> Result<PrivateKey, KeyFileError> {
        if self.encrypted && passphrase.is_none() {
            return Err(KeyFileError::NeedsPassphrase {
                path: self.path.clone(),
            });
        }
        let passphrase = if self.encrypted {
            passphrase.map(Secret::expose)
        } else {
            None
        };
        decode_secret_key(&self.text, passphrase).map_err(|error| {
            if self.encrypted {
                KeyFileError::WrongPassphrase {
                    path: self.path.clone(),
                }
            } else {
                KeyFileError::Invalid {
                    path: self.path.clone(),
                    reason: error.to_string(),
                }
            }
        })
    }
}

fn ppk_field<'a>(text: &'a str, field: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|line| line.strip_prefix(field))
        .map(str::trim)
}

fn ppk_is_encrypted(text: &str) -> bool {
    // An absent field is treated as encrypted: asking for a passphrase that turns out to
    // be unneeded is harmless, loading a key as unencrypted when it is not is an error.
    ppk_field(text, PPK_ENCRYPTION_FIELD).is_none_or(|value| value != PPK_UNENCRYPTED)
}

fn ppk_public_key(text: &str) -> Option<PublicKey> {
    let mut lines = text.lines();
    let count: usize = lines
        .by_ref()
        .find_map(|line| line.strip_prefix(PPK_PUBLIC_LINES_FIELD))?
        .trim()
        .parse()
        .ok()?;
    let encoded: String = lines.take(count).map(str::trim).collect();
    let blob = BASE64.decode(encoded.as_bytes()).ok()?;
    PublicKey::from_bytes(&blob).ok()
}
