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
use russh::{Preferred, cipher, compression, kex, mac};
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
    /// The server, `host:port`, as the diagnostics log names it.
    target: String,
    recorded: Vec<PublicKey>,
    /// Fingerprints pinned for the server, consulted when nothing is recorded.
    pins: Vec<String>,
    /// The key that matched a pin, to record in full once the exchange is through.
    pinned: PinnedKey,
    /// The key trusted, recorded or pinned, once the exchange is through.
    presented: PinnedKey,
    server_message: ServerMessage,
    /// Ports this side asked the server to listen on.
    routes: Routes,
}

/// A connection reached: its russh handle and the remote forwards its handler serves.
pub(crate) type Reached = (client::Handle<ClientHandler>, Routes);

impl ClientHandler {
    /// Decides about the server's key, and says so in the diagnostics log: a key trusted,
    /// one that changed, one of another algorithm, a certificate. An unknown key is said by
    /// whoever asks the user about it.
    fn decide(&self, server_key: &PublicKeyOrCertificate) -> Result<bool, HandlerError> {
        let target = &self.target;
        let PublicKeyOrCertificate::PublicKey { key, .. } = server_key else {
            log::warn!("{target} presented a host certificate, which is not trusted: refused");
            return Err(HandlerError::HostKey(HostKeyRejection::Certificate));
        };
        let rejection = match verdict(&self.recorded, key) {
            Verdict::Trusted => {
                log::info!(
                    "the host key of {target} is the one trusted: {}",
                    fingerprint(key)
                );
                self.keep_presented(key);
                return Ok(true);
            }
            Verdict::Unknown => match pin_verdict(&self.pins, key) {
                PinVerdict::None => HostKeyRejection::Unknown(Box::new(key.clone())),
                PinVerdict::Matches => {
                    log::info!(
                        "the host key of {target} has its pinned fingerprint {}",
                        fingerprint(key)
                    );
                    if let Ok(mut slot) = self.pinned.lock() {
                        *slot = Some(key.clone());
                    }
                    self.keep_presented(key);
                    return Ok(true);
                }
                PinVerdict::Differs { pinned } => {
                    log::warn!(
                        "the host key of {target} is not the one pinned: pinned {pinned}, presented {}: refused",
                        fingerprint(key)
                    );
                    HostKeyRejection::PinChanged {
                        pinned,
                        offered: Box::new(key.clone()),
                    }
                }
            },
            Verdict::Changed { recorded } => {
                log::warn!(
                    "the host key of {target} changed: recorded {}, presented {}: refused",
                    fingerprint(&recorded),
                    fingerprint(key)
                );
                HostKeyRejection::Changed {
                    recorded,
                    offered: Box::new(key.clone()),
                }
            }
            Verdict::OtherAlgorithm { recorded } => {
                log::warn!(
                    "{target} presented a {} host key, another algorithm than the {} recorded: refused",
                    key.algorithm(),
                    algorithm_names(&recorded).join(", ")
                );
                HostKeyRejection::OtherAlgorithm(recorded)
            }
        };
        Err(HandlerError::HostKey(rejection))
    }

    /// Keeps `key`, trusted, for whoever asked which key the server has.
    fn keep_presented(&self, key: &PublicKey) {
        if let Ok(mut slot) = self.presented.lock() {
            *slot = Some(key.clone());
        }
    }

    fn record_disconnect(
        &self,
        reason: DisconnectReason<HandlerError>,
    ) -> Result<(), HandlerError> {
        // Whatever the reason: what rides on the connection learns it is gone.
        self.routes.end();
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

    // The channels below are never asked for by this side: russh accepts each by default,
    // so each is refused here, dropping `reply` refusing it.

    fn server_channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        refuse_unasked(reply, "session")
    }

    fn server_channel_open_x11(
        &mut self,
        _channel: Channel<Msg>,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        refuse_unasked(reply, "X11")
    }

    fn server_channel_open_direct_tcpip(
        &mut self,
        _channel: Channel<Msg>,
        _host_to_connect: &str,
        _port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        refuse_unasked(reply, "direct TCP/IP")
    }

    fn server_channel_open_direct_streamlocal(
        &mut self,
        _channel: Channel<Msg>,
        _socket_path: &str,
        reply: ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        refuse_unasked(reply, "direct Unix socket")
    }

    fn server_channel_open_forwarded_streamlocal(
        &mut self,
        _channel: Channel<Msg>,
        _socket_path: &str,
        reply: ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        refuse_unasked(reply, "forwarded Unix socket")
    }
}

