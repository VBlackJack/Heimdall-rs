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

//! The external credential provider's settings, as the C# Settings page's card: each change
//! saved at once, the unlock secret kept with the saved passwords, and the Test button.

use std::time::Duration;

use heimdall_core::credential_provider::{MAX_TIMEOUT, MIN_TIMEOUT, PRESETS, ProviderKind};
use heimdall_ssh::Secret;

use super::{App, Dialog, Effect};
use crate::credential_provider::ProviderTest;

/// Where the unlock secret is kept among the saved passwords: a namespace of its own, apart
/// from the profiles' `password/` entries.
pub const UNLOCK_SECRET_ENTRY: &str = "provider/unlock";

/// A change to the provider's settings.
#[derive(Clone)]
pub enum ProviderMessage {
    /// Asked at all.
    Enabled(bool),
    /// Where the password comes from.
    Kind(ProviderKind),
    /// The password command.
    Command(String),
    /// The user name command.
    UsernameCommand(String),
    /// The password database.
    Database(String),
    /// The database's key file.
    KeyFile(String),
    /// Only the first line of the output.
    FirstLineOnly(bool),
    /// How long a command is given; refused out of the C# range.
    Timeout(Duration),
    /// The command of the preset at this place in [`PRESETS`].
    Preset(usize),
    /// Keep this unlock secret.
    SaveUnlockSecret(Secret),
    /// Forget the unlock secret.
    ForgetUnlockSecret,
    /// Run the password command with test values.
    Test,
    /// The test ended.
    Tested(ProviderTest),
}

impl std::fmt::Debug for ProviderMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Enabled(on) => write!(f, "Enabled({on})"),
            Self::Kind(kind) => write!(f, "Kind({kind:?})"),
            // Commands and paths may name a vault: kept out of the logs.
            Self::Command(_) => f.write_str("Command(..)"),
            Self::UsernameCommand(_) => f.write_str("UsernameCommand(..)"),
            Self::Database(_) => f.write_str("Database(..)"),
            Self::KeyFile(_) => f.write_str("KeyFile(..)"),
            Self::FirstLineOnly(on) => write!(f, "FirstLineOnly({on})"),
            Self::Timeout(timeout) => write!(f, "Timeout({timeout:?})"),
            Self::Preset(index) => write!(f, "Preset({index})"),
            Self::SaveUnlockSecret(_) => f.write_str("SaveUnlockSecret(..)"),
            Self::ForgetUnlockSecret => f.write_str("ForgetUnlockSecret"),
            Self::Test => f.write_str("Test"),
            Self::Tested(outcome) => write!(f, "Tested({outcome:?})"),
        }
    }
}

impl App {
    /// What the last Test found, while the page shows it.
    #[must_use]
    pub fn provider_test(&self) -> Option<&ProviderTest> {
        self.provider_test.as_ref()
    }

    /// Whether an unlock secret is kept.
    #[must_use]
    pub fn provider_unlock_saved(&self) -> bool {
        self.vault.read(UNLOCK_SECRET_ENTRY).is_some()
    }

    /// The unlock secret kept, when it can be read now.
    pub(super) fn provider_unlock_secret(&self) -> Option<Secret> {
        let bytes = self.vault.read(UNLOCK_SECRET_ENTRY)?;
        let text = std::str::from_utf8(&bytes).ok()?;
        Some(Secret::new(text.to_owned()))
    }

    /// Applies a change to the provider's settings.
    pub(super) fn provider_message(&mut self, message: ProviderMessage) -> Vec<Effect> {
        let provider = &mut self.settings.credential_provider;
        let before = provider.clone();
        match message {
            ProviderMessage::Enabled(on) => provider.enabled = on,
            ProviderMessage::Kind(kind) => provider.kind = kind,
            ProviderMessage::Command(command) => provider.command = command,
            ProviderMessage::UsernameCommand(command) => provider.username_command = command,
            ProviderMessage::Database(path) => provider.database = path,
            ProviderMessage::KeyFile(path) => provider.key_file = path,
            ProviderMessage::FirstLineOnly(on) => provider.first_line_only = on,
            ProviderMessage::Timeout(timeout) => {
                if !(MIN_TIMEOUT..=MAX_TIMEOUT).contains(&timeout) {
                    return Vec::new();
                }
                provider.timeout = timeout;
            }
            ProviderMessage::Preset(index) => {
                let Some((_, template)) = PRESETS.get(index) else {
                    return Vec::new();
                };
                (*template).clone_into(&mut provider.command);
            }
            ProviderMessage::SaveUnlockSecret(secret) => {
                let bytes = (!secret.expose().is_empty()).then(|| secret.expose().as_bytes());
                self.write_unlock_secret(bytes);
                return Vec::new();
            }
            ProviderMessage::ForgetUnlockSecret => {
                self.write_unlock_secret(None);
                return Vec::new();
            }
            ProviderMessage::Test => return self.test_provider(),
            ProviderMessage::Tested(outcome) => {
                // A test started before a change says nothing of what the page shows now.
                if self.provider_test == Some(ProviderTest::Running) {
                    self.provider_test = Some(outcome);
                }
                return Vec::new();
            }
        }
        if *provider == before {
            return Vec::new();
        }
        self.provider_test = None;
        if let Err(error) = self.settings.save(&self.settings_file) {
            self.settings.credential_provider = before;
            self.dialog = Some(Dialog::StoreError {
                detail: error.to_string(),
            });
        }
        Vec::new()
    }

    /// Keeps the unlock secret, or forgets it; a failure is said as for a password.
    fn write_unlock_secret(&mut self, bytes: Option<&[u8]>) {
        if let Err(detail) = self.vault.write(UNLOCK_SECRET_ENTRY, bytes) {
            log::warn!("the unlock secret could not be saved: {detail}");
            self.dialog = Some(Dialog::PasswordSaveFailed { detail });
        }
    }

    /// Runs the password command with the Test button's values, the settings as they are.
    fn test_provider(&mut self) -> Vec<Effect> {
        if self.provider_test == Some(ProviderTest::Running) {
            return Vec::new();
        }
        self.provider_test = Some(ProviderTest::Running);
        vec![Effect::TestCredentialProvider {
            settings: self.settings.credential_provider.clone(),
            unlock: self.provider_unlock_secret(),
        }]
    }
}
