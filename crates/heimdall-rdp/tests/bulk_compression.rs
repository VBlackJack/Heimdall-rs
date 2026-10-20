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

//! What a server bulk-compressed, read by the session: the fast-path updates and the
//! slow-path PDUs decompressed with the one history the server compresses everything with,
//! as the patched `ironrdp-session` does (`vendor/PATCHES.md`); anything malformed, or
//! compressed without compression negotiated, ends the session with an error.
//!
//! The compressed bytes were produced by `ironrdp-bulk` 0.1.1's own compressor, the one
//! whose decompressor the session runs, at each level, from three PDUs in a row:
//! 1. a slow-path Set Error Info with no error, padded with 96 bytes `P`;
//! 2. a fast-path pointer position (7, 9), padded with `P` then 96 new bytes `Q`: `P` is sent
//!    as a reference into the first PDU;
//! 3. a slow-path Set Error Info `ERRINFO_RPC_INITIATED_DISCONNECT`, padded with `Q`: sent as
//!    a reference into the fast-path update.

use std::borrow::Cow;

use ironrdp::connector::{
    ConnectionFinalizationSequence, ConnectionFinalizationState, Sequence as _,
};
use ironrdp::core::WriteBuf;
use ironrdp::graphics::image_processing::PixelFormat;
use ironrdp::pdu::Action;
use ironrdp::pdu::mcs::{McsMessage, SendDataIndication};
use ironrdp::pdu::rdp::client_info::CompressionType;
use ironrdp::pdu::rdp::server_error_info::{ErrorInfo, ProtocolIndependentCode};
use ironrdp::pdu::x224::X224;
use ironrdp::session::fast_path::ProcessorBuilder;
use ironrdp::session::image::DecodedImage;
use ironrdp::session::{
    ActiveStage, ActiveStageBuilder, ActiveStageOutput, BulkType, GracefulDisconnectReason,
    SessionResult,
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
const HEADERS: u16 = 18;
/// Each slow-path PDU's data uncompressed: the error code and its padding.
const UNCOMPRESSED: u16 = 100;
/// `updateHeader` of a fast-path update: `FASTPATH_UPDATETYPE_PTR_POSITION`, not
/// fragmented, `FASTPATH_OUTPUT_COMPRESSION_USED`.
const POINTER_POSITION_COMPRESSED: u8 = 0x88;
/// Where the fast-path update puts the pointer.
const POINTER: (u16, u16) = (7, 9);
/// Bulk flags: `PACKET_COMPRESSED`, and the type nibble no level uses.
const PACKET_COMPRESSED: u8 = 0x20;
const UNKNOWN_TYPE: u8 = 0x0f;

/// A compressed PDU's data: `compressedType` (flags and level) and the compressed bytes.
#[derive(Clone, Copy)]
struct Compressed {
    flags: u8,
    data: &'static [u8],
}

/// The three PDUs at one level: slow path, fast path, slow path.
struct Level {
    name: &'static str,
    pdus: [Compressed; 3],
}

const LEVELS: [Level; 4] = [
    Level {
        name: "RDP 4 (MPPC 8K)",
        pdus: [
            Compressed {
                flags: 0x60,
                data: &[
                    0x00, 0x00, 0x00, 0x00, 0x68, 0x65, 0x69, 0x6d, 0x64, 0x61, 0x6c, 0x6c, 0xf2,
                    0x32, 0xf4, 0x38, 0xbe, 0x0f, 0x0b, 0x80, 0x3a, 0x5b, 0x00,
                ],
            },
            Compressed {
                flags: 0x20,
                data: &[
                    0x07, 0x00, 0x09, 0xe2, 0x4f, 0xa1, 0x62, 0x69, 0x66, 0x72, 0x6f, 0x73, 0x74,
                    0x2d, 0xf2, 0x32, 0xf4, 0x38, 0xbe, 0x0f, 0x0b, 0x80, 0x3a, 0x4b, 0x40,
                ],
            },
            Compressed {
                flags: 0x20,
                data: &[0x01, 0xee, 0x87, 0x0e, 0x7c, 0xc7, 0x0c, 0x59, 0x68],
            },
        ],
    },
    Level {
        name: "RDP 5 (MPPC 64K, as xrdp)",
        pdus: [
            Compressed {
                flags: 0x61,
                data: &[
                    0x00, 0x00, 0x00, 0x00, 0x68, 0x65, 0x69, 0x6d, 0x64, 0x61, 0x6c, 0x6c, 0xf9,
                    0x19, 0x7d, 0x0e, 0x2f, 0xc1, 0xe1, 0x78, 0x03, 0xa5, 0xb0,
                ],
            },
            Compressed {
                flags: 0x21,
                data: &[
                    0x07, 0x00, 0x09, 0xf1, 0x27, 0xd0, 0xb1, 0x34, 0xb3, 0x39, 0x37, 0xb9, 0xba,
                    0x16, 0xfc, 0x8c, 0xbe, 0x87, 0x17, 0xe0, 0xf0, 0xbc, 0x01, 0xd2, 0x5a,
                ],
            },
            Compressed {
                flags: 0x21,
                data: &[0x01, 0xf7, 0x43, 0xc3, 0x9f, 0x31, 0xe1, 0x8b, 0x2d],
            },
        ],
    },
    Level {
        name: "RDP 6 (NCRUSH)",
        pdus: [
            Compressed {
                flags: 0x22,
                data: &[
                    0x04, 0x91, 0x8b, 0x03, 0xf6, 0x3d, 0xf0, 0xd0, 0x7d, 0xf6, 0x3c, 0xe4, 0x10,
                    0x9a, 0x4f, 0xfb, 0xbf, 0x00,
                ],
            },
            Compressed {
                flags: 0x22,
                data: &[
                    0x09, 0x22, 0x25, 0x92, 0xcf, 0xdf, 0xeb, 0xc0, 0xfd, 0x8e, 0x3c, 0xfc, 0xa8,
                    0xa3, 0xd7, 0x0a, 0x3f, 0xed, 0xff, 0x02,
                ],
            },
            Compressed {
                flags: 0x22,
                data: &[0x24, 0x42, 0xc1, 0xf1, 0x61, 0xe9, 0xab, 0xfd, 0x5f, 0x00],
            },
        ],
    },
    Level {
        name: "RDP 6.1 (XCRUSH, as Windows)",
        pdus: [
            Compressed {
                flags: 0x23,
                data: &[
                    0x12, 0x61, 0x00, 0x00, 0x00, 0x00, 0x68, 0x65, 0x69, 0x6d, 0x64, 0x61, 0x6c,
                    0x6c, 0xf9, 0x19, 0x7d, 0x0e, 0x2f, 0xc1, 0xe1, 0x78, 0x03, 0xa5, 0xb0,
                ],
            },
            Compressed {
                flags: 0x23,
                data: &[
                    0x12, 0x21, 0x07, 0x00, 0x09, 0xf1, 0x27, 0xd0, 0xb1, 0x34, 0xb3, 0x39, 0x37,
                    0xb9, 0xba, 0x16, 0xfc, 0x8c, 0xbe, 0x87, 0x17, 0xe0, 0xf0, 0xbc, 0x01, 0xd2,
                    0x5a,
                ],
            },
            Compressed {
                flags: 0x23,
                data: &[
                    0x12, 0x21, 0x01, 0xf7, 0x43, 0xc3, 0x9f, 0x31, 0xe1, 0x8b, 0x2d,
                ],
            },
        ],
    },
];

/// The session of a connection that negotiated `compression`, the server's pointer shown.
fn stage(compression: Option<CompressionType>) -> ActiveStage {
    ActiveStageBuilder {
        static_channels: StaticChannelSet::new(),
        user_channel_id: USER_CHANNEL,
        io_channel_id: IO_CHANNEL,
        message_channel_id: None,
        share_id: SHARE_ID,
        compression_type: compression,
        enable_server_pointer: true,
        pointer_software_rendering: true,
    }
    .build()
    .expect("built")
}

/// A Set Error Info PDU on the I/O channel, its data `data` compressed with `flags`, its
/// `compressedLength` the headers and `data` unless `compressed_length` says otherwise.
fn slow_path(flags: u8, data: &[u8], compressed_length: Option<u16>) -> Vec<u8> {
    let length = HEADERS + u16::try_from(data.len()).expect("small");
    let mut user_data = Vec::new();
    user_data.extend_from_slice(&length.to_le_bytes()); // totalLength
    user_data.extend_from_slice(&DATA_PDU.to_le_bytes());
    user_data.extend_from_slice(&USER_CHANNEL.to_le_bytes()); // pduSource
    user_data.extend_from_slice(&SHARE_ID.to_le_bytes());
    user_data.push(0); // pad1
    user_data.push(STREAM_LOW);
    user_data.extend_from_slice(&UNCOMPRESSED.to_le_bytes());
    user_data.push(SET_ERROR_INFO);
    user_data.push(flags);
    user_data.extend_from_slice(&compressed_length.unwrap_or(length).to_le_bytes());
    user_data.extend_from_slice(data);
    ironrdp::core::encode_vec(&X224(McsMessage::SendDataIndication(SendDataIndication {
        initiator_id: USER_CHANNEL,
        channel_id: IO_CHANNEL,
        user_data: Cow::Owned(user_data),
    })))
    .expect("encoded")
}

/// A fast-path pointer position update, its data `data` compressed with `flags`.
fn fast_path(flags: u8, data: &[u8]) -> Vec<u8> {
    let size = u16::try_from(data.len()).expect("small");
    // fpOutputHeader, then the length on two bytes, counting the whole frame.
    let length = 3 + 4 + size;
    let mut frame = vec![0, 0x80 | u8::try_from(length >> 8).expect("small")];
    frame.push(u8::try_from(length & 0xff).expect("a byte"));
    frame.push(POINTER_POSITION_COMPRESSED);
    frame.push(flags);
    frame.extend_from_slice(&size.to_le_bytes());
    frame.extend_from_slice(data);
    frame
}

fn image() -> DecodedImage {
    DecodedImage::new(PixelFormat::RgbA32, 64, 64)
}

fn disconnect() -> String {
    ErrorInfo::ProtocolIndependentCode(ProtocolIndependentCode::RpcInitiatedDisconnect)
        .description()
}

/// Reads the three PDUs of `level` in turn, the first through `stage`, the other two through
/// what `between` makes of it.
fn read_in_turn(
    level: &Level,
    mut stage: ActiveStage,
    between: impl Fn(&mut ActiveStage),
) -> SessionResult<()> {
    let name = level.name;
    let [first, second, third] = level.pdus;
    let mut image = image();
    let outputs = stage.process(
        &mut image,
        Action::X224,
        &slow_path(first.flags, first.data, None),
    )?;
    assert!(outputs.is_empty(), "{name}: no error, nothing to do");
    between(&mut stage);
    let outputs = stage.process(
        &mut image,
        Action::FastPath,
        &fast_path(second.flags, second.data),
    )?;
    let (x, y) = POINTER;
    assert!(
        outputs
            .iter()
            .any(|output| matches!(output, ActiveStageOutput::PointerPosition { x: at_x, y: at_y } if (*at_x, *at_y) == (x, y))),
        "{name}: the fast-path update read through the slow-path history"
    );
    let outputs = stage.process(
        &mut image,
        Action::X224,
        &slow_path(third.flags, third.data, None),
    )?;
    let disconnect = disconnect();
    assert!(
        matches!(
            outputs.as_slice(),
            [ActiveStageOutput::Terminate(GracefulDisconnectReason::Other(text))] if *text == disconnect
        ),
        "{name}: the slow-path PDU read through the fast-path history"
    );
    Ok(())
}

#[test]
fn fast_and_slow_path_pdus_are_read_through_each_other_s_history_at_every_level() {
    for level in &LEVELS {
        read_in_turn(level, stage(Some(CompressionType::Rdp61)), |_| {})
            .unwrap_or_else(|e| panic!("{}: {e}", level.name));
    }
}

#[test]
fn the_history_goes_on_across_a_reactivation() {
    // As the session rebuilds its fast-path processor after a Deactivate All.
    for level in &LEVELS {
        read_in_turn(level, stage(Some(CompressionType::Rdp61)), |stage| {
            let bulk_decompressor = stage.take_bulk_decompressor();
            assert!(bulk_decompressor.is_some(), "compression negotiated");
            stage.set_fastpath_processor(
                ProcessorBuilder {
                    io_channel_id: IO_CHANNEL,
                    user_channel_id: USER_CHANNEL,
                    share_id: SHARE_ID,
                    enable_server_pointer: true,
                    pointer_software_rendering: true,
                    bulk_decompressor,
                }
                .build(),
            );
        })
        .unwrap_or_else(|e| panic!("{}: {e}", level.name));
    }
}

#[test]
fn compressed_data_without_compression_negotiated_ends_the_session() {
    let [first, second, _] = LEVELS[1].pdus;
    let mut stage = stage(None);
    assert!(
        stage
            .process(
                &mut image(),
                Action::X224,
                &slow_path(first.flags, first.data, None)
            )
            .is_err(),
        "slow path"
    );
    assert!(
        stage
            .process(
                &mut image(),
                Action::FastPath,
                &fast_path(second.flags, second.data)
            )
            .is_err(),
        "fast path"
    );
}

#[test]
fn a_compressed_length_out_of_the_pdu_ends_the_session() {
    let [first, ..] = LEVELS[1].pdus;
    let data_length = u16::try_from(first.data.len()).expect("small");
    for compressed_length in [0, HEADERS - 1, HEADERS + data_length + 1, u16::MAX] {
        let outcome = stage(Some(CompressionType::Rdp61)).process(
            &mut image(),
            Action::X224,
            &slow_path(first.flags, first.data, Some(compressed_length)),
        );
        assert!(outcome.is_err(), "compressedLength {compressed_length}");
    }
    // Bytes past compressedLength are dropped, as FreeRDP drops them.
    let mut padded = first.data.to_vec();
    padded.extend_from_slice(&[0xff; 8]);
    let outputs = stage(Some(CompressionType::Rdp61))
        .process(
            &mut image(),
            Action::X224,
            &slow_path(first.flags, &padded, Some(HEADERS + data_length)),
        )
        .expect("read");
    assert!(outputs.is_empty());
}

#[test]
fn an_unknown_compression_type_ends_the_session() {
    let [first, second, _] = LEVELS[1].pdus;
    let flags = PACKET_COMPRESSED | UNKNOWN_TYPE;
    let mut stage = stage(Some(CompressionType::Rdp61));
    assert!(
        stage
            .process(
                &mut image(),
                Action::X224,
                &slow_path(flags, first.data, None)
            )
            .is_err()
    );
    assert!(
        stage
            .process(
                &mut image(),
                Action::FastPath,
                &fast_path(flags, second.data)
            )
            .is_err()
    );
}

#[test]
fn data_that_does_not_decompress_ends_the_session_with_an_error() {
    // Each refused by the decompressor itself: an invalid MPPC match length, an XCRUSH
    // packet too short for its level-1 header.
    for (flags, data) in [(0x21, &[0xff; 4][..]), (0x23, &[0x00][..])] {
        let mut stage = stage(Some(CompressionType::Rdp61));
        assert!(
            stage
                .process(&mut image(), Action::X224, &slow_path(flags, data, None))
                .is_err(),
            "slow path {flags:#04x}"
        );
        let mut stage = self::stage(Some(CompressionType::Rdp61));
        assert!(
            stage
                .process(&mut image(), Action::FastPath, &fast_path(flags, data))
                .is_err(),
            "fast path {flags:#04x}"
        );
    }
}

#[test]
fn a_compressed_pdu_while_the_connection_finalizes_is_refused() {
    // Its decompressor does not exist yet: refused rather than read as plain data.
    let [first, ..] = LEVELS[1].pdus;
    let mut sequence = ConnectionFinalizationSequence::new(IO_CHANNEL, USER_CHANNEL, SHARE_ID);
    sequence.state = ConnectionFinalizationState::WaitForResponse;
    let mut output = WriteBuf::new();
    let outcome = sequence.step(&slow_path(first.flags, first.data, None), &mut output);
    assert!(
        outcome
            .as_ref()
            .is_err_and(|error| format!("{error:?}").contains("bulk-compressed")),
        "{outcome:?}"
    );
}

/// A xorshift generator: the same sequence on every run, no crate needed.
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(bound).expect("small")).expect("small")
    }

    fn byte(&mut self) -> u8 {
        self.next().to_le_bytes()[0]
    }
}

