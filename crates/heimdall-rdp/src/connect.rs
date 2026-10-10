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
//! instead of the user name, and `CredSSP` runs only once the server's certificate is the one
//! recorded, or the one the user just accepted.
//!
//! A server is pinned by its whole certificate, as the C# pins it by its thumbprint
//! (`RdpCertificateTrust.Decide`): another certificate on the key trusted, renewed or minted
//! again by whoever holds the key, is asked about again, said to be a renewal; another key is
//! the changed-key alarm. A server recorded by an earlier Heimdall, which pinned keys alone,
//! is let through on its key as before, and the certificate it presents is adopted once the
//! connection is up: from then on, that certificate only.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use heimdall_core::profile::{AudioPlayback, RdpOptions};
use ironrdp::connector::credssp::{CredsspSequence, KerberosConfig};
use ironrdp::connector::sspi::generator::GeneratorState;
use ironrdp::connector::sspi::{self, credssp::ClientState};
use ironrdp::connector::{
    self, BitmapConfig, ClientConnector, ClientConnectorState, ConnectionResult, ConnectorError,
    ConnectorErrorKind, ConnectorResult, DesktopSize,
};
use ironrdp::core::WriteBuf;
use ironrdp::displaycontrol::client::DisplayControlClient;
use ironrdp::dvc::DrdynvcClient;
use ironrdp::pdu::gcc::KeyboardType;
use ironrdp::pdu::nego::NegoRequestData;
use ironrdp::pdu::rdp::capability_sets::{MajorPlatformType, client_codecs_capabilities};
use ironrdp::pdu::rdp::client_info::{PerformanceFlags, TimezoneInfo};
use ironrdp_tokio::{Framed, FramedRead, FramedWrite, MovableTokioFramed};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use ironrdp::cliprdr::CliprdrClient;
use ironrdp::rdpdr::Rdpdr;
use ironrdp::rdpsnd::client::{NoopRdpsndBackend, Rdpsnd, RdpsndClientHandler};
use tokio::sync::mpsc;

use crate::certificate::{CertificateHash, Fingerprint, ServerCertificate, Validity};
use crate::clipboard::{ClipboardBackend, Offered, Request};
use crate::drives::{DriveBackend, SharedDrive};
use crate::known_hosts::{CertificateVerdict, KnownRdpHosts};
use crate::reason::{self, Ending, Refusal};
use crate::time_zone::TimeZone;
use crate::{kdc, tls};

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

/// A certificate the user accepted for a server after the certificate question: recorded if
/// the server presents exactly it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedCertificate {
    /// Its key: another key presented is the changed-key alarm.
    pub key: Fingerprint,
    /// The hash of the whole of it: what is recorded.
    pub certificate: CertificateHash,
}

impl From<&ServerCertificate> for AcceptedCertificate {
    fn from(certificate: &ServerCertificate) -> Self {
        Self {
            key: certificate.fingerprint,
            certificate: certificate.certificate,
        }
    }
}

/// Where to connect.
#[derive(Debug, Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one switch per choice of the connection, each set on its own"
)]
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
    /// Several machines answer at the address: a certificate not trusted yet is asked
    /// about rather than refused as changed.
    pub several_servers: bool,
    /// Never ask about a certificate not trusted yet: one this computer's certificate
    /// authorities validate for the server's name goes through, any other is refused, as
    /// the C# `RdpStrictServerAuthentication`.
    pub strict_server_authentication: bool,
    /// Pins of the servers trusted so far.
    pub known_hosts: KnownRdpHosts,
    /// The certificate the user accepted for this server after an
    /// [`RdpError::UnknownCertificate`] or an [`RdpError::RenewedCertificate`]: recorded if
    /// the server presents exactly it.
    pub accepted: Option<AcceptedCertificate>,
    /// Phase timeouts.
    pub timeouts: Timeouts,
    /// Share the clipboard with the server, text only: its copies reach this side, and this
    /// side's text is offered to it.
    pub clipboard: bool,
    /// Drives of this computer the server may read and write; none when empty.
    pub drives: Vec<SharedDrive>,
    /// Certificates the user trusted for this server for this run only, by the hash of the
    /// whole of each, as the C# Heimdall's "Just this once": accepted, never recorded.
    pub trusted_for_run: Vec<CertificateHash>,
    /// Colour depth, sound and administrative session asked for.
    pub options: RdpOptions,
    /// Network Level Authentication may log on with Kerberos, the domain's KDC being reached
    /// from this computer, as mstsc does; NTLM when no KDC is found or answers. Only for a
    /// server reached directly: through a tunnel, the KDC is on the far side too.
    pub kerberos: bool,
    /// This computer's time zone, which a server redirecting time zones gives the session, as
    /// mstsc tells it; `None` tells UTC, the server's own time then shown.
    pub time_zone: Option<TimeZone>,
    /// The desktop scale factor asked for, in percent, as the C# Heimdall maps the screen's
    /// (see [`desktop_scale_factor`]): 100 draws the remote desktop at its own size.
    pub desktop_scale: u32,
    /// Told how far the connection has come, as the C# session header's phase stepper
    /// shows it; `None` when nobody follows it.
    pub progress: Option<Progress>,
}

