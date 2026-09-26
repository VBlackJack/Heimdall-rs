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

//! Authentication, in this order:
//!
//! 1. `none`, to learn the allowed methods;
//! 2. the profile's key file: signed by the agent when the agent holds that key, otherwise
//!    decrypted locally, so a wrong passphrase costs no server attempt;
//! 3. other agent keys, only when the profile names no key, and at most
//!    [`MAX_OTHER_AGENT_KEYS`], so they do not exhaust the server's `MaxAuthTries`;
//! 4. keyboard-interactive; once started it is never abandoned for another method, because
//!    russh waits for the round's answer and ignores anything else;
//! 5. password.
//!
//! After every refusal the connection is checked: a server that disconnected reports
//! "no methods left", which must not read as "authentication failed".

use std::path::Path;
use std::sync::Arc;

use russh::client::{Handle, KeyboardInteractiveAuthResponse};
use russh::keys::{Algorithm, HashAlg, PrivateKeyWithHashAlg, PublicKey};
use russh::{Disconnect, MethodKind, MethodSet};
use tokio_util::sync::CancellationToken;

use crate::agent::{self, Agent};
use crate::client::{ClientHandler, ServerMessage, ask};
use crate::error::{AuthMethod, ConnectError};
use crate::key_file::{KeyFile, KeyFileError};
use crate::options::ConnectOptions;
use crate::prompter::{
    KeyboardInteractivePrompt, KeyboardInteractiveQuestion, PassphraseQuestion, PasswordQuestion,
    Prompter,
};
use crate::secret::Secret;

/// Agent keys tried when the profile names no key file.
pub(crate) const MAX_OTHER_AGENT_KEYS: usize = 3;

/// Password questions before giving up.
pub(crate) const MAX_PASSWORD_ATTEMPTS: u32 = 3;

/// Passphrase questions before giving up.
pub(crate) const MAX_PASSPHRASE_ATTEMPTS: u32 = 3;

/// Description sent with a disconnect initiated by Heimdall-rs.
const DISCONNECT_DESCRIPTION: &str = "";

/// Language tag sent with a disconnect initiated by Heimdall-rs.
const DISCONNECT_LANGUAGE: &str = "";

pub(crate) struct AuthContext<'a, P: Prompter> {
    pub(crate) handle: &'a mut Handle<ClientHandler>,
    pub(crate) prompter: &'a P,
    pub(crate) host: &'a str,
    pub(crate) port: u16,
    pub(crate) username: &'a str,
    pub(crate) key_path: Option<&'a Path>,
    pub(crate) options: &'a ConnectOptions,
    pub(crate) cancel: &'a CancellationToken,
    pub(crate) server_message: &'a ServerMessage,
}

struct Attempts<'a, P: Prompter> {
    ctx: AuthContext<'a, P>,
    methods: MethodSet,
    tried: Vec<AuthMethod>,
    rsa_hash: Option<HashAlg>,
    agent: Option<Agent>,
    agent_keys: Vec<PublicKey>,
}

/// Hash for RSA signatures, from what the server announced in `server-sig-algs`.
///
/// `Some(h)`: the server named its preference. `None` (no announcement) or an error: SHA-256,
/// never SHA-1 by default, which OpenSSH refuses since 8.8. The in-process test server does
/// not check signature algorithms, so this function is tested on its own.
#[allow(
    clippy::option_option,
    reason = "the exact type russh's best_supported_rsa_hash returns"
)]
pub(crate) fn rsa_hash(
    announced: &Result<Option<Option<HashAlg>>, russh::Error>,
) -> Option<HashAlg> {
    match announced {
        Ok(Some(hash)) => *hash,
        Ok(None) | Err(_) => Some(HashAlg::Sha256),
    }
}

/// Takes the message the server sent with its disconnect, if any.
fn take_server_message(slot: &ServerMessage) -> Option<String> {
    slot.lock().ok().and_then(|mut message| message.take())
}

/// A request the server can no longer receive is its disconnect, not a protocol error:
/// OpenSSH closes an authentication left waiting longer than its `LoginGraceTime`.
fn request_failed<P: Prompter>(ctx: &AuthContext<'_, P>, error: russh::Error) -> ConnectError {
    if ctx.handle.is_closed() {
        return ConnectError::Disconnected {
            server_message: take_server_message(ctx.server_message),
        };
    }
    ConnectError::Protocol(error)
}

pub(crate) async fn authenticate<P: Prompter>(ctx: AuthContext<'_, P>) -> Result<(), ConnectError> {
    let first = ctx
        .handle
        .authenticate_none(ctx.username)
        .await
        .map_err(|error| request_failed(&ctx, error))?;
    let methods = match first {
        russh::client::AuthResult::Success => return Ok(()),
        russh::client::AuthResult::Failure {
            remaining_methods, ..
        } => remaining_methods,
    };
    let mut attempts = Attempts {
        ctx,
        methods,
        tried: Vec::new(),
        rsa_hash: None,
        agent: None,
        agent_keys: Vec::new(),
    };
    attempts.ensure_open()?;
    attempts.run().await
}