/// The seed of the malformed inputs, so that a failure replays.
const SEED: u64 = 0x1234_5678_9abc_def1;
/// Longest random input, in bytes.
const RANDOM_MAX: usize = 300;
/// Most mutations made to one input.
const MUTATIONS_MAX: usize = 8;
/// Inputs between two fresh sessions, each one's history starting over.
const SESSION_INPUTS: u64 = 10_000;

/// Feeds `inputs` malformed inputs to long-lived sessions, the slow path and the fast path
/// in turn: the compressed fixtures and random bytes, truncated, bit-flipped, overwritten
/// and grown, under every `compressedType` (each flag combination at each level, the
/// unknown ones too) and, on the slow path, a `compressedLength` that is sometimes wrong.
/// Each input either reads or ends with an error: none panics.
fn malformed_inputs_never_panic(inputs: u64) {
    let seeds: Vec<Compressed> = LEVELS.iter().flat_map(|level| level.pdus).collect();
    let mut random = Seeded(SEED);
    let mut stage = stage(Some(CompressionType::Rdp61));
    let mut image = image();
    for input in 0..inputs {
        if input.is_multiple_of(SESSION_INPUTS) {
            stage = self::stage(Some(CompressionType::Rdp61));
        }
        let (mut data, mut flags) = if random.next().is_multiple_of(2) {
            let seed = seeds[random.below(seeds.len())];
            (seed.data.to_vec(), seed.flags)
        } else {
            let length = random.below(RANDOM_MAX);
            ((0..length).map(|_| random.byte()).collect(), random.byte())
        };
        for _ in 0..random.below(MUTATIONS_MAX) {
            if data.is_empty() {
                break;
            }
            let at = random.below(data.len());
            match random.below(4) {
                0 => data[at] = random.byte(),
                1 => data[at] ^= 1 << random.below(8),
                2 => data.truncate(at),
                _ => data.insert(at, random.byte()),
            }
        }
        // Every compressedType in turn, now and then.
        if random.next().is_multiple_of(4) {
            flags = (input % 256).to_le_bytes()[0];
        }
        let frame = if input.is_multiple_of(2) {
            let compressed_length = random
                .next()
                .is_multiple_of(8)
                .then(|| u16::try_from(random.below(usize::from(u16::MAX))).expect("small"));
            slow_path(flags, &data, compressed_length)
        } else {
            fast_path(flags, &data)
        };
        let action = if input.is_multiple_of(2) {
            Action::X224
        } else {
            Action::FastPath
        };
        let _ = stage.process(&mut image, action, &frame);
    }
}