/// How far a connection has come, past its opening: what the C# session header's phase
/// stepper calls Connecting, then Loading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The server answered: the security exchange, TLS, the certificate and the logon.
    Connecting,
    /// Logged on: the session is being set up, until its desktop shows.
    Loading,
}

/// Where a connection reports its [`Step`]s, called from the connection's own task.
#[derive(Clone)]
pub struct Progress(Arc<dyn Fn(Step) + Send + Sync>);

impl Progress {
    /// Reports each step to `report`.
    pub fn new(report: impl Fn(Step) + Send + Sync + 'static) -> Self {
        Self(Arc::new(report))
    }

    /// Tells `step`.
    fn report(&self, step: Step) {
        (self.0)(step);
    }
}

impl std::fmt::Debug for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Progress")
    }
}

/// Tells `step` to whoever follows `config`'s connection.
fn report(config: &RdpConfig, step: Step) {
    if let Some(progress) = &config.progress {
        progress.report(step);
    }
}

/// The desktop scale factors a server is offered, in percent, as the C# `RdpDisplayHelper`
/// lists them.
pub const DESKTOP_SCALE_FACTORS: [u32; 9] = [100, 125, 150, 175, 200, 250, 300, 400, 500];

/// The desktop scale factor for a screen drawing `scale` physical pixels per logical one:
/// the nearest the server is offered, as the C# `MapDpiToDesktopScaleFactor`.
#[must_use]
pub fn desktop_scale_factor(scale: f32) -> u32 {
    let percent = (scale * 100.0).round();
    DESKTOP_SCALE_FACTORS
        .into_iter()
        .min_by(|a, b| {
            #[allow(
                clippy::cast_precision_loss,
                reason = "factors up to 500, exact in an f32"
            )]
            let distance = |factor: u32| (factor as f32 - percent).abs();
            distance(*a).total_cmp(&distance(*b))
        })
        .unwrap_or(100)
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
    #[error("the server refused the security requested: {detail}")]
    Negotiation {
        /// What the server answered, in `IronRDP`'s words.
        detail: String,
        /// The failure code of its RDP Negotiation Failure (MS-RDPBCGR 2.2.1.2.2), when it
        /// sent one.
        code: Option<u32>,
    },
    /// The TLS handshake failed, a forged certificate included.
    #[error("TLS failed: {0}")]
    Tls(#[source] io::Error),
    /// The server's certificate cannot be read.
    #[error("the server certificate cannot be read")]
    Certificate,
    /// Strict server authentication: the certificate is neither trusted yet nor validated
    /// by this computer's certificate authorities.
    #[error("the server's identity cannot be authenticated")]
    ServerNotAuthenticated,
    /// The server is not known: the user decides, then connects again with
    /// [`RdpConfig::accepted`]. Boxed: the error stays small.
    #[error("unknown server certificate {}", .0.fingerprint)]
    UnknownCertificate(Box<ServerCertificate>),
    /// The server presents another certificate on a key trusted, renewed or minted again by
    /// whoever holds the key: the user decides, as for [`RdpError::UnknownCertificate`],
    /// told it is a renewal.
    #[error("renewed server certificate {} on the key {}", .presented.certificate, .presented.fingerprint)]
    RenewedCertificate {
        /// What it presented, boxed as in [`RdpError::UnknownCertificate`].
        presented: Box<ServerCertificate>,
        /// When the certificate on record holds, when recorded.
        recorded: Option<Validity>,
    },
    /// The server presents another key than the recorded one. Never asked about.
    #[error("the server certificate changed from {recorded} to {}", presented.fingerprint)]
    CertificateChanged {
        /// The key recorded, or accepted, for it.
        recorded: Fingerprint,
        /// What it presented, boxed as in [`RdpError::UnknownCertificate`].
        presented: Box<ServerCertificate>,
    },
    /// The file of known servers cannot be read or written.
    #[error("known RDP servers: {0}")]
    KnownHosts(#[source] io::Error),
    /// The server refused the logon, and why.
    #[error("the server refused the logon: {refusal:?}")]
    Authentication {
        /// Why.
        refusal: Refusal,
        /// The NTSTATUS the server refused it with, when it gave one.
        status: Option<u32>,
    },
    /// The server ended the connection before its session started, and why.
    #[error("the server ended the connection: {ending:?}")]
    Ended {
        /// Why.
        ending: Ending,
        /// The code of its Set Error Info PDU (MS-RDPBCGR 2.2.5.1.1), when known.
        code: Option<u32>,
    },
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

/// What the session needs of the clipboard channel.
pub(crate) struct ClipboardLink {
    /// What the channel's backend asks the session to do.
    pub(crate) requests: mpsc::UnboundedReceiver<Request>,
    /// The text this side offers.
    pub(crate) offered: Offered,
}

/// An open connection, ready for its session.
pub struct RdpConnection {
    /// The stream, framed.
    pub framed: MovableTokioFramed<Upgraded>,
    /// The clipboard channel's side of the session, when the clipboard is shared.
    pub(crate) clipboard: Option<ClipboardLink>,
    /// What the connection sequence settled.
    pub result: ConnectionResult,
    /// The desktop scale factor asked for, kept for each later resize.
    pub(crate) desktop_scale: u32,
}

/// How the bytes reach the server: the stream once open, and the address this side reports
/// to the server as its own.
pub type Opening =
    Pin<Box<dyn Future<Output = io::Result<(Box<dyn Transport>, SocketAddr)>> + Send>>;

/// Opens a connection to `config.host` over TCP. The future can be spawned: the connection
/// sequence runs on a blocking thread of the current runtime, as `IronRDP`'s connector
/// futures cannot be shown to be `Send`.
///
/// # Errors
///
/// [`RdpError`]; [`RdpError::UnknownCertificate`] asks the caller to decide about the server.
pub async fn connect(
    config: RdpConfig,
    credentials: AskCredentials,
    cancel: CancellationToken,
) -> Result<RdpConnection, RdpError> {
    let host = config.host.clone();
    let port = config.port;
    let tcp: Opening = Box::pin(async move {
        let tcp = TcpStream::connect((host.as_str(), port)).await?;
        let local = tcp.local_addr()?;
        Ok((Box::new(tcp) as Box<dyn Transport>, local))
    });
    let limit = config.timeouts.connect;
    connect_through(config, credentials, cancel, tcp, Some(limit)).await
}

/// [`connect`], with the stream opened by `opening` instead of a TCP connection of its own:
/// through an SSH tunnel, for one. The server is still `config.host` for its certificate and
/// for the logon: only the way to it changes.
///
/// `limit` bounds the opening; `None` leaves it to `opening`, which may wait for a person (a
/// gateway's password) and bounds its own steps.
///
/// # Errors
///
/// As [`connect`]; a failure of `opening` is [`RdpError::Network`].
pub async fn connect_through(
    config: RdpConfig,
    credentials: AskCredentials,
    cancel: CancellationToken,
    opening: Opening,
    limit: Option<Duration>,
) -> Result<RdpConnection, RdpError> {
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        runtime.block_on(connect_in_place(
            &config,
            credentials,
            &cancel,
            opening,
            limit,
        ))
    })
    .await
    .map_err(|error| RdpError::Protocol(error.to_string()))?
}

