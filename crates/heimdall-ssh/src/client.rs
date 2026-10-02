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

//! Connection: key exchange, host key check, then authentication; the shell on top.

use std::borrow::Cow;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use heimdall_core::profile::SshProfile;
use russh::Channel;
use russh::client::{self, ChannelOpenHandle, DisconnectReason, Msg};
use russh::keys::{Algorithm, PublicKey, PublicKeyOrCertificate};
use russh::{Preferred, compression};
use tokio_util::sync::CancellationToken;

use crate::auth::{self, AuthContext};
use crate::connection::Connection;
use crate::error::ConnectError;
use crate::forward::{self, Routes};
use crate::known_hosts::{
    KnownHosts, KnownHostsError, Verdict, fingerprint, validate_host, verdict,
};
use crate::options::ConnectOptions;
use crate::pins::{PinVerdict, Pins, pin_verdict};
use crate::prompter::{Prompter, UsernameQuestion};
use crate::session::ShellSession;

/// Message the server sent with its disconnect, shared between the russh session task and
/// the code waiting on authentication.
pub(crate) type ServerMessage = Arc<Mutex<Option<String>>>;

/// The key that matched a pinned fingerprint, shared between the russh session task and the
/// code that records it.
type PinnedKey = Arc<Mutex<Option<PublicKey>>>;

