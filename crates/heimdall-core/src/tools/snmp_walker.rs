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

//! Pure SNMPv2c codec and walk engine matching the C# `Heimdall.Core.Network.SnmpCodec`.
//!
//! Provides ASN.1/BER encoding/decoding, SNMP packet construction, OID resolution,
//! line output parsing, UDP packet exchange, and CSV export.

use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::LazyLock;
use std::time::Duration;

/// Default SNMP UDP port.
pub const DEFAULT_PORT: u16 = 161;
/// Default probe timeout in milliseconds.
pub const DEFAULT_TIMEOUT_MS: u64 = 3000;
/// Maximum results returned by a single walk operation.
pub const MAX_WALK_RESULTS: usize = 10000;

/// A single SNMP MIB variable result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnmpEntry {
    pub oid: String,
    pub name: String,
    pub val_type: String,
    pub value: String,
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

/// Parses a dotted OID string into integer components (e.g. "1.3.6.1.2.1.1").
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
    if out.is_empty() { None } else { Some(out) }
}

/// Wraps content bytes with an ASN.1 tag and length header.
#[must_use]
pub fn asn1_wrap(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + 4);
    out.push(tag);

    let len = content.len();
    if len < 0x80 {
        out.push(len as u8);
    } else if len <= 0xFF {
        out.push(0x81);
        out.push(len as u8);
    } else {
        out.push(0x82);
        out.push(((len >> 8) & 0xFF) as u8);
        out.push((len & 0xFF) as u8);
    }

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
    asn1_wrap(0x02, &bytes)
}

/// Encodes an octet string into ASN.1 BER format.
#[must_use]
pub fn asn1_encode_octet_string(s: &str) -> Vec<u8> {
    asn1_wrap(0x04, s.as_bytes())
}

/// Encodes an OID array into ASN.1 BER format.
#[must_use]
pub fn asn1_encode_oid(components: &[u32]) -> Vec<u8> {
    if components.len() < 2 {
        return asn1_wrap(0x06, &[]);
    }

    let mut body = Vec::new();
    // First byte is (X * 40) + Y
    body.push((components[0] * 40 + components[1]) as u8);

    for &comp in &components[2..] {
        if comp < 0x80 {
            body.push(comp as u8);
        } else {
            let mut stack = Vec::new();
            let mut val = comp;
            while val > 0 {
                stack.push((val & 0x7F) as u8);
                val >>= 7;
            }
            for (idx, b) in stack.iter().rev().enumerate() {
                if idx < stack.len() - 1 {
                    body.push(b | 0x80);
                } else {
                    body.push(*b);
                }
            }
        }
    }

    asn1_wrap(0x06, &body)
}

/// Decodes ASN.1 BER OID bytes into a dotted string.
#[must_use]
pub fn asn1_decode_oid(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    let mut comps = Vec::new();
    let first = bytes[0];
    comps.push(format!("{}", first / 40));
    comps.push(format!("{}", first % 40));

    let mut current: u64 = 0;
    for &b in &bytes[1..] {
        current = (current << 7) | ((b & 0x7F) as u64);
        if (b & 0x80) == 0 {
            comps.push(format!("{}", current));
            current = 0;
        }
    }

    comps.join(".")
}

/// Reads an ASN.1 TLV (tag, length, value) at a given offset.
pub fn asn1_read_tlv(bytes: &[u8], offset: usize) -> Option<(usize, Vec<u8>)> {
    if offset >= bytes.len() {
        return None;
    }

    let _tag = bytes[offset];
    let mut cur = offset + 1;
    if cur >= bytes.len() {
        return None;
    }

    let len_byte = bytes[cur];
    cur += 1;

    let len = if (len_byte & 0x80) == 0 {
        len_byte as usize
    } else {
        let num_bytes = (len_byte & 0x7F) as usize;
        if cur + num_bytes > bytes.len() {
            return None;
        }
        let mut l = 0usize;
        for i in 0..num_bytes {
            l = (l << 8) | (bytes[cur + i] as usize);
        }
        cur += num_bytes;
        l
    };

    if cur + len > bytes.len() {
        return None;
    }

    let total_consumed = (cur + len) - offset;
    let content = bytes[cur..cur + len].to_vec();
    Some((total_consumed, content))
}