#[test]
fn malformed_compressed_input_never_panics() {
    malformed_inputs_never_panic(300_000);
}

/// The same, longer: `cargo test -- --ignored`.
#[test]
#[ignore = "about a minute: run by hand"]
fn malformed_compressed_input_never_panics_long() {
    malformed_inputs_never_panic(5_000_000);
}

/// The type each level of `LEVELS` uses, in order.
const LEVEL_TYPES: [BulkType; 4] = BulkType::ALL;

/// A fast-path pointer position update, not compressed.
fn fast_path_uncompressed() -> Vec<u8> {
    let (x, y) = POINTER;
    let mut frame = vec![0, 9, 0x08, 4, 0];
    frame.extend_from_slice(&x.to_le_bytes());
    frame.extend_from_slice(&y.to_le_bytes());
    frame
}

#[test]
fn the_first_pdu_of_each_type_is_reported_once_and_the_counts_add_up() {
    for (level, bulk_type) in LEVELS.iter().zip(LEVEL_TYPES) {
        let name = level.name;
        let [first, second, third] = level.pdus;
        let mut stage = stage(Some(CompressionType::Rdp61));
        let mut image = image();
        // A slow-path PDU first: the type is reported.
        stage
            .process(
                &mut image,
                Action::X224,
                &slow_path(first.flags, first.data, None),
            )
            .expect("slow path read");
        assert_eq!(
            stage.take_first_seen_bulk_types(),
            vec![bulk_type],
            "{name}"
        );
        // A fast-path update of the same type: not reported again.
        stage
            .process(
                &mut image,
                Action::FastPath,
                &fast_path(second.flags, second.data),
            )
            .expect("fast path read");
        assert!(stage.take_first_seen_bulk_types().is_empty(), "{name}");
        // And a slow-path PDU again, the last one: nothing new either.
        stage
            .process(
                &mut image,
                Action::X224,
                &slow_path(third.flags, third.data, None),
            )
            .expect("slow path read");
        assert!(stage.take_first_seen_bulk_types().is_empty(), "{name}");
        let tally = stage.bulk_tally();
        assert_eq!(tally.compressed(bulk_type), 3, "{name}");
        assert_eq!(tally.uncompressed(), 0, "{name}");
        for other in BulkType::ALL
            .into_iter()
            .filter(|other| *other != bulk_type)
        {
            assert_eq!(tally.compressed(other), 0, "{name}: {other}");
        }
    }
}

