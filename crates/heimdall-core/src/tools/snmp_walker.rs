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

//! Pure SNMPv2c codec and walk engine matching the C# `Heimdall.Core.Network.SnmpCodec`
//! and `SnmpWalkerService.WalkDirectAsync`.
//!
//! Provides ASN.1/BER encoding/decoding, SNMP packet construction, OID resolution,
//! line output parsing, UDP packet exchange, and CSV export.
//!
//! A reply comes from whoever answers on UDP 161: every length and every number it carries
//! is checked before use, and a reply that does not decode is set aside, never a panic.

use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs as _, UdpSocket};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::banner_grab::clean_banner;
use super::csv_cell::quoted;
use super::system_program::POLL_INTERVAL;

/// Default SNMP UDP port.
pub const DEFAULT_PORT: u16 = 161;
/// Default probe timeout in milliseconds.
pub const DEFAULT_TIMEOUT_MS: u64 = 3000;
/// Maximum results returned by a single walk operation.
pub const MAX_WALK_RESULTS: usize = 10000;
/// The community used when none is typed, as the C# `NetworkToolPresets.SnmpDefaultCommunity`.
pub const DEFAULT_COMMUNITY: &str = "public";
/// The subtree walked when none is typed, as the C# `NetworkToolPresets.SnmpDefaultOid`:
/// the MIB-2 system group.
pub const DEFAULT_OID: &str = "1.3.6.1.2.1.1";
/// The longest community sent, in bytes.
pub const MAX_COMMUNITY_LEN: usize = 255;
/// The most sub-identifiers of an OID (RFC 2578 section 3.5).
pub const MAX_OID_ARCS: usize = 128;
/// The communities a community test tries, in order, as the C#
/// `NetworkToolPresets.SnmpCommonCommunities`.
pub const COMMON_COMMUNITIES: [&str; 12] = [
    "public",
    "private",
    "community",
    "default",
    "snmp",
    "monitor",
    "admin",
    "manager",
    "test",
    "cisco",
    "secret",
    "write",
];
/// How long a community test waits for each answer, as the C# `QuerySnmpAsync(host,
/// community, 2000, ct)`.
pub const COMMUNITY_TIMEOUT_MS: u64 = 2000;
/// What a community test asks: the subtree of `sysName`, whose value the C# shows beside an
/// accepted community.
const SYS_NAME_SUBTREE: &str = "1.3.6.1.2.1.1.5";

/// The largest UDP payload a reply can carry.
const MAX_DATAGRAM: usize = 65_535;
/// The most bytes of a long-form BER length read: a reply longer than 4 GB does not exist.
const MAX_LENGTH_BYTES: usize = 4;
/// The most bytes of an INTEGER read, a signed 64-bit value.
const MAX_SIGNED_BYTES: usize = 8;
/// The most bytes of an unsigned value read: a `Counter64` with its leading zero.
const MAX_UNSIGNED_BYTES: usize = 9;
/// SNMP version 2c, as the message's version field says it.
const SNMP_V2C: i64 = 1;
/// The highest request id sent, the C# `Random.Shared.Next(1, 0x7FFFFFFF)` bound.
const MAX_REQUEST_ID: u32 = 0x7FFF_FFFE;

/// ASN.1 and SNMP tags read and written.
const TAG_INTEGER: u8 = 0x02;
const TAG_OCTET_STRING: u8 = 0x04;
const TAG_NULL: u8 = 0x05;
const TAG_OID: u8 = 0x06;
const TAG_SEQUENCE: u8 = 0x30;
const TAG_IP_ADDRESS: u8 = 0x40;
const TAG_COUNTER32: u8 = 0x41;
const TAG_GAUGE32: u8 = 0x42;
const TAG_TIMETICKS: u8 = 0x43;
const TAG_OPAQUE: u8 = 0x44;
const TAG_COUNTER64: u8 = 0x46;
const TAG_NO_SUCH_OBJECT: u8 = 0x80;
const TAG_NO_SUCH_INSTANCE: u8 = 0x81;
const TAG_END_OF_MIB_VIEW: u8 = 0x82;
const TAG_GET_NEXT_REQUEST: u8 = 0xA1;
const TAG_GET_RESPONSE: u8 = 0xA2;

/// A single SNMP MIB variable result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnmpEntry {
    pub oid: String,
    pub name: String,
    pub val_type: String,
    pub value: String,
}

/// Why a walk could not start or got no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkError {
    /// The subtree typed is no OID.
    InvalidOid,
    /// The community typed is longer than [`MAX_COMMUNITY_LEN`].
    CommunityTooLong,
    /// The host is no address and no name that resolves.
    Unresolved,
    /// No UDP socket could be opened or the request sent.
    Socket,
    /// The agent did not answer in time.
    NoResponse,
}

/// How a walk that got answers ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkEnd {
    /// The subtree was read to its end, or the most entries read.
    Complete,
    /// The stop was asked.
    Stopped,
    /// The agent stopped answering: what it said before is kept, as the C#.
    TimedOut,
}

/// What one community of a community test came to, as the C# `CommunityResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityResult {
    pub community: String,
    /// The agent answered it.
    pub accepted: bool,
    /// The `sysName` it gave, when it gave one.
    pub sys_name: Option<String>,
}

/// Static map of well-known MIB-2 OIDs to friendly names.
static WELL_KNOWN_OIDS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("1.3.6.1.2.1.1.1", "sysDescr");
    m.insert("1.3.6.1.2.1.1.1.0", "sysDescr.0");
    m.insert("1.3.6.1.2.1.1.2", "sysObjectID");
    m.insert("1.3.6.1.2.1.1.2.0", "sysObjectID.0");
    m.insert("1.3.6.1.2.1.1.3", "sysUpTime");
    m.insert("1.3.6.1.2.1.1.3.0", "sysUpTime.0");
    m.insert("1.3.6.1.2.1.1.4", "sysContact");
    m.insert("1.3.6.1.2.1.1.4.0", "sysContact.0");
    m.insert("1.3.6.1.2.1.1.5", "sysName");
    m.insert("1.3.6.1.2.1.1.5.0", "sysName.0");
    m.insert("1.3.6.1.2.1.1.6", "sysLocation");
    m.insert("1.3.6.1.2.1.1.6.0", "sysLocation.0");
    m.insert("1.3.6.1.2.1.1.7", "sysServices");
    m.insert("1.3.6.1.2.1.1.7.0", "sysServices.0");
    m.insert("1.3.6.1.2.1.2.1", "ifNumber");
    m.insert("1.3.6.1.2.1.2.1.0", "ifNumber.0");
    m.insert("1.3.6.1.2.1.2.2", "ifTable");
    m.insert("1.3.6.1.2.1.2.2.1.1", "ifIndex");
    m.insert("1.3.6.1.2.1.2.2.1.2", "ifDescr");
    m.insert("1.3.6.1.2.1.2.2.1.3", "ifType");
    m.insert("1.3.6.1.2.1.2.2.1.4", "ifMtu");
    m.insert("1.3.6.1.2.1.2.2.1.5", "ifSpeed");
    m.insert("1.3.6.1.2.1.2.2.1.6", "ifPhysAddress");
    m.insert("1.3.6.1.2.1.2.2.1.7", "ifAdminStatus");
    m.insert("1.3.6.1.2.1.2.2.1.8", "ifOperStatus");
    m.insert("1.3.6.1.2.1.2.2.1.10", "ifInOctets");
    m.insert("1.3.6.1.2.1.2.2.1.16", "ifOutOctets");
    m.insert("1.3.6.1.2.1.4.1", "ipForwarding");
    m.insert("1.3.6.1.2.1.4.2", "ipDefaultTTL");
    m.insert("1.3.6.1.2.1.4.20", "ipAddrTable");
    m.insert("1.3.6.1.2.1.4.21", "ipRouteTable");
    m.insert("1.3.6.1.2.1.6.1", "tcpRtoAlgorithm");
    m.insert("1.3.6.1.2.1.6.9", "tcpCurrEstab");
    m
});

