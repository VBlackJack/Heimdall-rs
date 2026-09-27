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

//! A file of named secrets sealed with a password.
//!
//! - The password gives a key-encryption key through Argon2id; its cost is written in the
//!   file, and a file asking for less than [`KdfParams::FLOOR`] or more than
//!   [`KdfParams::CEILING`] is refused before the derivation runs.
//! - A random data key, wrapped by that key with AES-256-GCM, seals the whole body with
//!   AES-256-GCM: names, values and their number are hidden, and no entry can be moved or
//!   swapped without breaking the seal. Changing the password wraps the same data key again.
//! - Every header byte is authenticated, each use under its own context string, and every
//!   encryption takes a fresh random nonce.
//! - Whatever is wrong (the password, a changed byte, a cut file) is one [`VaultError::Unreadable`]:
//!   nothing tells which, and nothing is ever read as an empty vault.
//!
//! What it does not do: anyone who can copy the file can try passwords offline, at the
//! Argon2id cost, as often as they like; and without `unsafe`, keys and secrets can reach
//! swap or a crash dump while the vault is open. Keep it open briefly.

mod body;
mod format;

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Aead, KeyInit, Nonce, Payload};
use zeroize::Zeroizing;

use crate::format::{Header, KEY_LEN, NONCE_LEN, SALT_LEN};

pub use crate::format::KdfParams;

/// Context of the data key's wrapping, so no other sealing can pass for it.
const WRAP_CONTEXT: &[u8] = b"sealvault v1 data key";

/// Context of the body's sealing.
const BODY_CONTEXT: &[u8] = b"sealvault v1 body";

/// Extension added to the vault's file name for the copy kept before each save.
const BACKUP_SUFFIX: &str = ".bak";

/// Why a vault could not be used.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    /// The file could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// File or folder concerned.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// A vault is already there: creating one would replace it.
    #[error("{0}: a vault is already there")]
    AlreadyExists(PathBuf),
    /// Wrong password, or a file that is not a vault, has been changed, or was cut short:
    /// deliberately one answer.
    #[error("the vault cannot be opened with this password")]
    Unreadable,
    /// The operating system gave no random bytes.
    #[error("no random bytes: {0}")]
    Randomness(String),
}

/// An open vault: its secrets in memory, sealed again on [`Vault::save`].
pub struct Vault {
    path: PathBuf,
    params: KdfParams,
    salt: [u8; SALT_LEN],
    wrap_nonce: [u8; NONCE_LEN],
    wrapped_key: Vec<u8>,
    data_key: Zeroizing<[u8; KEY_LEN]>,
    entries: BTreeMap<String, Zeroizing<Vec<u8>>>,
}

