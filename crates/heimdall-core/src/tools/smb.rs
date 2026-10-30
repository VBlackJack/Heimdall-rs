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

//! SMB enumeration engine, mirroring the C# `SmbEnumerationEngine`, `NtlmProbe`,
//! and `UdpProbeEngine` (NetBIOS NBSTAT probe).
//!
//! Provides protocol packet crafting, binary parsing, security findings generation,
//! and localized report generation without external cryptography dependencies.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::module_name_repetitions,
    clippy::redundant_closure_for_method_calls,
    clippy::too_many_lines
)]

use serde::{Deserialize, Serialize};

use super::network_fingerprint;

/// SMB Dialect 2.0.2.
pub const DIALECT_SMB202: u16 = 0x0202;
/// SMB Dialect 2.1.
pub const DIALECT_SMB21: u16 = 0x0210;
/// SMB Dialect 3.0.
pub const DIALECT_SMB30: u16 = 0x0300;
/// SMB Dialect 3.0.2.
pub const DIALECT_SMB302: u16 = 0x0302;
/// SMB Dialect 3.1.1.
pub const DIALECT_SMB311: u16 = 0x0311;
/// SMBv1 legacy dialect revision.
pub const DIALECT_SMB1: u16 = 0x00FF;

/// Minimum secure dialect (SMB 2.0.2).
pub const MIN_SECURE_DIALECT: u16 = DIALECT_SMB202;

/// SMB2 magic signature: `[0xFE, 'S', 'M', 'B']`.
pub const SMB2_MAGIC: [u8; 4] = [0xFE, b'S', b'M', b'B'];

/// NTLMSSP signature: `"NTLMSSP\0"`.
pub const NTLMSSP_SIGNATURE: [u8; 8] = [b'N', b'T', b'L', b'M', b'S', b'S', b'P', 0x00];

/// NTLMSSP `NEGOTIATE_VERSION` flag: when set, an 8-byte Version field is present.
pub const NTLMSSP_NEGOTIATE_VERSION: u32 = 0x0200_0000;

/// SMB2 header size: the Negotiate and Session Setup bodies start right after it.
const SMB2_HEADER_SIZE: usize = 64;
/// Offset of the 32-bit status in an SMB2 header.
const SMB2_STATUS_OFFSET: usize = 8;
/// `STATUS_SUCCESS`.
const SMB2_STATUS_SUCCESS: u32 = 0;
/// `StructureSize` of a successful SMB2 Negotiate response body.
const SMB2_NEGOTIATE_STRUCTURE_SIZE: u16 = 65;

/// NTLMSSP Type 2: the message type the challenge carries.
const NTLM_MESSAGE_TYPE_CHALLENGE: u32 = 2;
/// Offset of the message type in an NTLMSSP message.
const NTLM_MESSAGE_TYPE_FIELD: usize = 8;
/// Signature, type, TargetName fields, flags and challenge.
const NTLM_TYPE2_MIN_LEN: usize = 32;
/// Offset of the TargetInfo length field (the offset field follows it at +4).
const NTLM_TARGET_INFO_LEN_FIELD: usize = 40;
const NTLM_TARGET_INFO_OFFSET_FIELD: usize = 44;
/// End of the TargetInfo fields, and start of the Version field.
const NTLM_TARGET_INFO_FIELDS_END: usize = 48;
/// End of the Version field.
const NTLM_VERSION_END: usize = 56;

/// Formats a 16-bit SMB dialect revision code into a human-readable string.
#[must_use]
pub fn format_dialect(dialect: u16) -> String {
    match dialect {
        DIALECT_SMB202 => "SMB 2.0.2".to_string(),
        DIALECT_SMB21 => "SMB 2.1".to_string(),
        DIALECT_SMB30 => "SMB 3.0".to_string(),
        DIALECT_SMB302 => "SMB 3.0.2".to_string(),
        DIALECT_SMB311 => "SMB 3.1.1".to_string(),
        DIALECT_SMB1 => "SMBv1 (legacy)".to_string(),
        other => format!("0x{other:04X}"),
    }
}

/// Source of the enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmbEnumerationSource {
    Direct,
    Tunnel,
}

/// Severity of a security finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmbFindingSeverity {
    Info,
    Success,
    Warning,
    Critical,
}

/// A single security finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmbFinding {
    pub severity: SmbFindingSeverity,
    pub message_key: String,
    pub message_args: Vec<String>,
}

impl SmbFinding {
    /// Creates a new finding with no arguments.
    #[must_use]
    pub fn new(severity: SmbFindingSeverity, key: impl Into<String>) -> Self {
        Self {
            severity,
            message_key: key.into(),
            message_args: Vec::new(),
        }
    }

    /// Creates a new finding with formatting arguments.
    #[must_use]
    pub fn with_args(
        severity: SmbFindingSeverity,
        key: impl Into<String>,
        args: Vec<String>,
    ) -> Self {
        Self {
            severity,
            message_key: key.into(),
            message_args: args,
        }
    }
}

/// Information extracted from an NTLM Type 2 challenge response.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NtlmInfo {
    pub nb_computer: Option<String>,
    pub nb_domain: Option<String>,
    pub dns_computer: Option<String>,
    pub dns_domain: Option<String>,
    pub dns_forest: Option<String>,
    pub os_build: Option<String>,
}

/// SMB2 Negotiate metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmbNegotiateInfo {
    pub server_guid: Option<String>,
    pub dialect_revision: u16,
    pub signing_required: bool,
    pub signing_enabled: bool,
    pub system_time: Option<String>,
    pub server_start_time: Option<String>,
    pub capabilities: u32,
}

/// Consolidated SMB enumeration result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmbEnumerationResult {
    pub source: Option<SmbEnumerationSource>,
    pub computer_name: Option<String>,
    pub domain: Option<String>,
    pub dns_name: Option<String>,
    pub dns_domain: Option<String>,
    pub forest: Option<String>,
    pub os_build: Option<String>,
    pub mac_address: Option<String>,
    pub dialect_raw: Option<u16>,
    pub dialect: Option<String>,
    pub signing_required: Option<bool>,
    pub server_guid: Option<String>,
    pub system_time: Option<String>,
    pub boot_time: Option<String>,
    pub findings: Vec<SmbFinding>,
    pub report: Option<String>,
}

// ── Packet Crafting ─────────────────────────────────────────────────────────

/// Builds a 4-byte NetBIOS Session Service header for a payload of length `len`.
#[must_use]
pub fn build_netbios_header(len: usize) -> [u8; 4] {
    [
        0x00,
        ((len >> 16) & 0xFF) as u8,
        ((len >> 8) & 0xFF) as u8,
        (len & 0xFF) as u8,
    ]
}

/// Reads payload length from a 4-byte NetBIOS Session Service header.
#[must_use]
pub fn read_netbios_length(header: &[u8; 4]) -> Option<usize> {
    if header[0] != 0x00 {
        return None;
    }
    let len = ((header[1] as usize) << 16) | ((header[2] as usize) << 8) | (header[3] as usize);
    if len == 0 || len > 65536 {
        None
    } else {
        Some(len)
    }
}