#[test]
fn a_fast_path_update_alone_reports_its_type() {
    let [_, second, _] = LEVELS[3].pdus;
    let mut stage = stage(Some(CompressionType::Rdp61));
    // The fast-path update refers into the first slow-path PDU's history, so feed it first.
    let [first, _, _] = LEVELS[3].pdus;
    let mut image = image();
    stage
        .process(
            &mut image,
            Action::X224,
            &slow_path(first.flags, first.data, None),
        )
        .expect("slow path read");
    assert_eq!(
        stage.take_first_seen_bulk_types(),
        vec![BulkType::Rdp61Xcrush]
    );
    stage
        .process(
            &mut image,
            Action::FastPath,
            &fast_path(second.flags, second.data),
        )
        .expect("fast path read");
    assert!(stage.take_first_seen_bulk_types().is_empty());
    assert_eq!(stage.bulk_tally().compressed(BulkType::Rdp61Xcrush), 2);
}

#[test]
fn uncompressed_pdus_are_counted_and_report_no_type() {
    let mut stage = stage(Some(CompressionType::Rdp61));
    let mut image = image();
    stage
        .process(&mut image, Action::FastPath, &fast_path_uncompressed())
        .expect("fast path read");
    stage
        .process(
            &mut image,
            Action::X224,
            &slow_path(0, &[0; UNCOMPRESSED as usize], None),
        )
        .expect("slow path read");
    assert!(stage.take_first_seen_bulk_types().is_empty());
    let tally = stage.bulk_tally();
    assert_eq!(tally.uncompressed(), 2);
    assert!(BulkType::ALL.into_iter().all(|t| tally.compressed(t) == 0));
}