impl fmt::Debug for Vault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Neither keys, nor values, nor names.
        f.debug_struct("Vault")
            .field("path", &self.path)
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl Vault {
    /// Creates an empty vault at `path`, sealed with `password`, and saves it.
    ///
    /// # Errors
    ///
    /// [`VaultError::AlreadyExists`] when a file is there, [`VaultError::Io`] when it cannot
    /// be written, [`VaultError::Randomness`].
    pub fn create(path: impl Into<PathBuf>, password: &[u8]) -> Result<Self, VaultError> {
        Self::create_with(path.into(), password, KdfParams::FLOOR)
    }

    fn create_with(path: PathBuf, password: &[u8], params: KdfParams) -> Result<Self, VaultError> {
        if path.exists() {
            return Err(VaultError::AlreadyExists(path));
        }
        let mut data_key = Zeroizing::new([0; KEY_LEN]);
        random(data_key.as_mut_slice())?;
        let mut vault = Self {
            path,
            params,
            salt: [0; SALT_LEN],
            wrap_nonce: [0; NONCE_LEN],
            wrapped_key: Vec::new(),
            data_key,
            entries: BTreeMap::new(),
        };
        vault.wrap(password)?;
        vault.save()?;
        Ok(vault)
    }

    /// Opens the vault at `path` with `password`.
    ///
    /// # Errors
    ///
    /// [`VaultError::Unreadable`] for a wrong password or a file that is not an intact vault,
    /// [`VaultError::Io`] when it cannot be read.
    pub fn open(path: impl Into<PathBuf>, password: &[u8]) -> Result<Self, VaultError> {
        let path = path.into();
        let bytes = fs::read(&path).map_err(|source| VaultError::Io {
            path: path.clone(),
            source,
        })?;
        let (header, sealed_body) = Header::parse(&bytes).ok_or(VaultError::Unreadable)?;
        // Before the derivation: a file must not choose a weak one, nor one that exhausts.
        if !header.params.is_acceptable() {
            return Err(VaultError::Unreadable);
        }
        let wrapping_key = derive(password, &header.params, &header.salt)?;
        let data_key = open_sealed(
            &wrapping_key,
            &header.wrap_nonce,
            &header.wrapped_key,
            &[
                WRAP_CONTEXT,
                &Header::wrap_authenticated_for(&header.params, &header.salt, &header.wrap_nonce),
            ]
            .concat(),
        )?;
        let data_key: Zeroizing<[u8; KEY_LEN]> = Zeroizing::new(
            data_key
                .as_slice()
                .try_into()
                .map_err(|_| VaultError::Unreadable)?,
        );
        let plain = open_sealed(
            &data_key,
            &header.body_nonce,
            sealed_body,
            &[BODY_CONTEXT, &header.to_bytes()].concat(),
        )?;
        let entries = body::decode(&plain).ok_or(VaultError::Unreadable)?;
        Ok(Self {
            path,
            params: header.params,
            salt: header.salt,
            wrap_nonce: header.wrap_nonce,
            wrapped_key: header.wrapped_key,
            data_key,
            entries,
        })
    }

    /// The secret named `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.entries.get(name).map(|value| value.as_slice())
    }

    /// Sets the secret named `name`; saved on [`Vault::save`].
    pub fn set(&mut self, name: impl Into<String>, secret: impl Into<Vec<u8>>) {
        self.entries
            .insert(name.into(), Zeroizing::new(secret.into()));
    }

    /// Removes the secret named `name`; whether it was there.
    pub fn remove(&mut self, name: &str) -> bool {
        self.entries.remove(name).is_some()
    }

    /// Names of the secrets, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Seals the vault again with `password`: a new salt and a new wrapping of the same data
    /// key, saved at once. The old password no longer opens it.
    ///
    /// # Errors
    ///
    /// [`VaultError::Io`], [`VaultError::Randomness`]; the file is then unchanged.
    pub fn change_password(&mut self, password: &[u8]) -> Result<(), VaultError> {
        let before = (self.salt, self.wrap_nonce, self.wrapped_key.clone());
        self.wrap(password)?;
        if let Err(error) = self.save() {
            (self.salt, self.wrap_nonce, self.wrapped_key) = before;
            return Err(error);
        }
        Ok(())
    }

    /// Writes the vault, sealed under a fresh nonce. The file is replaced whole, never left
    /// half written, and the previous one is kept beside it with `.bak` added to its name.
    ///
    /// # Errors
    ///
    /// [`VaultError::Io`], [`VaultError::Randomness`].
    pub fn save(&self) -> Result<(), VaultError> {
        let mut body_nonce = [0; NONCE_LEN];
        random(&mut body_nonce)?;
        let header = Header {
            params: self.params,
            salt: self.salt,
            wrap_nonce: self.wrap_nonce,
            wrapped_key: self.wrapped_key.clone(),
            body_nonce,
        };
        let header_bytes = header.to_bytes();
        let plain = body::encode(&self.entries);
        let sealed = seal(
            &self.data_key,
            &body_nonce,
            &plain,
            &[BODY_CONTEXT, &header_bytes].concat(),
        )?;
        self.write(&[header_bytes.as_slice(), &sealed].concat())
    }

    /// A new salt, a key derived from `password` with it, and the data key wrapped by it.
    fn wrap(&mut self, password: &[u8]) -> Result<(), VaultError> {
        let mut salt = [0; SALT_LEN];
        random(&mut salt)?;
        let mut wrap_nonce = [0; NONCE_LEN];
        random(&mut wrap_nonce)?;
        let wrapping_key = derive(password, &self.params, &salt)?;
        let authenticated = Header::wrap_authenticated_for(&self.params, &salt, &wrap_nonce);
        self.wrapped_key = seal(
            &wrapping_key,
            &wrap_nonce,
            self.data_key.as_slice(),
            &[WRAP_CONTEXT, &authenticated].concat(),
        )?;
        self.salt = salt;
        self.wrap_nonce = wrap_nonce;
        Ok(())
    }

    fn write(&self, bytes: &[u8]) -> Result<(), VaultError> {
        let io_error = |path: &Path| {
            let path = path.to_owned();
            move |source| VaultError::Io { path, source }
        };
        let dir = self
            .path
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(dir).map_err(io_error(dir))?;
        if self.path.exists() {
            let backup = backup_path(&self.path);
            fs::copy(&self.path, &backup).map_err(io_error(&backup))?;
        }
        let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(io_error(dir))?;
        temp.write_all(bytes).map_err(io_error(temp.path()))?;
        temp.as_file().sync_all().map_err(io_error(temp.path()))?;
        temp.persist(&self.path)
            .map_err(|error| io_error(&self.path)(error.error))?;
        Ok(())
    }
}

