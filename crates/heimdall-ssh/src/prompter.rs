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

//! Questions the library asks a human during authentication.
//!
//! The host key question is not here on purpose: it is asked between two connection
//! attempts, never while a key exchange is waiting. See [`crate::ConnectError::UnknownHostKey`].

use std::future::Future;
use std::path::PathBuf;

use crate::secret::Secret;

/// Asked when the profile has no user name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsernameQuestion {
    /// Host being connected to.
    pub host: String,
    /// Its port.
    pub port: u16,
}

/// Asked for password authentication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordQuestion {
    /// Host being connected to.
    pub host: String,
    /// Its port.
    pub port: u16,
    /// Account the password is for.
    pub username: String,
    /// 1 for the first question, 2 after one wrong password, and so on.
    pub attempt: u32,
}

/// Asked when a key file is encrypted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassphraseQuestion {
    /// The key file.
    pub key_path: PathBuf,
    /// 1 for the first question, 2 after one wrong passphrase, and so on.
    pub attempt: u32,
}

/// One prompt of a keyboard-interactive round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardInteractivePrompt {
    /// Text the server shows, for example `Verification code: `.
    pub text: String,
    /// Whether the answer may be shown while typed.
    pub echo: bool,
}

/// A keyboard-interactive round with at least one prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardInteractiveQuestion {
    /// Host being connected to.
    pub host: String,
    /// Account being authenticated.
    pub username: String,
    /// Title sent by the server, often empty.
    pub name: String,
    /// Instructions sent by the server, often empty.
    pub instructions: String,
    /// Prompts, answered in order.
    pub prompts: Vec<KeyboardInteractivePrompt>,
}

/// Asks the user. Returning `None` means the user cancelled, which ends the connection.
///
/// Every question is raced against the connection's cancellation and a deadline, so an
/// implementation may simply wait for the user.
pub trait Prompter: Send + Sync + 'static {
    /// User name for a profile that has none.
    fn username(&self, question: UsernameQuestion) -> impl Future<Output = Option<String>> + Send;

    /// Password of an account.
    fn password(&self, question: PasswordQuestion) -> impl Future<Output = Option<Secret>> + Send;

    /// Passphrase of an encrypted key file.
    fn key_passphrase(
        &self,
        question: PassphraseQuestion,
    ) -> impl Future<Output = Option<Secret>> + Send;

    /// Answers to a keyboard-interactive round, one per prompt, in order.
    fn keyboard_interactive(
        &self,
        question: KeyboardInteractiveQuestion,
    ) -> impl Future<Output = Option<Vec<Secret>>> + Send;
}