/// Builds an SMB2 Negotiate request packet.
#[must_use]
pub fn build_smb2_negotiate() -> Vec<u8> {
    let mut header = vec![0u8; 64];
    header[0..4].copy_from_slice(&SMB2_MAGIC);
    header[4..6].copy_from_slice(&64u16.to_le_bytes()); // StructureSize = 64
    header[12..14].copy_from_slice(&0x0000u16.to_le_bytes()); // Command = Negotiate
    header[14..16].copy_from_slice(&1u16.to_le_bytes()); // CreditRequest = 1

    let mut body = vec![0u8; 40];
    body[0..2].copy_from_slice(&36u16.to_le_bytes()); // StructureSize = 36
    body[2..4].copy_from_slice(&3u16.to_le_bytes()); // DialectCount = 3
    body[4..6].copy_from_slice(&1u16.to_le_bytes()); // SecurityMode: SIGNING_ENABLED

    // Dialects: SMB 2.0.2, SMB 2.1, SMB 3.0
    body[32..34].copy_from_slice(&DIALECT_SMB202.to_le_bytes());
    body[34..36].copy_from_slice(&DIALECT_SMB21.to_le_bytes());
    body[36..38].copy_from_slice(&DIALECT_SMB30.to_le_bytes());

    let mut packet = Vec::with_capacity(header.len() + body.len());
    packet.extend_from_slice(&header);
    packet.extend_from_slice(&body);
    packet
}

/// Builds a minimal NTLMSSP Negotiate (Type 1) message.
#[must_use]
pub fn build_ntlmssp_type1() -> Vec<u8> {
    let mut msg = vec![0u8; 32];
    msg[0..8].copy_from_slice(&NTLMSSP_SIGNATURE);
    msg[8..12].copy_from_slice(&1u32.to_le_bytes()); // MessageType = 1 (Negotiate)
    // Flags: UNICODE | REQUEST_TARGET | NTLM | ALWAYS_SIGN (0x00088207)
    msg[12..16].copy_from_slice(&0x0008_8207u32.to_le_bytes());
    msg
}

/// Wraps content in ASN.1 DER tag + encoded length.
fn der_wrap(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut res = Vec::new();
    res.push(tag);
    if content.len() < 0x80 {
        res.push(content.len() as u8);
    } else if content.len() < 0x100 {
        res.push(0x81);
        res.push(content.len() as u8);
    } else {
        res.push(0x82);
        res.push((content.len() >> 8) as u8);
        res.push((content.len() & 0xFF) as u8);
    }
    res.extend_from_slice(content);
    res
}

/// Wraps an NTLMSSP payload in a minimal SPNEGO/GSS-API `NegTokenInit` structure.
#[must_use]
pub fn wrap_in_spnego(ntlmssp: &[u8]) -> Vec<u8> {
    // OID 1.3.6.1.5.5.2 (SPNEGO)
    let spnego_oid: [u8; 8] = [0x06, 0x06, 0x2B, 0x06, 0x01, 0x05, 0x05, 0x02];
    // OID 1.3.6.1.4.1.311.2.2.10 (NTLMSSP)
    let ntlm_oid: [u8; 12] = [
        0x06, 0x0A, 0x2B, 0x06, 0x01, 0x04, 0x01, 0x82, 0x37, 0x02, 0x02, 0x0A,
    ];

    let mech_types = der_wrap(0x30, &ntlm_oid);
    let mech_types_ctx = der_wrap(0xA0, &mech_types);
    let mech_token = der_wrap(0x04, ntlmssp);
    let mech_token_ctx = der_wrap(0xA2, &mech_token);

    let mut neg_token_init = Vec::with_capacity(mech_types_ctx.len() + mech_token_ctx.len());
    neg_token_init.extend_from_slice(&mech_types_ctx);
    neg_token_init.extend_from_slice(&mech_token_ctx);

    let neg_token_init_seq = der_wrap(0x30, &neg_token_init);
    let ctx0 = der_wrap(0xA0, &neg_token_init_seq);

    let mut inner = Vec::with_capacity(spnego_oid.len() + ctx0.len());
    inner.extend_from_slice(&spnego_oid);
    inner.extend_from_slice(&ctx0);

    der_wrap(0x60, &inner)
}

/// Builds an SMB2 Session Setup request packet wrapping a SPNEGO security buffer.
#[must_use]
pub fn build_smb2_session_setup(security_buffer: &[u8]) -> Vec<u8> {
    let mut header = vec![0u8; 64];
    header[0..4].copy_from_slice(&SMB2_MAGIC);
    header[4..6].copy_from_slice(&64u16.to_le_bytes());
    header[12..14].copy_from_slice(&0x0001u16.to_le_bytes()); // Command = SessionSetup
    header[14..16].copy_from_slice(&1u16.to_le_bytes()); // CreditRequest = 1
    header[28..36].copy_from_slice(&1u64.to_le_bytes()); // MessageId = 1

    let mut body = vec![0u8; 24 + security_buffer.len()];
    body[0..2].copy_from_slice(&25u16.to_le_bytes()); // StructureSize = 25
    body[2] = 0; // Flags
    body[3] = 1; // SecurityMode: SIGNING_ENABLED
    // SecurityBufferOffset = 64 (header) + 24 (body fixed) = 88
    body[12..14].copy_from_slice(&88u16.to_le_bytes());
    body[14..16].copy_from_slice(&(security_buffer.len() as u16).to_le_bytes());
    body[24..24 + security_buffer.len()].copy_from_slice(security_buffer);

    let mut packet = Vec::with_capacity(header.len() + body.len());
    packet.extend_from_slice(&header);
    packet.extend_from_slice(&body);
    packet
}

/// Builds a NetBIOS NBSTAT query packet (UDP 137).
#[must_use]
pub fn build_netbios_nbstat_query(tx_id: u16) -> [u8; 50] {
    let mut packet = [0u8; 50];
    packet[0] = (tx_id >> 8) as u8;
    packet[1] = (tx_id & 0xFF) as u8;
    // Flags = 0x0000, Questions = 1
    packet[4] = 0x00;
    packet[5] = 0x01;

    // Encoded NetBIOS name for wildcard "*" query
    packet[12] = 0x20; // Length = 32
    let mut name = [0x20u8; 16];
    name[0] = b'*';

    for (i, &b) in name.iter().enumerate() {
        packet[13 + i * 2] = b'A' + ((b >> 4) & 0x0F);
        packet[14 + i * 2] = b'A' + (b & 0x0F);
    }

    packet[45] = 0x00; // Null terminator
    packet[46] = 0x00; // Type: NBSTAT (0x0021)
    packet[47] = 0x21;
    packet[48] = 0x00; // Class: IN (0x0001)
    packet[49] = 0x01;

    packet
}

// ── Packet Parsing ──────────────────────────────────────────────────────────

/// Searches a buffer for the NTLMSSP signature and returns the remaining bytes.
#[must_use]
pub fn find_ntlmssp_message(buffer: &[u8]) -> Option<&[u8]> {
    if buffer.len() < NTLMSSP_SIGNATURE.len() {
        return None;
    }
    for i in 0..=buffer.len() - NTLMSSP_SIGNATURE.len() {
        if buffer[i] == b'N' && &buffer[i..i + NTLMSSP_SIGNATURE.len()] == NTLMSSP_SIGNATURE {
            return Some(&buffer[i..]);
        }
    }
    None
}