impl<P: Prompter> Attempts<'_, P> {
    async fn run(&mut self) -> Result<(), ConnectError> {
        self.rsa_hash = rsa_hash(&self.ctx.handle.best_supported_rsa_hash().await);
        if self.allows(MethodKind::PublicKey) {
            self.agent = agent::connect(&self.ctx.options.agent).await;
            if let Some(agent) = self.agent.as_mut() {
                self.agent_keys = agent::identities(agent).await;
            }
        }

        if let Some(key_path) = self.ctx.key_path {
            if self.allows(MethodKind::PublicKey) && self.key_file(key_path).await? {
                return Ok(());
            }
        } else if self.other_agent_keys().await? {
            return Ok(());
        }
        if self.allows(MethodKind::KeyboardInteractive) && self.keyboard_interactive().await? {
            return Ok(());
        }
        if self.allows(MethodKind::Password) && self.password().await? {
            return Ok(());
        }
        Err(ConnectError::AuthenticationFailed {
            tried: std::mem::take(&mut self.tried),
        })
    }

    fn allows(&self, method: MethodKind) -> bool {
        self.methods.contains(&method)
    }

    fn hash_for(&self, key: &PublicKey) -> Option<HashAlg> {
        matches!(key.algorithm(), Algorithm::Rsa { .. })
            .then_some(self.rsa_hash)
            .flatten()
    }

    fn server_message(&self) -> Option<String> {
        take_server_message(self.ctx.server_message)
    }

    /// Fails when the server has gone, so an empty method list is not taken for a refusal.
    fn ensure_open(&mut self) -> Result<(), ConnectError> {
        if !self.ctx.handle.is_closed() {
            return Ok(());
        }
        match self.server_message() {
            Some(message) => Err(ConnectError::Disconnected {
                server_message: Some(message),
            }),
            None if self.methods.is_empty() => Err(ConnectError::AuthenticationFailed {
                tried: std::mem::take(&mut self.tried),
            }),
            None => Err(ConnectError::Disconnected {
                server_message: None,
            }),
        }
    }

    /// Records a refusal; `true` on success.
    fn settle(&mut self, result: russh::client::AuthResult) -> Result<bool, ConnectError> {
        match result {
            russh::client::AuthResult::Success => Ok(true),
            russh::client::AuthResult::Failure {
                remaining_methods, ..
            } => {
                self.methods = remaining_methods;
                self.ensure_open()?;
                Ok(false)
            }
        }
    }

    async fn with_agent(&mut self, key: &PublicKey) -> Result<bool, ConnectError> {
        let hash = self.hash_for(key);
        let Some(agent) = self.agent.as_mut() else {
            return Ok(false);
        };
        self.tried.push(AuthMethod::Agent);
        let result = self
            .ctx
            .handle
            .authenticate_publickey_with(self.ctx.username, key.clone(), hash, agent)
            .await;
        match result {
            Ok(result) => self.settle(result),
            // The agent failed to sign: not a server refusal, try the next method.
            Err(_) => Ok(false),
        }
    }

    async fn key_file(&mut self, path: &Path) -> Result<bool, ConnectError> {
        let file = KeyFile::read(path)?;
        if let Some(public) = file.public_key().cloned()
            && self.agent_keys.contains(&public)
            && self.with_agent(&public).await?
        {
            return Ok(true);
        }
        if !self.allows(MethodKind::PublicKey) {
            return Ok(false);
        }
        let key = self.decrypt(&file).await?;
        let hash = self.hash_for(key.public_key());
        self.tried.push(AuthMethod::KeyFile);
        let result = self
            .ctx
            .handle
            .authenticate_publickey(
                self.ctx.username,
                PrivateKeyWithHashAlg::new(Arc::new(key), hash),
            )
            .await
            .map_err(|error| request_failed(&self.ctx, error))?;
        self.settle(result)
    }

    async fn decrypt(&self, file: &KeyFile) -> Result<russh::keys::PrivateKey, ConnectError> {
        if !file.is_encrypted() {
            return Ok(file.decrypt(None)?);
        }
        let mut attempt = 1;
        loop {
            let question = PassphraseQuestion {
                key_path: file.path().to_owned(),
                attempt,
            };
            let passphrase: Secret = ask(
                self.ctx.prompter.key_passphrase(question),
                self.ctx.cancel,
                self.ctx.options.prompt_timeout,
            )
            .await?;
            match file.decrypt(Some(&passphrase)) {
                Ok(key) => return Ok(key),
                Err(KeyFileError::WrongPassphrase { .. }) if attempt < MAX_PASSPHRASE_ATTEMPTS => {
                    attempt += 1;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    async fn other_agent_keys(&mut self) -> Result<bool, ConnectError> {
        let keys: Vec<PublicKey> = self
            .agent_keys
            .iter()
            .take(MAX_OTHER_AGENT_KEYS)
            .cloned()
            .collect();
        for key in keys {
            if !self.allows(MethodKind::PublicKey) {
                break;
            }
            if self.with_agent(&key).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn disconnect(&self) {
        let _ = self
            .ctx
            .handle
            .disconnect(
                Disconnect::ByApplication,
                DISCONNECT_DESCRIPTION,
                DISCONNECT_LANGUAGE,
            )
            .await;
    }

    async fn keyboard_interactive(&mut self) -> Result<bool, ConnectError> {
        self.tried.push(AuthMethod::KeyboardInteractive);
        let mut response = self
            .ctx
            .handle
            .authenticate_keyboard_interactive_start(self.ctx.username, None::<String>)
            .await
            .map_err(|error| request_failed(&self.ctx, error))?;
        loop {
            match response {
                KeyboardInteractiveAuthResponse::Success => return Ok(true),
                KeyboardInteractiveAuthResponse::Failure {
                    remaining_methods, ..
                } => {
                    self.methods = remaining_methods;
                    self.ensure_open()?;
                    return Ok(false);
                }
                KeyboardInteractiveAuthResponse::InfoRequest {
                    name,
                    instructions,
                    prompts,
                } => {
                    let answers = if prompts.is_empty() {
                        Vec::new()
                    } else {
                        let question = KeyboardInteractiveQuestion {
                            host: self.ctx.host.to_owned(),
                            username: self.ctx.username.to_owned(),
                            name,
                            instructions,
                            prompts: prompts
                                .into_iter()
                                .map(|prompt| KeyboardInteractivePrompt {
                                    text: prompt.prompt,
                                    echo: prompt.echo,
                                })
                                .collect(),
                        };
                        let asked = ask(
                            self.ctx.prompter.keyboard_interactive(question),
                            self.ctx.cancel,
                            self.ctx.options.prompt_timeout,
                        )
                        .await;
                        match asked {
                            Ok(answers) => answers
                                .iter()
                                .map(|answer| answer.expose().to_owned())
                                .collect(),
                            Err(error) => {
                                // russh ignores every message but this round's answer:
                                // switching method would hang, so the connection ends.
                                self.disconnect().await;
                                return Err(error);
                            }
                        }
                    };
                    response = self
                        .ctx
                        .handle
                        .authenticate_keyboard_interactive_respond(answers)
                        .await
                        .map_err(|error| request_failed(&self.ctx, error))?;
                }
            }
        }
    }

    async fn password(&mut self) -> Result<bool, ConnectError> {
        self.tried.push(AuthMethod::Password);
        for attempt in 1..=MAX_PASSWORD_ATTEMPTS {
            if !self.allows(MethodKind::Password) {
                break;
            }
            let question = PasswordQuestion {
                host: self.ctx.host.to_owned(),
                port: self.ctx.port,
                username: self.ctx.username.to_owned(),
                attempt,
            };
            let password: Secret = ask(
                self.ctx.prompter.password(question),
                self.ctx.cancel,
                self.ctx.options.prompt_timeout,
            )
            .await?;
            let result = self
                .ctx
                .handle
                .authenticate_password(self.ctx.username, password.expose())
                .await
                .map_err(|error| request_failed(&self.ctx, error))?;
            if self.settle(result)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use russh::keys::HashAlg;

    use super::rsa_hash;

    #[test]
    fn the_servers_announced_hash_is_used() {
        assert_eq!(
            rsa_hash(&Ok(Some(Some(HashAlg::Sha512)))),
            Some(HashAlg::Sha512)
        );
        assert_eq!(
            rsa_hash(&Ok(Some(Some(HashAlg::Sha256)))),
            Some(HashAlg::Sha256)
        );
    }

    #[test]
    fn without_an_announcement_sha256_is_used_never_sha1() {
        assert_eq!(rsa_hash(&Ok(None)), Some(HashAlg::Sha256));
        assert_eq!(
            rsa_hash(&Err(russh::Error::Disconnect)),
            Some(HashAlg::Sha256)
        );
    }

    #[test]
    fn a_server_that_announces_only_sha1_gets_sha1() {
        // `Some(None)`: the server lists `ssh-rsa` alone; SHA-2 would be refused.
        assert_eq!(rsa_hash(&Ok(Some(None))), None);
    }
}