/// [`connect_through`], on the calling task: its future is not `Send`.
async fn connect_in_place(
    config: &RdpConfig,
    credentials: AskCredentials,
    cancel: &CancellationToken,
    opening: Opening,
    limit: Option<Duration>,
) -> Result<RdpConnection, RdpError> {
    // A server waits for the credentials only so long: Windows Server 2022 dropped a
    // connection between 60 and 90 s (measured on 2026-09-26). So a server whose identity is
    // settled gets them before the connection opens.
    let settled = config.accepted.is_some()
        || !config.trusted_for_run.is_empty()
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
    let opened = if let Some(limit) = limit {
        phase(limit, cancel, opening).await?
    } else {
        tokio::select! {
            () = cancel.cancelled() => return Err(RdpError::Cancelled),
            opened = opening => opened,
        }
    };
    let (stream, client_addr) = opened.map_err(RdpError::Network)?;
    Box::pin(connect_over(
        stream,
        client_addr,
        config,
        credentials,
        cancel,
    ))
    .await
}

/// Attaches the sound channel and the device one, always together: a Windows server opens
/// the device channel only beside the sound one, and starts the sound only through the
/// device one. The sound is played here when the profile asks and this computer can; else
/// the channel declines it. The device channel announces the shared drives, maybe none.
fn attach_sound_and_drives(connector: &mut ClientConnector, config: &RdpConfig) {
    let speakers = (config.options.audio == AudioPlayback::Local)
        .then(crate::audio::local_speakers)
        .flatten()
        .map(|backend| Box::new(backend) as Box<dyn RdpsndClientHandler>);
    if let Some((sound, devices)) = sound_and_devices(speakers, &config.drives) {
        connector.attach_static_channel(sound);
        connector.attach_static_channel(devices);
    }
}

