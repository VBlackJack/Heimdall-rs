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

//! What a profile's display and session options put on the wire, read by a fake server.
//!
//! The server negotiates TLS without Network Level Authentication, as xrdp does, reads the
//! client's MCS Connect Initial (colour depth, administrative session), answers it, skips the
//! channel join, and reads the Client Info PDU (where the sound goes). It stops there: what
//! the options change is all said by then.

#![allow(
    clippy::large_futures,
    reason = "a connection future is large; tests await it once"
)]

use std::sync::Arc;
use std::time::Duration;

use heimdall_core::profile::{AudioPlayback, ColorDepth, Experience, RdpOptions};
use heimdall_rdp::{
    KnownRdpHosts, RdpConfig, Security, TimeZone, Timeouts, Transition, connect_over, given,
};
use ironrdp::pdu::gcc::{
    ClientClusterData, ClientEarlyCapabilityFlags, ClientGccBlocks, ConferenceCreateResponse,
    HighColorDepth, RdpVersion, RedirectionFlags, RedirectionVersion, ServerCoreData,
    ServerCoreOptionalData, ServerEarlyCapabilityFlags, ServerGccBlocks, ServerNetworkData,
    ServerSecurityData,
};
use ironrdp::pdu::mcs::{
    AttachUserConfirm, ConnectInitial, ConnectResponse, DomainParameters, McsMessage,
};
use ironrdp::pdu::nego::{ConnectionConfirm, ResponseFlags, SecurityProtocol};
use ironrdp::pdu::rdp::ClientInfoPdu;
use ironrdp::pdu::rdp::client_info::{ClientInfoFlags, PerformanceFlags};
use ironrdp::pdu::x224::{X224, X224Data};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::ring::default_provider;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const HOST: &str = "rdp.test";
const PORT: u16 = 3389;
const CERT: &[u8] = include_bytes!("fixtures/server-cert.der");
const KEY: &[u8] = include_bytes!("fixtures/server-key.der");
/// SHA-256 of the fixture's `SubjectPublicKeyInfo`, computed by openssl.
const PIN: &str = include_str!("fixtures/server-spki-sha256.txt");
/// Bound on waiting for what the client does next.
const WAIT: Duration = Duration::from_secs(10);
/// The MCS user and I/O channels the fake server hands out.
const USER_CHANNEL: u16 = 1007;
const IO_CHANNEL: u16 = 1003;

/// What the client sent that the options decide.
struct Sent {
    gcc: ClientGccBlocks,
    info: ClientInfoPdu,
}

fn acceptor() -> TlsAcceptor {
    let config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(CERT.to_vec())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY.to_vec())),
        )
        .expect("certificate");
    TlsAcceptor::from(Arc::new(config))
}

/// One TPKT frame, header included.
async fn frame<S: AsyncRead + Unpin>(stream: &mut S) -> Vec<u8> {
    let mut header = [0; 4];
    stream.read_exact(&mut header).await.expect("TPKT header");
    let length = usize::from(u16::from_be_bytes([header[2], header[3]]));
    let mut bytes = header.to_vec();
    bytes.resize(length, 0);
    stream.read_exact(&mut bytes[4..]).await.expect("TPKT body");
    bytes
}

async fn send<S: AsyncWrite + Unpin, T: ironrdp_core::Encode>(stream: &mut S, pdu: &T) {
    let bytes = ironrdp_core::encode_vec(pdu).expect("encode");
    stream.write_all(&bytes).await.expect("send");
}

/// An MCS message the client sends, read off the stream.
async fn mcs<S: AsyncRead + Unpin>(stream: &mut S) -> ironrdp::pdu::mcs::OwnedMcsMessage {
    let bytes = frame(stream).await;
    let X224(message) = ironrdp_core::decode::<X224<McsMessage<'_>>>(&bytes).expect("MCS");
    ironrdp_core::IntoOwned::into_owned(message)
}

