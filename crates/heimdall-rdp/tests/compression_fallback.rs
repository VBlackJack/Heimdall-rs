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

//! A server that bulk-compresses what the built-in client cannot read, met by a fake server:
//! a compressed PDU of the connection finalization has the connection opened again, once,
//! without compression; a compressed update the session cannot read ends it with
//! [`CloseReason::CompressionRefused`], which the application connects again on.
//!
//! The server negotiates TLS without Network Level Authentication, as xrdp does, skips the
//! channel join, tells the client its licence is valid, demands its capabilities, reads its
//! confirmation and finalization PDUs, then answers with a Font Map, the last PDU of the
//! finalization; or, when it compresses, with a compressed Set Error Info first.

#![allow(
    clippy::large_futures,
    reason = "a connection future is large; tests await it once"
)]

use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use heimdall_core::profile::RdpOptions;
use heimdall_rdp::session::{self, RdpEvent};
use heimdall_rdp::{
    CloseReason, KnownRdpHosts, Openings, RdpConfig, RdpConnection, RdpError, Security, Timeouts,
    Transport, connect_through, given,
};
use ironrdp::pdu::gcc::{
    ConferenceCreateResponse, RdpVersion, ServerCoreData, ServerCoreOptionalData,
    ServerEarlyCapabilityFlags, ServerGccBlocks, ServerNetworkData, ServerSecurityData,
};
use ironrdp::pdu::mcs::{
    AttachUserConfirm, ConnectInitial, ConnectResponse, DomainParameters, McsMessage,
    SendDataIndication,
};
use ironrdp::pdu::nego::{ConnectionConfirm, ResponseFlags, SecurityProtocol};
use ironrdp::pdu::rdp::ClientInfoPdu;
use ironrdp::pdu::rdp::capability_sets::{DemandActive, SERVER_CHANNEL_ID, ServerDemandActive};
use ironrdp::pdu::rdp::client_info::{ClientInfoFlags, CompressionType};
use ironrdp::pdu::rdp::finalization_messages::FontPdu;
use ironrdp::pdu::rdp::headers::{
    CompressionFlags, ShareControlHeader, ShareControlPdu, ShareDataHeader, ShareDataPdu,
    StreamPriority,
};
use ironrdp::pdu::rdp::server_license::{LicensePdu, LicensingErrorMessage};
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
/// Bound on waiting for what the other side does next.
const WAIT: Duration = Duration::from_secs(10);
/// The MCS user and I/O channels the fake server hands out.
const USER_CHANNEL: u16 = 1007;
const IO_CHANNEL: u16 = 1003;
const SHARE_ID: u32 = 0x0001_03ea;
/// What the client sends past the Demand Active, each in its own frame: Confirm Active,
/// Synchronize, Control (Cooperate), Control (Request Control), Font List.
const CLIENT_FINALIZATION_FRAMES: usize = 5;
/// A Set Error Info PDU with no error, compressed with MPPC 64K (`PACKET_COMPRESSED`,
/// `PACKET_AT_FRONT`, RDP 5), as `tests/bulk_compression.rs` makes it: its Share Control and
/// Share Data headers, then its data.
const COMPRESSED_FLAGS: u8 = 0x61;
const COMPRESSED_DATA: &[u8] = &[
    0x00, 0x00, 0x00, 0x00, 0x68, 0x65, 0x69, 0x6d, 0x64, 0x61, 0x6c, 0x6c, 0xf9, 0x19, 0x7d, 0x0e,
    0x2f, 0xc1, 0xe1, 0x78, 0x03, 0xa5, 0xb0,
];
/// `pduType`: a Data PDU (7), protocol version 1 (0x10).
const DATA_PDU: u16 = 0x17;
/// `streamId`: `STREAM_LOW`.
const STREAM_LOW: u8 = 1;
/// `pduType2`: `PDUTYPE2_SET_ERROR_INFO_PDU`.
const SET_ERROR_INFO: u8 = 0x2f;
/// Share Control and Share Data headers, counted by `compressedLength`.
const HEADERS: u16 = 18;
/// The Set Error Info's data uncompressed: the error code and its padding.
const UNCOMPRESSED: u16 = 100;
/// A fast-path pointer position update, MPPC 64K compressed, whose data does not decompress
/// (an invalid match length): `updateHeader` with `FASTPATH_OUTPUT_COMPRESSION_USED`, then
/// `compressionFlags`.
const POINTER_POSITION_COMPRESSED: u8 = 0x88;
const BROKEN_FLAGS: u8 = 0x21;
const BROKEN_DATA: &[u8] = &[0xff; 4];

/// When the fake server compresses a PDU of the connection finalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Finalization {
    /// When the client offered compression, as a server could.
    CompressedWhenOffered,
    /// Always, offered or not: a server compressing whatever it is told.
    AlwaysCompressed,
    /// Never: then, once connected, a compressed update the session cannot read.
    BrokenUpdateAfter,
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