/// The sound and device channels, as a pair or not at all: none when there is neither
/// sound to play here nor drive to share.
fn sound_and_devices(
    speakers: Option<Box<dyn RdpsndClientHandler>>,
    drives: &[SharedDrive],
) -> Option<(Rdpsnd, Rdpdr)> {
    if speakers.is_none() && drives.is_empty() {
        return None;
    }
    let sound = Rdpsnd::new(speakers.unwrap_or_else(|| Box::new(NoopRdpsndBackend)));
    let backend = DriveBackend::new(drives);
    let devices = backend.devices();
    Some((
        sound,
        Rdpdr::new(Box::new(backend), CLIENT_NAME.to_owned()).with_drives(Some(devices)),
    ))
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
    report(config, Step::Connecting);
    // Display Control lets the desktop follow the size of the tab showing it.
    let mut connector = ClientConnector::new(connector_config(config), client_addr)
        .with_static_channel(
            DrdynvcClient::new()
                .with_dynamic_channel(DisplayControlClient::new(|_| Ok(Vec::new()))),
        );
    let clipboard = config.clipboard.then(|| {
        let (requests, received) = mpsc::unbounded_channel();
        let offered = Offered::default();
        connector.attach_static_channel(CliprdrClient::new(Box::new(ClipboardBackend::new(
            requests,
            offered.clone(),
        ))));
        ClipboardLink {
            requests: received,
            offered,
        }
    });
    attach_sound_and_drives(&mut connector, config);
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
        ConnectorErrorKind::Reason(reason) => RdpError::Negotiation {
            detail: reason.clone(),
            code: None,
        },
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
    let system = config
        .strict_server_authentication
        .then(tls::SystemTrust::default);
    let tls = phase(
        config.timeouts.handshake,
        cancel,
        tls::connector(system.as_ref()).connect(server_name, stream),
    )
    .await?
    .map_err(RdpError::Tls)?;
    let (certificate, passed) = checked(&tls, config, system.as_ref())?;
    let pin = (
        certificate.fingerprint,
        certificate.certificate,
        certificate.validity,
    );

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

    ironrdp_tokio::mark_as_upgraded(should_upgrade, &mut connector);
    let mut framed = MovableTokioFramed::new(tls);
    let result = phase(
        config.timeouts.logon,
        cancel,
        finalize(
            connector,
            &mut framed,
            config,
            config.kerberos,
            connector::ServerName::new(config.host.clone()),
            certificate.public_key,
        ),
    )
    .await?
    .map_err(|error| failure(&error))?;
    let DesktopSize { width, height } = result.desktop_size;
    if width > MAX_DESKTOP_SIDE || height > MAX_DESKTOP_SIDE {
        return Err(RdpError::DesktopTooLarge { width, height });
    }
    if passed == Passed::KeyOnly {
        adopt(config, pin)?;
    }
    Ok(RdpConnection {
        framed,
        clipboard,
        result,
        desktop_scale: config.desktop_scale,
    })
}

/// The certificate the server presented over `tls`, read, and what the pin check let
/// through; `system`, the system's check when strict server authentication asked for it.
fn checked(
    tls: &Upgraded,
    config: &RdpConfig,
    system: Option<&tls::SystemTrust>,
) -> Result<(ServerCertificate, Passed), RdpError> {
    let der = tls
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certificates| certificates.first())
        .ok_or(RdpError::Certificate)?;
    // The pin and the key CredSSP binds to come from this one certificate.
    let certificate = ServerCertificate::from_der(der).map_err(|_| RdpError::Certificate)?;
    let passed = trust(
        config,
        &certificate,
        system.is_some_and(tls::SystemTrust::validated),
    )?;
    Ok((certificate, passed))
}

