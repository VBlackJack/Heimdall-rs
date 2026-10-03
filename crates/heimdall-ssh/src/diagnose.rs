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

//! A gateway route tested hop by hop, as the C# "Test route": each gateway connected to and
//! signed in to in turn, then, if one is given, a destination's TCP access through the last.
//!
//! It asks nobody and writes nothing:
//! - a gateway whose key is not trusted is not dialled;
//! - a key that matched a pin is not recorded;
//! - each gateway's question is answered only from what the caller gave for it, once.
//!
//! Each step is bounded as a whole, sign-in included, and the walk stops at the first one
//! that fails.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use heimdall_core::profile::SshProfile;
use russh::client;
use tokio_util::sync::CancellationToken;

use crate::client::{ClientHandler, ORIGINATOR_ADDRESS, Pinning, hop};
use crate::error::ConnectError;
use crate::known_hosts::{KnownHosts, validate_host};
use crate::options::ConnectOptions;
use crate::pins::Pins;
use crate::prompter::{
    KeyboardInteractiveQuestion, PassphraseQuestion, PasswordQuestion, Prompter, UsernameQuestion,
};
use crate::secret::Secret;

/// What the caller gives a gateway to sign in with: given at its first question only.
#[derive(Debug, Clone, Default)]
pub struct HopSecrets {
    /// Its password.
    pub password: Option<Secret>,
    /// Its key file's passphrase.
    pub passphrase: Option<Secret>,
}

/// What a step is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOf {
    /// The gateway at this place on the route, from 0, the nearest.
    Gateway(usize),
    /// The destination's TCP access through the last gateway.
    Destination,
}

/// How a step ended, as the C# tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It worked.
    Passed,
    /// No key is trusted for the gateway: not dialled.
    TrustRequired,
    /// The gateway presented another key than the one trusted.
    TrustChanged,
    /// It took longer than the step's time.
    Timeout,
    /// The gateway could not be reached.
    Network,
    /// The destination could not be reached through the gateway.
    Forwarding,
    /// The user stopped the test.
    Cancelled,
    /// The gateway asks questions only a person can answer.
    Interactive,
    /// The sign-in was refused, or what it needs is missing.
    Auth,
    /// Anything else.
    Unavailable,
}

/// One step of the test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    /// What it is about.
    pub of: StepOf,
    /// How it ended.
    pub outcome: Outcome,
    /// How long it took.
    pub elapsed: Duration,
}

/// Whether a key is trusted for `host:port`: recorded, trusted for the run, or pinned.
///
/// # Errors
///
/// The host name cannot be used, or the files cannot be read.
pub fn has_trusted_key(
    host: &str,
    port: u16,
    options: &ConnectOptions,
) -> Result<bool, ConnectError> {
    let host = validate_host(host)?;
    if !KnownHosts::new(&options.known_hosts)
        .recorded(&host, port)?
        .is_empty()
        || !options.run_trust.keys(&host, port).is_empty()
    {
        return Ok(true);
    }
    Ok(!Pins::beside(&options.known_hosts)
        .pinned(&host, port)?
        .is_empty())
}

/// Tests `hops`, nearest first, each signed in to with its `secrets`, then the `destination`
/// through the last: every step said to `on_step` as it ends, until one fails or `cancel`
/// fires. Each step has `options.connect_timeout` as a whole.
pub async fn diagnose_route(
    hops: &[SshProfile],
    secrets: &[HopSecrets],
    destination: Option<(String, u16)>,
    options: &ConnectOptions,
    cancel: CancellationToken,
    mut on_step: impl FnMut(Step),
) {
    // Every gateway reached stays connected until the test ends: the next goes through it.
    let mut reached: Vec<client::Handle<ClientHandler>> = Vec::new();
    for (index, profile) in hops.iter().enumerate() {
        let started = Instant::now();
        let given = secrets.get(index).cloned().unwrap_or_default();
        let outcome = match gateway(profile, reached.last(), given, options, &cancel).await {
            Ok(handle) => {
                reached.push(handle);
                Outcome::Passed
            }
            Err(outcome) => outcome,
        };
        on_step(Step {
            of: StepOf::Gateway(index),
            outcome,
            elapsed: started.elapsed(),
        });
        if outcome != Outcome::Passed {
            return;
        }
    }
    let (Some((host, port)), Some(last)) = (destination, reached.last()) else {
        return;
    };
    let started = Instant::now();
    let opening = last.channel_open_direct_tcpip(host, u32::from(port), ORIGINATOR_ADDRESS, 0);
    let outcome = tokio::select! {
        biased;
        () = cancel.cancelled() => Outcome::Cancelled,
        opened = tokio::time::timeout(options.connect_timeout, opening) => match opened {
            Err(_) => Outcome::Timeout,
            Ok(Ok(_channel)) => Outcome::Passed,
            Ok(Err(russh::Error::ChannelOpenFailure(_))) => Outcome::Forwarding,
            Ok(Err(_)) => Outcome::Unavailable,
        },
    };
    on_step(Step {
        of: StepOf::Destination,
        outcome,
        elapsed: started.elapsed(),
    });
}