/// Refuses a channel the server opened of a kind this side never asks for, and says so in
/// the diagnostics log.
fn refuse_unasked(
    reply: ChannelOpenHandle,
    kind: &'static str,
) -> impl Future<Output = Result<(), HandlerError>> + Send {
    log::warn!("the server opened a {kind} channel, which is never asked for: refused");
    drop(reply);
    std::future::ready(Ok(()))
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
/// Key exchanges, ciphers and MACs offered to a server that allows legacy algorithms, after
/// the current ones, as SSH.NET (the C# Heimdall's client) offers them: SHA-1 Diffie-Hellman,
/// CBC ciphers, HMAC-SHA1.
const LEGACY_KEX: [kex::Name; 3] = [kex::DH_G14_SHA1, kex::DH_GEX_SHA1, kex::DH_G1_SHA1];
const LEGACY_CIPHERS: [cipher::Name; 4] = [
    cipher::AES_128_CBC,
    cipher::AES_192_CBC,
    cipher::AES_256_CBC,
    cipher::TRIPLE_DES_CBC,
];
const LEGACY_MACS: [mac::Name; 2] = [mac::HMAC_SHA1_ETM, mac::HMAC_SHA1];

/// The algorithms offered to a server: the current ones, its recorded key's family first,
/// then, when `legacy` allows them, the older ones after them.
pub(crate) fn preferred(recorded: &[PublicKey], compression: bool, legacy: bool) -> Preferred {
    let mut key = preferred_host_key_algorithms(recorded);
    let mut preferred = Preferred {
        compression: preferred_compression(compression),
        ..Preferred::DEFAULT
    };
    if legacy {
        let rsa_sha1 = Algorithm::Rsa { hash: None };
        if recorded.is_empty()
            || recorded
                .iter()
                .any(|known| matches!(known.algorithm(), Algorithm::Rsa { .. }))
        {
            key.push(rsa_sha1);
        }
        preferred.kex = Cow::Owned([Preferred::DEFAULT.kex.as_ref(), &LEGACY_KEX[..]].concat());
        preferred.cipher =
            Cow::Owned([Preferred::DEFAULT.cipher.as_ref(), &LEGACY_CIPHERS[..]].concat());
        preferred.mac = Cow::Owned([Preferred::DEFAULT.mac.as_ref(), &LEGACY_MACS[..]].concat());
    }
    preferred.key = Cow::Owned(key);
    preferred
}

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
    // A key file that is not there, or named by a relative path, is said before anything is
    // dialled, as the C# preflight.
    for hop in route.iter().chain(std::iter::once(profile)) {
        if let Some(path) = &hop.key_path
            && !path.is_absolute()
        {
            return Err(ConnectError::KeyFile(
                crate::key_file::KeyFileError::NotAbsolute { path: path.clone() },
            ));
        }
        if let Some(path) = &hop.key_path
            && let Err(error) = std::fs::metadata(path)
            && error.kind() == std::io::ErrorKind::NotFound
        {
            return Err(ConnectError::KeyFile(crate::key_file::KeyFileError::Io {
                path: path.clone(),
                source: error,
            }));
        }
    }
    // Reached over TCP: the nearest gateway, or the server itself without one.
    let (first, onward): (&SshProfile, Vec<&SshProfile>) = match route.split_first() {
        Some((nearest, rest)) => (
            nearest,
            rest.iter().chain(std::iter::once(profile)).collect(),
        ),
        None => (profile, Vec::new()),
    };
    // Every hop but the last is a gateway: a refusal there is the gateway's.
    let last = onward.len();
    let mut reached = hop(first, None, options, prompter, cancel, Pinning::Record)
        .await
        .map_err(|error| at_gateway(error, last > 0))?;
    let mut gateway = None;
    for (index, next) in onward.into_iter().enumerate() {
        let onward = hop(
            next,
            Some(&reached.0),
            options,
            prompter,
            cancel,
            Pinning::Record,
        )
        .await
        .map_err(|error| at_gateway(error, index + 1 < last))?;
        gateway = Some(std::mem::replace(&mut reached, onward));
    }
    Ok((reached, gateway))
}

/// `error`, said of a gateway when `gateway` and it is a refusal.
#[must_use]
pub fn at_gateway(error: ConnectError, gateway: bool) -> ConnectError {
    match error {
        ConnectError::AuthenticationFailed {
            tried, agent_keys, ..
        } if gateway => ConnectError::AuthenticationFailed {
            tried,
            agent_keys,
            gateway: true,
        },
        other => other,
    }
}