async fn serve<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S) -> Sent {
    let _request = frame(&mut stream).await;
    send(
        &mut stream,
        &X224(ConnectionConfirm::Response {
            flags: ResponseFlags::empty(),
            protocol: SecurityProtocol::SSL,
        }),
    )
    .await;
    let mut tls = acceptor().accept(stream).await.expect("TLS");

    let bytes = frame(&mut tls).await;
    let X224(data) = ironrdp_core::decode::<X224<X224Data<'_>>>(&bytes).expect("X.224 data");
    let initial = ironrdp_core::decode::<ConnectInitial>(&data.data).expect("initial");
    let gcc = initial.conference_create_request.into_gcc_blocks();
    let channels = gcc
        .network
        .as_ref()
        .map_or(0, |network| network.channels.len());
    let response = ConnectResponse {
        conference_create_response: ConferenceCreateResponse::new(
            USER_CHANNEL,
            ServerGccBlocks {
                core: ServerCoreData {
                    version: RdpVersion::V5_PLUS,
                    optional_data: ServerCoreOptionalData {
                        client_requested_protocols: Some(SecurityProtocol::SSL),
                        // No channel join: the Client Info PDU follows the attach.
                        early_capability_flags: Some(
                            ServerEarlyCapabilityFlags::SKIP_CHANNELJOIN_SUPPORTED,
                        ),
                    },
                },
                network: ServerNetworkData {
                    channel_ids: (0..channels)
                        .map(|index| IO_CHANNEL + 1 + u16::try_from(index).expect("few"))
                        .collect(),
                    io_channel: IO_CHANNEL,
                },
                security: ServerSecurityData::no_security(),
                message_channel: None,
                multi_transport_channel: None,
            },
        )
        .expect("response"),
        called_connect_id: 1,
        domain_parameters: DomainParameters::target(),
    };
    let response = ironrdp_core::encode_vec(&response).expect("encode");
    send(
        &mut tls,
        &X224(X224Data {
            data: response.into(),
        }),
    )
    .await;

    // Erect domain, then attach user.
    let _erect = mcs(&mut tls).await;
    let _attach = mcs(&mut tls).await;
    send(
        &mut tls,
        &X224(McsMessage::AttachUserConfirm(AttachUserConfirm {
            result: 0,
            initiator_id: USER_CHANNEL,
        })),
    )
    .await;
    let ironrdp::pdu::mcs::OwnedMcsMessage::SendDataRequest(data) = mcs(&mut tls).await else {
        panic!("the Client Info PDU");
    };
    let info = ironrdp_core::decode::<ClientInfoPdu>(&data.user_data).expect("client info");
    Sent { gcc, info }
}

/// Connects with `options` to the fake server and returns what it read.
async fn sent_with(options: RdpOptions) -> Sent {
    sent_in(options, None).await
}

/// [`sent_with`], this computer said to be in `time_zone`.
async fn sent_in(options: RdpOptions, time_zone: Option<TimeZone>) -> Sent {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    std::fs::write(&known, format!("{HOST}:{PORT} SHA256:{}\n", PIN.trim())).expect("known");
    let config = RdpConfig {
        host: HOST.to_owned(),
        port: PORT,
        domain: None,
        desktop: (1024, 768),
        keyboard_layout: 0,
        security: Security::NlaOrTls,
        known_hosts: KnownRdpHosts::new(&known),
        accepted: None,
        timeouts: Timeouts {
            connect: WAIT,
            handshake: WAIT,
            logon: WAIT,
        },
        clipboard: false,
        drives: Vec::new(),
        trusted_for_run: Vec::new(),
        options,
        several_servers: false,
        strict_server_authentication: false,
        kerberos: false,
        time_zone,
        desktop_scale: 150,
        progress: None,
    };
    let (client, server) = tokio::io::duplex(1 << 16);
    let server = tokio::spawn(serve(server));
    // The client waits for the licence the fake server never sends: the outcome is not read.
    let _ = tokio::time::timeout(
        WAIT,
        connect_over(
            Box::new(client),
            "127.0.0.1:50000".parse().expect("address"),
            &config,
            given("user".to_owned(), Zeroizing::new("password".to_owned())),
            &CancellationToken::new(),
        ),
    )
    .await;
    tokio::time::timeout(WAIT, server)
        .await
        .expect("in time")
        .expect("server")
}

fn depth_asked(gcc: &ClientGccBlocks) -> (Option<HighColorDepth>, bool) {
    let optional = &gcc.core.optional_data;
    let wants_32 = optional
        .early_capability_flags
        .is_some_and(|flags| flags.contains(ClientEarlyCapabilityFlags::WANT_32_BPP_SESSION));
    (optional.high_color_depth, wants_32)
}

#[tokio::test]
async fn the_defaults_ask_32_bits_no_sound_and_the_ordinary_session() {
    let sent = sent_with(RdpOptions::default()).await;
    assert_eq!(depth_asked(&sent.gcc), (Some(HighColorDepth::Bpp24), true));
    assert_eq!(sent.gcc.cluster, None);
    assert_eq!(
        sent.gcc.core.optional_data.desktop_scale_factor,
        Some(150),
        "the screen's scale, as mstsc tells it"
    );
    let flags = sent.info.client_info.flags;
    assert!(flags.contains(ClientInfoFlags::NO_AUDIO_PLAYBACK));
    assert!(!flags.contains(ClientInfoFlags::REMOTE_CONSOLE_AUDIO));
}

