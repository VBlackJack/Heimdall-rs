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

//! Opening an RDP connection: negotiation, TLS, the pin, then Network Level Authentication.
//!
//! No credential leaves before the pin passes: the X.224 request carries a neutral cookie
//! instead of the user name, and `CredSSP` runs only once the server's key is the one
//! recorded, or the one the user just accepted.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::time::Duration;

use ironrdp::connector::sspi::generator::NetworkRequest;
use ironrdp::connector::{
    self, ClientConnector, ConnectionResult, ConnectorError, ConnectorErrorKind, ConnectorResult,
    DesktopSize,
};
use ironrdp::displaycontrol::client::DisplayControlClient;
use ironrdp::dvc::DrdynvcClient;
use ironrdp::pdu::gcc::KeyboardType;
use ironrdp::pdu::nego::NegoRequestData;
use ironrdp::pdu::rdp::capability_sets::MajorPlatformType;
use ironrdp::pdu::rdp::client_info::{PerformanceFlags, TimezoneInfo};
use ironrdp_tokio::{MovableTokioFramed, NetworkClient};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::certificate::{Fingerprint, ServerCertificate};
use crate::known_hosts::{KnownRdpHosts, Verdict};
use crate::tls;

/// Cookie of the X.224 request. Without one, `IronRDP` sends the user name, in clear, before
/// TLS.
pub const NEGOTIATION_COOKIE: &str = "heimdall";

/// Name the server is told the client has; it keeps the first 15 characters.
const CLIENT_NAME: &str = "heimdall-rs";

/// Widest and tallest desktop accepted, in pixels: bounds the framebuffer a server can make
/// the client allocate.
pub const MAX_DESKTOP_SIDE: u16 = 8192;

/// Function keys of the keyboard the server is told about.
const FUNCTION_KEYS: u32 = 12;

/// A byte stream an RDP connection runs over.
pub trait Transport: AsyncRead + AsyncWrite + Unpin + Send + Sync {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + Sync> Transport for T {}

/// The stream once TLS runs over it.
pub type Upgraded = TlsStream<Box<dyn Transport>>;

/// How long each phase may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// TCP connection.
    pub connect: Duration,
    /// Negotiation and TLS handshake.
    pub handshake: Duration,
    /// Authentication and the rest of the connection sequence.
    pub logon: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(15),
            handshake: Duration::from_secs(15),
            logon: Duration::from_secs(60),
        }
    }
}

/// The security protocols offered to the server.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Security {
    /// Network Level Authentication only: the credentials are checked before a session
    /// opens, and a server that does not support it is refused.
    #[default]
    Nla,
    /// Network Level Authentication, or plain TLS for servers without it (xrdp): the
    /// password then travels in the logon packet, inside TLS, after the pin passed. Offering
    /// both lets the server, or anyone in between on a first connection, pick TLS.
    NlaOrTls,
}

/// An account and its password.
pub type Credentials = (String, Zeroizing<String>);

/// Asks for the credentials, only for a server whose identity is settled: one trusted before,
/// or whose key the user just accepted. Such a server is asked for them before the connection
/// opens, as it would not wait for a person typing; they still leave only once its key is
/// checked. A server never seen stops at its certificate and asks nothing. `None` cancels the
/// connection.
pub type AskCredentials =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Option<Credentials>> + Send>> + Send>;

/// Credentials already known, for callers that have them.
#[must_use]
pub fn given(username: String, password: Zeroizing<String>) -> AskCredentials {
    Box::new(move || Box::pin(std::future::ready(Some((username, password)))))
}

/// Where to connect.
#[derive(Debug, Clone)]
pub struct RdpConfig {
    /// Server.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Windows domain of the account.
    pub domain: Option<String>,
    /// Desktop size asked for.
    pub desktop: (u16, u16),
    /// Keyboard layout identifier the server is told (0: its default).
    pub keyboard_layout: u32,
    /// Security protocols offered.
    pub security: Security,
    /// Pins of the servers trusted so far.
    pub known_hosts: KnownRdpHosts,
    /// A key the user accepted for this server after an [`RdpError::UnknownCertificate`]:
    /// recorded if the server presents exactly it.
    pub accepted: Option<Fingerprint>,
    /// Phase timeouts.
    pub timeouts: Timeouts,
}

