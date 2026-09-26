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

//! Connection: key exchange, host key check, then authentication and the shell.

use std::borrow::Cow;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use heimdall_core::profile::SshProfile;
use russh::Preferred;
use russh::client::{self, DisconnectReason};
use russh::keys::{Algorithm, PublicKey, PublicKeyOrCertificate};
use tokio_util::sync::CancellationToken;

use crate::auth::{self, AuthContext};
use crate::error::ConnectError;
use crate::known_hosts::{
    KnownHosts, KnownHostsError, Verdict, fingerprint, validate_host, verdict,
};
use crate::options::ConnectOptions;
use crate::prompter::{Prompter, UsernameQuestion};
use crate::session::{self, ShellSession};

/// Message the server sent with its disconnect, shared between the russh session task and
/// the code waiting on authentication.
pub(crate) type ServerMessage = Arc<Mutex<Option<String>>>;

/// Why the host key check stopped the key exchange.
#[derive(Debug)]
pub(crate) enum HostKeyRejection {
    Unknown(Box<PublicKey>),
    Changed {
        recorded: Box<PublicKey>,
        offered: Box<PublicKey>,
    },
    OtherAlgorithm(Vec<Algorithm>),
    Certificate,
}

/// Error type of the russh handler: a russh error, or a host key decision carried intact to
/// [`connect`]. Returning `Ok(false)` instead would reach the caller as an anonymous
/// `UnknownKey`.
#[derive(Debug, thiserror::Error)]
pub(crate) enum HandlerError {
    #[error(transparent)]
    Russh(#[from] russh::Error),
    #[error("host key rejected")]
    HostKey(HostKeyRejection),
}

pub(crate) struct ClientHandler {
    recorded: Vec<PublicKey>,
    server_message: ServerMessage,
}

impl ClientHandler {
    fn decide(&self, server_key: &PublicKeyOrCertificate) -> Result<bool, HandlerError> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = server_key else {
            return Err(HandlerError::HostKey(HostKeyRejection::Certificate));
        };
        let rejection = match verdict(&self.recorded, key) {
            Verdict::Trusted => return Ok(true),
            Verdict::Unknown => HostKeyRejection::Unknown(Box::new(key.clone())),
            Verdict::Changed { recorded } => HostKeyRejection::Changed {
                recorded,
                offered: Box::new(key.clone()),
            },
            Verdict::OtherAlgorithm { recorded } => HostKeyRejection::OtherAlgorithm(recorded),
        };
        Err(HandlerError::HostKey(rejection))
    }

    fn record_disconnect(
        &self,
        reason: DisconnectReason<HandlerError>,
    ) -> Result<(), HandlerError> {
        match reason {
            DisconnectReason::ReceivedDisconnect(info) => {
                if let Ok(mut slot) = self.server_message.lock() {
                    *slot = Some(info.message);
                }
                Ok(())
            }
            DisconnectReason::Error(error) => Err(error),
        }
    }
}

impl client::Handler for ClientHandler {
    type Error = HandlerError;

    fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        std::future::ready(self.decide(server_key))
    }

    fn disconnected(
        &mut self,
        reason: DisconnectReason<Self::Error>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        std::future::ready(self.record_disconnect(reason))
    }
}

/// Host key algorithms to accept, in the default preference order.
///
/// Never `ssh-rsa` with SHA-1. For a known host, only the recorded algorithms: a server that
/// no longer offers any of them fails negotiation instead of presenting a key that would be
/// asked about as new. A recorded RSA key accepts `rsa-sha2-512` and `rsa-sha2-256`.
pub(crate) fn preferred_host_key_algorithms(recorded: &[PublicKey]) -> Vec<Algorithm> {
    let modern = |candidate: &Algorithm| !matches!(candidate, Algorithm::Rsa { hash: None });
    let recorded_family = |candidate: &Algorithm| {
        recorded
            .iter()
            .any(|key| match (key.algorithm(), candidate) {
                (Algorithm::Rsa { .. }, Algorithm::Rsa { .. }) => true,
                (known, candidate) => known == *candidate,
            })
    };
    Preferred::DEFAULT
        .key
        .iter()
        .filter(|candidate| modern(candidate))
        .filter(|candidate| recorded.is_empty() || recorded_family(candidate))
        .cloned()
        .collect()
}

/// Waits for the user, giving up on cancellation or after `deadline`.
pub(crate) async fn ask<T>(
    question: impl Future<Output = Option<T>>,
    cancel: &CancellationToken,
    deadline: Duration,
) -> Result<T, ConnectError> {
    tokio::select! {
        () = cancel.cancelled() => Err(ConnectError::Cancelled),
        answer = tokio::time::timeout(deadline, question) => match answer {
            Err(_) => Err(ConnectError::PromptTimedOut),
            Ok(None) => Err(ConnectError::Cancelled),
            Ok(Some(value)) => Ok(value),
        },
    }
}

fn algorithm_names(algorithms: &[Algorithm]) -> Vec<String> {
    algorithms.iter().map(ToString::to_string).collect()
}