/// Converts a 64-bit Windows FILETIME (100ns intervals since 1601-01-01) into ISO 8601 UTC.
#[must_use]
pub fn filetime_to_iso(filetime: u64) -> Option<String> {
    // Difference between Windows epoch (1601-01-01) and Unix epoch (1970-01-01) in 100-ns intervals.
    const WINDOWS_TICK: u64 = 10_000_000;
    const SEC_TO_UNIX_EPOCH: u64 = 11_644_473_600;
    const WINDOWS_TO_UNIX_TICKS: u64 = SEC_TO_UNIX_EPOCH * WINDOWS_TICK;

    if filetime < WINDOWS_TO_UNIX_TICKS {
        return None;
    }

    let unix_secs = (filetime - WINDOWS_TO_UNIX_TICKS) / WINDOWS_TICK;

    // Break down seconds into days and time
    let day_secs = unix_secs % 86400;
    let hour = day_secs / 3600;
    let min = (day_secs % 3600) / 60;
    let sec = day_secs % 60;

    let mut days = (unix_secs / 86400) as i64;

    // Civil date from days since 1970-01-01 (Algorithm by Howard Hinnant)
    days += 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = (days - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        y, m, d, hour, min, sec
    ))
}

/// Parses an SMB2 Negotiate Response packet.
#[must_use]
pub fn parse_negotiate_response(data: &[u8]) -> Option<SmbNegotiateInfo> {
    const BODY_OFF: usize = SMB2_HEADER_SIZE;
    if data.len() < BODY_OFF + 0x40 {
        return None;
    }
    // Only a successful SMB2 Negotiate response has this body: an error reply, or one that
    // merely starts with the magic, would be read as a GUID, dialect and signing mode the
    // host never reported (C# `ParseNegotiateResponse`).
    if data[..SMB2_MAGIC.len()] != SMB2_MAGIC {
        return None;
    }
    let status = u32::from_le_bytes([
        data[SMB2_STATUS_OFFSET],
        data[SMB2_STATUS_OFFSET + 1],
        data[SMB2_STATUS_OFFSET + 2],
        data[SMB2_STATUS_OFFSET + 3],
    ]);
    if status != SMB2_STATUS_SUCCESS
        || u16::from_le_bytes([data[BODY_OFF], data[BODY_OFF + 1]]) != SMB2_NEGOTIATE_STRUCTURE_SIZE
    {
        return None;
    }

    let security_mode = u16::from_le_bytes([data[BODY_OFF + 0x02], data[BODY_OFF + 0x03]]);
    let dialect = u16::from_le_bytes([data[BODY_OFF + 0x04], data[BODY_OFF + 0x05]]);

    // Server GUID at body + 0x08 (16 bytes)
    let guid_bytes = &data[BODY_OFF + 0x08..BODY_OFF + 0x18];
    let server_guid = format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        guid_bytes[3],
        guid_bytes[2],
        guid_bytes[1],
        guid_bytes[0],
        guid_bytes[5],
        guid_bytes[4],
        guid_bytes[7],
        guid_bytes[6],
        guid_bytes[8],
        guid_bytes[9],
        guid_bytes[10],
        guid_bytes[11],
        guid_bytes[12],
        guid_bytes[13],
        guid_bytes[14],
        guid_bytes[15]
    );

    let capabilities = u32::from_le_bytes([
        data[BODY_OFF + 0x18],
        data[BODY_OFF + 0x19],
        data[BODY_OFF + 0x1A],
        data[BODY_OFF + 0x1B],
    ]);

    // SystemTime at body + 0x28 (FILETIME)
    let sys_ticks = u64::from_le_bytes([
        data[BODY_OFF + 0x28],
        data[BODY_OFF + 0x29],
        data[BODY_OFF + 0x2A],
        data[BODY_OFF + 0x2B],
        data[BODY_OFF + 0x2C],
        data[BODY_OFF + 0x2D],
        data[BODY_OFF + 0x2E],
        data[BODY_OFF + 0x2F],
    ]);
    let system_time = filetime_to_iso(sys_ticks);

    // ServerStartTime at body + 0x30 (FILETIME)
    let start_ticks = u64::from_le_bytes([
        data[BODY_OFF + 0x30],
        data[BODY_OFF + 0x31],
        data[BODY_OFF + 0x32],
        data[BODY_OFF + 0x33],
        data[BODY_OFF + 0x34],
        data[BODY_OFF + 0x35],
        data[BODY_OFF + 0x36],
        data[BODY_OFF + 0x37],
    ]);
    let server_start_time = filetime_to_iso(start_ticks);

    Some(SmbNegotiateInfo {
        server_guid: Some(server_guid),
        dialect_revision: dialect,
        signing_required: (security_mode & 0x0002) != 0,
        signing_enabled: (security_mode & 0x0001) != 0,
        system_time,
        server_start_time,
        capabilities,
    })
}

/// Parses an NTLMSSP Type 2 challenge message to extract AV_PAIR structures.
///
/// The message comes from the host being probed, so every offset and length in it is hostile
/// until bounded: they are compared in 64 bits, and a malformed TargetInfo block costs only
/// that block, the OS build of the Version field being kept (C# `ParseNtlmType2`). Names are
/// cleaned of control characters and bounded.
#[must_use]
pub fn parse_ntlm_type2(data: &[u8]) -> Option<NtlmInfo> {
    if data.len() < NTLM_TYPE2_MIN_LEN || data[..NTLMSSP_SIGNATURE.len()] != NTLMSSP_SIGNATURE {
        return None;
    }
    let msg_type = u32::from_le_bytes([
        data[NTLM_MESSAGE_TYPE_FIELD],
        data[NTLM_MESSAGE_TYPE_FIELD + 1],
        data[NTLM_MESSAGE_TYPE_FIELD + 2],
        data[NTLM_MESSAGE_TYPE_FIELD + 3],
    ]);
    if msg_type != NTLM_MESSAGE_TYPE_CHALLENGE {
        return None;
    }

    if data.len() < NTLM_TARGET_INFO_FIELDS_END {
        return Some(NtlmInfo::default());
    }

    let os_build = read_os_build(data);
    let target_info_len = u64::from(u16::from_le_bytes([
        data[NTLM_TARGET_INFO_LEN_FIELD],
        data[NTLM_TARGET_INFO_LEN_FIELD + 1],
    ]));
    let target_info_offset = u64::from(u32::from_le_bytes([
        data[NTLM_TARGET_INFO_OFFSET_FIELD],
        data[NTLM_TARGET_INFO_OFFSET_FIELD + 1],
        data[NTLM_TARGET_INFO_OFFSET_FIELD + 2],
        data[NTLM_TARGET_INFO_OFFSET_FIELD + 3],
    ]));
    let target_info_end = target_info_offset + target_info_len;
    // `usize::try_from` of a value below `data.len()` cannot fail, so the casts below are
    // taken only after the comparison in 64 bits.
    if target_info_len == 0
        || target_info_offset < NTLM_TARGET_INFO_FIELDS_END as u64
        || target_info_end > data.len() as u64
    {
        return Some(NtlmInfo {
            os_build,
            ..NtlmInfo::default()
        });
    }

    let mut nb_computer = None;
    let mut nb_domain = None;
    let mut dns_computer = None;
    let mut dns_domain = None;
    let mut dns_forest = None;

    let mut offset = target_info_offset as usize;
    let end_offset = target_info_end as usize;

    while offset + 4 <= end_offset {
        let av_id = u16::from_le_bytes([data[offset], data[offset + 1]]);
        let av_len = usize::from(u16::from_le_bytes([data[offset + 2], data[offset + 3]]));
        offset += 4;

        if av_id == 0 {
            break; // MsvAvEOL
        }

        if av_len > end_offset - offset {
            break;
        }

        let utf16: Vec<u16> = data[offset..offset + av_len]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        let val = network_fingerprint::clean(&String::from_utf16_lossy(&utf16));

        match av_id {
            1 => nb_computer = val,
            2 => nb_domain = val,
            3 => dns_computer = val,
            4 => dns_domain = val,
            5 => dns_forest = val,
            _ => {}
        }

        offset += av_len;
    }

    Some(NtlmInfo {
        nb_computer,
        nb_domain,
        dns_computer,
        dns_domain,
        dns_forest,
        os_build,
    })
}

