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

//! Slow-path PDUs a server bulk-compressed, read by the session: decompressed with the one
//! history the server compresses everything with, as the patched `ironrdp-session` does
//! (`vendor/PATCHES.md`).
//!
//! The compressed bytes were produced by `ironrdp-bulk` 0.1.1's own compressor, the one
//! whose decompressor the session runs, from two Set Error Info PDUs padded with the same
//! 96 bytes: the first carries no error, the second `ERRINFO_RPC_INITIATED_DISCONNECT`, its
//! padding sent as a reference back into the first.

use std::borrow::Cow;

use ironrdp::graphics::image_processing::PixelFormat;
use ironrdp::pdu::Action;
use ironrdp::pdu::mcs::{McsMessage, SendDataIndication};
use ironrdp::pdu::rdp::client_info::CompressionType;
use ironrdp::pdu::rdp::server_error_info::{ErrorInfo, ProtocolIndependentCode};
use ironrdp::pdu::x224::X224;
use ironrdp::session::fast_path::ProcessorBuilder;
use ironrdp::session::image::DecodedImage;
use ironrdp::session::{
    ActiveStage, ActiveStageBuilder, ActiveStageOutput, GracefulDisconnectReason,
};
use ironrdp::svc::StaticChannelSet;

const USER_CHANNEL: u16 = 1007;
const IO_CHANNEL: u16 = 1003;
const SHARE_ID: u32 = 0x0001_03ea;
/// `pduType`: a Data PDU (7), protocol version 1 (0x10).
const DATA_PDU: u16 = 0x17;
/// `streamId`: `STREAM_LOW`.
const STREAM_LOW: u8 = 1;
/// `pduType2`: `PDUTYPE2_SET_ERROR_INFO_PDU`.
const SET_ERROR_INFO: u8 = 0x2f;
/// Share Control and Share Data headers, counted by `compressedLength`.
const HEADERS: usize = 18;
/// The error code and its padding, each PDU's data uncompressed.
const UNCOMPRESSED: u16 = 100;

/// A compressed PDU: `compressedType` (flags and level) and the compressed data.
struct Compressed {
    flags: u8,
    data: &'static [u8],
}

/// MPPC with 64 KB of history (RDP 5), as xrdp compresses.
const RDP5: [Compressed; 2] = [
    Compressed {
        flags: 0x61,
        data: &[
            0x00, 0x00, 0x00, 0x00, 0x68, 0x65, 0x69, 0x6d, 0x64, 0x61, 0x6c, 0x6c, 0xf9, 0x19,
            0x7d, 0x0e, 0x2f, 0xc1, 0xe1, 0x78, 0x03, 0xa5, 0xb0,
        ],
    },
    Compressed {
        flags: 0x21,
        data: &[0x01, 0xf1, 0x27, 0xd1, 0x36, 0x00],
    },
];

/// XCRUSH (RDP 6.1), the level offered, as Windows compresses.
const RDP61: [Compressed; 2] = [
    Compressed {
        flags: 0x23,
        data: &[
            0x12, 0x61, 0x00, 0x00, 0x00, 0x00, 0x68, 0x65, 0x69, 0x6d, 0x64, 0x61, 0x6c, 0x6c,
            0xf9, 0x19, 0x7d, 0x0e, 0x2f, 0xc1, 0xe1, 0x78, 0x03, 0xa5, 0xb0,
        ],
    },
    Compressed {
        flags: 0x23,
        data: &[0x12, 0x21, 0x01, 0xf1, 0x27, 0xd1, 0x36, 0x00],
    },
];

/// The session of a connection that negotiated `compression`.
fn stage(compression: Option<CompressionType>) -> ActiveStage {
    ActiveStageBuilder {
        static_channels: StaticChannelSet::new(),
        user_channel_id: USER_CHANNEL,
        io_channel_id: IO_CHANNEL,
        message_channel_id: None,
        share_id: SHARE_ID,
        compression_type: compression,
        enable_server_pointer: false,
        pointer_software_rendering: true,
    }
    .build()
}

/// `pdu` on the I/O channel, as the server frames it.
fn frame(pdu: &Compressed) -> Vec<u8> {
    let compressed_length = u16::try_from(HEADERS + pdu.data.len()).expect("small");
    let mut user_data = Vec::new();
    user_data.extend_from_slice(&compressed_length.to_le_bytes()); // totalLength
    user_data.extend_from_slice(&DATA_PDU.to_le_bytes());
    user_data.extend_from_slice(&USER_CHANNEL.to_le_bytes()); // pduSource
    user_data.extend_from_slice(&SHARE_ID.to_le_bytes());
    user_data.push(0); // pad1
    user_data.push(STREAM_LOW);
    user_data.extend_from_slice(&UNCOMPRESSED.to_le_bytes());
    user_data.push(SET_ERROR_INFO);
    user_data.push(pdu.flags);
    user_data.extend_from_slice(&compressed_length.to_le_bytes());
    user_data.extend_from_slice(pdu.data);
    ironrdp::core::encode_vec(&X224(McsMessage::SendDataIndication(SendDataIndication {
        initiator_id: USER_CHANNEL,
        channel_id: IO_CHANNEL,
        user_data: Cow::Owned(user_data),
    })))
    .expect("encoded")
}

fn image() -> DecodedImage {
    DecodedImage::new(PixelFormat::RgbA32, 64, 64)
}

#[test]
fn compressed_slow_path_pdus_are_read_with_the_history_of_the_ones_before() {
    let disconnect =
        ErrorInfo::ProtocolIndependentCode(ProtocolIndependentCode::RpcInitiatedDisconnect)
            .description();
    for (level, pdus) in [("RDP 5", RDP5), ("RDP 6.1", RDP61)] {
        let mut stage = stage(Some(CompressionType::Rdp61));
        let mut image = image();
        let [no_error, error] = pdus;
        let outputs = stage
            .process(&mut image, Action::X224, &frame(&no_error))
            .unwrap_or_else(|e| panic!("{level}: {e}"));
        assert!(outputs.is_empty(), "{level}: no error, nothing to do");
        let outputs = stage
            .process(&mut image, Action::X224, &frame(&error))
            .unwrap_or_else(|e| panic!("{level}: {e}"));
        assert!(
            matches!(
                outputs.as_slice(),
                [ActiveStageOutput::Terminate(GracefulDisconnectReason::Other(text))] if *text == disconnect
            ),
            "{level}: the error read through the first PDU's history"
        );
    }
}

#[test]
fn the_history_goes_on_across_a_reactivation() {
    // As the session rebuilds its fast-path processor after a Deactivate All.
    let mut stage = stage(Some(CompressionType::Rdp61));
    let mut image = image();
    let [no_error, error] = RDP61;
    stage
        .process(&mut image, Action::X224, &frame(&no_error))
        .expect("read");
    let bulk_decompressor = stage.take_bulk_decompressor();
    assert!(bulk_decompressor.is_some(), "compression negotiated");
    stage.set_fastpath_processor(
        ProcessorBuilder {
            io_channel_id: IO_CHANNEL,
            user_channel_id: USER_CHANNEL,
            share_id: SHARE_ID,
            enable_server_pointer: false,
            pointer_software_rendering: true,
            bulk_decompressor,
        }
        .build(),
    );
    let outputs = stage
        .process(&mut image, Action::X224, &frame(&error))
        .expect("read");
    assert!(matches!(
        outputs.as_slice(),
        [ActiveStageOutput::Terminate(
            GracefulDisconnectReason::Other(_)
        )]
    ));
}