/// What the pin check let through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Passed {
    /// A certificate trusted, or accepted: nothing left to do.
    Trusted,
    /// A certificate on a key recorded alone, by a Heimdall that pinned keys alone: let
    /// through on its key, as before, and adopted once the connection is up.
    KeyOnly,
}

/// Decides about the server's certificate; `Ok` lets the credentials go. `validated`: this
/// computer's certificate authorities validate its chain for the server's name. Each
/// decision but a question, which the caller asks, is said in the diagnostics log.
///
/// It runs once the TLS handshake is done, every handshake signature verified against the
/// key of `certificate` ([`tls::connector`]): the server proved it holds that key, so a
/// certificate minted again on the key by someone without it never gets this far.
fn trust(
    config: &RdpConfig,
    certificate: &ServerCertificate,
    validated: bool,
) -> Result<Passed, RdpError> {
    let (presented, whole) = (certificate.fingerprint, certificate.certificate);
    let target = heimdall_core::profile::display_address(&config.host, config.port);
    let verdict = match config
        .known_hosts
        .certificate_verdict(&config.host, config.port, &presented, &whole)
        .map_err(RdpError::KnownHosts)?
    {
        // Several machines answer at the address: a key not trusted yet is another machine
        // to ask about, trusted beside the others, not a change.
        CertificateVerdict::Changed { .. } if config.several_servers => CertificateVerdict::Unknown,
        verdict => verdict,
    };
    let verdict = match verdict {
        CertificateVerdict::Known => {
            log::info!("the certificate of {target} is the one trusted: {whole}, key {presented}");
            return Ok(Passed::Trusted);
        }
        CertificateVerdict::KeyOnly => {
            log::info!(
                "the certificate of {target} carries the key trusted alone {presented}: {whole}, adopted once the connection is up"
            );
            return Ok(Passed::KeyOnly);
        }
        CertificateVerdict::Changed { recorded } => {
            log::warn!(
                "the certificate of {target} changed: trusted {recorded}, presented {presented}: refused"
            );
            return Err(RdpError::CertificateChanged {
                recorded,
                presented: Box::new(certificate.clone()),
            });
        }
        asked @ (CertificateVerdict::Renewed { .. } | CertificateVerdict::Unknown) => asked,
    };
    unsettled(config, certificate, validated, verdict)
}

/// Decides about a certificate not trusted yet, as `verdict` says: never seen, or renewed on a
/// key trusted. Trusted for this run, accepted just now, decided by the system under strict
/// authentication, else asked about.
fn unsettled(
    config: &RdpConfig,
    certificate: &ServerCertificate,
    validated: bool,
    verdict: CertificateVerdict,
) -> Result<Passed, RdpError> {
    let (presented, whole) = (certificate.fingerprint, certificate.certificate);
    let target = heimdall_core::profile::display_address(&config.host, config.port);
    if config.trusted_for_run.contains(&whole) {
        log::info!("the certificate of {target} is trusted for this run: {whole}, key {presented}");
        return Ok(Passed::Trusted);
    }
    match config.accepted {
        Some(accepted) if accepted.certificate == whole => {
            config
                .known_hosts
                .record_whole_certificate(
                    &config.host,
                    config.port,
                    certificate,
                    (&whole, Some(&certificate.validity)),
                )
                .map_err(RdpError::KnownHosts)?;
            log::info!(
                "the certificate of {target} accepted by the user is recorded: {whole}, key {presented}"
            );
            return Ok(Passed::Trusted);
        }
        // The key changed between the question and this connection.
        Some(accepted) if accepted.key != presented => {
            log::warn!(
                "the certificate of {target} changed since it was accepted: accepted {}, presented {presented}: refused",
                accepted.key
            );
            return Err(RdpError::CertificateChanged {
                recorded: accepted.key,
                presented: Box::new(certificate.clone()),
            });
        }
        // The same key, another certificate than the one accepted: asked about again.
        Some(_) | None => {}
    }
    // Strict: never asked about; the system's certificate authorities decide.
    if config.strict_server_authentication {
        if validated {
            log::info!(
                "the certificate of {target} is validated by this computer's authorities: {presented}"
            );
            return Ok(Passed::Trusted);
        }
        log::warn!(
            "the certificate of {target} is not validated by this computer's authorities: refused"
        );
        return Err(RdpError::ServerNotAuthenticated);
    }
    Err(match verdict {
        CertificateVerdict::Renewed { recorded } => {
            log::info!(
                "{target} presented a renewed certificate {whole}: the same key {presented}, another certificate than the one trusted"
            );
            RdpError::RenewedCertificate {
                presented: Box::new(certificate.clone()),
                recorded,
            }
        }
        _ => RdpError::UnknownCertificate(Box::new(certificate.clone())),
    })
}