/// The OS build of the Version field: Major(1) Minor(1) Build(2) Reserved(3) Revision(1).
/// The field is only present when the server set `NTLMSSP_NEGOTIATE_VERSION` in the flags at
/// offset 20; otherwise those bytes belong to another field and must not be read as a version.
fn read_os_build(data: &[u8]) -> Option<String> {
    let negotiate_flags = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);
    if negotiate_flags & NTLMSSP_NEGOTIATE_VERSION == 0 || data.len() < NTLM_VERSION_END {
        return None;
    }
    let major = data[NTLM_TARGET_INFO_FIELDS_END];
    let minor = data[NTLM_TARGET_INFO_FIELDS_END + 1];
    let build = u16::from_le_bytes([
        data[NTLM_TARGET_INFO_FIELDS_END + 2],
        data[NTLM_TARGET_INFO_FIELDS_END + 3],
    ]);
    (major > 0 && build > 0).then(|| format!("{major}.{minor}.{build}"))
}

/// Advances past a NetBIOS name field (either label-length or compression pointer).
fn skip_netbios_name(data: &[u8], mut offset: usize) -> usize {
    while offset < data.len() {
        let len = data[offset] as usize;
        if len == 0x00 {
            return offset + 1;
        }
        if (len & 0xC0) == 0xC0 {
            return offset + 2;
        }
        offset += len + 1;
    }
    offset
}

/// Parses a NetBIOS NBSTAT response (UDP 137).
/// Returns `(computer_name, domain, mac_address)`.
#[must_use]
pub fn parse_netbios_response(data: &[u8]) -> (Option<String>, Option<String>, Option<String>) {
    if data.len() < 25 {
        return (None, None, None);
    }

    let qd_count = (((data[4] as usize) << 8) | (data[5] as usize)).min(4);
    let mut offset = 12;

    for _ in 0..qd_count {
        if offset >= data.len() {
            return (None, None, None);
        }
        offset = skip_netbios_name(data, offset);
        if offset + 4 > data.len() {
            return (None, None, None);
        }
        offset += 4;
    }

    if offset >= data.len() {
        return (None, None, None);
    }

    offset = skip_netbios_name(data, offset);
    if offset + 10 > data.len() {
        return (None, None, None);
    }
    offset += 10;

    if offset >= data.len() {
        return (None, None, None);
    }

    let name_count = data[offset] as usize;
    offset += 1;

    let mut computer_name = None;
    let mut domain = None;

    for _ in 0..name_count {
        if offset + 18 > data.len() {
            break;
        }
        let raw_name = &data[offset..offset + 15];
        // A name made only of control characters or spaces is no name.
        let name_str = network_fingerprint::clean(&String::from_utf8_lossy(raw_name));
        let suffix = data[offset + 15];
        let flags = ((data[offset + 16] as u16) << 8) | (data[offset + 17] as u16);
        let is_group = (flags & 0x8000) != 0;

        if suffix == 0x00 && !is_group && computer_name.is_none() {
            computer_name = name_str;
        } else if suffix == 0x00 && is_group && domain.is_none() {
            domain = name_str;
        }

        offset += 18;
    }

    let mut mac = None;
    if offset + 6 <= data.len() {
        let mac_bytes = &data[offset..offset + 6];
        let formatted = format!(
            "{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}",
            mac_bytes[0], mac_bytes[1], mac_bytes[2], mac_bytes[3], mac_bytes[4], mac_bytes[5]
        );
        if formatted != "00-00-00-00-00-00" && formatted != "FF-FF-FF-FF-FF-FF" {
            mac = Some(formatted);
        }
    }

    (computer_name, domain, mac)
}

// ── Security Findings & Consolidation ───────────────────────────────────────

/// Builds security findings from probe data.
#[must_use]
pub fn build_security_findings(
    _ntlm: Option<&NtlmInfo>,
    smb: Option<&SmbNegotiateInfo>,
    netbios_failed: bool,
) -> Vec<SmbFinding> {
    let mut findings = Vec::new();

    if let Some(smb) = smb {
        if smb.signing_required {
            findings.push(SmbFinding::new(
                SmbFindingSeverity::Success,
                "ToolSmbSigningEnabled",
            ));
        } else {
            findings.push(SmbFinding::new(
                SmbFindingSeverity::Warning,
                "ToolSmbSigningDisabled",
            ));
        }

        // The negotiate request only offers SMB2+ dialects, so the response can never
        // carry an SMBv1 revision: a server that speaks only SMBv1 fails the SMB2 parse
        // earlier and produces no SmbNegotiateInfo. There is therefore no SMBv1 finding
        // here; detecting SMBv1 would require a separate SMB1 negotiate, which this tool
        // deliberately does not send.
        findings.push(SmbFinding::with_args(
            SmbFindingSeverity::Success,
            "ToolSmbModernDialect",
            vec![format_dialect(smb.dialect_revision)],
        ));
    }

    if netbios_failed {
        findings.push(SmbFinding::new(
            SmbFindingSeverity::Info,
            "ToolSmbNetBiosFailed",
        ));
    }

    findings
}