/// Resolves an OID to a friendly name using well-known prefixes.
#[must_use]
pub fn resolve_oid_name(oid: &str) -> String {
    if let Some(&name) = WELL_KNOWN_OIDS.get(oid) {
        return name.to_owned();
    }

    let mut current = oid;
    while let Some(dot_idx) = current.rfind('.') {
        let prefix = &current[..dot_idx];
        if let Some(&base_name) = WELL_KNOWN_OIDS.get(prefix) {
            let suffix = &oid[dot_idx + 1..];
            return format!("{}.{}", base_name, suffix);
        }
        current = prefix;
    }

    String::new()
}

/// Parses a dotted OID string into its sub-identifiers (e.g. "1.3.6.1.2.1.1"), empty
/// segments skipped as the C# `ParseOidString`; `None` unless it can be encoded: two arcs
/// at least, [`MAX_OID_ARCS`] at most, a first arc of 0, 1 or 2 and, under 2, a second
/// below 40 (X.690 section 8.19.4).
#[must_use]
pub fn parse_oid_string(oid: &str) -> Option<Vec<u32>> {
    let mut out = Vec::new();
    for seg in oid.split('.') {
        let s = seg.trim();
        if s.is_empty() {
            continue;
        }
        out.push(s.parse::<u32>().ok()?);
    }
    oid_encodable(&out).then_some(out)
}

/// Whether `components` can be encoded as an OID.
fn oid_encodable(components: &[u32]) -> bool {
    match components {
        [first, second, ..] => {
            components.len() <= MAX_OID_ARCS && (*first == 2 || (*first < 2 && *second < 40))
        }
        _ => false,
    }
}

/// The dotted form of `components`.
fn dotted(components: &[u32]) -> String {
    components
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

/// Whether `oid` lies strictly under `root`: their components compared, never their text,
/// so `1.3.6.1.2.1.10` is not under `1.3.6.1.2.1.1`.
#[must_use]
pub fn is_in_subtree(root: &[u32], oid: &[u32]) -> bool {
    oid.len() > root.len() && oid.starts_with(root)
}

/// The BER length bytes of `len`: short form under 128, else the fewest big-endian bytes.
fn asn1_length(len: usize) -> Vec<u8> {
    if len < 0x80 {
        return vec![len as u8];
    }
    let bytes = len.to_be_bytes();
    let first = bytes
        .iter()
        .position(|b| *b != 0)
        .unwrap_or(bytes.len() - 1);
    let mut out = vec![0x80 | (bytes.len() - first) as u8];
    out.extend_from_slice(&bytes[first..]);
    out
}

/// Wraps content bytes with an ASN.1 tag and length header.
#[must_use]
pub fn asn1_wrap(tag: u8, content: &[u8]) -> Vec<u8> {
    let length = asn1_length(content.len());
    let mut out = Vec::with_capacity(content.len() + length.len() + 1);
    out.push(tag);
    out.extend_from_slice(&length);
    out.extend_from_slice(content);
    out
}

/// Encodes an integer value into ASN.1 BER format.
#[must_use]
pub fn asn1_encode_integer(val: i64) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut v = val;

    loop {
        bytes.push((v & 0xFF) as u8);
        v >>= 8;
        if (v == 0 && (bytes.last().copied().unwrap_or(0) & 0x80) == 0)
            || (v == -1 && (bytes.last().copied().unwrap_or(0) & 0x80) != 0)
        {
            break;
        }
    }

    bytes.reverse();
    asn1_wrap(TAG_INTEGER, &bytes)
}

/// Encodes an octet string into ASN.1 BER format.
#[must_use]
pub fn asn1_encode_octet_string(s: &str) -> Vec<u8> {
    asn1_wrap(TAG_OCTET_STRING, s.as_bytes())
}

/// Appends `value` in base 128, high groups first, every byte but the last flagged.
fn push_base128(out: &mut Vec<u8>, value: u64) {
    let mut groups = vec![(value & 0x7F) as u8];
    let mut rest = value >> 7;
    while rest > 0 {
        groups.push(0x80 | (rest & 0x7F) as u8);
        rest >>= 7;
    }
    out.extend(groups.iter().rev());
}

/// Encodes an OID array into ASN.1 BER format; an empty OID when it is not one (see
/// [`parse_oid_string`]). The first two arcs are combined in 64 bits, so `2.4294967295`
/// encodes without overflow.
#[must_use]
pub fn asn1_encode_oid(components: &[u32]) -> Vec<u8> {
    if !oid_encodable(components) {
        return asn1_wrap(TAG_OID, &[]);
    }

    let mut body = Vec::new();
    push_base128(
        &mut body,
        u64::from(components[0]) * 40 + u64::from(components[1]),
    );
    for &comp in &components[2..] {
        push_base128(&mut body, u64::from(comp));
    }

    asn1_wrap(TAG_OID, &body)
}

/// Decodes ASN.1 BER OID bytes into its sub-identifiers; `None` when a sub-identifier is cut
/// short, does not fit 32 bits, or there are too many.
#[must_use]
pub fn asn1_decode_oid_components(bytes: &[u8]) -> Option<Vec<u32>> {
    let mut values = Vec::new();
    let mut current: u64 = 0;
    let mut pending = false;
    for &b in bytes {
        if current > u64::from(u32::MAX) {
            return None;
        }
        current = (current << 7) | u64::from(b & 0x7F);
        pending = (b & 0x80) != 0;
        if !pending {
            values.push(current);
            current = 0;
        }
    }
    if pending || values.is_empty() {
        return None;
    }
    let first = values[0];
    let (arc0, arc1) = match first {
        0..40 => (0, first),
        40..80 => (1, first - 40),
        _ => (2, first - 80),
    };
    let mut out = vec![arc0];
    out.push(u32::try_from(arc1).ok()?);
    for value in &values[1..] {
        out.push(u32::try_from(*value).ok()?);
    }
    (out.len() <= MAX_OID_ARCS).then_some(out)
}