/// Originator address reported to a gateway when it is asked to connect onward: the client
/// has no address of its own to give inside the tunnel.
pub(crate) const ORIGINATOR_ADDRESS: &str = "127.0.0.1";

/// What a hop does with a key that matched a pinned fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pinning {
    /// Records it in full and drops the pin: a connection.
    Record,
    /// Leaves both files as they are: a diagnosis writes nothing.
    Leave,
}

/// A hop about to be dialled: the keys it is checked against, and what its handler shares
/// with the code waiting on it.
struct Dial {
    host: String,
    port: u16,
    known_hosts: KnownHosts,
    pins: Pins,
    /// The keys recorded for it, and those trusted for this run.
    recorded: Vec<PublicKey>,
    pinned: PinnedKey,
    presented: PinnedKey,
    server_message: ServerMessage,
    routes: Routes,
}

impl Dial {
    /// Ready to dial `profile`: its host name checked, the keys trusted for it read.
    fn new(profile: &SshProfile, options: &ConnectOptions) -> Result<Self, ConnectError> {
        let host = validate_host(&profile.host)?;
        let port = profile.port;
        let known_hosts = KnownHosts::new(&options.known_hosts);
        let mut recorded = known_hosts.recorded(&host, port)?;
        recorded.extend(options.run_trust.keys(&host, port));
        Ok(Self {
            host,
            port,
            known_hosts,
            pins: Pins::beside(&options.known_hosts),
            recorded,
            pinned: PinnedKey::default(),
            presented: PinnedKey::default(),
            server_message: ServerMessage::default(),
            routes: Routes::default(),
        })
    }

    /// Connects over TCP or, when `carrier` is given, over a connection it opens onward, and
    /// exchanges keys, the server's checked; within the connection's time, `cancel` ending
    /// it at any point. The pins are consulted only when no key is recorded.
    async fn exchange(
        &self,
        profile: &SshProfile,
        carrier: Option<&client::Handle<ClientHandler>>,
        options: &ConnectOptions,
        cancel: &CancellationToken,
    ) -> Result<client::Handle<ClientHandler>, ConnectError> {
        let (host, port, recorded) = (self.host.as_str(), self.port, &self.recorded);
        let pins = if recorded.is_empty() {
            self.pins.pinned(host, port)?
        } else {
            Vec::new()
        };
        let handler = ClientHandler {
            target: heimdall_core::profile::display_address(host, port),
            recorded: recorded.clone(),
            pins,
            pinned: self.pinned.clone(),
            presented: self.presented.clone(),
            server_message: self.server_message.clone(),
            routes: self.routes.clone(),
        };
        let config = Arc::new(client::Config {
            inactivity_timeout: None,
            keepalive_interval: Some(options.keepalive_interval),
            keepalive_max: options.keepalive_max,
            nodelay: true,
            preferred: preferred(recorded, options.compression, profile.legacy_algorithms),
            ..client::Config::default()
        });
        let connecting = async {
            match carrier {
                None => client::connect(config, (host, port), handler).await,
                Some(carrier) => {
                    let channel = carrier
                        .channel_open_direct_tcpip(host, u32::from(port), ORIGINATOR_ADDRESS, 0)
                        .await
                        .map_err(|error| match error {
                            russh::Error::ChannelOpenFailure(_) => HandlerError::Refused,
                            other => HandlerError::Russh(other),
                        })?;
                    client::connect_stream(config, channel.into_stream(), handler).await
                }
            }
        };
        tokio::select! {
            () = cancel.cancelled() => Err(ConnectError::Cancelled),
            result = tokio::time::timeout(options.connect_timeout, connecting) => match result {
                Err(_) => Err(ConnectError::Timeout),
                Ok(Err(error)) => Err(map_handler_error(error, host, port, recorded)),
                Ok(Ok(handle)) => Ok(handle),
            },
        }
    }

    /// Records in full the key that matched a pin, when it is to be.
    fn record(&self, pinning: Pinning) {
        if pinning == Pinning::Record {
            record_pinned(
                &self.known_hosts,
                &self.pins,
                &self.host,
                self.port,
                &self.pinned,
            );
        }
    }
}

/// The key `profile`'s server presents, once trusted: recorded, trusted for this run, or
/// matching a pin, which then records it in full as a connection does. Nobody signs in: the
/// connection ends once the keys are exchanged, for another program to connect told to
/// accept that key alone.
///
/// # Errors
///
/// As [`connect`] before anyone signs in: [`ConnectError::UnknownHostKey`] for a key never
/// seen, which the caller asks the user about, records, and probes again.
pub async fn trusted_host_key(
    profile: &SshProfile,
    options: &ConnectOptions,
    cancel: &CancellationToken,
) -> Result<PublicKey, ConnectError> {
    probe(profile, None, options, cancel).await
}