/// Builds an SNMPv2c GetNextRequest packet for tree walking.
#[must_use]
pub fn build_snmp_get_next_request(community: &str, oid: &[u32], request_id: i32) -> Vec<u8> {
    let oid_bytes = asn1_encode_oid(oid);
    let null_val = [0x05, 0x00]; // Null tag

    let mut varbind_content = Vec::new();
    varbind_content.extend_from_slice(&oid_bytes);
    varbind_content.extend_from_slice(&null_val);
    let varbind = asn1_wrap(0x30, &varbind_content);

    let varbind_list = asn1_wrap(0x30, &varbind);

    let req_id_bytes = asn1_encode_integer(request_id as i64);
    let error_status = asn1_encode_integer(0);
    let error_index = asn1_encode_integer(0);

    let mut pdu_content = Vec::new();
    pdu_content.extend_from_slice(&req_id_bytes);
    pdu_content.extend_from_slice(&error_status);
    pdu_content.extend_from_slice(&error_index);
    pdu_content.extend_from_slice(&varbind_list);

    let pdu = asn1_wrap(0xA1, &pdu_content); // GetNextRequest-PDU (0xA1)

    let version = asn1_encode_integer(1); // SNMPv2c = 1
    let comm_bytes = asn1_encode_octet_string(community);

    let mut msg_content = Vec::new();
    msg_content.extend_from_slice(&version);
    msg_content.extend_from_slice(&comm_bytes);
    msg_content.extend_from_slice(&pdu);

    asn1_wrap(0x30, &msg_content)
}

/// Parses an SNMP Response packet extracting (OID, Type, Value).
pub fn parse_snmp_get_next_response(data: &[u8]) -> (Option<String>, String, String) {
    let Some((_, msg_content)) = asn1_read_tlv(data, 0) else {
        return (None, String::new(), String::new());
    };

    let mut offset = 0;
    // Version
    let Some((ver_len, _)) = asn1_read_tlv(&msg_content, offset) else {
        return (None, String::new(), String::new());
    };
    offset += ver_len;

    // Community
    let Some((comm_len, _)) = asn1_read_tlv(&msg_content, offset) else {
        return (None, String::new(), String::new());
    };
    offset += comm_len;

    // PDU (Response = 0xA2)
    let Some((_, pdu_content)) = asn1_read_tlv(&msg_content, offset) else {
        return (None, String::new(), String::new());
    };

    let mut pdu_offset = 0;
    // Skip requestId, errorStatus, errorIndex
    for _ in 0..3 {
        let Some((skip_len, _)) = asn1_read_tlv(&pdu_content, pdu_offset) else {
            return (None, String::new(), String::new());
        };
        pdu_offset += skip_len;
    }

    // VarBindList
    let Some((_, varbind_list)) = asn1_read_tlv(&pdu_content, pdu_offset) else {
        return (None, String::new(), String::new());
    };

    // VarBind
    let Some((_, varbind)) = asn1_read_tlv(&varbind_list, 0) else {
        return (None, String::new(), String::new());
    };

    // OID
    let Some((oid_len, oid_content)) = asn1_read_tlv(&varbind, 0) else {
        return (None, String::new(), String::new());
    };

    let oid = asn1_decode_oid(&oid_content);
    if oid_len >= varbind.len() {
        return (Some(oid), String::new(), String::new());
    }

    let val_tag = varbind[oid_len];
    let val_content = if let Some((_, c)) = asn1_read_tlv(&varbind, oid_len) {
        c
    } else {
        Vec::new()
    };

    let (val_type, value) = decode_snmp_value(val_tag, &val_content);
    (Some(oid), val_type, value)
}

/// Decodes an SNMP value based on ASN.1 tag.
fn decode_snmp_value(tag: u8, content: &[u8]) -> (String, String) {
    match tag {
        0x02 => {
            // Integer
            let mut val = 0i64;
            for &b in content {
                val = (val << 8) | (b as i64);
            }
            ("INTEGER".to_owned(), format!("{}", val))
        }
        0x04 => {
            // OctetString
            let s = String::from_utf8_lossy(content).to_string();
            ("STRING".to_owned(), s)
        }
        0x06 => {
            // Object Identifier
            ("OID".to_owned(), asn1_decode_oid(content))
        }
        0x40 => {
            // IpAddress
            if content.len() == 4 {
                let ip = format!(
                    "{}.{}.{}.{}",
                    content[0], content[1], content[2], content[3]
                );
                ("IpAddress".to_owned(), ip)
            } else {
                ("IpAddress".to_owned(), "-".to_owned())
            }
        }
        0x41 => ("Counter32".to_owned(), format!("{:?}", content)),
        0x42 => ("Gauge32".to_owned(), format!("{:?}", content)),
        0x43 => {
            // Timeticks (hundredths of a second)
            let mut ticks = 0u64;
            for &b in content {
                ticks = (ticks << 8) | (b as u64);
            }
            let secs = ticks / 100;
            let days = secs / 86400;
            let hours = (secs % 86400) / 3600;
            let mins = (secs % 3600) / 60;
            let s = secs % 60;
            (
                "Timeticks".to_owned(),
                format!(
                    "{} days, {:02}:{:02}:{:02} ({})",
                    days, hours, mins, s, ticks
                ),
            )
        }
        _ => (format!("Tag(0x{:02X})", tag), format!("{:?}", content)),
    }
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
        value,
    })
}