/// Decodes ASN.1 BER OID bytes into a dotted string; empty when they are no OID.
#[must_use]
pub fn asn1_decode_oid(bytes: &[u8]) -> String {
    asn1_decode_oid_components(bytes)
        .map(|components| dotted(&components))
        .unwrap_or_default()
}

/// One ASN.1 TLV read: its tag, its content and the bytes it took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tlv<'a> {
    pub tag: u8,
    pub content: &'a [u8],
    pub consumed: usize,
}

/// Reads an ASN.1 TLV (tag, length, value) at a given offset; `None` when it does not fit
/// in `bytes`, whatever length it claims.
#[must_use]
pub fn asn1_read_tlv(bytes: &[u8], offset: usize) -> Option<Tlv<'_>> {
    let tag = *bytes.get(offset)?;
    let len_byte = *bytes.get(offset.checked_add(1)?)?;
    let mut cur = offset + 2;

    let len = if (len_byte & 0x80) == 0 {
        usize::from(len_byte)
    } else {
        let num_bytes = usize::from(len_byte & 0x7F);
        // Zero is BER's indefinite length, which SNMP never uses.
        if num_bytes == 0 || num_bytes > MAX_LENGTH_BYTES {
            return None;
        }
        let length_bytes = bytes.get(cur..cur.checked_add(num_bytes)?)?;
        cur += num_bytes;
        length_bytes
            .iter()
            .fold(0usize, |l, b| (l << 8) | usize::from(*b))
    };

    let end = cur.checked_add(len)?;
    let content = bytes.get(cur..end)?;
    Some(Tlv {
        tag,
        content,
        consumed: end - offset,
    })
}

/// A BER INTEGER's content as a signed value; `None` when empty or past 64 bits.
fn decode_signed(content: &[u8]) -> Option<i64> {
    if content.is_empty() || content.len() > MAX_SIGNED_BYTES {
        return None;
    }
    let start: i64 = if content[0] & 0x80 != 0 { -1 } else { 0 };
    Some(
        content
            .iter()
            .fold(start, |value, b| (value << 8) | i64::from(*b)),
    )
}

/// An unsigned value's content, as the C# `Asn1DecodeUnsignedInt`; `None` past 64 bits.
fn decode_unsigned(content: &[u8]) -> Option<u64> {
    let significant = match content {
        [0, rest @ ..] if content.len() == MAX_UNSIGNED_BYTES => rest,
        _ if content.len() >= MAX_UNSIGNED_BYTES => return None,
        _ => content,
    };
    Some(
        significant
            .iter()
            .fold(0u64, |value, b| (value << 8) | u64::from(*b)),
    )
}

/// Builds an SNMPv2c GetNextRequest packet for tree walking.
#[must_use]
pub fn build_snmp_get_next_request(community: &str, oid: &[u32], request_id: i32) -> Vec<u8> {
    let oid_bytes = asn1_encode_oid(oid);
    let null_val = [TAG_NULL, 0x00];

    let mut varbind_content = Vec::new();
    varbind_content.extend_from_slice(&oid_bytes);
    varbind_content.extend_from_slice(&null_val);
    let varbind = asn1_wrap(TAG_SEQUENCE, &varbind_content);

    let varbind_list = asn1_wrap(TAG_SEQUENCE, &varbind);

    let req_id_bytes = asn1_encode_integer(i64::from(request_id));
    let error_status = asn1_encode_integer(0);
    let error_index = asn1_encode_integer(0);

    let mut pdu_content = Vec::new();
    pdu_content.extend_from_slice(&req_id_bytes);
    pdu_content.extend_from_slice(&error_status);
    pdu_content.extend_from_slice(&error_index);
    pdu_content.extend_from_slice(&varbind_list);

    let pdu = asn1_wrap(TAG_GET_NEXT_REQUEST, &pdu_content);

    let version = asn1_encode_integer(SNMP_V2C);
    let comm_bytes = asn1_encode_octet_string(community);

    let mut msg_content = Vec::new();
    msg_content.extend_from_slice(&version);
    msg_content.extend_from_slice(&comm_bytes);
    msg_content.extend_from_slice(&pdu);

    asn1_wrap(TAG_SEQUENCE, &msg_content)
}

/// An SNMP GetResponse read: the request it answers, its error status and its first
/// variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnmpResponse {
    pub request_id: i64,
    pub error_status: i64,
    pub oid: Vec<u32>,
    /// The value's tag, telling the end of the MIB view and missing objects apart.
    pub value_tag: u8,
    pub val_type: String,
    pub value: String,
}

impl SnmpResponse {
    /// Whether the value says there is nothing more to read here, as the C# stops on
    /// `endOfMibView`, `noSuchObject` and `noSuchInstance`.
    #[must_use]
    pub fn is_exception(&self) -> bool {
        matches!(
            self.value_tag,
            TAG_NO_SUCH_OBJECT | TAG_NO_SUCH_INSTANCE | TAG_END_OF_MIB_VIEW
        )
    }
}

/// Parses an SNMP GetResponse packet; `None` when it is not one, whatever its bytes.
#[must_use]
pub fn parse_snmp_response(data: &[u8]) -> Option<SnmpResponse> {
    let message = asn1_read_tlv(data, 0).filter(|tlv| tlv.tag == TAG_SEQUENCE)?;
    let msg = message.content;

    let version = asn1_read_tlv(msg, 0).filter(|tlv| tlv.tag == TAG_INTEGER)?;
    let community =
        asn1_read_tlv(msg, version.consumed).filter(|tlv| tlv.tag == TAG_OCTET_STRING)?;
    let pdu = asn1_read_tlv(msg, version.consumed + community.consumed)
        .filter(|tlv| tlv.tag == TAG_GET_RESPONSE)?;
    let pdu = pdu.content;

    let request_id = asn1_read_tlv(pdu, 0).filter(|tlv| tlv.tag == TAG_INTEGER)?;
    let error_status =
        asn1_read_tlv(pdu, request_id.consumed).filter(|tlv| tlv.tag == TAG_INTEGER)?;
    let error_index = asn1_read_tlv(pdu, request_id.consumed + error_status.consumed)
        .filter(|tlv| tlv.tag == TAG_INTEGER)?;
    let list = asn1_read_tlv(
        pdu,
        request_id.consumed + error_status.consumed + error_index.consumed,
    )
    .filter(|tlv| tlv.tag == TAG_SEQUENCE)?;
    let varbind = asn1_read_tlv(list.content, 0).filter(|tlv| tlv.tag == TAG_SEQUENCE)?;
    let oid = asn1_read_tlv(varbind.content, 0).filter(|tlv| tlv.tag == TAG_OID)?;
    let value = asn1_read_tlv(varbind.content, oid.consumed)?;

    let (val_type, text) = decode_snmp_value(value.tag, value.content);
    Some(SnmpResponse {
        request_id: decode_signed(request_id.content)?,
        error_status: decode_signed(error_status.content)?,
        oid: asn1_decode_oid_components(oid.content)?,
        value_tag: value.tag,
        val_type,
        value: text,
    })
}