/// Consolidates probe outputs into a unified `SmbEnumerationResult`.
#[must_use]
pub fn build_result(
    ntlm: Option<NtlmInfo>,
    smb: Option<SmbNegotiateInfo>,
    netbios_name: Option<String>,
    netbios_domain: Option<String>,
    netbios_mac: Option<String>,
    netbios_failed: bool,
) -> SmbEnumerationResult {
    let findings = build_security_findings(ntlm.as_ref(), smb.as_ref(), netbios_failed);

    let computer_name = ntlm
        .as_ref()
        .and_then(|n| n.nb_computer.clone())
        .or(netbios_name);
    let domain = ntlm
        .as_ref()
        .and_then(|n| n.nb_domain.clone())
        .or(netbios_domain);
    let dns_name = ntlm.as_ref().and_then(|n| n.dns_computer.clone());
    let dns_domain = ntlm.as_ref().and_then(|n| n.dns_domain.clone());
    let forest = ntlm.as_ref().and_then(|n| n.dns_forest.clone());
    let os_build = ntlm.as_ref().and_then(|n| n.os_build.clone());

    let dialect_raw = smb.as_ref().map(|s| s.dialect_revision);
    let dialect = dialect_raw.map(format_dialect);
    let signing_required = smb.as_ref().map(|s| s.signing_required);
    let server_guid = smb.as_ref().and_then(|s| s.server_guid.clone());
    let system_time = smb.as_ref().and_then(|s| s.system_time.clone());
    let boot_time = smb.as_ref().and_then(|s| s.server_start_time.clone());

    SmbEnumerationResult {
        source: Some(SmbEnumerationSource::Direct),
        computer_name,
        domain,
        dns_name,
        dns_domain,
        forest,
        os_build,
        mac_address: netbios_mac,
        dialect_raw,
        dialect,
        signing_required,
        server_guid,
        system_time,
        boot_time,
        findings,
        report: None,
    }
}

/// Builds the formatted plaintext clipboard report.
#[must_use]
pub fn build_report(result: &SmbEnumerationResult, localize: impl Fn(&str) -> String) -> String {
    let na = localize("ToolSmbNotAvailable");
    let mut sb = String::new();

    sb.push_str(&localize("ToolSmbReportIdentity"));
    sb.push('\n');
    append_row(
        &mut sb,
        &localize("ToolSmbComputerName"),
        result.computer_name.as_deref().unwrap_or(&na),
    );
    append_row(
        &mut sb,
        &localize("ToolSmbDomain"),
        result.domain.as_deref().unwrap_or(&na),
    );
    append_row(
        &mut sb,
        &localize("ToolSmbDnsName"),
        result.dns_name.as_deref().unwrap_or(&na),
    );
    append_row(
        &mut sb,
        &localize("ToolSmbDnsDomain"),
        result.dns_domain.as_deref().unwrap_or(&na),
    );
    append_row(
        &mut sb,
        &localize("ToolSmbForest"),
        result.forest.as_deref().unwrap_or(&na),
    );
    append_row(
        &mut sb,
        &localize("ToolSmbOsBuild"),
        result.os_build.as_deref().unwrap_or(&na),
    );
    append_row(
        &mut sb,
        &localize("ToolSmbMac"),
        result.mac_address.as_deref().unwrap_or(&na),
    );

    if result.dialect_raw.is_some() {
        sb.push('\n');
        sb.push_str(&localize("ToolSmbReportProtocol"));
        sb.push('\n');
        append_row(
            &mut sb,
            &localize("ToolSmbDialect"),
            result.dialect.as_deref().unwrap_or(&na),
        );
        let signing_str = match result.signing_required {
            Some(true) => localize("ToolSmbYes"),
            Some(false) => localize("ToolSmbNo"),
            None => na.clone(),
        };
        append_row(&mut sb, &localize("ToolSmbSigning"), &signing_str);
        append_row(
            &mut sb,
            &localize("ToolSmbServerGuid"),
            result.server_guid.as_deref().unwrap_or(&na),
        );
        append_row(
            &mut sb,
            &localize("ToolSmbSystemTime"),
            result.system_time.as_deref().unwrap_or(&na),
        );
        append_row(
            &mut sb,
            &localize("ToolSmbBootTime"),
            result.boot_time.as_deref().unwrap_or(&na),
        );
    }

    if !result.findings.is_empty() {
        sb.push('\n');
        sb.push_str(&localize("ToolSmbReportFindings"));
        sb.push('\n');
        for finding in &result.findings {
            let prefix = match finding.severity {
                SmbFindingSeverity::Success => "[OK]  ",
                SmbFindingSeverity::Warning => "[WARN]",
                SmbFindingSeverity::Critical => "[CRIT]",
                SmbFindingSeverity::Info => "[INFO]",
            };
            let template = localize(&finding.message_key);
            let formatted_msg = if let Some(arg) = finding.message_args.first() {
                template.replace("{0}", arg)
            } else {
                template
            };
            sb.push_str(&format!("{prefix} {formatted_msg}\n"));
        }
    }

    sb
}