/// Builds CSV export content for SNMP entries.
#[must_use]
pub fn build_snmp_csv(entries: &[SnmpEntry]) -> String {
    let mut out = String::new();
    out.push_str("OID,Name,Type,Value\n");

    for e in entries {
        let safe_name = e.name.replace('"', "\"\"");
        let safe_val = e.value.replace('"', "\"\"");
        out.push_str(&format!(
            "{},\"{}\",{},\"{}\"\n",
            e.oid, safe_name, e.val_type, safe_val
        ));
    }

    out
}

/// Performs an SNMP walk over UDP against a target host starting from `start_oid`.
pub fn walk_oid(
    host: &str,
    port: u16,
    community: &str,
    start_oid_str: &str,
    max_entries: usize,
    timeout_ms: u64,
) -> Result<Vec<SnmpEntry>, String> {
    let start_oid = parse_oid_string(start_oid_str)
        .ok_or_else(|| format!("Invalid start OID: {}", start_oid_str))?;

    let socket =
        UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("Failed to bind UDP socket: {}", e))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .ok();
    socket
        .set_write_timeout(Some(Duration::from_millis(timeout_ms)))
        .ok();

    let target = format!("{}:{}", host, port);
    let mut current_oid = start_oid;
    let mut results = Vec::new();
    let mut req_id = 1000;

    for _ in 0..max_entries.min(MAX_WALK_RESULTS) {
        req_id += 1;
        let packet = build_snmp_get_next_request(community, &current_oid, req_id);
        socket
            .send_to(&packet, &target)
            .map_err(|e| format!("UDP send failed: {}", e))?;

        let mut buf = [0u8; 4096];
        let (len, _) = socket
            .recv_from(&mut buf)
            .map_err(|e| format!("UDP recv timeout or error: {}", e))?;

        let (next_oid_opt, val_type, value) = parse_snmp_get_next_response(&buf[..len]);
        let Some(next_oid) = next_oid_opt else {
            break;
        };

        // If returned OID is not within the requested prefix or didn't advance, stop walk
        if !next_oid.starts_with(start_oid_str)
            || next_oid
                == current_oid
                    .iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(".")
        {
            break;
        }

        let name = resolve_oid_name(&next_oid);
        results.push(SnmpEntry {
            oid: next_oid.clone(),
            name,
            val_type,
            value,
        });

        if let Some(parsed) = parse_oid_string(&next_oid) {
            current_oid = parsed;
        } else {
            break;
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oid_parsing_and_resolution() {
        let oid = parse_oid_string("1.3.6.1.2.1.1.1.0").unwrap();
        assert_eq!(oid, vec![1, 3, 6, 1, 2, 1, 1, 1, 0]);

        assert_eq!(resolve_oid_name("1.3.6.1.2.1.1.1.0"), "sysDescr.0");
        assert_eq!(resolve_oid_name("1.3.6.1.2.1.1.5.0"), "sysName.0");
    }

    #[test]
    fn asn1_oid_round_trip() {
        let oid = [1, 3, 6, 1, 2, 1, 1, 1, 0];
        let encoded = asn1_encode_oid(&oid);
        assert_eq!(encoded[0], 0x06); // OID tag
        let decoded = asn1_decode_oid(&encoded[2..]);
        assert_eq!(decoded, "1.3.6.1.2.1.1.1.0");
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
        let entries = vec![SnmpEntry {
            oid: "1.3.6.1.2.1.1.1.0".to_owned(),
            name: "sysDescr.0".to_owned(),
            val_type: "STRING".to_owned(),
            value: "Linux 6.1".to_owned(),
        }];

        let csv = build_snmp_csv(&entries);
        assert!(csv.contains("OID,Name,Type,Value"));
        assert!(csv.contains("1.3.6.1.2.1.1.1.0,\"sysDescr.0\",STRING,\"Linux 6.1\""));
    }
}