/// The connection is up, its certificate let through on a key an earlier Heimdall recorded
/// alone: adopts that certificate, `(key, hash, validity)`, for the server, so the next
/// connections take that certificate only.
///
/// Only now, for what the server proved by now: its TLS handshake was signed with the
/// private key of the certificate ([`tls::connector`] verifies every handshake signature),
/// and the connection sequence went through to its end, `CredSSP` included under Network
/// Level Authentication, which binds the logon to that very key. A connection that did not
/// get that far adopts nothing: the next one is checked on the key again, as before.
fn adopt(
    config: &RdpConfig,
    (key, whole, validity): (Fingerprint, CertificateHash, Validity),
) -> Result<(), RdpError> {
    let adopted = config
        .known_hosts
        .adopt_certificate(&config.host, config.port, &key, (&whole, Some(&validity)))
        .map_err(RdpError::KnownHosts)?;
    if adopted {
        log::info!(
            "the RDP server {}, trusted by its key {key} alone, is now pinned by its whole certificate {whole}",
            heimdall_core::profile::display_address(&config.host, config.port)
        );
    }
    Ok(())
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
        // What `None` gives, the colour depth aside.
        bitmap: Some(BitmapConfig {
            lossy_compression: false,
            color_depth: config.options.color_depth.bits(),
            codecs: client_codecs_capabilities(&[]).expect("no codec named, none unknown"),
        }),
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
        // Played here when the profile asks; else kept on the server, or not played at all.
        enable_audio_playback: config.options.audio == AudioPlayback::Local,
        remote_console_audio: config.options.audio == AudioPlayback::OnServer,
        console_session: config.options.admin_session,
        compression_type: None,
        pointer_software_rendering: true,
        multitransport_flags: None,
        performance_flags: performance_flags(config.options.performance_flags),
        desktop_scale_factor: config.desktop_scale,
        hardware_id: None,
        license_cache: None,
        timezone_info: config
            .time_zone
            .as_ref()
            .map_or_else(TimezoneInfo::default, TimeZone::info),
        alternate_shell: String::new(),
        work_dir: String::new(),
    }
}