/// The name of a value's tag, as the C# `GetTagName`.
fn tag_name(tag: u8) -> String {
    match tag {
        TAG_INTEGER => "INTEGER".to_owned(),
        TAG_OCTET_STRING => "STRING".to_owned(),
        TAG_NULL => "NULL".to_owned(),
        TAG_OID => "OID".to_owned(),
        TAG_IP_ADDRESS => "IpAddress".to_owned(),
        TAG_COUNTER32 => "Counter32".to_owned(),
        TAG_GAUGE32 => "Gauge32".to_owned(),
        TAG_TIMETICKS => "TimeTicks".to_owned(),
        TAG_OPAQUE => "Opaque".to_owned(),
        TAG_COUNTER64 => "Counter64".to_owned(),
        TAG_NO_SUCH_OBJECT => "noSuchObject".to_owned(),
        TAG_NO_SUCH_INSTANCE => "noSuchInstance".to_owned(),
        TAG_END_OF_MIB_VIEW => "endOfMibView".to_owned(),
        _ => format!("0x{tag:02X}"),
    }
}

/// Bytes as upper-case hexadecimal, as the C# `Convert.ToHexString`.
fn hex(content: &[u8]) -> String {
    content.iter().map(|b| format!("{b:02X}")).collect()
}

/// An octet string as the C# `DecodeOctetString` shows it: text when it holds no control
/// byte but tab and line ends, else its bytes in hexadecimal joined by hyphens.
fn decode_octet_string(content: &[u8]) -> String {
    let printable = content
        .iter()
        .all(|b| *b >= 0x20 || matches!(b, 0x09 | 0x0A | 0x0D));
    if printable {
        String::from_utf8_lossy(content).trim().to_owned()
    } else {
        content
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join("-")
    }
}

/// The C# `FormatTimeTicks`: days, hours, minutes and seconds, then the seconds.
fn format_timeticks(centiseconds: u64) -> String {
    let secs = centiseconds / 100;
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{days}d {hours:02}:{mins:02}:{s:02} ({secs}s)")
}

/// Decodes an SNMP value based on its ASN.1 tag, as the C# `DecodeSnmpValue`: numbers as
/// numbers, counters and gauges unsigned. The text is cleaned of control characters, so
/// what an agent sends never reaches the window or the clipboard as it came.
fn decode_snmp_value(tag: u8, content: &[u8]) -> (String, String) {
    if content.is_empty() {
        return (tag_name(tag), String::new());
    }
    let value = match tag {
        TAG_INTEGER => decode_signed(content).map_or_else(|| hex(content), |v| v.to_string()),
        TAG_OCTET_STRING => decode_octet_string(content),
        TAG_NULL | TAG_NO_SUCH_OBJECT | TAG_NO_SUCH_INSTANCE | TAG_END_OF_MIB_VIEW => String::new(),
        TAG_OID => asn1_decode_oid(content),
        TAG_IP_ADDRESS => match content {
            [a, b, c, d] => format!("{a}.{b}.{c}.{d}"),
            _ => hex(content),
        },
        TAG_COUNTER32 | TAG_GAUGE32 | TAG_COUNTER64 => {
            decode_unsigned(content).map_or_else(|| hex(content), |v| v.to_string())
        }
        TAG_TIMETICKS => decode_unsigned(content).map_or_else(|| hex(content), format_timeticks),
        _ => hex(content),
    };
    (tag_name(tag), clean_banner(&value).unwrap_or_default())
}

/// Parses a line from standard `snmpwalk` output.
#[must_use]
pub fn parse_snmpwalk_line(line: &str) -> Option<SnmpEntry> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (oid_part, value_part) = trimmed.split_once(" = ")?;

    let mut clean_oid = oid_part.trim().replace("iso.", "1.");
    if clean_oid.starts_with("iso") {
        clean_oid = format!("1{}", &clean_oid[3..]);
    }

    let display_name = if let Some((_, after_colons)) = clean_oid.split_once("::") {
        after_colons.to_owned()
    } else {
        resolve_oid_name(&clean_oid)
    };

    let (val_type, value) = if let Some((t, v)) = value_part.split_once(": ") {
        (t.trim().to_owned(), v.trim().to_owned())
    } else {
        (value_part.trim().to_owned(), String::new())
    };

    Some(SnmpEntry {
        oid: clean_oid,
        name: display_name,
        val_type,
        value: clean_banner(&value).unwrap_or_default(),
    })
}

/// Builds CSV export content for SNMP entries under the column names `header` (OID, name,
/// type, value), every cell sanitized against formulas and quoted, as the C#
/// `BuildCsvExport` sanitizes them.
#[must_use]
pub fn build_snmp_csv(entries: &[SnmpEntry], header: [&str; 4]) -> String {
    let mut out = header.map(quoted).join(",");
    out.push('\n');

    for e in entries {
        out.push_str(&format!(
            "{},{},{},{}\n",
            quoted(&e.oid),
            quoted(&e.name),
            quoted(&e.val_type),
            quoted(&e.value)
        ));
    }

    out
}

/// A request id, positive as the C# draws it; a fixed one when the system has no random
/// bytes, the check of the reply's id still holding.
fn next_request_id() -> i32 {
    let drawn = sealvault::random::array::<4>().map_or(1, u32::from_be_bytes);
    let id = drawn % MAX_REQUEST_ID + 1;
    i32::try_from(id).unwrap_or(1)
}

/// The agent's address: an IPv4 one first, as the C# picks it, else the first.
fn resolve(host: &str, port: u16) -> Result<SocketAddr, WalkError> {
    let addresses: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|_| WalkError::Unresolved)?
        .collect();
    addresses
        .iter()
        .find(|address| address.is_ipv4())
        .or_else(|| addresses.first())
        .copied()
        .ok_or(WalkError::Unresolved)
}

/// What waiting for the answer to one request gave.
enum Answer {
    Got(SnmpResponse),
    Stopped,
    TimedOut,
}