/// One gateway: not dialled without a trusted key, then connected to and signed in to within
/// the step's time.
async fn gateway(
    profile: &SshProfile,
    carrier: Option<&client::Handle<ClientHandler>>,
    given: HopSecrets,
    options: &ConnectOptions,
    cancel: &CancellationToken,
) -> Result<client::Handle<ClientHandler>, Outcome> {
    if let Some(path) = &profile.key_path
        && std::fs::metadata(path).is_err()
    {
        return Err(Outcome::Auth);
    }
    match has_trusted_key(&profile.host, profile.port, options) {
        Ok(true) => {}
        Ok(false) => return Err(Outcome::TrustRequired),
        Err(_) => return Err(Outcome::Unavailable),
    }
    let prompter = Answering::new(given);
    let connecting = hop(profile, carrier, options, &prompter, cancel, Pinning::Leave);
    let result = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err(Outcome::Cancelled),
        result = tokio::time::timeout(options.connect_timeout, connecting) => result,
    };
    match result {
        Err(_) => Err(Outcome::Timeout),
        Ok(Ok((handle, _routes))) => Ok(handle),
        Ok(Err(error)) => Err(outcome(&error, prompter.declined(), cancel.is_cancelled())),
    }
}

/// The outcome `error` stands for; `declined`, the question the gateway asked that was not
/// answered, if any.
fn outcome(error: &ConnectError, declined: Option<Declined>, stopped: bool) -> Outcome {
    match error {
        ConnectError::Cancelled if stopped => Outcome::Cancelled,
        // A question nobody here answers: one only a person could is "interactive", the rest
        // is a sign-in that cannot go on.
        ConnectError::Cancelled | ConnectError::PromptTimedOut => match declined {
            Some(Declined::KeyboardInteractive) => Outcome::Interactive,
            Some(_) => Outcome::Auth,
            None => Outcome::Unavailable,
        },
        ConnectError::UnknownHostKey { .. } => Outcome::TrustRequired,
        ConnectError::HostKeyChanged { .. } | ConnectError::HostKeyAlgorithmMismatch { .. } => {
            Outcome::TrustChanged
        }
        ConnectError::Timeout => Outcome::Timeout,
        ConnectError::Network(_) | ConnectError::JumpRefused { .. } => Outcome::Network,
        ConnectError::AuthenticationFailed { .. } | ConnectError::KeyFile(_) => Outcome::Auth,
        _ => Outcome::Unavailable,
    }
}

/// A question the gateway asked that was not answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Declined {
    Username,
    Password,
    Passphrase,
    KeyboardInteractive,
}

/// Answers a gateway's first password and passphrase questions from what was given, and
/// nothing else; remembers what it declined.
struct Answering {
    given: Mutex<HopSecrets>,
    declined: Arc<Mutex<Option<Declined>>>,
}

impl Answering {
    fn new(given: HopSecrets) -> Self {
        Self {
            given: Mutex::new(given),
            declined: Arc::new(Mutex::new(None)),
        }
    }

    fn declined(&self) -> Option<Declined> {
        *self.declined.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn decline<T>(&self, what: Declined) -> Option<T> {
        *self.declined.lock().unwrap_or_else(PoisonError::into_inner) = Some(what);
        None
    }

    /// The secret given, once: asked again, the gateway refused it.
    fn take(&self, password: bool) -> Option<Secret> {
        let mut given = self.given.lock().unwrap_or_else(PoisonError::into_inner);
        if password {
            given.password.take()
        } else {
            given.passphrase.take()
        }
    }
}

impl Prompter for Answering {
    fn username(&self, _question: UsernameQuestion) -> impl Future<Output = Option<String>> + Send {
        std::future::ready(self.decline(Declined::Username))
    }

    fn password(&self, _question: PasswordQuestion) -> impl Future<Output = Option<Secret>> + Send {
        std::future::ready(self.take(true).or_else(|| self.decline(Declined::Password)))
    }

    fn key_passphrase(
        &self,
        _question: PassphraseQuestion,
    ) -> impl Future<Output = Option<Secret>> + Send {
        std::future::ready(
            self.take(false)
                .or_else(|| self.decline(Declined::Passphrase)),
        )
    }

    fn keyboard_interactive(
        &self,
        _question: KeyboardInteractiveQuestion,
    ) -> impl Future<Output = Option<Vec<Secret>>> + Send {
        std::future::ready(self.decline(Declined::KeyboardInteractive))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declined_question_is_told_apart_from_a_stop() {
        let declined = ConnectError::Cancelled;
        assert_eq!(
            outcome(&declined, Some(Declined::KeyboardInteractive), false),
            Outcome::Interactive
        );
        assert_eq!(
            outcome(&declined, Some(Declined::Password), false),
            Outcome::Auth
        );
        assert_eq!(
            outcome(&declined, Some(Declined::Passphrase), false),
            Outcome::Auth
        );
        assert_eq!(
            outcome(&declined, Some(Declined::Password), true),
            Outcome::Cancelled,
            "the user's stop wins"
        );
        assert_eq!(outcome(&declined, None, false), Outcome::Unavailable);
    }

    #[tokio::test]
    async fn a_secret_is_given_once_then_the_question_is_declined() {
        let answering = Answering::new(HopSecrets {
            password: Some(Secret::new("pw".to_owned())),
            passphrase: None,
        });
        let question = || PasswordQuestion {
            host: "bastion.lab".to_owned(),
            port: 22,
            username: "jump".to_owned(),
            attempt: 1,
        };
        assert!(answering.password(question()).await.is_some());
        assert!(
            answering.password(question()).await.is_none(),
            "asked again"
        );
        assert_eq!(answering.declined(), Some(Declined::Password));
    }
}