/// `user_data` sent to the client on the I/O channel.
async fn indicate<S: AsyncWrite + Unpin>(stream: &mut S, user_data: Vec<u8>) {
    send(
        stream,
        &X224(McsMessage::SendDataIndication(SendDataIndication {
            initiator_id: SERVER_CHANNEL_ID,
            channel_id: IO_CHANNEL,
            user_data: Cow::Owned(user_data),
        })),
    )
    .await;
}

/// `pdu`, a Share Control PDU of the share, encoded.
fn share_control(pdu: ShareControlPdu) -> Vec<u8> {
    ironrdp_core::encode_vec(&ShareControlHeader {
        share_control_pdu: pdu,
        pdu_source: SERVER_CHANNEL_ID,
        share_id: SHARE_ID,
    })
    .expect("encode")
}

/// The Set Error Info of [`COMPRESSED_DATA`], its headers saying it compressed.
fn compressed_set_error_info() -> Vec<u8> {
    let length = HEADERS + u16::try_from(COMPRESSED_DATA.len()).expect("small");
    let mut user_data = Vec::new();
    user_data.extend_from_slice(&length.to_le_bytes()); // totalLength
    user_data.extend_from_slice(&DATA_PDU.to_le_bytes());
    user_data.extend_from_slice(&SERVER_CHANNEL_ID.to_le_bytes()); // pduSource
    user_data.extend_from_slice(&SHARE_ID.to_le_bytes());
    user_data.push(0); // pad1
    user_data.push(STREAM_LOW);
    user_data.extend_from_slice(&UNCOMPRESSED.to_le_bytes());
    user_data.push(SET_ERROR_INFO);
    user_data.push(COMPRESSED_FLAGS);
    user_data.extend_from_slice(&length.to_le_bytes()); // compressedLength
    user_data.extend_from_slice(COMPRESSED_DATA);
    user_data
}

/// A fast-path update whose compressed data does not decompress.
fn broken_fast_path_update() -> Vec<u8> {
    let size = u16::try_from(BROKEN_DATA.len()).expect("small");
    // fpOutputHeader, then the length on two bytes, counting the whole frame.
    let length = 3 + 4 + size;
    let mut frame = vec![0, 0x80 | u8::try_from(length >> 8).expect("small")];
    frame.push(u8::try_from(length & 0xff).expect("a byte"));
    frame.push(POINTER_POSITION_COMPRESSED);
    frame.push(BROKEN_FLAGS);
    frame.extend_from_slice(&size.to_le_bytes());
    frame.extend_from_slice(BROKEN_DATA);
    frame
}

/// The MCS connection and the Client Info PDU, over `tls`: whether the client offered
/// compression.
async fn logon<S: AsyncRead + AsyncWrite + Unpin>(tls: &mut S) -> bool {
    let bytes = frame(tls).await;
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
        tls,
        &X224(X224Data {
            data: response.into(),
        }),
    )
    .await;

    // Erect domain, then attach user.
    let _erect = frame(tls).await;
    let _attach = frame(tls).await;
    send(
        tls,
        &X224(McsMessage::AttachUserConfirm(AttachUserConfirm {
            result: 0,
            initiator_id: USER_CHANNEL,
        })),
    )
    .await;
    let bytes = frame(tls).await;
    let X224(McsMessage::SendDataRequest(data)) =
        ironrdp_core::decode::<X224<McsMessage<'_>>>(&bytes).expect("MCS")
    else {
        panic!("the Client Info PDU");
    };
    let info = ironrdp_core::decode::<ClientInfoPdu>(&data.user_data).expect("client info");
    info.client_info
        .flags
        .contains(ClientInfoFlags::COMPRESSION)
}

/// Serves one connection as `finalization` says; whether the client offered compression.
async fn serve<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    finalization: Finalization,
) -> bool {
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
    let offered = logon(&mut tls).await;

    // The licence is valid, then the capabilities.
    let licence = LicensePdu::LicensingErrorMessage(
        LicensingErrorMessage::new_valid_client().expect("licence"),
    );
    indicate(
        &mut tls,
        ironrdp_core::encode_vec(&licence).expect("encode"),
    )
    .await;
    indicate(
        &mut tls,
        share_control(ShareControlPdu::ServerDemandActive(ServerDemandActive {
            pdu: DemandActive {
                source_descriptor: "RDP".to_owned(),
                capability_sets: Vec::new(),
            },
        })),
    )
    .await;
    for _ in 0..CLIENT_FINALIZATION_FRAMES {
        let _ = frame(&mut tls).await;
    }

    let compressed = match finalization {
        Finalization::CompressedWhenOffered => offered,
        Finalization::AlwaysCompressed => true,
        Finalization::BrokenUpdateAfter => false,
    };
    if compressed {
        indicate(&mut tls, compressed_set_error_info()).await;
    }
    indicate(
        &mut tls,
        share_control(ShareControlPdu::Data(ShareDataHeader {
            share_data_pdu: ShareDataPdu::FontMap(FontPdu::default()),
            stream_priority: StreamPriority::Medium,
            compression_flags: CompressionFlags::empty(),
            compression_type: CompressionType::K8,
        })),
    )
    .await;
    if finalization == Finalization::BrokenUpdateAfter {
        tls.write_all(&broken_fast_path_update())
            .await
            .expect("send");
    }
    // Until the client hangs up: whatever it sends meanwhile is read and dropped.
    let mut sink = [0; 1024];
    while let Ok(Ok(read)) = tokio::time::timeout(WAIT, tls.read(&mut sink)).await {
        if read == 0 {
            break;
        }
    }
    offered
}