#[test]
fn nothing_is_reported_when_compression_is_off() {
    let mut stage = stage(None);
    let mut image = image();
    stage
        .process(&mut image, Action::FastPath, &fast_path_uncompressed())
        .expect("fast path read");
    assert!(stage.take_first_seen_bulk_types().is_empty());
    let tally = stage.bulk_tally();
    assert!(BulkType::ALL.into_iter().all(|t| tally.compressed(t) == 0));
}

#[test]
fn the_tally_goes_on_across_a_reactivation() {
    let [first, second, _] = LEVELS[2].pdus;
    let mut stage = stage(Some(CompressionType::Rdp61));
    let mut image = image();
    stage
        .process(
            &mut image,
            Action::X224,
            &slow_path(first.flags, first.data, None),
        )
        .expect("slow path read");
    let bulk_decompressor = stage.take_bulk_decompressor();
    stage.set_fastpath_processor(
        ProcessorBuilder {
            io_channel_id: IO_CHANNEL,
            user_channel_id: USER_CHANNEL,
            share_id: SHARE_ID,
            enable_server_pointer: true,
            pointer_software_rendering: true,
            bulk_decompressor,
        }
        .build(),
    );
    stage
        .process(
            &mut image,
            Action::FastPath,
            &fast_path(second.flags, second.data),
        )
        .expect("fast path read");
    assert_eq!(stage.bulk_tally().compressed(BulkType::Rdp6Ncrush), 2);
    // Reported once, before the reactivation, and not again after it.
    assert_eq!(
        stage.take_first_seen_bulk_types(),
        vec![BulkType::Rdp6Ncrush]
    );
}