/// As [`trusted_host_key`], the server reached through `gateway`, which connects onward to
/// it. The key is checked against what is trusted for the server's own host and port, never
/// for the gateway's.
///
/// # Errors
///
/// As [`trusted_host_key`]; [`ConnectError::JumpRefused`] when the gateway will not reach
/// the server.
pub async fn trusted_host_key_via(
    gateway: &Connection,
    profile: &SshProfile,
    options: &ConnectOptions,
    cancel: &CancellationToken,
) -> Result<PublicKey, ConnectError> {
    probe(profile, Some(gateway.handle()), options, cancel).await
}

/// Exchanges keys with `profile`'s server, over TCP or, when `carrier` is given, over a
/// connection it opens onward; the key trusted, then the connection ended.
async fn probe(
    profile: &SshProfile,
    carrier: Option<&client::Handle<ClientHandler>>,
    options: &ConnectOptions,
    cancel: &CancellationToken,
) -> Result<PublicKey, ConnectError> {
    let dial = Dial::new(profile, options)?;
    let handle = dial.exchange(profile, carrier, options, cancel).await?;
    dial.record(Pinning::Record);
    let key = dial.presented.lock().ok().and_then(|mut slot| slot.take());
    let _ = handle
        .disconnect(
            russh::Disconnect::ByApplication,
            PROBE_DISCONNECT_DESCRIPTION,
            PROBE_DISCONNECT_LANGUAGE,
        )
        .await;
    // A key exchange that went through checked a key: one was kept.
    key.ok_or(ConnectError::Protocol(russh::Error::UnknownKey))
}

/// What the probe's disconnect says: nothing.
const PROBE_DISCONNECT_DESCRIPTION: &str = "";

/// The language of that nothing.
const PROBE_DISCONNECT_LANGUAGE: &str = "";

/// Connects to `profile` and authenticates, over TCP or, when `carrier` is given, over a
/// connection it opens onward.
pub(crate) async fn hop<P: Prompter>(
    profile: &SshProfile,
    carrier: Option<&client::Handle<ClientHandler>>,
    options: &ConnectOptions,
    prompter: &P,
    cancel: &CancellationToken,
    pinning: Pinning,
) -> Result<Reached, ConnectError> {
    let dial = Dial::new(profile, options)?;
    let mut handle = dial.exchange(profile, carrier, options, cancel).await?;
    dial.record(pinning);
    let (host, port) = (dial.host.clone(), dial.port);

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
        server_message: &dial.server_message,
    })
    .await?;

    Ok((handle, dial.routes))
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

    use super::{
        LEGACY_CIPHERS, LEGACY_KEX, LEGACY_MACS, preferred, preferred_compression,
        preferred_host_key_algorithms,
    };

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

    #[test]
    fn legacy_algorithms_come_only_when_allowed_and_after_the_current_ones() {
        let modern = preferred(&[], false, false);
        assert!(LEGACY_KEX.iter().all(|kex| !modern.kex.contains(kex)));
        assert!(
            LEGACY_CIPHERS
                .iter()
                .all(|cipher| !modern.cipher.contains(cipher))
        );
        assert!(LEGACY_MACS.iter().all(|mac| !modern.mac.contains(mac)));
        assert!(!modern.key.contains(&Algorithm::Rsa { hash: None }));

        let legacy = preferred(&[], false, true);
        assert_eq!(
            &legacy.kex[..modern.kex.len()],
            &modern.kex[..],
            "current ones first"
        );
        assert_eq!(&legacy.kex[modern.kex.len()..], &LEGACY_KEX[..]);
        assert_eq!(&legacy.cipher[modern.cipher.len()..], &LEGACY_CIPHERS[..]);
        assert_eq!(&legacy.mac[modern.mac.len()..], &LEGACY_MACS[..]);
        assert_eq!(legacy.key.last(), Some(&Algorithm::Rsa { hash: None }));

        // A server recorded with an Ed25519 key is never offered SHA-1 RSA.
        let ed25519 = PublicKey::from_openssh(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGb9wFcIfI8bE9fMhXj1bDO0N2sVX4r3r7cS3nNa2yZf",
        )
        .expect("key");
        let recorded = preferred(&[ed25519], false, true);
        assert!(!recorded.key.contains(&Algorithm::Rsa { hash: None }));
    }
}