#[tokio::test]
async fn each_colour_depth_is_asked_as_mstsc_asks_it() {
    for (depth, asked) in [
        (
            ColorDepth::Bpp16,
            (Some(HighColorDepth::Rgb565Bpp16), false),
        ),
        (ColorDepth::Bpp24, (Some(HighColorDepth::Bpp24), false)),
        (ColorDepth::Bpp32, (Some(HighColorDepth::Bpp24), true)),
    ] {
        let sent = sent_with(RdpOptions {
            color_depth: depth,
            ..RdpOptions::default()
        })
        .await;
        assert_eq!(depth_asked(&sent.gcc), asked, "{depth:?}");
    }
}

#[tokio::test]
async fn sound_kept_on_the_server_is_asked_for_and_never_played_here() {
    let sent = sent_with(RdpOptions {
        audio: AudioPlayback::OnServer,
        ..RdpOptions::default()
    })
    .await;
    let flags = sent.info.client_info.flags;
    assert!(flags.contains(ClientInfoFlags::REMOTE_CONSOLE_AUDIO));
    assert!(flags.contains(ClientInfoFlags::NO_AUDIO_PLAYBACK));
}

#[tokio::test]
async fn sound_played_here_is_asked_for_and_not_kept_on_the_server() {
    let sent = sent_with(RdpOptions {
        audio: AudioPlayback::Local,
        ..RdpOptions::default()
    })
    .await;
    let flags = sent.info.client_info.flags;
    assert!(!flags.contains(ClientInfoFlags::NO_AUDIO_PLAYBACK));
    assert!(!flags.contains(ClientInfoFlags::REMOTE_CONSOLE_AUDIO));
}

/// The performance flags the Client Info PDU carries.
fn performance_flags(sent: &Sent) -> Option<PerformanceFlags> {
    sent.info
        .client_info
        .extra_info
        .optional_data
        .performance_flags()
}

#[tokio::test]
async fn the_boxes_ticked_are_the_flags_sent_and_none_keeps_the_experience_given_so_far() {
    let sent = sent_with(RdpOptions::default()).await;
    assert_eq!(
        performance_flags(&sent),
        Some(
            PerformanceFlags::DISABLE_FULLWINDOWDRAG
                | PerformanceFlags::DISABLE_MENUANIMATIONS
                | PerformanceFlags::ENABLE_FONT_SMOOTHING
        )
    );
    let mut options = RdpOptions::default();
    options.set(Experience::DisableWallpaper, true);
    options.set(Experience::EnableComposition, true);
    let sent = sent_with(options).await;
    assert_eq!(
        performance_flags(&sent),
        Some(PerformanceFlags::DISABLE_WALLPAPER | PerformanceFlags::ENABLE_DESKTOP_COMPOSITION),
        "exactly the boxes ticked, as the C# control is given them"
    );
}

#[tokio::test]
async fn the_time_zone_of_this_computer_is_the_one_the_server_is_told() {
    let sent = sent_with(RdpOptions::default()).await;
    let utc = &sent.info.client_info.extra_info.optional_data;
    assert_eq!(
        utc.timezone().map(|zone| zone.bias),
        Some(0),
        "none known: UTC"
    );
    // New York: UTC is local time plus five hours, an hour less in summer.
    let rule = |month: u16, occurrence: u16| {
        let fields: [u16; 8] = [0, month, 0, occurrence, 2, 0, 0, 0];
        let bytes: Vec<u8> = fields
            .iter()
            .flat_map(|field| field.to_le_bytes())
            .collect();
        Transition::from_systemtime(&bytes)
    };
    let zone = TimeZone {
        bias: 300,
        standard_name: "Eastern Standard Time".to_owned(),
        standard_start: rule(11, 1),
        standard_bias: 0,
        daylight_name: "Eastern Daylight Time".to_owned(),
        daylight_start: rule(3, 2),
        daylight_bias: -60,
    };
    let sent = sent_in(RdpOptions::default(), Some(zone)).await;
    let told = sent
        .info
        .client_info
        .extra_info
        .optional_data
        .timezone()
        .expect("a time zone");
    assert_eq!(
        (told.bias, told.daylight_bias, told.standard_name.as_str()),
        (300, -60, "Eastern Standard Time")
    );
    assert!(told.standard_date.0.is_some() && told.daylight_date.0.is_some());
}

#[tokio::test]
async fn the_administrative_session_asks_for_session_0_as_mstsc_admin() {
    let sent = sent_with(RdpOptions {
        admin_session: true,
        ..RdpOptions::default()
    })
    .await;
    assert_eq!(
        sent.gcc.cluster,
        Some(ClientClusterData {
            flags: RedirectionFlags::REDIRECTION_SUPPORTED
                | RedirectionFlags::REDIRECTED_SESSION_FIELD_VALID,
            redirection_version: RedirectionVersion::V5,
            redirected_session_id: 0,
        })
    );
}