/// Each connection opened to a fresh fake server serving as `finalization`; whether each
/// connection offered compression lands in `served`, in order.
fn openings(finalization: Finalization, served: Arc<Mutex<Vec<bool>>>) -> Openings {
    Box::new(move || {
        let (near, far) = tokio::io::duplex(1 << 16);
        let served = Arc::clone(&served);
        drop(tokio::spawn(async move {
            let offered = serve(far, finalization).await;
            served.lock().expect("lock").push(offered);
        }));
        Box::pin(async move {
            Ok((
                Box::new(near) as Box<dyn Transport>,
                "127.0.0.1:50000".parse().expect("address"),
            ))
        })
    })
}

/// Connects, compression offered when `compression`, to fake servers serving as
/// `finalization`: the outcome, and whether each connection made offered compression.
async fn attempt(
    finalization: Finalization,
    compression: bool,
) -> (Result<RdpConnection, RdpError>, Arc<Mutex<Vec<bool>>>) {
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
        keep_alive: heimdall_rdp::DEFAULT_KEEP_ALIVE,
        timeouts: Timeouts {
            connect: WAIT,
            handshake: WAIT,
            logon: WAIT,
        },
        clipboard: false,
        drives: Vec::new(),
        trusted_for_run: Vec::new(),
        options: RdpOptions::default(),
        several_servers: false,
        strict_server_authentication: false,
        kerberos: false,
        time_zone: None,
        desktop_scale: 100,
        compression,
        progress: None,
    };
    let served = Arc::new(Mutex::new(Vec::new()));
    let outcome = tokio::time::timeout(
        WAIT * 3,
        connect_through(
            config,
            given("user".to_owned(), Zeroizing::new("password".to_owned())),
            CancellationToken::new(),
            openings(finalization, Arc::clone(&served)),
            Some(WAIT),
        ),
    )
    .await
    .expect("in time");
    (outcome, served)
}

/// What `served` holds once every fake server is done.
async fn offered(served: &Arc<Mutex<Vec<bool>>>, connections: usize) -> Vec<bool> {
    tokio::time::timeout(WAIT, async {
        loop {
            let seen = served.lock().expect("lock").clone();
            if seen.len() >= connections {
                return seen;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("every server done")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_compressing_its_finalization_is_connected_to_again_once_without_compression() {
    let (outcome, served) = attempt(Finalization::CompressedWhenOffered, true).await;
    let connection = outcome.expect("connected the second time");
    assert!(connection.compression_turned_off, "and says so");
    // Hung up: the second server is done.
    drop(connection);
    assert_eq!(
        offered(&served, 2).await,
        [true, false],
        "offered, refused, then not offered"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_compressing_its_finalization_even_uncompressed_fails_after_that_one_retry() {
    let (outcome, served) = attempt(Finalization::AlwaysCompressed, true).await;
    assert!(
        matches!(outcome, Err(RdpError::CompressionRefused)),
        "{:?}",
        outcome.map(|_| ())
    );
    // Never a loop: one connection again, then the failure.
    assert_eq!(offered(&served, 2).await, [true, false]);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(served.lock().expect("lock").len(), 2, "no third connection");
}

#[tokio::test(flavor = "multi_thread")]
async fn without_compression_offered_a_compressed_finalization_fails_at_once() {
    let (outcome, served) = attempt(Finalization::AlwaysCompressed, false).await;
    assert!(
        matches!(outcome, Err(RdpError::CompressionRefused)),
        "{:?}",
        outcome.map(|_| ())
    );
    assert_eq!(offered(&served, 1).await, [false]);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(served.lock().expect("lock").len(), 1, "nothing to turn off");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_compressed_update_the_session_cannot_read_ends_it_as_compression_refused() {
    let (outcome, served) = attempt(Finalization::BrokenUpdateAfter, true).await;
    let connection = outcome.expect("connected");
    assert!(!connection.compression_turned_off);
    let mut session = session::start(connection, CancellationToken::new());
    let reason = tokio::time::timeout(WAIT, async {
        loop {
            match session.events.recv().await {
                Some(RdpEvent::Closed(reason)) => return Some(reason),
                Some(_) => {}
                None => return None,
            }
        }
    })
    .await
    .expect("in time");
    assert!(
        matches!(reason, Some(CloseReason::CompressionRefused(_))),
        "{reason:?}"
    );
    assert_eq!(offered(&served, 1).await, [true]);
}