/// Waits up to `timeout` for the answer to request `request_id` on `socket`, connected to
/// the agent: datagrams that do not decode, or answer another request, are set aside, as a
/// late answer to an earlier request would otherwise be read as this one's.
fn await_answer(
    socket: &UdpSocket,
    request_id: i32,
    timeout: Duration,
    stop: &AtomicBool,
) -> Answer {
    let started = Instant::now();
    let mut buffer = vec![0u8; MAX_DATAGRAM];
    loop {
        if stop.load(Ordering::Relaxed) {
            return Answer::Stopped;
        }
        if started.elapsed() >= timeout {
            return Answer::TimedOut;
        }
        let len = match socket.recv(&mut buffer) {
            Ok(len) => len,
            // The poll's own timeout.
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            // Windows reports an ICMP port unreachable on the next receive: waited out as
            // a silence, without spinning.
            Err(_) => {
                std::thread::sleep(POLL_INTERVAL);
                continue;
            }
        };
        if let Some(response) = parse_snmp_response(&buffer[..len])
            && response.request_id == i64::from(request_id)
        {
            return Answer::Got(response);
        }
    }
}

/// Walks the subtree `start_oid_str` of the agent at `host`:`port` with `community`, as the
/// C# `WalkDirectAsync`: GetNext after GetNext, each answer handed to `on_entry` as it
/// comes, until the answer leaves the subtree, says the MIB view is over, reports an error
/// or does not move forward, `max_entries` (at most [`MAX_WALK_RESULTS`]) are read, or
/// `stop` is set.
///
/// # Errors
///
/// [`WalkError`] when the walk cannot start, or the agent never answers its first request;
/// once it has answered, a silence ends the walk with [`WalkEnd::TimedOut`] and what was
/// read is kept, as the C#.
pub fn walk(
    host: &str,
    port: u16,
    community: &str,
    start_oid_str: &str,
    max_entries: usize,
    timeout_ms: u64,
    stop: &AtomicBool,
    mut on_entry: impl FnMut(SnmpEntry),
) -> Result<WalkEnd, WalkError> {
    let root = parse_oid_string(start_oid_str).ok_or(WalkError::InvalidOid)?;
    if community.len() > MAX_COMMUNITY_LEN {
        return Err(WalkError::CommunityTooLong);
    }
    let agent = resolve(host, port)?;
    let local: SocketAddr = if agent.is_ipv4() {
        ([0, 0, 0, 0], 0).into()
    } else {
        ([0u16; 8], 0).into()
    };
    let socket = UdpSocket::bind(local).map_err(|_| WalkError::Socket)?;
    socket.connect(agent).map_err(|_| WalkError::Socket)?;
    socket
        .set_read_timeout(Some(POLL_INTERVAL))
        .map_err(|_| WalkError::Socket)?;
    let timeout = Duration::from_millis(timeout_ms);

    let mut current = root.clone();
    let mut read = 0;
    while read < max_entries.min(MAX_WALK_RESULTS) {
        let request_id = next_request_id();
        let packet = build_snmp_get_next_request(community, &current, request_id);
        socket.send(&packet).map_err(|_| WalkError::Socket)?;

        let response = match await_answer(&socket, request_id, timeout, stop) {
            Answer::Got(response) => response,
            Answer::Stopped => return Ok(WalkEnd::Stopped),
            Answer::TimedOut if read == 0 => return Err(WalkError::NoResponse),
            Answer::TimedOut => return Ok(WalkEnd::TimedOut),
        };
        if response.error_status != 0
            || response.is_exception()
            || !is_in_subtree(&root, &response.oid)
            || response.oid <= current
        {
            break;
        }

        let oid = dotted(&response.oid);
        on_entry(SnmpEntry {
            name: resolve_oid_name(&oid),
            oid,
            val_type: response.val_type,
            value: response.value,
        });
        read += 1;
        current = response.oid;
    }

    Ok(WalkEnd::Complete)
}

/// Tries each of `communities` on the agent at `host`:`port`, one after the other as the
/// C# `TestCommunitiesAsync`, each waiting `timeout_ms` for its answer: a community the
/// agent answers is accepted, with the `sysName` it gives; one it leaves unanswered is
/// rejected, as SNMPv2c agents stay silent to a wrong one. `on_result` is told of each as it
/// ends. Better than the C#, which says only those accepted, every community tried is told.
///
/// # Errors
///
/// [`WalkError::Unresolved`] or [`WalkError::Socket`] when no request can be sent, which no
/// other community would change.
pub fn test_communities(
    host: &str,
    port: u16,
    communities: &[&str],
    timeout_ms: u64,
    stop: &AtomicBool,
    mut on_result: impl FnMut(CommunityResult),
) -> Result<WalkEnd, WalkError> {
    for community in communities {
        if stop.load(Ordering::Relaxed) {
            return Ok(WalkEnd::Stopped);
        }
        let mut sys_name = None;
        let accepted = match walk(
            host,
            port,
            community,
            SYS_NAME_SUBTREE,
            1,
            timeout_ms,
            stop,
            |entry| sys_name = Some(entry.value),
        ) {
            Ok(WalkEnd::Stopped) => return Ok(WalkEnd::Stopped),
            // An answer, even one saying the subtree is empty or not in the view.
            Ok(WalkEnd::Complete | WalkEnd::TimedOut) => true,
            Err(WalkError::NoResponse | WalkError::CommunityTooLong | WalkError::InvalidOid) => {
                false
            }
            Err(error @ (WalkError::Unresolved | WalkError::Socket)) => return Err(error),
        };
        on_result(CommunityResult {
            community: (*community).to_owned(),
            accepted,
            sys_name: sys_name.filter(|name| !name.is_empty()),
        });
    }
    Ok(WalkEnd::Complete)
}

#[cfg(test)]
mod tests {
    use std::net::UdpSocket;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use super::*;

    #[test]
    fn oid_parsing_and_resolution() {
        let oid = parse_oid_string("1.3.6.1.2.1.1.1.0").unwrap();
        assert_eq!(oid, vec![1, 3, 6, 1, 2, 1, 1, 1, 0]);

        assert_eq!(resolve_oid_name("1.3.6.1.2.1.1.1.0"), "sysDescr.0");
        assert_eq!(resolve_oid_name("1.3.6.1.2.1.1.5.0"), "sysName.0");
    }