/// Why a connection did not open.
#[derive(Debug, thiserror::Error)]
pub enum RdpError {
    /// The server cannot be reached.
    #[error("the server cannot be reached: {0}")]
    Network(#[source] io::Error),
    /// A phase took longer than its timeout.
    #[error("the server did not answer in time")]
    Timeout,
    /// Stopped by the caller.
    #[error("cancelled")]
    Cancelled,
    /// The server does not accept Network Level Authentication, or refused the request.
    #[error("the server refused the security requested: {0}")]
    Negotiation(String),
    /// The TLS handshake failed, a forged certificate included.
    #[error("TLS failed: {0}")]
    Tls(#[source] io::Error),
    /// The server's certificate cannot be read.
    #[error("the server certificate cannot be read")]
    Certificate,
    /// The server is not known: the user decides, then connects again with
    /// [`RdpConfig::accepted`].
    #[error("unknown server certificate {}", .0.fingerprint)]
    UnknownCertificate(ServerCertificate),
    /// The server presents another key than the recorded one. Never asked about.
    #[error("the server certificate changed from {recorded} to {}", presented.fingerprint)]
    CertificateChanged {
        /// The key recorded, or accepted, for it.
        recorded: Fingerprint,
        /// What it presented.
        presented: ServerCertificate,
    },
    /// The file of known servers cannot be read or written.
    #[error("known RDP servers: {0}")]
    KnownHosts(#[source] io::Error),
    /// The credentials were refused.
    #[error("the server refused the credentials")]
    Authentication,
    /// The server asked for a desktop larger than [`MAX_DESKTOP_SIDE`].
    #[error("the server asked for a {width}x{height} desktop")]
    DesktopTooLarge {
        /// Width.
        width: u16,
        /// Height.
        height: u16,
    },
    /// The connection sequence failed otherwise.
    #[error("RDP: {0}")]
    Protocol(String),
}

/// An open connection, ready for its session.
pub struct RdpConnection {
    /// The stream, framed.
    pub framed: MovableTokioFramed<Upgraded>,
    /// What the connection sequence settled.
    pub result: ConnectionResult,
}

/// Opens a connection to `config.host`. The future can be spawned: the connection sequence
/// runs on a blocking thread of the current runtime, as `IronRDP`'s connector futures cannot
/// be shown to be `Send`.
///
/// # Errors
///
/// [`RdpError`]; [`RdpError::UnknownCertificate`] asks the caller to decide about the server.
pub async fn connect(
    config: RdpConfig,
    credentials: AskCredentials,
    cancel: CancellationToken,
) -> Result<RdpConnection, RdpError> {
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        runtime.block_on(connect_in_place(&config, credentials, &cancel))
    })
    .await
    .map_err(|error| RdpError::Protocol(error.to_string()))?
}

/// [`connect`], on the calling task: its future is not `Send`.
async fn connect_in_place(
    config: &RdpConfig,
    credentials: AskCredentials,
    cancel: &CancellationToken,
) -> Result<RdpConnection, RdpError> {
    // A server waits for the credentials only so long: Windows Server 2022 dropped a
    // connection between 60 and 90 s (measured on 2026-09-26). So a server whose identity is
    // settled gets them before the connection opens.
    let settled = config.accepted.is_some()
        || config
            .known_hosts
            .knows(&config.host, config.port)
            .map_err(RdpError::KnownHosts)?;
    let credentials = if settled {
        let (username, password) = tokio::select! {
            () = cancel.cancelled() => return Err(RdpError::Cancelled),
            answer = credentials() => answer.ok_or(RdpError::Cancelled)?,
        };
        given(username, password)
    } else {
        credentials
    };
    let tcp = phase(
        config.timeouts.connect,
        cancel,
        TcpStream::connect((config.host.as_str(), config.port)),
    )
    .await?
    .map_err(RdpError::Network)?;
    let client_addr = tcp.local_addr().map_err(RdpError::Network)?;
    Box::pin(connect_over(
        Box::new(tcp),
        client_addr,
        config,
        credentials,
        cancel,
    ))
    .await
}

/// Opens a connection over `stream`, already connected to the server.
///
/// # Errors
///
/// As [`connect`].
pub async fn connect_over(
    stream: Box<dyn Transport>,
    client_addr: SocketAddr,
    config: &RdpConfig,
    credentials: AskCredentials,
    cancel: &CancellationToken,
) -> Result<RdpConnection, RdpError> {
    // Display Control lets the desktop follow the size of the tab showing it.
    let mut connector = ClientConnector::new(connector_config(config), client_addr)
        .with_static_channel(
            DrdynvcClient::new()
                .with_dynamic_channel(DisplayControlClient::new(|_| Ok(Vec::new()))),
        );
    // Movable: its futures are `Send`, so a connection can run in a spawned task.
    let mut framed = MovableTokioFramed::new(stream);
    let should_upgrade = phase(
        config.timeouts.handshake,
        cancel,
        ironrdp_tokio::connect_begin(&mut framed, &mut connector),
    )
    .await?
    .map_err(|error| match error.kind() {
        // Before TLS, a stated reason is the negotiation failing: the server wants a
        // security protocol that was not offered.
        ConnectorErrorKind::Reason(reason) => RdpError::Negotiation(reason.clone()),
        _ => failure(&error),
    })?;
    let (stream, leftover) = framed.into_inner();
    if !leftover.is_empty() {
        return Err(RdpError::Protocol(
            "data before the TLS handshake".to_owned(),
        ));
    }

    let server_name = ServerName::try_from(config.host.clone())
        .map_err(|_| RdpError::Protocol("the server name is not valid".to_owned()))?;
    let tls = phase(
        config.timeouts.handshake,
        cancel,
        tls::connector().connect(server_name, stream),
    )
    .await?
    .map_err(RdpError::Tls)?;
    let der = tls
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certificates| certificates.first())
        .ok_or(RdpError::Certificate)?;
    // The pin and the key CredSSP binds to come from this one certificate.
    let certificate = ServerCertificate::from_der(der).map_err(|_| RdpError::Certificate)?;
    trust(config, certificate.clone())?;