/// The performance flags the server is sent: the profile's as they are, as the C# control
/// takes them; when no box is ticked, `IronRDP`'s balance, the experience given so far (font
/// smoothing on, window contents and menu animations off while they move).
fn performance_flags(profile: u32) -> PerformanceFlags {
    if profile == 0 {
        PerformanceFlags::default()
    } else {
        PerformanceFlags::from_bits_retain(profile)
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
    // A Set Error Info while the connection finishes: a licence, a denied account.
    if let ConnectorErrorKind::Reason(text) = error.kind()
        && let Some(ending) = reason::ending_in_failure(text)
    {
        return RdpError::Ended {
            ending,
            code: reason::error_info_code(text),
        };
    }
    match error.kind() {
        ConnectorErrorKind::Credssp(error) => RdpError::Authentication {
            refusal: Refusal::of(error),
            status: reason::status(error),
        },
        // Early User Authorization: the account may not open a session there.
        ConnectorErrorKind::AccessDenied => RdpError::Authentication {
            refusal: Refusal::BadCredentials,
            status: None,
        },
        ConnectorErrorKind::Negotiation(failure) => RdpError::Negotiation {
            detail: failure.to_string(),
            code: Some(u32::from(failure.code())),
        },

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

/// The connection sequence from Network Level Authentication on, as `IronRDP`'s
/// `connect_finalize` runs it but for one thing: a KDC that cannot be reached is reported to
/// sspi, which then logs on with NTLM, instead of ending the connection.
async fn finalize<S: FramedRead + FramedWrite>(
    mut connector: ClientConnector,
    framed: &mut Framed<S>,
    config: &RdpConfig,
    kerberos: bool,
    server_name: connector::ServerName,
    server_public_key: Vec<u8>,
) -> ConnectorResult<ConnectionResult> {
    let mut buf = WriteBuf::new();
    if connector.should_perform_credssp() {
        credssp(
            &mut connector,
            framed,
            &mut buf,
            kerberos,
            server_name,
            server_public_key,
        )
        .await?;
    }
    // Logged on: what is left sets the session up, as the C# Loading phase after its
    // login completes.
    report(config, Step::Loading);
    loop {
        ironrdp_tokio::single_sequence_step(framed, &mut connector, &mut buf).await?;
        if let ClientConnectorState::Connected { result } = connector.state {
            return Ok(result);
        }
    }
}

/// Network Level Authentication: `CredSSP` over the TLS stream, its KDC requests sent from
/// here.
async fn credssp<S: FramedRead + FramedWrite>(
    connector: &mut ClientConnector,
    framed: &mut Framed<S>,
    buf: &mut WriteBuf,
    kerberos: bool,
    server_name: connector::ServerName,
    server_public_key: Vec<u8>,
) -> ConnectorResult<()> {
    let ClientConnectorState::Credssp { selected_protocol } = connector.state else {
        return Err(ConnectorError::new(
            "CredSSP outside its step",
            ConnectorErrorKind::General,
        ));
    };
    // Negotiate, Kerberos first, when offered; NTLM alone otherwise.
    let kerberos = kerberos.then(|| KerberosConfig {
        kdc_proxy_url: None,
        hostname: CLIENT_NAME.to_owned(),
    });
    let (mut sequence, mut request) = CredsspSequence::init(
        connector.config.credentials.clone(),
        connector.config.domain.as_deref(),
        selected_protocol,
        server_name,
        server_public_key,
        kerberos,
    )?;
    // Requests sent: a server closing after the second, which carries the account, refused
    // it. Servers before `CredSSP` version 3 say nothing else.
    let mut sent = 0_u32;
    loop {
        let state = {
            let mut generator = sequence.process_ts_request(request);
            let mut step = generator.start();
            loop {
                match step {
                    GeneratorState::Suspended(network) => {
                        let reply = kdc::send(&network).await.map_err(|error| {
                            sspi::Error::new(
                                sspi::ErrorKind::NoAuthenticatingAuthority,
                                error.to_string(),
                            )
                        });
                        step = generator.resume(reply);
                    }
                    GeneratorState::Completed(state) => break state,
                }
            }
        };
        let state: ClientState = state
            .map_err(|error| ConnectorError::new("CredSSP", ConnectorErrorKind::Credssp(error)))?;
        buf.clear();
        let written = sequence.handle_process_result(state, buf)?;
        if let Some(length) = written.size() {
            framed
                .write_all(&buf[..length])
                .await
                .map_err(|error| ironrdp::connector::custom_err!("write all", error))?;
            sent += 1;
        }
        let Some(hint) = sequence.next_pdu_hint() else {
            break;
        };
        let pdu = framed.read_by_hint(hint).await.map_err(|error| {
            if sent >= ACCOUNT_REQUEST && closed(&error) {
                ConnectorError::new(
                    "CredSSP",
                    ConnectorErrorKind::Credssp(sspi::Error::new(
                        sspi::ErrorKind::LogonDenied,
                        "the server closed the connection after the account was sent",
                    )),
                )
            } else {
                ironrdp::connector::custom_err!("read frame by hint", error)
            }
        })?;
        match sequence.decode_server_message(&pdu)? {
            Some(next) => request = next,
            None => break,
        }
    }
    connector.mark_credssp_as_done();
    Ok(())
}

/// The `CredSSP` request that carries the account, counted from 1: the NTLM authenticate
/// message, after the negotiate one.
const ACCOUNT_REQUEST: u32 = 2;

/// Whether `error` is the server closing the connection.
fn closed(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture certificate of the trust tests, and another one on its key.
    const CERT: &[u8] = include_bytes!("../tests/fixtures/server-cert.der");
    const RENEWED: &[u8] = include_bytes!("../tests/fixtures/renewed-cert.der");
    const HOST: &str = "rdp.test";
    const PORT: u16 = 3389;

    /// A connection to [`HOST`]:[`PORT`] checked against the pins of `known`.
    fn pinned_by(known: &std::path::Path) -> RdpConfig {
        RdpConfig {
            host: HOST.to_owned(),
            port: PORT,
            domain: None,
            desktop: (1024, 768),
            keyboard_layout: 0,
            security: Security::Nla,
            several_servers: false,
            strict_server_authentication: false,
            known_hosts: KnownRdpHosts::new(known),
            accepted: None,
            timeouts: Timeouts::default(),
            clipboard: false,
            drives: Vec::new(),
            trusted_for_run: Vec::new(),
            options: RdpOptions::default(),
            kerberos: false,
            time_zone: None,
            desktop_scale: 100,
            progress: None,
        }
    }

    #[test]
    fn a_key_recorded_alone_adopts_the_certificate_once_the_connection_is_up_then_is_strict() {
        let dir = tempfile::tempdir().expect("dir");
        let known = dir.path().join("known_rdp_hosts");
        let certificate = ServerCertificate::from_der(CERT).expect("fixture");
        let renewed = ServerCertificate::from_der(RENEWED).expect("fixture");
        assert_eq!(renewed.fingerprint, certificate.fingerprint, "the same key");
        // As a Heimdall that pinned keys alone recorded it.
        let file = KnownRdpHosts::new(&known);
        file.record(HOST, PORT, &certificate.fingerprint)
            .expect("legacy");
        let config = pinned_by(&known);
        let whole = |file: &KnownRdpHosts| {
            let [entry] = file.entries().expect("read").try_into().expect("one entry");
            (entry.certificate, entry.validity)
        };

        // Checked on its key, as before; nothing written by the check.
        assert_eq!(
            trust(&config, &certificate, false).ok(),
            Some(Passed::KeyOnly)
        );
        assert_eq!(whole(&file), (None, None));
        // The connection up: the certificate it presented is adopted, with its validity.
        adopt(
            &config,
            (
                certificate.fingerprint,
                certificate.certificate,
                certificate.validity,
            ),
        )
        .expect("adopted");
        assert_eq!(
            whole(&file),
            (Some(certificate.certificate), Some(certificate.validity))
        );

        // From then on, that certificate only: another on the key is a renewal to ask about.
        assert_eq!(
            trust(&config, &certificate, false).ok(),
            Some(Passed::Trusted)
        );
        assert!(
            matches!(
                trust(&config, &renewed, false),
                Err(RdpError::RenewedCertificate { recorded: Some(recorded), .. })
                    if recorded == certificate.validity
            ),
            "never adopted in its turn"
        );
        // Adopting again changes nothing: the line names its certificate now.
        let text = std::fs::read_to_string(&known).expect("file");
        adopt(
            &config,
            (renewed.fingerprint, renewed.certificate, renewed.validity),
        )
        .expect("nothing to adopt");
        assert_eq!(std::fs::read_to_string(&known).expect("file"), text);
    }

    #[test]
    fn a_renewal_under_strict_authentication_is_never_asked_about() {
        let dir = tempfile::tempdir().expect("dir");
        let known = dir.path().join("known_rdp_hosts");
        let certificate = ServerCertificate::from_der(CERT).expect("fixture");
        let renewed = ServerCertificate::from_der(RENEWED).expect("fixture");
        KnownRdpHosts::new(&known)
            .record_whole_certificate(
                HOST,
                PORT,
                &certificate,
                (&certificate.certificate, Some(&certificate.validity)),
            )
            .expect("pinned");
        let strict = RdpConfig {
            strict_server_authentication: true,
            ..pinned_by(&known)
        };
        // As the C# `RdpStrictServerAuthentication`: the system's authorities decide.
        assert!(matches!(
            trust(&strict, &renewed, false),
            Err(RdpError::ServerNotAuthenticated)
        ));
        assert_eq!(trust(&strict, &renewed, true).ok(), Some(Passed::Trusted));
        assert_eq!(
            trust(&strict, &certificate, false).ok(),
            Some(Passed::Trusted)
        );
    }

    #[test]
    fn a_screen_scale_is_the_nearest_factor_a_server_is_offered_as_the_csharp_maps_it() {
        for (scale, factor) in [
            (1.0, 100),
            (1.1, 100),
            (1.25, 125),
            (1.5, 150),
            (1.75, 175),
            (2.0, 200),
            (2.2, 200),
            (3.0, 300),
            (6.0, 500),
            (0.5, 100),
        ] {
            assert_eq!(desktop_scale_factor(scale), factor, "{scale}");
        }
    }

    #[test]
    fn sound_played_here_brings_the_device_channel_without_any_drive() {
        // A Windows server starts the sound only through the device channel.
        assert!(sound_and_devices(Some(Box::new(NoopRdpsndBackend)), &[]).is_some());
    }

    #[test]
    fn a_shared_drive_brings_the_sound_channel_without_sound_played_here() {
        let drive = SharedDrive {
            name: "C".to_owned(),
            root: std::env::temp_dir(),
        };
        assert!(sound_and_devices(None, &[drive]).is_some());
    }

    #[test]
    fn neither_sound_nor_drive_opens_neither_channel() {
        assert!(sound_and_devices(None, &[]).is_none());
    }
}
