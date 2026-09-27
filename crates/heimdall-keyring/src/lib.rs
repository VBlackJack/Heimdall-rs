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

//! Named secrets in the operating system's own credential store, protected by the user's
//! session: Windows Credential Manager (DPAPI) on Windows, the Secret Service (GNOME
//! Keyring, `KWallet`) on Linux.
//!
//! This is where saved passwords go when no master password is set, as the C# Heimdall keeps
//! them under DPAPI. Anyone running as the same user can read them back: the protection is
//! against other users and against the file being copied elsewhere, not against the user's
//! own programs.

use std::sync::OnceLock;

use keyring_core::{Entry, Error};
use zeroize::Zeroizing;

/// Why the system store could not be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyringError {
    /// There is no store on this system, or it cannot be reached (no Secret Service running,
    /// a locked keyring the user declined to open).
    #[error("the system credential store is not available: {0}")]
    Unavailable(String),
    /// The store refused or failed the operation.
    #[error("the system credential store failed: {0}")]
    Failed(String),
}

/// The secrets of one application in the system store.
#[derive(Debug, Clone)]
pub struct SystemKeyring {
    service: String,
}

impl SystemKeyring {
    /// The secrets filed under `service`, the application's name in the store.
    ///
    /// # Errors
    ///
    /// [`KeyringError::Unavailable`] when this system has no store to use.
    pub fn open(service: &str) -> Result<Self, KeyringError> {
        default_store()?;
        Ok(Self {
            service: service.to_owned(),
        })
    }

    /// The secret named `name`, if there is one.
    ///
    /// # Errors
    ///
    /// [`KeyringError`].
    pub fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>, KeyringError> {
        match self.entry(name)?.get_secret() {
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(Error::NoEntry) => Ok(None),
            Err(error) => Err(failure(&error)),
        }
    }

    /// Sets the secret named `name`, replacing any.
    ///
    /// # Errors
    ///
    /// [`KeyringError`].
    pub fn set(&self, name: &str, secret: &[u8]) -> Result<(), KeyringError> {
        self.entry(name)?
            .set_secret(secret)
            .map_err(|error| failure(&error))
    }

    /// Removes the secret named `name`; whether there was one.
    ///
    /// # Errors
    ///
    /// [`KeyringError`].
    pub fn remove(&self, name: &str) -> Result<bool, KeyringError> {
        match self.entry(name)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(Error::NoEntry) => Ok(false),
            Err(error) => Err(failure(&error)),
        }
    }

    fn entry(&self, name: &str) -> Result<Entry, KeyringError> {
        Entry::new(&self.service, name).map_err(|error| failure(&error))
    }
}

fn failure(error: &Error) -> KeyringError {
    match error {
        Error::NoStorageAccess(_) | Error::NoDefaultStore => {
            KeyringError::Unavailable(error.to_string())
        }
        other => KeyringError::Failed(other.to_string()),
    }
}

/// Sets this platform's store as the default, once for the process.
fn default_store() -> Result<(), KeyringError> {
    static STORE: OnceLock<Result<(), KeyringError>> = OnceLock::new();
    STORE.get_or_init(platform_store).clone()
}

#[cfg(windows)]
fn platform_store() -> Result<(), KeyringError> {
    let store = windows_native_keyring_store::Store::new().map_err(|error| failure(&error))?;
    keyring_core::set_default_store(store);
    Ok(())
}

#[cfg(target_os = "linux")]
fn platform_store() -> Result<(), KeyringError> {
    // No Secret Service running (a server, WSL) is no store, not a failure.
    let store = zbus_secret_service_keyring_store::Store::new()
        .map_err(|error| KeyringError::Unavailable(error.to_string()))?;
    keyring_core::set_default_store(store);
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn platform_store() -> Result<(), KeyringError> {
    Err(KeyringError::Unavailable(
        "no credential store is supported on this system".to_owned(),
    ))
}