    // The server is trusted: now, and only now, the credentials. No timeout: a person is
    // typing; the server may give up meanwhile, which then reads as a network failure.
    let (username, password) = tokio::select! {
        () = cancel.cancelled() => return Err(RdpError::Cancelled),
        answer = credentials() => answer.ok_or(RdpError::Cancelled)?,
    };
    // With a password in hand the server logs straight in, as mstsc does: a TLS-only server
    // such as xrdp would otherwise show its own login form after Heimdall's question.
    connector.config.autologon = !password.is_empty();
    // IronRDP and sspi hold the password as a plain String from here on.
    connector.config.credentials = connector::Credentials::UsernamePassword {
        username,
        password: password.to_string(),
    };
    drop(password);

    let upgraded = ironrdp_tokio::mark_as_upgraded(should_upgrade, &mut connector);
    let mut framed = MovableTokioFramed::new(tls);
    let result = phase(
        config.timeouts.logon,
        cancel,
        ironrdp_tokio::connect_finalize(
            upgraded,
            connector,
            &mut framed,
            &mut NoNetwork,
            connector::ServerName::new(config.host.clone()),
            certificate.public_key,
            None,
        ),
    )
    .await?
    .map_err(|error| failure(&error))?;
    let DesktopSize { width, height } = result.desktop_size;
    if width > MAX_DESKTOP_SIDE || height > MAX_DESKTOP_SIDE {
        return Err(RdpError::DesktopTooLarge { width, height });
    }
    Ok(RdpConnection { framed, result })
}