    #[test]
    fn an_oid_typed_that_cannot_be_encoded_is_refused_never_a_panic() {
        for refused in [
            "",
            "1",
            "3.1",
            "1.40",
            "0.99.1",
            "4294967295.1",
            "1.3.x",
            "1.3.99999999999",
            "-1.3",
        ] {
            assert!(parse_oid_string(refused).is_none(), "{refused:?}");
        }
        let many = vec!["1"; MAX_OID_ARCS + 1].join(".");
        assert!(parse_oid_string(&many).is_none(), "past 128 arcs");
        // The largest arcs encode and decode back.
        let large = parse_oid_string("2.4294967295.4294967295").expect("valid");
        let encoded = asn1_encode_oid(&large);
        let tlv = asn1_read_tlv(&encoded, 0).expect("TLV");
        assert_eq!(
            asn1_decode_oid_components(tlv.content),
            Some(vec![2, 4_294_967_295, 4_294_967_295])
        );
        assert!(
            asn1_encode_oid(&[3, 1]).ends_with(&[TAG_OID, 0]),
            "not encoded"
        );
    }

    #[test]
    fn asn1_oid_round_trip() {
        let oid = [1, 3, 6, 1, 2, 1, 1, 1, 0];
        let encoded = asn1_encode_oid(&oid);
        assert_eq!(encoded[0], 0x06); // OID tag
        let decoded = asn1_decode_oid(&encoded[2..]);
        assert_eq!(decoded, "1.3.6.1.2.1.1.1.0");
        let large = [1, 3, 6, 1, 4, 1, 311, 128, 16_384];
        let encoded = asn1_encode_oid(&large);
        assert_eq!(
            asn1_decode_oid_components(&encoded[2..]).as_deref(),
            Some(&large[..])
        );
    }

    #[test]
    fn a_long_form_length_past_the_data_is_refused_without_overflow() {
        // 0x88: eight length bytes, all 0xFF: cur + len would overflow.
        let mut data = vec![0x30, 0x88];
        data.extend_from_slice(&[0xFF; 8]);
        data.extend_from_slice(&[0; 16]);
        assert!(asn1_read_tlv(&data, 0).is_none());
        // Four length bytes claiming 4 GB.
        assert!(asn1_read_tlv(&[0x04, 0x84, 0xFF, 0xFF, 0xFF, 0xFF, 1], 0).is_none());
        // Indefinite length.
        assert!(asn1_read_tlv(&[0x30, 0x80, 0, 0], 0).is_none());
        // An offset past the end, and one at usize::MAX.
        assert!(asn1_read_tlv(&[0x02, 0x01, 0x05], 3).is_none());
        assert!(asn1_read_tlv(&[0x02, 0x01, 0x05], usize::MAX).is_none());
        let long = asn1_wrap(TAG_OCTET_STRING, &[7; 300]);
        assert_eq!(&long[..4], &[TAG_OCTET_STRING, 0x82, 0x01, 0x2C]);
        let read = asn1_read_tlv(&long, 0).expect("read");
        assert_eq!((read.content.len(), read.consumed), (300, long.len()));
    }

    /// A GetResponse to `request_id` carrying `oid` with a value of `tag` and `content`.
    fn response(request_id: i32, oid: &[u32], tag: u8, content: &[u8]) -> Vec<u8> {
        let mut varbind = asn1_encode_oid(oid);
        varbind.extend(asn1_wrap(tag, content));
        let list = asn1_wrap(TAG_SEQUENCE, &asn1_wrap(TAG_SEQUENCE, &varbind));
        let mut pdu = asn1_encode_integer(i64::from(request_id));
        pdu.extend(asn1_encode_integer(0));
        pdu.extend(asn1_encode_integer(0));
        pdu.extend(list);
        let mut message = asn1_encode_integer(SNMP_V2C);
        message.extend(asn1_encode_octet_string("public"));
        message.extend(asn1_wrap(TAG_GET_RESPONSE, &pdu));
        asn1_wrap(TAG_SEQUENCE, &message)
    }