fn append_row(sb: &mut String, label: &str, value: &str) {
    sb.push_str(&format!("{label:<14}: {value}\n"));
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_formatting_matches_csharp() {
        assert_eq!(format_dialect(DIALECT_SMB202), "SMB 2.0.2");
        assert_eq!(format_dialect(DIALECT_SMB21), "SMB 2.1");
        assert_eq!(format_dialect(DIALECT_SMB30), "SMB 3.0");
        assert_eq!(format_dialect(DIALECT_SMB302), "SMB 3.0.2");
        assert_eq!(format_dialect(DIALECT_SMB311), "SMB 3.1.1");
        assert_eq!(format_dialect(DIALECT_SMB1), "SMBv1 (legacy)");
        assert_eq!(format_dialect(0x1234), "0x1234");
    }

    #[test]
    fn netbios_query_packet_has_correct_structure() {
        let packet = build_netbios_nbstat_query(0x1234);
        assert_eq!(packet.len(), 50);
        assert_eq!(packet[0], 0x12);
        assert_eq!(packet[1], 0x34);
        assert_eq!(packet[4], 0x00);
        assert_eq!(packet[5], 0x01); // 1 Question
        assert_eq!(packet[12], 32); // Name len
        assert_eq!(packet[46..48], [0x00, 0x21]); // NBSTAT
        assert_eq!(packet[48..50], [0x00, 0x01]); // IN class
    }

    #[test]
    fn smb2_negotiate_packet_has_valid_header_and_body() {
        let packet = build_smb2_negotiate();
        assert_eq!(packet.len(), 104);
        assert_eq!(&packet[0..4], &SMB2_MAGIC);
        assert_eq!(&packet[4..6], &64u16.to_le_bytes());
        // StructureSize of body = 36
        assert_eq!(&packet[64..66], &36u16.to_le_bytes());
        // Dialect count = 3
        assert_eq!(&packet[66..68], &3u16.to_le_bytes());
    }

    #[test]
    fn ntlmssp_type1_has_proper_signature_and_flags() {
        let msg = build_ntlmssp_type1();
        assert_eq!(&msg[0..8], &NTLMSSP_SIGNATURE);
        assert_eq!(&msg[8..12], &1u32.to_le_bytes());
        assert_eq!(&msg[12..16], &0x0008_8207u32.to_le_bytes());
    }

    #[test]
    fn spnego_wrapper_properly_encodes_asn1_der() {
        let ntlm = build_ntlmssp_type1();
        let wrapped = wrap_in_spnego(&ntlm);
        assert_eq!(wrapped[0], 0x60); // Application constructed tag
        assert!(wrapped.len() > ntlm.len());
        assert!(find_ntlmssp_message(&wrapped).is_some());
    }

    #[test]
    fn session_setup_packet_encapsulates_security_buffer() {
        let sec = vec![1, 2, 3, 4, 5];
        let pkt = build_smb2_session_setup(&sec);
        assert_eq!(&pkt[0..4], &SMB2_MAGIC);
        assert_eq!(&pkt[12..14], &1u16.to_le_bytes()); // Command = SessionSetup
        assert_eq!(&pkt[88..93], &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn parse_ntlm_type2_extracts_av_pairs_and_os_build() {
        // Construct a synthetic NTLMSSP Type 2 message
        let mut msg = vec![0u8; 100];
        msg[0..8].copy_from_slice(&NTLMSSP_SIGNATURE);
        msg[8..12].copy_from_slice(&2u32.to_le_bytes()); // Type 2

        // NegotiateFlags with NEGOTIATE_VERSION set, so the Version field is read.
        msg[20..24].copy_from_slice(&NTLMSSP_NEGOTIATE_VERSION.to_le_bytes());

        // Version: 10.0.19041 at offset 48
        msg[48] = 10;
        msg[49] = 0;
        msg[50..52].copy_from_slice(&19041u16.to_le_bytes());

        // TargetInfo: offset = 56
        let target_info_off = 56;
        msg[44..48].copy_from_slice(&(target_info_off as u32).to_le_bytes());

        // AV 1: MsvAvNbComputerName = "SERVER1"
        let utf16_comp: Vec<u8> = "SERVER1"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        msg[56..58].copy_from_slice(&1u16.to_le_bytes());
        msg[58..60].copy_from_slice(&(utf16_comp.len() as u16).to_le_bytes());
        msg[60..60 + utf16_comp.len()].copy_from_slice(&utf16_comp);

        // AV 2: MsvAvNbDomainName = "CORP"
        let cur = 60 + utf16_comp.len();
        let utf16_dom: Vec<u8> = "CORP"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        msg[cur..cur + 2].copy_from_slice(&2u16.to_le_bytes());
        msg[cur + 2..cur + 4].copy_from_slice(&(utf16_dom.len() as u16).to_le_bytes());
        msg[cur + 4..cur + 4 + utf16_dom.len()].copy_from_slice(&utf16_dom);

        // MsvAvEOL at end
        let eol = cur + 4 + utf16_dom.len();
        msg[eol..eol + 4].copy_from_slice(&[0, 0, 0, 0]);

        let target_info_len = (eol + 4 - target_info_off) as u16;
        msg[40..42].copy_from_slice(&target_info_len.to_le_bytes());

        let info = parse_ntlm_type2(&msg).expect("parsing ntlm type 2 should succeed");
        assert_eq!(info.nb_computer.as_deref(), Some("SERVER1"));
        assert_eq!(info.nb_domain.as_deref(), Some("CORP"));
        assert_eq!(info.os_build.as_deref(), Some("10.0.19041"));
    }

    #[test]
    fn parse_negotiate_response_reads_dialect_and_signing() {
        let mut resp = vec![0u8; 128];
        resp[0..4].copy_from_slice(&SMB2_MAGIC);
        resp[64..66].copy_from_slice(&SMB2_NEGOTIATE_STRUCTURE_SIZE.to_le_bytes());
        // SecurityMode: SIGNING_REQUIRED (0x0002) + SIGNING_ENABLED (0x0001) = 0x0003
        resp[64 + 2..64 + 4].copy_from_slice(&0x0003u16.to_le_bytes());
        // Dialect: SMB 3.1.1 (0x0311)
        resp[64 + 4..64 + 6].copy_from_slice(&DIALECT_SMB311.to_le_bytes());

        let parsed = parse_negotiate_response(&resp).expect("should parse negotiate");
        assert_eq!(parsed.dialect_revision, DIALECT_SMB311);
        assert!(parsed.signing_required);
        assert!(parsed.signing_enabled);
        assert!(parsed.server_guid.is_some());
    }

    #[test]
    fn security_findings_detect_signing_and_never_emit_smbv1() {
        let mut smb = SmbNegotiateInfo {
            server_guid: None,
            dialect_revision: DIALECT_SMB202,
            signing_required: false,
            signing_enabled: true,
            system_time: None,
            server_start_time: None,
            capabilities: 0,
        };

        let findings = build_security_findings(None, Some(&smb), true);
        assert!(
            findings
                .iter()
                .any(|f| f.message_key == "ToolSmbSigningDisabled"
                    && f.severity == SmbFindingSeverity::Warning)
        );
        // An SMB2 negotiate response can never report SMBv1, so the finding is never built.
        assert!(
            findings
                .iter()
                .all(|f| f.message_key != "ToolSmbV1Detected")
        );
        assert!(
            findings
                .iter()
                .any(|f| f.message_key == "ToolSmbModernDialect"
                    && f.severity == SmbFindingSeverity::Success)
        );
        assert!(
            findings
                .iter()
                .any(|f| f.message_key == "ToolSmbNetBiosFailed"
                    && f.severity == SmbFindingSeverity::Info)
        );

        // Modern dialect with signing
        smb.dialect_revision = DIALECT_SMB311;
        smb.signing_required = true;
        let findings_ok = build_security_findings(None, Some(&smb), false);
        assert!(
            findings_ok
                .iter()
                .any(|f| f.message_key == "ToolSmbSigningEnabled"
                    && f.severity == SmbFindingSeverity::Success)
        );
        assert!(
            findings_ok
                .iter()
                .any(|f| f.message_key == "ToolSmbModernDialect"
                    && f.severity == SmbFindingSeverity::Success)
        );
    }

    #[test]
    fn parse_ntlm_type2_omits_os_build_without_version_flag() {
        // Same layout as the AV-pair test but with NegotiateFlags left at zero: the
        // Version bytes must be ignored.
        let mut msg = vec![0u8; 100];
        msg[0..8].copy_from_slice(&NTLMSSP_SIGNATURE);
        msg[8..12].copy_from_slice(&2u32.to_le_bytes());
        msg[48] = 10;
        msg[49] = 0;
        msg[50..52].copy_from_slice(&19041u16.to_le_bytes());
        let target_info_off = 56u32;
        msg[44..48].copy_from_slice(&target_info_off.to_le_bytes());
        msg[40..42].copy_from_slice(&0u16.to_le_bytes());

        let info = parse_ntlm_type2(&msg).expect("parsing succeeds");
        assert_eq!(info.os_build, None);
    }

    #[test]
    fn report_builder_formats_all_sections() {
        let result = SmbEnumerationResult {
            source: Some(SmbEnumerationSource::Direct),
            computer_name: Some("WIN-SRV".to_string()),
            domain: Some("CONTOSO".to_string()),
            dns_name: Some("win-srv.contoso.local".to_string()),
            dns_domain: Some("contoso.local".to_string()),
            forest: Some("contoso.local".to_string()),
            os_build: Some("10.0.20348".to_string()),
            mac_address: Some("00-15-5D-01-02-03".to_string()),
            dialect_raw: Some(DIALECT_SMB311),
            dialect: Some("SMB 3.1.1".to_string()),
            signing_required: Some(true),
            server_guid: Some("12345678-1234-1234-1234-123456789ABC".to_string()),
            system_time: Some("2026-10-14 12:00:00 UTC".to_string()),
            boot_time: Some("2026-10-01 08:00:00 UTC".to_string()),
            findings: vec![SmbFinding::new(
                SmbFindingSeverity::Success,
                "ToolSmbSigningEnabled",
            )],
            report: None,
        };

        let report = build_report(&result, |k| match k {
            "ToolSmbReportIdentity" => "=== Host Identity ===".to_string(),
            "ToolSmbComputerName" => "Computer Name".to_string(),
            "ToolSmbDomain" => "Domain".to_string(),
            "ToolSmbDnsName" => "DNS Name".to_string(),
            "ToolSmbDnsDomain" => "DNS Domain".to_string(),
            "ToolSmbForest" => "Forest".to_string(),
            "ToolSmbOsBuild" => "OS Build".to_string(),
            "ToolSmbMac" => "MAC Address".to_string(),
            "ToolSmbReportProtocol" => "=== SMB Protocol ===".to_string(),
            "ToolSmbDialect" => "SMB Dialect".to_string(),
            "ToolSmbSigning" => "Signing Required".to_string(),
            "ToolSmbYes" => "Yes".to_string(),
            "ToolSmbServerGuid" => "Server GUID".to_string(),
            "ToolSmbSystemTime" => "System Time".to_string(),
            "ToolSmbBootTime" => "Boot Time".to_string(),
            "ToolSmbReportFindings" => "=== Security Findings ===".to_string(),
            "ToolSmbSigningEnabled" => "SMB signing is required".to_string(),
            _ => k.to_string(),
        });

        assert!(report.contains("=== Host Identity ==="));
        assert!(report.contains("WIN-SRV"));
        assert!(report.contains("SMB 3.1.1"));
        assert!(report.contains("[OK]   SMB signing is required"));
    }

    // ── Hostile replies (C# 4d68e0ea, 92815c40) ─────────────────────────────

    /// A valid Type 2 challenge: names SERVER1 and CORP, build 10.0.19041, TargetInfo at 56.
    fn valid_challenge() -> Vec<u8> {
        let mut msg = vec![0u8; 100];
        msg[0..8].copy_from_slice(&NTLMSSP_SIGNATURE);
        msg[8..12].copy_from_slice(&2u32.to_le_bytes());
        msg[20..24].copy_from_slice(&NTLMSSP_NEGOTIATE_VERSION.to_le_bytes());
        msg[48] = 10;
        msg[50..52].copy_from_slice(&19041u16.to_le_bytes());
        msg[44..48].copy_from_slice(&56u32.to_le_bytes());
        let mut at = 56;
        for (id, text) in [(1u16, "SERVER1"), (2, "CORP")] {
            let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
            msg[at..at + 2].copy_from_slice(&id.to_le_bytes());
            msg[at + 2..at + 4].copy_from_slice(&(utf16.len() as u16).to_le_bytes());
            msg[at + 4..at + 4 + utf16.len()].copy_from_slice(&utf16);
            at += 4 + utf16.len();
        }
        msg[40..42].copy_from_slice(&((at + 4 - 56) as u16).to_le_bytes());
        msg
    }

    /// A challenge whose first attribute is `id` holding the UTF-16 `units`.
    fn challenge_with_attribute(id: u16, units: &[u16]) -> Vec<u8> {
        let payload: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
        let mut msg = valid_challenge();
        msg.resize(60 + payload.len() + 4, 0);
        msg[56..58].copy_from_slice(&id.to_le_bytes());
        msg[58..60].copy_from_slice(&(payload.len() as u16).to_le_bytes());
        msg[60..60 + payload.len()].copy_from_slice(&payload);
        msg[40..42].copy_from_slice(&((payload.len() + 8) as u16).to_le_bytes());
        msg
    }

    #[test]
    fn ntlm_challenge_shorter_than_the_fixed_header_is_not_a_challenge() {
        let msg = valid_challenge();
        for len in [0, 8, 31] {
            assert_eq!(parse_ntlm_type2(&msg[..len]), None);
        }
    }

    #[test]
    fn ntlm_challenge_of_another_type_or_signature_is_refused() {
        for kind in [0u32, 1, 3, u32::MAX] {
            let mut msg = valid_challenge();
            msg[8..12].copy_from_slice(&kind.to_le_bytes());
            assert_eq!(parse_ntlm_type2(&msg), None);
        }
        let mut msg = valid_challenge();
        msg[0] = b'X';
        assert_eq!(parse_ntlm_type2(&msg), None);
    }

    #[test]
    fn ntlm_challenge_without_target_info_fields_gives_an_empty_info() {
        let msg = valid_challenge();
        for len in [32, 40, 47] {
            assert_eq!(parse_ntlm_type2(&msg[..len]), Some(NtlmInfo::default()));
        }
    }

    #[test]
    fn ntlm_target_info_offset_outside_the_buffer_keeps_the_build_and_reads_no_name() {
        for offset in [
            0xFFFF_FFF0u32,
            0x8000_0000,
            u32::MAX,
            0x7FFF_FFFF,
            0x7FFF_FFF0,
            0x0001_0000,
            1,
            40,
            47,
        ] {
            let mut msg = valid_challenge();
            msg[44..48].copy_from_slice(&offset.to_le_bytes());
            let info = parse_ntlm_type2(&msg).expect("a challenge");
            assert_eq!(info.os_build.as_deref(), Some("10.0.19041"), "{offset:#x}");
            assert_eq!(info.nb_computer, None, "{offset:#x}");
            assert_eq!(info.nb_domain, None, "{offset:#x}");
        }
    }

    #[test]
    fn ntlm_target_info_offset_plus_length_overflowing_keeps_the_build() {
        for (offset, len) in [
            (0x7FFF_FFF8u32, 0x10u16),
            (u32::MAX, u16::MAX),
            (0xFFFF_FFF8, 0x10),
            (0x7FFF_FFFF, 1),
        ] {
            let mut msg = valid_challenge();
            msg[44..48].copy_from_slice(&offset.to_le_bytes());
            msg[40..42].copy_from_slice(&len.to_le_bytes());
            let info = parse_ntlm_type2(&msg).expect("a challenge");
            assert_eq!(info.os_build.as_deref(), Some("10.0.19041"));
            assert_eq!(info.nb_computer, None);
        }
    }

    #[test]
    fn ntlm_target_info_longer_than_the_buffer_or_empty_reads_no_name() {
        for len in [0u16, 1000] {
            let mut msg = valid_challenge();
            msg[40..42].copy_from_slice(&len.to_le_bytes());
            let info = parse_ntlm_type2(&msg).expect("a challenge");
            assert_eq!(info.nb_computer, None);
            assert_eq!(info.os_build.as_deref(), Some("10.0.19041"));
        }
    }

    #[test]
    fn ntlm_attribute_longer_than_the_block_keeps_the_ones_before_it() {
        let mut msg = valid_challenge();
        // The second attribute (CORP) starts at 56 + 4 + 14 = 74 and claims 0xFFFF bytes.
        msg[76..78].copy_from_slice(&u16::MAX.to_le_bytes());
        let info = parse_ntlm_type2(&msg).expect("a challenge");
        assert_eq!(info.nb_computer.as_deref(), Some("SERVER1"));
        assert_eq!(info.nb_domain, None);
    }

    #[test]
    fn ntlm_end_of_list_marker_stops_the_read_and_an_odd_length_does_not_panic() {
        let mut msg = valid_challenge();
        msg[56..58].copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(
            parse_ntlm_type2(&msg).expect("a challenge").nb_computer,
            None
        );

        let mut msg = valid_challenge();
        msg[58..60].copy_from_slice(&13u16.to_le_bytes());
        assert!(parse_ntlm_type2(&msg).is_some());
    }

    #[test]
    fn ntlm_names_lose_control_characters_and_are_bounded() {
        let units: Vec<u16> = "SRV\u{1b}[2J\r\n01".encode_utf16().collect();
        let info = parse_ntlm_type2(&challenge_with_attribute(1, &units)).expect("a challenge");
        assert_eq!(info.nb_computer.as_deref(), Some("SRV[2J01"));

        let long = vec![u16::from(b'a'); 2000];
        let info = parse_ntlm_type2(&challenge_with_attribute(1, &long)).expect("a challenge");
        assert_eq!(
            info.nb_computer.map(|name| name.chars().count()),
            Some(network_fingerprint::MAX_FIELD_CHARS)
        );

        let only_controls = [0x07u16, 0x1b];
        let info =
            parse_ntlm_type2(&challenge_with_attribute(1, &only_controls)).expect("a challenge");
        assert_eq!(info.nb_computer, None);
    }

    /// Mutates four bytes of `message` per round, `rounds` times, from a fixed seed.
    fn corruptions(message: &[u8], rounds: usize) -> impl Iterator<Item = Vec<u8>> + '_ {
        let mut state = 0x9E37_79B9u32;
        (0..rounds).map(move |_| {
            let mut copy = message.to_vec();
            for _ in 0..4 {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let at = (state >> 8) as usize % copy.len();
                copy[at] = (state >> 24) as u8;
            }
            copy
        })
    }

    #[test]
    fn ntlm_every_truncation_and_corruption_never_panics() {
        let msg = valid_challenge();
        for len in 0..=msg.len() {
            let _ = parse_ntlm_type2(&msg[..len]);
        }
        for copy in corruptions(&msg, 4000) {
            let _ = parse_ntlm_type2(&copy);
        }
    }

    fn valid_negotiate() -> Vec<u8> {
        let mut resp = vec![0u8; 128];
        resp[0..4].copy_from_slice(&SMB2_MAGIC);
        resp[64..66].copy_from_slice(&SMB2_NEGOTIATE_STRUCTURE_SIZE.to_le_bytes());
        resp[68..70].copy_from_slice(&DIALECT_SMB311.to_le_bytes());
        resp
    }

    #[test]
    fn negotiate_truncated_error_status_foreign_header_or_wrong_size_is_refused() {
        let valid = valid_negotiate();
        assert!(parse_negotiate_response(&valid).is_some());
        for len in [0, 63, 64, 100, 127] {
            assert_eq!(parse_negotiate_response(&valid[..len]), None);
        }
        let mut error = valid.clone();
        error[8..12].copy_from_slice(&0xC000_0022u32.to_le_bytes());
        assert_eq!(parse_negotiate_response(&error), None);
        let mut foreign = valid.clone();
        foreign[0] = b'X';
        assert_eq!(parse_negotiate_response(&foreign), None);
        for size in [0u16, 9, 64, u16::MAX] {
            let mut wrong = valid.clone();
            wrong[64..66].copy_from_slice(&size.to_le_bytes());
            assert_eq!(parse_negotiate_response(&wrong), None);
        }
    }

    #[test]
    fn negotiate_unrepresentable_file_times_are_dropped_not_panicked_on() {
        for ticks in [u64::MAX, 1u64 << 63, 0, 1] {
            let mut resp = valid_negotiate();
            resp[64 + 0x28..64 + 0x30].copy_from_slice(&ticks.to_le_bytes());
            resp[64 + 0x30..64 + 0x38].copy_from_slice(&ticks.to_le_bytes());
            assert!(parse_negotiate_response(&resp).is_some());
        }
        let mut oversized = valid_negotiate();
        oversized.resize(70_000, 0xAB);
        assert!(parse_negotiate_response(&oversized).is_some());
    }

    #[test]
    fn negotiate_every_truncation_and_corruption_never_panics() {
        let resp = valid_negotiate();
        for len in 0..=resp.len() {
            let _ = parse_negotiate_response(&resp[..len]);
        }
        for copy in corruptions(&resp, 4000) {
            let _ = parse_negotiate_response(&copy);
        }
    }

    /// An NBSTAT answer holding `names` (padded to 15 bytes, suffix 0, group flag) and a MAC.
    fn nbstat_answer(names: &[(&[u8], bool)]) -> Vec<u8> {
        let mut data = vec![0u8; 12];
        data.push(0); // the question name, ended by its null label
        data.extend_from_slice(&[0; 4]);
        data.push(0); // the answer name
        data.extend_from_slice(&[0; 10]);
        data.push(names.len() as u8);
        for (name, group) in names {
            let mut padded = [b' '; 15];
            padded[..name.len()].copy_from_slice(name);
            data.extend_from_slice(&padded);
            data.push(0);
            data.extend_from_slice(&(if *group { 0x8000u16 } else { 0 }).to_be_bytes());
        }
        data.extend_from_slice(&[0x00, 0x0C, 0x29, 0xAB, 0xCD, 0xEF]);
        data
    }

    #[test]
    fn netbios_names_lose_control_characters_and_a_blank_name_is_none() {
        let mut answer = nbstat_answer(&[(b"SRV\x1b[2J01", false), (b"\x07\x1b", true)]);
        answer[4..6].copy_from_slice(&1u16.to_be_bytes());
        let (name, domain, mac) = parse_netbios_response(&answer);
        assert_eq!(name.as_deref(), Some("SRV[2J01"));
        assert_eq!(domain, None);
        assert_eq!(mac.as_deref(), Some("00-0C-29-AB-CD-EF"));
    }

    #[test]
    fn netbios_huge_counts_and_label_lengths_never_panic() {
        let mut answer = nbstat_answer(&[(b"HOST", false)]);
        answer[4..6].copy_from_slice(&1u16.to_be_bytes());
        let count_at = 12 + 1 + 4 + 1 + 10;
        answer[count_at] = u8::MAX;
        let _ = parse_netbios_response(&answer);
        // A question count past the buffer, and labels claiming 255 bytes.
        let mut hostile = vec![0xFFu8; 60];
        hostile[4] = 0xFF;
        hostile[5] = 0xFF;
        let _ = parse_netbios_response(&hostile);
        for len in 0..answer.len() {
            let _ = parse_netbios_response(&answer[..len]);
        }
        for copy in corruptions(&answer, 4000) {
            let _ = parse_netbios_response(&copy);
        }
    }
}