/// Where the copy of the previous file goes.
#[must_use]
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(BACKUP_SUFFIX);
    path.with_file_name(name)
}

fn random(buffer: &mut [u8]) -> Result<(), VaultError> {
    getrandom::fill(buffer).map_err(|error| VaultError::Randomness(error.to_string()))
}

/// The key `password` gives under `params` and `salt`.
fn derive(
    password: &[u8],
    params: &KdfParams,
    salt: &[u8],
) -> Result<Zeroizing<[u8; KEY_LEN]>, VaultError> {
    let argon = argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(
            params.memory_kib,
            params.iterations,
            params.lanes,
            Some(KEY_LEN),
        )
        .map_err(|_| VaultError::Unreadable)?,
    );
    let mut key = Zeroizing::new([0; KEY_LEN]);
    argon
        .hash_password_into(password, salt, key.as_mut_slice())
        .map_err(|_| VaultError::Unreadable)?;
    Ok(key)
}

fn seal(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    plain: &[u8],
    authenticated: &[u8],
) -> Result<Vec<u8>, VaultError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| VaultError::Unreadable)?;
    cipher
        .encrypt(
            &Nonce::<Aes256Gcm>::from(*nonce),
            Payload {
                msg: plain,
                aad: authenticated,
            },
        )
        .map_err(|_| VaultError::Unreadable)
}

fn open_sealed(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    sealed: &[u8],
    authenticated: &[u8],
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| VaultError::Unreadable)?;
    cipher
        .decrypt(
            &Nonce::<Aes256Gcm>::from(*nonce),
            Payload {
                msg: sealed,
                aad: authenticated,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| VaultError::Unreadable)
}

#[cfg(test)]
mod tests {
    use super::{KdfParams, Vault, VaultError};

    #[test]
    fn a_file_asking_for_a_weak_derivation_is_refused() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("weak.svlt");
        let weak = KdfParams {
            memory_kib: 1024,
            iterations: 1,
            lanes: 1,
        };
        Vault::create_with(path.clone(), b"pw", weak).expect("written");
        assert!(matches!(
            Vault::open(&path, b"pw"),
            Err(VaultError::Unreadable)
        ));
    }
}