fn map_handler_error(
    error: HandlerError,
    host: &str,
    port: u16,
    recorded: &[PublicKey],
) -> ConnectError {
    let host = host.to_owned();
    match error {
        HandlerError::HostKey(HostKeyRejection::Unknown(key)) => {
            ConnectError::UnknownHostKey { host, port, key }
        }
        HandlerError::HostKey(HostKeyRejection::Changed { recorded, offered }) => {
            ConnectError::HostKeyChanged {
                host,
                port,
                recorded: fingerprint(&recorded),
                offered: fingerprint(&offered),
            }
        }
        HandlerError::HostKey(HostKeyRejection::OtherAlgorithm(algorithms)) => {
            ConnectError::HostKeyAlgorithmMismatch {
                host,
                port,
                recorded: algorithm_names(&algorithms),
            }
        }
        HandlerError::HostKey(HostKeyRejection::Certificate) => {
            ConnectError::HostCertificateRefused
        }
        HandlerError::Russh(russh::Error::NoCommonAlgo { .. }) if !recorded.is_empty() => {
            // Offered only the recorded algorithms and the server has none of them.
            let algorithms: Vec<Algorithm> = recorded.iter().map(PublicKey::algorithm).collect();
            ConnectError::HostKeyAlgorithmMismatch {
                host,
                port,
                recorded: algorithm_names(&algorithms),
            }
        }
        HandlerError::Russh(russh::Error::IO(error)) => ConnectError::Network(error),
        HandlerError::Russh(error) => ConnectError::Protocol(error),
    }
}

impl From<KnownHostsError> for ConnectError {
    fn from(error: KnownHostsError) -> Self {
        match error {
            KnownHostsError::InvalidHost => Self::InvalidHost,
            other => Self::KnownHosts(other),
        }
    }
}

/// Opens an interactive shell to `profile`.
///
/// `cancel` ends the attempt at any point, and once the session is open, the session too.
///
/// # Errors
///
/// See [`ConnectError`]. [`ConnectError::UnknownHostKey`] is the normal outcome of a first
/// connection: the caller asks the user, records the key, and connects again.
pub async fn connect<P: Prompter>(
    profile: &SshProfile,
    options: &ConnectOptions,
    prompter: Arc<P>,
    cancel: CancellationToken,
) -> Result<ShellSession, ConnectError> {
    let host = validate_host(&profile.host)?;
    let port = profile.port;
    let recorded = KnownHosts::new(&options.known_hosts).recorded(&host, port)?;

    let server_message = ServerMessage::default();
    let handler = ClientHandler {
        recorded: recorded.clone(),
        server_message: server_message.clone(),
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(options.keepalive_interval),
        keepalive_max: options.keepalive_max,
        nodelay: true,
        preferred: Preferred {
            key: Cow::Owned(preferred_host_key_algorithms(&recorded)),
            ..Preferred::DEFAULT
        },
        ..client::Config::default()
    });

    let connecting = client::connect(config, (host.as_str(), port), handler);
    let mut handle = tokio::select! {
        () = cancel.cancelled() => return Err(ConnectError::Cancelled),
        result = tokio::time::timeout(options.connect_timeout, connecting) => match result {
            Err(_) => return Err(ConnectError::Timeout),
            Ok(Err(error)) => return Err(map_handler_error(error, &host, port, &recorded)),
            Ok(Ok(handle)) => handle,
        },
    };

    let username = if let Some(username) = profile.username.clone() {
        username
    } else {
        let question = UsernameQuestion {
            host: host.clone(),
            port,
        };
        ask(prompter.username(question), &cancel, options.prompt_timeout).await?
    };

    auth::authenticate(AuthContext {
        handle: &mut handle,
        prompter: prompter.as_ref(),
        host: &host,
        port,
        username: &username,
        key_path: profile.key_path.as_deref(),
        options,
        cancel: &cancel,
        server_message: &server_message,
    })
    .await?;

    session::open(handle, options, cancel).await
}

#[cfg(test)]
mod tests {
    use russh::keys::{Algorithm, HashAlg, PublicKey};

    use super::preferred_host_key_algorithms;

    const ED25519: &str = include_str!("../tests/fixtures/keys/ed25519-openssh.pub");

    fn key(text: &str) -> PublicKey {
        PublicKey::from_openssh(text.trim()).expect("valid key")
    }

    #[test]
    fn sha1_rsa_is_never_offered() {
        let algorithms = preferred_host_key_algorithms(&[]);
        assert!(!algorithms.contains(&Algorithm::Rsa { hash: None }));
        assert!(algorithms.contains(&Algorithm::Rsa {
            hash: Some(HashAlg::Sha256)
        }));
        assert!(algorithms.contains(&Algorithm::Ed25519));
    }

    #[test]
    fn a_known_host_is_offered_only_its_recorded_algorithms() {
        assert_eq!(
            preferred_host_key_algorithms(&[key(ED25519)]),
            vec![Algorithm::Ed25519]
        );
    }
}