/// Why the host key check stopped the key exchange.
#[derive(Debug)]
pub(crate) enum HostKeyRejection {
    Unknown(Box<PublicKey>),
    Changed {
        recorded: Box<PublicKey>,
        offered: Box<PublicKey>,
    },
    /// Only a fingerprint is pinned, and the key has another.
    PinChanged {
        pinned: String,
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
    /// A gateway refused to open the connection onward.
    #[error("the gateway refused to connect onward")]
    Refused,
}

pub(crate) struct ClientHandler {
    recorded: Vec<PublicKey>,
    /// Fingerprints pinned for the server, consulted when nothing is recorded.
    pins: Vec<String>,
    /// The key that matched a pin, to record in full once the exchange is through.
    pinned: PinnedKey,
    server_message: ServerMessage,
    /// Ports this side asked the server to listen on.
    routes: Routes,
}

/// A connection reached: its russh handle and the remote forwards its handler serves.
pub(crate) type Reached = (client::Handle<ClientHandler>, Routes);

impl ClientHandler {
    fn decide(&self, server_key: &PublicKeyOrCertificate) -> Result<bool, HandlerError> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = server_key else {
            return Err(HandlerError::HostKey(HostKeyRejection::Certificate));
        };
        let rejection = match verdict(&self.recorded, key) {
            Verdict::Trusted => return Ok(true),
            Verdict::Unknown => match pin_verdict(&self.pins, key) {
                PinVerdict::None => HostKeyRejection::Unknown(Box::new(key.clone())),
                PinVerdict::Matches => {
                    if let Ok(mut slot) = self.pinned.lock() {
                        *slot = Some(key.clone());
                    }
                    return Ok(true);
                }
                PinVerdict::Differs { pinned } => HostKeyRejection::PinChanged {
                    pinned,
                    offered: Box::new(key.clone()),
                },
            },
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

    /// An agent channel reaches the agent only on a connection whose shell asked to forward
    /// it; otherwise it is refused, dropping `reply` refusing it.
    fn server_channel_open_agent_forward(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let source = self.routes.agent();
        async move {
            if let Some(source) = source {
                reply.accept().await;
                tokio::spawn(forward::carry_agent(channel, source));
            } else {
                log::warn!("the server asked for the SSH agent, which was not forwarded: refused");
                drop(reply);
            }
            Ok(())
        }
    }

    /// A connection the server took on a port this side asked it to listen on goes to its
    /// local port; on any other port it is refused, dropping `reply` refusing it.
    fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        _connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let local = self.routes.local_port(connected_port);
        async move {
            if let Some(local) = local {
                reply.accept().await;
                tokio::spawn(forward::carry(channel, local));
            } else {
                log::warn!(
                    "the server forwarded a connection from its port {connected_port}, which was not asked for: refused"
                );
                drop(reply);
            }
            Ok(())
        }
    }
}

/// Compression asked for with `ssh -C`, in OpenSSH's order: compressed first, uncompressed
/// still accepted from a server that offers nothing else.
const COMPRESSED_FIRST: &[compression::Name] = &[
    compression::ZLIB_LEGACY,
    compression::ZLIB,
    compression::NONE,
];

/// Compression to offer: compressed first when asked, else russh's own order, uncompressed
/// first.
pub(crate) fn preferred_compression(on: bool) -> Cow<'static, [compression::Name]> {
    if on {
        Cow::Borrowed(COMPRESSED_FIRST)
    } else {
        Preferred::DEFAULT.compression
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
        HandlerError::HostKey(HostKeyRejection::PinChanged { pinned, offered }) => {
            ConnectError::HostKeyChanged {
                host,
                port,
                recorded: pinned,
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
        HandlerError::Refused => ConnectError::JumpRefused { host, port },
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

/// Opens an interactive shell to `profile` on a connection of its own.
///
/// `cancel` ends the attempt at any point, and once the session is open, the session too;
/// the connection goes with it unless the caller kept no other use of it, which here it
/// cannot.
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
    let connection = establish(profile, options, prompter, cancel.clone()).await?;
    connection.open_shell(options, cancel).await
}

/// Connects to `profile` and authenticates: host key checked, then the user proven. The
/// [`Connection`] then carries shells and subsystems.
///
/// # Errors
///
/// See [`ConnectError`]; as for [`connect`], an unknown host key ends the attempt.
pub async fn establish<P: Prompter>(
    profile: &SshProfile,
    options: &ConnectOptions,
    prompter: Arc<P>,
    cancel: CancellationToken,
) -> Result<Connection, ConnectError> {
    establish_via(&[], profile, options, prompter, cancel).await
}

/// Connects to `profile` through `route`, the gateways nearest first: each one, once its host
/// key is checked and the user proven, opens a connection onward to the next, and the next
/// runs its own SSH over it, its own host key checked and its own user proven. The server
/// sees the last gateway as its client; no gateway sees what passes, only that it passes.
///
/// # Errors
///
/// See [`ConnectError`]. A host key error names the host it is about, gateway or server;
/// [`ConnectError::JumpRefused`] names the host a gateway would not reach.
pub async fn establish_via<P: Prompter>(
    route: &[SshProfile],
    profile: &SshProfile,
    options: &ConnectOptions,
    prompter: Arc<P>,
    cancel: CancellationToken,
) -> Result<Connection, ConnectError> {
    // The gateway's handle can go: its session lives on for as long as the connection
    // onward is open, and ends when the hop after it disconnects.
    let (server, _gateway) = walk(route, profile, options, prompter.as_ref(), &cancel).await?;
    Ok(Connection::new(server))
}

/// A server reached through a gateway, and that gateway, still usable.
#[derive(Debug)]
pub struct Routed {
    /// The server.
    pub server: Connection,
    /// The last gateway of the route, to connect onward elsewhere too (a SOCKS proxy);
    /// `None` without a route. Dropping it disconnects the gateway, and with it the server
    /// carried over it: keep it as long as the server.
    pub gateway: Option<Connection>,
}

/// As [`establish_via`], keeping the last gateway of the route usable.
///
/// # Errors
///
/// As [`establish_via`].
pub async fn establish_via_keeping_gateway<P: Prompter>(
    route: &[SshProfile],
    profile: &SshProfile,
    options: &ConnectOptions,
    prompter: Arc<P>,
    cancel: CancellationToken,
) -> Result<Routed, ConnectError> {
    let (server, gateway) = walk(route, profile, options, prompter.as_ref(), &cancel).await?;
    Ok(Routed {
        server: Connection::new(server),
        gateway: gateway.map(Connection::new),
    })
}

/// Walks `route` to `profile`: the server's handle, and the last gateway's.
async fn walk<P: Prompter>(
    route: &[SshProfile],
    profile: &SshProfile,
    options: &ConnectOptions,
    prompter: &P,
    cancel: &CancellationToken,
) -> Result<(Reached, Option<Reached>), ConnectError> {
    // Reached over TCP: the nearest gateway, or the server itself without one.
    let (first, onward): (&SshProfile, Vec<&SshProfile>) = match route.split_first() {
        Some((nearest, rest)) => (
            nearest,
            rest.iter().chain(std::iter::once(profile)).collect(),
        ),
        None => (profile, Vec::new()),
    };
    let mut reached = hop(first, None, options, prompter, cancel).await?;
    let mut gateway = None;
    for next in onward {
        let onward = hop(next, Some(&reached.0), options, prompter, cancel).await?;
        gateway = Some(std::mem::replace(&mut reached, onward));
    }
    Ok((reached, gateway))
}

/// Originator address reported to a gateway when it is asked to connect onward: the client
/// has no address of its own to give inside the tunnel.
const ORIGINATOR_ADDRESS: &str = "127.0.0.1";

/// Connects to `profile` and authenticates, over TCP or, when `carrier` is given, over a
/// connection it opens onward.
async fn hop<P: Prompter>(
    profile: &SshProfile,
    carrier: Option<&client::Handle<ClientHandler>>,
    options: &ConnectOptions,
    prompter: &P,
    cancel: &CancellationToken,
) -> Result<Reached, ConnectError> {
    let host = validate_host(&profile.host)?;
    let port = profile.port;
    let known_hosts = KnownHosts::new(&options.known_hosts);
    let mut recorded = known_hosts.recorded(&host, port)?;
    recorded.extend(options.run_trust.keys(&host, port));
    let pins = Pins::beside(&options.known_hosts);
    let pinned_fingerprints = if recorded.is_empty() {
        pins.pinned(&host, port)?
    } else {
        Vec::new()
    };
    let pinned = PinnedKey::default();

    let server_message = ServerMessage::default();
    let routes = Routes::default();
    let handler = ClientHandler {
        recorded: recorded.clone(),
        pins: pinned_fingerprints,
        pinned: pinned.clone(),
        server_message: server_message.clone(),
        routes: routes.clone(),
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(options.keepalive_interval),
        keepalive_max: options.keepalive_max,
        nodelay: true,
        preferred: Preferred {
            key: Cow::Owned(preferred_host_key_algorithms(&recorded)),
            compression: preferred_compression(options.compression),
            ..Preferred::DEFAULT
        },
        ..client::Config::default()
    });

    let connecting = async {
        match carrier {
            None => client::connect(config, (host.as_str(), port), handler).await,
            Some(carrier) => {
                let channel = carrier
                    .channel_open_direct_tcpip(
                        host.as_str(),
                        u32::from(port),
                        ORIGINATOR_ADDRESS,
                        0,
                    )
                    .await
                    .map_err(|error| match error {
                        russh::Error::ChannelOpenFailure(_) => HandlerError::Refused,
                        other => HandlerError::Russh(other),
                    })?;
                client::connect_stream(config, channel.into_stream(), handler).await
            }
        }
    };
    let mut handle = tokio::select! {
        () = cancel.cancelled() => return Err(ConnectError::Cancelled),
        result = tokio::time::timeout(options.connect_timeout, connecting) => match result {
            Err(_) => return Err(ConnectError::Timeout),
            Ok(Err(error)) => return Err(map_handler_error(error, &host, port, &recorded)),
            Ok(Ok(handle)) => handle,
        },
    };
    record_pinned(&known_hosts, &pins, &host, port, &pinned);

    let username = if let Some(username) = profile.username.clone() {
        username
    } else {
        let question = UsernameQuestion {
            host: host.clone(),
            port,
        };
        ask(prompter.username(question), cancel, options.prompt_timeout).await?
    };

    auth::authenticate(AuthContext {
        handle: &mut handle,
        prompter,
        host: &host,
        port,
        username: &username,
        key_path: profile.key_path.as_deref(),
        options,
        cancel,
        server_message: &server_message,
    })
    .await?;

    Ok((handle, routes))
}

/// Records in full the key that matched a pinned fingerprint, and drops the pin: from now
/// on the server is checked against its whole key. A failure leaves the pin, which still
/// trusts that key and no other.
fn record_pinned(known_hosts: &KnownHosts, pins: &Pins, host: &str, port: u16, pinned: &PinnedKey) {
    let Some(key) = pinned.lock().ok().and_then(|mut slot| slot.take()) else {
        return;
    };
    match known_hosts.learn(host, port, &key) {
        Ok(()) => {
            if let Err(error) = pins.unpin(host, port) {
                log::warn!("the pin of a server recorded in full stays: {error}");
            }
        }
        Err(error) => log::warn!("a pinned server's key is not recorded in full: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use russh::keys::{Algorithm, HashAlg, PublicKey};

    use russh::compression;

    use super::{preferred_compression, preferred_host_key_algorithms};

    #[test]
    fn compression_asked_for_is_offered_first_and_uncompressed_is_still_accepted() {
        assert_eq!(
            preferred_compression(true).as_ref(),
            [
                compression::ZLIB_LEGACY,
                compression::ZLIB,
                compression::NONE
            ]
        );
        assert_eq!(
            preferred_compression(false).first(),
            Some(&compression::NONE),
            "off: uncompressed first"
        );
    }

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