    #[test]
    fn counters_and_gauges_are_numbers_and_integers_signed() {
        let read = |tag, content: &[u8]| {
            parse_snmp_response(&response(7, &[1, 3, 6, 1, 2, 1, 1, 3, 0], tag, content))
                .expect("parsed")
        };
        let counter = read(TAG_COUNTER32, &[0x00, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(
            (counter.val_type.as_str(), counter.value.as_str()),
            ("Counter32", "4294967295")
        );
        let gauge = read(TAG_GAUGE32, &[0x01, 0x00]);
        assert_eq!(
            (gauge.val_type.as_str(), gauge.value.as_str()),
            ("Gauge32", "256")
        );
        let counter64 = read(
            TAG_COUNTER64,
            &[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        );
        assert_eq!(counter64.value, u64::MAX.to_string());
        assert_eq!(read(TAG_INTEGER, &[0xFF]).value, "-1");
        assert_eq!(read(TAG_INTEGER, &[0x01, 0x00]).value, "256");
        assert_eq!(read(TAG_TIMETICKS, &[0x01, 0x00]).value, "0d 00:00:02 (2s)");
        assert_eq!(read(TAG_IP_ADDRESS, &[10, 0, 0, 1]).value, "10.0.0.1");
        assert_eq!(read(TAG_OCTET_STRING, b"Router-01").value, "Router-01");
        assert_eq!(
            read(TAG_OCTET_STRING, &[0x00, 0x1B, 0xFF]).value,
            "00-1B-FF"
        );
        assert_eq!(
            read(TAG_OCTET_STRING, b"line\r\nnext\tcol").value,
            "line next col",
            "no control character reaches the window"
        );
        assert!(read(TAG_END_OF_MIB_VIEW, &[]).is_exception());
        assert_eq!(
            read(TAG_COUNTER32, &[1; 12]).value,
            "010101010101010101010101"
        );
    }

    #[test]
    fn the_request_id_and_the_error_status_are_read() {
        let parsed =
            parse_snmp_response(&response(-5, &[1, 3, 6, 1], TAG_NULL, &[])).expect("parsed");
        assert_eq!(parsed.request_id, -5);
        assert_eq!(parsed.error_status, 0);
        assert_eq!(parsed.oid, vec![1, 3, 6, 1]);
        // A request is no response.
        assert!(parse_snmp_response(&build_snmp_get_next_request("public", &[1, 3], 1)).is_none());
    }

    #[test]
    fn the_subtree_is_checked_by_components_not_text() {
        let root = [1, 3, 6, 1, 2, 1, 1];
        assert!(is_in_subtree(&root, &[1, 3, 6, 1, 2, 1, 1, 5, 0]));
        assert!(
            !is_in_subtree(&root, &[1, 3, 6, 1, 2, 1, 10, 1]),
            "1.10 is no child of 1.1"
        );
        assert!(!is_in_subtree(&root, &root), "the root itself");
        assert!(!is_in_subtree(&root, &[1, 3, 6, 1, 2, 1, 2]));
    }

    /// A small generator, so the malformed replies below are the same at every run.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }
    }

    #[test]
    fn no_reply_whatever_its_bytes_makes_the_parser_panic() {
        let valid = response(42, &[1, 3, 6, 1, 2, 1, 1, 5, 0], TAG_OCTET_STRING, b"name");
        let mut random = Lcg(0x5EED_5A1F);
        for round in 0..20_000 {
            let mut data = valid.clone();
            match round % 4 {
                // A few bytes changed.
                0 | 1 => {
                    for _ in 0..=random.next() % 4 {
                        let at = (random.next() as usize) % data.len();
                        data[at] = random.next() as u8;
                    }
                }
                // Cut short.
                2 => data.truncate((random.next() as usize) % data.len()),
                // Bytes of nothing in particular.
                _ => {
                    let len = (random.next() as usize) % 64;
                    data = (0..len).map(|_| random.next() as u8).collect();
                }
            }
            let _ = parse_snmp_response(&data);
            let _ = asn1_decode_oid(&data);
            let _ = parse_snmp_response(&data[..data.len() / 2]);
        }
    }

    #[test]
    fn hostile_values_are_bounded_never_wrapped_and_never_carry_control_characters() {
        let oid = [1, 3, 6, 1, 2, 1, 1, 3, 0];
        let read = |tag, content: &[u8]| {
            parse_snmp_response(&response(7, &oid, tag, content)).expect("parsed")
        };
        // TimeTicks and counters wider than 64 bits are shown as bytes, not wrapped.
        let wide = [0x01u8; 12];
        assert_eq!(read(TAG_TIMETICKS, &wide).value, hex(&wide));
        assert_eq!(read(TAG_COUNTER32, &wide).value, hex(&wide));
        assert_eq!(read(TAG_GAUGE32, &[0xFF; 9]).value, hex(&[0xFF; 9]));
        // The largest time is formatted without overflow.
        assert!(
            read(TAG_TIMETICKS, &[0xFF; 8])
                .value
                .starts_with(&format!("{}d", u64::MAX / 100 / 86_400))
        );
        // An INTEGER past 64 bits is bytes too.
        assert_eq!(read(TAG_INTEGER, &[0x7F; 12]).value, hex(&[0x7F; 12]));
        // Text with escape sequences or a carriage return is never shown as it came.
        let shown = read(TAG_OCTET_STRING, b"host\x1b[2J\rname").value;
        assert!(!shown.chars().any(char::is_control), "{shown:?}");
        let shown = read(TAG_OCTET_STRING, "caf\u{85}\u{202e}x\u{0}".as_bytes()).value;
        assert!(!shown.chars().any(char::is_control), "{shown:?}");
        // A megabyte of text is cut.
        assert!(read(TAG_OCTET_STRING, &vec![b'a'; 60_000]).value.len() <= 60_000);
    }

    #[test]
    fn nested_lengths_larger_than_their_container_are_refused() {
        // A message whose PDU claims more than the message holds, and a varbind list whose
        // varbind claims more than the list.
        let valid = response(1, &[1, 3, 6, 1], TAG_OCTET_STRING, b"x");
        assert!(parse_snmp_response(&valid).is_some());
        for at in 0..valid.len() - 1 {
            for claim in [0x7Fu8, 0xFF] {
                let mut data = valid.clone();
                if data[at] == 0x30 || data[at] == TAG_GET_RESPONSE {
                    data[at + 1] = claim;
                    let _ = parse_snmp_response(&data);
                }
            }
        }
        let mut data = vec![0x30, 0x84, 0xFF, 0xFF, 0xFF, 0xFF];
        data.extend_from_slice(&valid[2..]);
        assert!(parse_snmp_response(&data).is_none());
        let mut data = vec![0x30, 0x88];
        data.extend_from_slice(&[0xFF; 8]);
        assert!(parse_snmp_response(&data).is_none());
        // Empty and one-byte datagrams.
        assert!(parse_snmp_response(&[]).is_none());
        assert!(parse_snmp_response(&[0x30]).is_none());
    }

    #[test]
    fn parse_snmpwalk_line_output() {
        let line = "iso.3.6.1.2.1.1.5.0 = STRING: Router-Core-01";
        let entry = parse_snmpwalk_line(line).unwrap();
        assert_eq!(entry.oid, "1.3.6.1.2.1.1.5.0");
        assert_eq!(entry.name, "sysName.0");
        assert_eq!(entry.val_type, "STRING");
        assert_eq!(entry.value, "Router-Core-01");
    }

    #[test]
    fn snmp_csv_export() {
        let entries = vec![
            SnmpEntry {
                oid: "1.3.6.1.2.1.1.1.0".to_owned(),
                name: "sysDescr.0".to_owned(),
                val_type: "STRING".to_owned(),
                value: "Linux 6.1".to_owned(),
            },
            SnmpEntry {
                oid: "1.3.6.1.2.1.1.4.0".to_owned(),
                name: "sysContact.0".to_owned(),
                val_type: "STRING".to_owned(),
                value: "=cmd|' /C calc'!A0 \"x\"".to_owned(),
            },
        ];

        let csv = build_snmp_csv(&entries, ["OID", "Name", "Type", "Value"]);
        assert!(csv.starts_with("\"OID\",\"Name\",\"Type\",\"Value\"\n"));
        assert!(csv.contains("\"1.3.6.1.2.1.1.1.0\",\"sysDescr.0\",\"STRING\",\"Linux 6.1\""));
        assert!(
            csv.contains("\"'=cmd|' /C calc'!A0 \"\"x\"\"\""),
            "a formula is defused and quotes doubled: {csv}"
        );
    }

    /// A fake agent on the loopback answering GetNext requests from `table`, sorted, after
    /// first sending a stray reply to another request id; it answers `answers` requests.
    fn agent(table: Vec<(Vec<u32>, Vec<u8>)>, answers: usize) -> u16 {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("bind");
        let port = socket.local_addr().expect("addr").port();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        std::thread::spawn(move || {
            let mut buffer = [0u8; 2048];
            for _ in 0..answers {
                let Ok((len, from)) = socket.recv_from(&mut buffer) else {
                    return;
                };
                let request = parse_request(&buffer[..len]);
                let (id, asked) = request;
                let next = table.iter().find(|(oid, _)| *oid > asked);
                let reply = match next {
                    Some((oid, value)) => response(id, oid, TAG_OCTET_STRING, value),
                    None => response(id, &asked, TAG_END_OF_MIB_VIEW, &[]),
                };
                // A stray answer to another request, then the right one.
                let stray = response(id.wrapping_add(1), &[1, 3, 6, 1, 9, 9], TAG_NULL, &[]);
                let _ = socket.send_to(&stray, from);
                let _ = socket.send_to(&[0x30, 0x88, 0xFF], from);
                let _ = socket.send_to(&reply, from);
            }
        });
        port
    }

    /// The request id and OID of a GetNext request.
    fn parse_request(data: &[u8]) -> (i32, Vec<u32>) {
        let message = asn1_read_tlv(data, 0).expect("message").content;
        let version = asn1_read_tlv(message, 0).expect("version");
        let community = asn1_read_tlv(message, version.consumed).expect("community");
        let pdu = asn1_read_tlv(message, version.consumed + community.consumed)
            .expect("pdu")
            .content;
        let id = asn1_read_tlv(pdu, 0).expect("id");
        let status = asn1_read_tlv(pdu, id.consumed).expect("status");
        let index = asn1_read_tlv(pdu, id.consumed + status.consumed).expect("index");
        let list = asn1_read_tlv(pdu, id.consumed + status.consumed + index.consumed)
            .expect("list")
            .content;
        let varbind = asn1_read_tlv(list, 0).expect("varbind").content;
        let oid = asn1_read_tlv(varbind, 0).expect("oid").content;
        let id = decode_signed(id.content).expect("id");
        (
            i32::try_from(id).expect("i32"),
            asn1_decode_oid_components(oid).expect("oid"),
        )
    }

    #[test]
    fn a_walk_reads_its_subtree_only_and_sets_aside_stray_replies() {
        let table = vec![
            (vec![1, 3, 6, 1, 2, 1, 1, 1, 0], b"Linux".to_vec()),
            (vec![1, 3, 6, 1, 2, 1, 1, 5, 0], b"router".to_vec()),
            (vec![1, 3, 6, 1, 2, 1, 10, 1, 0], b"outside".to_vec()),
        ];
        let port = agent(table, 10);
        let stop = AtomicBool::new(false);
        let mut entries = Vec::new();
        let end = walk(
            "127.0.0.1",
            port,
            DEFAULT_COMMUNITY,
            DEFAULT_OID,
            MAX_WALK_RESULTS,
            2000,
            &stop,
            |entry| entries.push(entry),
        )
        .expect("walked");
        assert_eq!(end, WalkEnd::Complete);
        let read: Vec<(&str, &str)> = entries
            .iter()
            .map(|entry| (entry.oid.as_str(), entry.value.as_str()))
            .collect();
        assert_eq!(
            read,
            [
                ("1.3.6.1.2.1.1.1.0", "Linux"),
                ("1.3.6.1.2.1.1.5.0", "router")
            ],
            "1.3.6.1.2.1.10 is outside 1.3.6.1.2.1.1"
        );
        assert_eq!(entries[1].name, "sysName.0");
    }

    #[test]
    fn a_silent_agent_is_no_response_and_a_stop_ends_the_walk() {
        let silent = UdpSocket::bind("127.0.0.1:0").expect("bind");
        let port = silent.local_addr().expect("addr").port();
        let stop = AtomicBool::new(false);
        let started = std::time::Instant::now();
        assert_eq!(
            walk(
                "127.0.0.1",
                port,
                "public",
                "1.3.6.1",
                10,
                300,
                &stop,
                |_| {}
            ),
            Err(WalkError::NoResponse)
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        let stop = AtomicBool::new(true);
        assert_eq!(
            walk(
                "127.0.0.1",
                port,
                "public",
                "1.3.6.1",
                10,
                60_000,
                &stop,
                |_| {}
            ),
            Ok(WalkEnd::Stopped)
        );
        assert_eq!(
            walk("127.0.0.1", port, "public", "1.x", 10, 300, &stop, |_| {}),
            Err(WalkError::InvalidOid)
        );
        let long = "c".repeat(MAX_COMMUNITY_LEN + 1);
        assert_eq!(
            walk("127.0.0.1", port, &long, "1.3", 10, 300, &stop, |_| {}),
            Err(WalkError::CommunityTooLong)
        );
        drop(silent);
    }

    /// A fake agent on the loopback answering only the community `accepted`, its `sysName.0`
    /// being `name`, and silent to any other, as an SNMPv2c agent; it reads `requests`.
    fn community_agent(accepted: &'static str, name: &'static [u8], requests: usize) -> u16 {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("bind");
        let port = socket.local_addr().expect("addr").port();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("timeout");
        std::thread::spawn(move || {
            let mut buffer = [0u8; 2048];
            for _ in 0..requests {
                let Ok((len, from)) = socket.recv_from(&mut buffer) else {
                    return;
                };
                let data = &buffer[..len];
                let message = asn1_read_tlv(data, 0).expect("message").content;
                let version = asn1_read_tlv(message, 0).expect("version");
                let community = asn1_read_tlv(message, version.consumed).expect("community");
                if community.content != accepted.as_bytes() {
                    continue;
                }
                let (id, _) = parse_request(data);
                let reply = response(id, &[1, 3, 6, 1, 2, 1, 1, 5, 0], TAG_OCTET_STRING, name);
                let _ = socket.send_to(&reply, from);
            }
        });
        port
    }

    #[test]
    fn a_community_test_tries_each_and_says_the_accepted_ones_with_their_sys_name() {
        let port = community_agent("monitor", b"core-sw1", COMMON_COMMUNITIES.len());
        let stop = AtomicBool::new(false);
        let mut results = Vec::new();
        let end = test_communities(
            "127.0.0.1",
            port,
            &["public", "monitor", "secret"],
            300,
            &stop,
            |result| results.push(result),
        );
        assert_eq!(end, Ok(WalkEnd::Complete));
        let said: Vec<(&str, bool, Option<&str>)> = results
            .iter()
            .map(|r| (r.community.as_str(), r.accepted, r.sys_name.as_deref()))
            .collect();
        assert_eq!(
            said,
            [
                ("public", false, None),
                ("monitor", true, Some("core-sw1")),
                ("secret", false, None)
            ]
        );
        assert_eq!(COMMON_COMMUNITIES[0], DEFAULT_COMMUNITY);
    }

    #[test]
    fn a_stopped_community_test_tries_no_more_and_an_unknown_host_is_said() {
        let silent = UdpSocket::bind("127.0.0.1:0").expect("bind");
        let port = silent.local_addr().expect("addr").port();
        let stop = AtomicBool::new(true);
        let mut tried = 0;
        assert_eq!(
            test_communities("127.0.0.1", port, &COMMON_COMMUNITIES, 300, &stop, |_| {
                tried += 1;
            }),
            Ok(WalkEnd::Stopped)
        );
        assert_eq!(tried, 0);
        let stop = AtomicBool::new(false);
        assert_eq!(
            test_communities(
                "no-such-host.invalid",
                port,
                &COMMON_COMMUNITIES,
                300,
                &stop,
                |_| {
                    tried += 1;
                }
            ),
            Err(WalkError::Unresolved)
        );
        assert_eq!(
            tried, 0,
            "no community said rejected for a host that is none"
        );
        drop(silent);
    }
}