/// Decides about the server's key; `Ok` lets the credentials go.
fn trust(config: &RdpConfig, certificate: ServerCertificate) -> Result<(), RdpError> {
    let presented = certificate.fingerprint;
    let verdict = config
        .known_hosts
        .verdict(&config.host, config.port, &presented)
        .map_err(RdpError::KnownHosts)?;
    match (verdict, config.accepted) {
        (Verdict::Known, _) => Ok(()),
        (Verdict::Changed { recorded }, _) => Err(RdpError::CertificateChanged {
            recorded,
            presented: certificate,
        }),
        (Verdict::Unknown, Some(accepted)) if accepted == presented => config
            .known_hosts
            .record(&config.host, config.port, &presented)
            .map_err(RdpError::KnownHosts),
        // The key changed between the question and this connection.
        (Verdict::Unknown, Some(accepted)) => Err(RdpError::CertificateChanged {
            recorded: accepted,
            presented: certificate,
        }),
        (Verdict::Unknown, None) => Err(RdpError::UnknownCertificate(certificate)),
    }
}

fn connector_config(config: &RdpConfig) -> connector::Config {
    let (width, height) = config.desktop;
    connector::Config {
        // Replaced once the server is trusted; nothing reads it before.
        credentials: connector::Credentials::UsernamePassword {
            username: String::new(),
            password: String::new(),
        },
        domain: config.domain.clone(),
        // Network Level Authentication unless the profile allows plain TLS: offering TLS lets a
        // server, or anyone in between, pick it and receive the password in the logon packet.
        enable_tls: config.security == Security::NlaOrTls,
        enable_credssp: true,
        keyboard_type: KeyboardType::IbmEnhanced,
        keyboard_subtype: 0,
        keyboard_layout: config.keyboard_layout,
        keyboard_functional_keys_count: FUNCTION_KEYS,
        ime_file_name: String::new(),
        dig_product_id: String::new(),
        desktop_size: DesktopSize { width, height },
        bitmap: None,
        client_build: 0,
        client_name: CLIENT_NAME.to_owned(),
        client_dir: String::new(),
        platform: if cfg!(windows) {
            MajorPlatformType::WINDOWS
        } else {
            MajorPlatformType::UNIX
        },
        enable_server_pointer: false,
        request_data: Some(NegoRequestData::cookie(NEGOTIATION_COOKIE.to_owned())),
        autologon: false,
        enable_audio_playback: false,
        compression_type: None,
        pointer_software_rendering: true,
        multitransport_flags: None,
        performance_flags: PerformanceFlags::default(),
        desktop_scale_factor: 0,
        hardware_id: None,
        license_cache: None,
        timezone_info: TimezoneInfo::default(),
        alternate_shell: String::new(),
        work_dir: String::new(),
    }
}

/// `future`, bounded by `limit` and stopped by `cancel`.
async fn phase<T>(
    limit: Duration,
    cancel: &CancellationToken,
    future: impl Future<Output = T>,
) -> Result<T, RdpError> {
    tokio::select! {
        () = cancel.cancelled() => Err(RdpError::Cancelled),
        outcome = tokio::time::timeout(limit, future) => outcome.map_err(|_| RdpError::Timeout),
    }
}

/// The category of a connector failure.
fn failure(error: &ConnectorError) -> RdpError {
    match error.kind() {
        ConnectorErrorKind::Credssp(_) | ConnectorErrorKind::AccessDenied => {
            RdpError::Authentication
        }
        ConnectorErrorKind::Negotiation(failure) => RdpError::Negotiation(failure.to_string()),
        _ => {
            let mut source = std::error::Error::source(error);
            while let Some(cause) = source {
                if let Some(io) = cause.downcast_ref::<io::Error>() {
                    return RdpError::Network(io::Error::new(io.kind(), io.to_string()));
                }
                source = cause.source();
            }
            // The kind, not the whole error: its text carries a source file location.
            RdpError::Protocol(error.kind().to_string())
        }
    }
}

/// Kerberos is not offered, so `CredSSP` never needs the network: NTLM runs over the RDP
/// connection itself.
struct NoNetwork;

impl NetworkClient for NoNetwork {
    fn send(
        &mut self,
        _request: &NetworkRequest,
    ) -> impl Future<Output = ConnectorResult<Vec<u8>>> {
        std::future::ready(Err(ConnectorError::new(
            "network requests are not supported: Kerberos is not offered",
            ConnectorErrorKind::General,
        )))
    }
}
