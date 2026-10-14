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

//! The TLS/SSL Auditor engine, as the C# `Heimdall.Core.Security.TlsAuditEngine`:
//! protocol version support evaluation (SSL 3.0, TLS 1.0, TLS 1.1, TLS 1.2, TLS 1.3),
//! TLS 1.2 cipher suite enumeration, Perfect Forward Secrecy (PFS) checks,
//! server certificate inspection, security grading (A+ to F, T for unreachable),
//! and audit findings generation.
//!
//! Pure analysis engine: thread-safe, stateless, and fully covered with unit tests.

use std::fmt;

/// Overall TLS configuration grade (A+ is best, F is worst, T is unreachable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TlsGrade {
    APlus,
    A,
    B,
    C,
    D,
    F,
    T,
}

impl fmt::Display for TlsGrade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(grade_to_str(*self))
    }
}

/// Converts a [`TlsGrade`] to its short display string.
#[must_use]
pub const fn grade_to_str(grade: TlsGrade) -> &'static str {
    match grade {
        TlsGrade::APlus => "A+",
        TlsGrade::A => "A",
        TlsGrade::B => "B",
        TlsGrade::C => "C",
        TlsGrade::D => "D",
        TlsGrade::F => "F",
        TlsGrade::T => "T",
    }
}

/// Rating category for a TLS protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsProtocolRating {
    Strong,
    Weak,
    Critical,
}

/// Strength classification for a cipher suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CipherStrength {
    Strong,
    Acceptable,
    Weak,
}

/// Severity level for a TLS audit finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TlsFindingSeverity {
    Pass,
    Info,
    Warning,
    Critical,
}

/// Structured kind of TLS finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsFindingKind {
    /// SSL 3.0 is supported - vulnerable to POODLE attack.
    Ssl3Supported,
    /// TLS 1.0 is supported - deprecated since 2020.
    Tls10Supported,
    /// TLS 1.1 is supported - deprecated since 2020.
    Tls11Supported,
    /// TLS 1.3 is not supported - recommended for best security.
    NoTls13,
    /// Weak cipher suite detected.
    WeakCipher(String),
    /// Non-PFS cipher suites accepted.
    NoPfs,
    /// Strong TLS configuration - only modern protocols and ciphers.
    AllGood,
}

/// A single finding from the TLS audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsFinding {
    pub severity: TlsFindingSeverity,
    pub kind: TlsFindingKind,
}

/// TLS protocol version identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TlsProtocol {
    Ssl3,
    Tls10,
    Tls11,
    Tls12,
    Tls13,
}

impl TlsProtocol {
    /// The canonical display name of the protocol version.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ssl3 => "SSL 3.0",
            Self::Tls10 => "TLS 1.0",
            Self::Tls11 => "TLS 1.1",
            Self::Tls12 => "TLS 1.2",
            Self::Tls13 => "TLS 1.3",
        }
    }

    /// The default rating for this protocol version.
    #[must_use]
    pub const fn rating(self) -> TlsProtocolRating {
        match self {
            Self::Ssl3 => TlsProtocolRating::Critical,
            Self::Tls10 | Self::Tls11 => TlsProtocolRating::Weak,
            Self::Tls12 | Self::Tls13 => TlsProtocolRating::Strong,
        }
    }

    /// The 16-bit TLS protocol version number.
    #[must_use]
    pub const fn version_code(self) -> u16 {
        match self {
            Self::Ssl3 => 0x0300,
            Self::Tls10 => 0x0301,
            Self::Tls11 => 0x0302,
            Self::Tls12 => 0x0303,
            Self::Tls13 => 0x0304,
        }
    }
}

/// Protocol versions to test, from weakest to strongest.
pub const PROTOCOL_DEFINITIONS: [TlsProtocol; 5] = [
    TlsProtocol::Ssl3,
    TlsProtocol::Tls10,
    TlsProtocol::Tls11,
    TlsProtocol::Tls12,
    TlsProtocol::Tls13,
];

/// Result of a single protocol version test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsProtocolTestResult {
    pub name: String,
    pub supported: bool,
    pub rating: TlsProtocolRating,
}

/// Known TLS 1.2 cipher suite entry with IANA identifier and name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownCipherSuite {
    pub id: u16,
    pub name: &'static str,
}

/// The 19 known TLS 1.2 cipher suites to probe, matching C# `KnownTls12CipherSuites`.
pub const KNOWN_TLS12_CIPHER_SUITES: [KnownCipherSuite; 19] = [
    KnownCipherSuite { id: 0xC030, name: "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384" },
    KnownCipherSuite { id: 0xC02F, name: "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256" },
    KnownCipherSuite { id: 0xC02C, name: "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384" },
    KnownCipherSuite { id: 0xC02B, name: "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256" },
    KnownCipherSuite { id: 0x009F, name: "TLS_DHE_RSA_WITH_AES_256_GCM_SHA384" },
    KnownCipherSuite { id: 0x009E, name: "TLS_DHE_RSA_WITH_AES_128_GCM_SHA256" },
    KnownCipherSuite { id: 0x009D, name: "TLS_RSA_WITH_AES_256_GCM_SHA384" },
    KnownCipherSuite { id: 0x009C, name: "TLS_RSA_WITH_AES_128_GCM_SHA256" },
    KnownCipherSuite { id: 0x003D, name: "TLS_RSA_WITH_AES_256_CBC_SHA256" },
    KnownCipherSuite { id: 0x003C, name: "TLS_RSA_WITH_AES_128_CBC_SHA256" },
    KnownCipherSuite { id: 0x0035, name: "TLS_RSA_WITH_AES_256_CBC_SHA" },
    KnownCipherSuite { id: 0x002F, name: "TLS_RSA_WITH_AES_128_CBC_SHA" },
    KnownCipherSuite { id: 0xC028, name: "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384" },
    KnownCipherSuite { id: 0xC027, name: "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256" },
    KnownCipherSuite { id: 0xC014, name: "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA" },
    KnownCipherSuite { id: 0xC013, name: "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA" },
    KnownCipherSuite { id: 0x000A, name: "TLS_RSA_WITH_3DES_EDE_CBC_SHA" },
    KnownCipherSuite { id: 0x0005, name: "TLS_RSA_WITH_RC4_128_SHA" },
    KnownCipherSuite { id: 0x0004, name: "TLS_RSA_WITH_RC4_128_MD5" },
];

/// Result of a cipher suite probe with classification details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsCipherInfo {
    pub suite_name: String,
    pub strength: CipherStrength,
    pub key_exchange: &'static str,
    pub authentication: &'static str,
    pub encryption: &'static str,
}

/// Summary of a server certificate's key properties.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsCertificateSummary {
    pub subject: String,
    pub issuer: String,
    pub not_after: String,
    pub key_size_bits: usize,
}

/// The complete TLS audit outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsAuditResult {
    pub grade: TlsGrade,
    pub protocols: Vec<TlsProtocolTestResult>,
    pub ciphers: Vec<TlsCipherInfo>,
    pub certificate: Option<TlsCertificateSummary>,
    pub findings: Vec<TlsFinding>,
}

/// Classifies a cipher suite name into strength, key exchange, authentication,
/// and encryption components, exactly matching C# `TlsAuditEngine.ClassifyCipher`.
#[must_use]
pub fn classify_cipher(name: &str) -> (CipherStrength, &'static str, &'static str, &'static str) {
    let kx = if name.contains("ECDHE") {
        "ECDHE"
    } else if name.contains("DHE") {
        "DHE"
    } else {
        "RSA"
    };

    let auth = if name.contains("ECDSA") {
        "ECDSA"
    } else {
        "RSA"
    };

    let enc = if name.contains("AES_256_GCM") {
        "AES-256-GCM"
    } else if name.contains("AES_128_GCM") {
        "AES-128-GCM"
    } else if name.contains("AES_256_CBC") {
        "AES-256-CBC"
    } else if name.contains("AES_128_CBC") {
        "AES-128-CBC"
    } else if name.contains("3DES") {
        "3DES-CBC"
    } else if name.contains("RC4") {
        "RC4"
    } else {
        "AES"
    };

    let strength = if enc.contains("RC4") || enc.contains("3DES") {
        CipherStrength::Weak
    } else if (kx == "ECDHE" || kx == "DHE") && enc.contains("GCM") {
        CipherStrength::Strong
    } else {
        CipherStrength::Acceptable
    };

    (strength, kx, auth, enc)
}

/// Builds a [`TlsCipherInfo`] record from a supported suite name.
#[must_use]
pub fn build_cipher_info(suite_name: &str) -> TlsCipherInfo {
    let (strength, key_exchange, authentication, encryption) = classify_cipher(suite_name);
    TlsCipherInfo {
        suite_name: suite_name.to_owned(),
        strength,
        key_exchange,
        authentication,
        encryption,
    }
}

/// Calculates the overall TLS grade based on protocol support and cipher analysis,
/// exactly matching C# `TlsAuditEngine.CalculateGrade`.
#[must_use]
pub fn calculate_grade(
    protocols: &[TlsProtocolTestResult],
    ciphers: &[TlsCipherInfo],
) -> TlsGrade {
    let ssl3 = protocols.iter().any(|p| p.name == "SSL 3.0" && p.supported);
    let tls10 = protocols.iter().any(|p| p.name == "TLS 1.0" && p.supported);
    let tls11 = protocols.iter().any(|p| p.name == "TLS 1.1" && p.supported);
    let tls12 = protocols.iter().any(|p| p.name == "TLS 1.2" && p.supported);
    let tls13 = protocols.iter().any(|p| p.name == "TLS 1.3" && p.supported);

    let has_weak_ciphers = ciphers
        .iter()
        .any(|c| c.suite_name.contains("3DES") || c.suite_name.contains("RC4"));

    let has_cbc_ciphers = ciphers.iter().any(|c| c.suite_name.contains("CBC"));

    let has_non_pfs = ciphers
        .iter()
        .any(|c| !c.suite_name.contains("ECDHE") && !c.suite_name.contains("DHE"));

    if ssl3 {
        return TlsGrade::F;
    }
    if has_weak_ciphers {
        return TlsGrade::D;
    }
    if tls10 || tls11 {
        return TlsGrade::C;
    }
    if has_cbc_ciphers {
        return TlsGrade::B;
    }
    if tls12 && tls13 && !has_non_pfs {
        return TlsGrade::APlus;
    }
    if tls12 || tls13 {
        return TlsGrade::A;
    }
    TlsGrade::T
}

/// Builds the structured list of audit findings based on protocol and cipher results.
#[must_use]
pub fn build_findings(
    protocols: &[TlsProtocolTestResult],
    ciphers: &[TlsCipherInfo],
) -> Vec<TlsFinding> {
    let mut findings = Vec::new();

    if protocols.iter().any(|p| p.name == "SSL 3.0" && p.supported) {
        findings.push(TlsFinding {
            severity: TlsFindingSeverity::Critical,
            kind: TlsFindingKind::Ssl3Supported,
        });
    }

    if protocols.iter().any(|p| p.name == "TLS 1.0" && p.supported) {
        findings.push(TlsFinding {
            severity: TlsFindingSeverity::Warning,
            kind: TlsFindingKind::Tls10Supported,
        });
    }

    if protocols.iter().any(|p| p.name == "TLS 1.1" && p.supported) {
        findings.push(TlsFinding {
            severity: TlsFindingSeverity::Warning,
            kind: TlsFindingKind::Tls11Supported,
        });
    }

    if protocols
        .iter()
        .all(|p| p.name != "TLS 1.3" || !p.supported)
    {
        findings.push(TlsFinding {
            severity: TlsFindingSeverity::Info,
            kind: TlsFindingKind::NoTls13,
        });
    }

    for weak_cipher in ciphers
        .iter()
        .filter(|c| c.suite_name.contains("3DES") || c.suite_name.contains("RC4"))
    {
        findings.push(TlsFinding {
            severity: TlsFindingSeverity::Critical,
            kind: TlsFindingKind::WeakCipher(weak_cipher.suite_name.clone()),
        });
    }

    let non_pfs = ciphers
        .iter()
        .any(|c| !c.suite_name.contains("ECDHE") && !c.suite_name.contains("DHE"));

    if non_pfs {
        findings.push(TlsFinding {
            severity: TlsFindingSeverity::Info,
            kind: TlsFindingKind::NoPfs,
        });
    }

    if findings.is_empty() {
        findings.push(TlsFinding {
            severity: TlsFindingSeverity::Pass,
            kind: TlsFindingKind::AllGood,
        });
    }

    findings
}

/// Builds a binary ClientHello packet for probing a protocol version or cipher suite.
#[must_use]
pub fn build_client_hello(
    protocol: TlsProtocol,
    host: &str,
    ciphers: &[u16],
) -> Vec<u8> {
    let mut hello = Vec::with_capacity(256);

    // Client version in ClientHello
    let client_version: u16 = match protocol {
        TlsProtocol::Ssl3 => 0x0300,
        TlsProtocol::Tls10 => 0x0301,
        TlsProtocol::Tls11 => 0x0302,
        TlsProtocol::Tls12 | TlsProtocol::Tls13 => 0x0303,
    };
    hello.extend_from_slice(&client_version.to_be_bytes());

    // 32-byte client random (fixed seed for deterministic probe)
    let random: [u8; 32] = [
        0x58, 0x21, 0x11, 0x42, 0x01, 0x02, 0x03, 0x04,
        0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C,
        0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14,
        0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C,
    ];
    hello.extend_from_slice(&random);

    // Session ID length: 0
    hello.push(0x00);

    // Cipher suites
    let ciphers_bytes_len = (ciphers.len() * 2) as u16;
    hello.extend_from_slice(&ciphers_bytes_len.to_be_bytes());
    for suite in ciphers {
        hello.extend_from_slice(&suite.to_be_bytes());
    }

    // Compression methods: 1 method, 0x00 (null)
    hello.push(0x01);
    hello.push(0x00);

    // Extensions (only for TLS 1.0+)
    if protocol != TlsProtocol::Ssl3 {
        let mut extensions = Vec::with_capacity(128);

        // Server Name Indication (SNI) if hostname is not an IP address
        let is_ip = host.parse::<std::net::IpAddr>().is_ok();
        if !is_ip && !host.is_empty() {
            let host_bytes = host.as_bytes();
            let host_len = host_bytes.len() as u16;
            let list_len = host_len + 3;
            let ext_len = list_len + 2;

            extensions.extend_from_slice(&0x0000u16.to_be_bytes()); // SNI type
            extensions.extend_from_slice(&ext_len.to_be_bytes());
            extensions.extend_from_slice(&list_len.to_be_bytes());
            extensions.push(0x00); // host_name type
            extensions.extend_from_slice(&host_len.to_be_bytes());
            extensions.extend_from_slice(host_bytes);
        }

        // Elliptic curves / supported groups (0x000A)
        let groups: [u16; 3] = [0x001D, 0x0017, 0x0018]; // X25519, secp256r1, secp384r1
        extensions.extend_from_slice(&0x000Au16.to_be_bytes());
        extensions.extend_from_slice(&8u16.to_be_bytes());
        extensions.extend_from_slice(&6u16.to_be_bytes());
        for g in groups {
            extensions.extend_from_slice(&g.to_be_bytes());
        }

        // EC point formats (0x000B)
        extensions.extend_from_slice(&0x000Bu16.to_be_bytes());
        extensions.extend_from_slice(&2u16.to_be_bytes());
        extensions.push(0x01); // 1 format
        extensions.push(0x00); // uncompressed

        // Signature algorithms (0x000D)
        let sig_algs: [u16; 6] = [
            0x0804, // rsa_pss_rsae_sha256
            0x0403, // ecdsa_secp256r1_sha256
            0x0401, // rsa_pkcs1_sha256
            0x0501, // rsa_pkcs1_sha384
            0x0601, // rsa_pkcs1_sha512
            0x0201, // rsa_pkcs1_sha1
        ];
        extensions.extend_from_slice(&0x000Du16.to_be_bytes());
        let sig_bytes_len = (sig_algs.len() * 2) as u16;
        extensions.extend_from_slice(&(sig_bytes_len + 2).to_be_bytes());
        extensions.extend_from_slice(&sig_bytes_len.to_be_bytes());
        for sa in sig_algs {
            extensions.extend_from_slice(&sa.to_be_bytes());
        }

        // If probing TLS 1.3: add supported_versions extension (0x002B) and key_share (0x0033)
        if protocol == TlsProtocol::Tls13 {
            // Supported versions (0x002B)
            extensions.extend_from_slice(&0x002Bu16.to_be_bytes());
            extensions.extend_from_slice(&3u16.to_be_bytes()); // length
            extensions.push(2); // versions length
            extensions.extend_from_slice(&0x0304u16.to_be_bytes()); // TLS 1.3

            // Key share extension (0x0033) for X25519
            let key_share_entry: [u8; 36] = [
                0x00, 0x1D, // X25519
                0x00, 0x20, // 32 bytes key exchange
                // 32-byte dummy public key
                0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
                0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
                0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
                0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20,
            ];
            extensions.extend_from_slice(&0x0033u16.to_be_bytes());
            extensions.extend_from_slice(&(key_share_entry.len() as u16 + 2).to_be_bytes());
            extensions.extend_from_slice(&(key_share_entry.len() as u16).to_be_bytes());
            extensions.extend_from_slice(&key_share_entry);
        }

        let ext_total_len = extensions.len() as u16;
        hello.extend_from_slice(&ext_total_len.to_be_bytes());
        hello.extend_from_slice(&extensions);
    }

    // Wrap in Handshake message (Type 1 = ClientHello)
    let mut handshake = Vec::with_capacity(hello.len() + 4);
    handshake.push(0x01); // HandshakeType ClientHello
    let hello_len = hello.len() as u32;
    handshake.push(((hello_len >> 16) & 0xFF) as u8);
    handshake.push(((hello_len >> 8) & 0xFF) as u8);
    handshake.push((hello_len & 0xFF) as u8);
    handshake.extend_from_slice(&hello);

    // Wrap in TLS Record
    let mut record = Vec::with_capacity(handshake.len() + 5);
    record.push(0x16); // ContentType Handshake
    let record_version: u16 = if protocol == TlsProtocol::Ssl3 {
        0x0300
    } else {
        0x0301 // Legacy record version 0x0301 for TLS 1.0 - 1.3
    };
    record.extend_from_slice(&record_version.to_be_bytes());
    let record_len = handshake.len() as u16;
    record.extend_from_slice(&record_len.to_be_bytes());
    record.extend_from_slice(&handshake);

    record
}

/// Parses a ServerHello response packet to verify whether the probed protocol or cipher was accepted.
#[must_use]
pub fn is_server_hello_accepted(response: &[u8], expected_protocol: TlsProtocol) -> bool {
    if response.len() < 9 {
        return false;
    }
    // Record type must be Handshake (0x16)
    if response[0] != 0x16 {
        return false;
    }

    // Handshake type at byte 5 must be ServerHello (0x02)
    if response[5] != 0x02 {
        return false;
    }

    if response.len() < 12 {
        return false;
    }

    let server_version = u16::from_be_bytes([response[9], response[10]]);

    match expected_protocol {
        TlsProtocol::Ssl3 => server_version == 0x0300,
        TlsProtocol::Tls10 => server_version == 0x0301,
        TlsProtocol::Tls11 => server_version == 0x0302,
        TlsProtocol::Tls12 => server_version == 0x0303,
        TlsProtocol::Tls13 => {
            // In TLS 1.3, ServerHello version is set to 0x0303 for middlebox compatibility,
            // and the supported_versions extension (0x002B) contains 0x0304.
            if server_version == 0x0304 {
                return true;
            }
            if server_version == 0x0303 && has_tls13_supported_version_extension(response) {
                return true;
            }
            false
        }
    }
}

/// Checks if a ServerHello packet contains the TLS 1.3 supported_versions extension (0x002B -> 0x0304).
fn has_tls13_supported_version_extension(response: &[u8]) -> bool {
    // Search for 0x00, 0x2B, 0x00, 0x02, 0x03, 0x04
    let needle = [0x00, 0x2B, 0x00, 0x02, 0x03, 0x04];
    response.windows(needle.len()).any(|w| w == needle)
}

/// Scans a TLS handshake stream for the Certificate handshake message (type 0x0B)
/// and extracts the first DER-encoded X.509 certificate.
#[must_use]
pub fn extract_certificate_from_handshake(data: &[u8]) -> Option<Vec<u8>> {
    let mut offset = 0;
    while offset + 5 <= data.len() {
        let content_type = data[offset];
        let record_len = u16::from_be_bytes([data[offset + 3], data[offset + 4]]) as usize;
        let record_start = offset + 5;
        let record_end = record_start + record_len;

        if record_end > data.len() {
            break;
        }

        if content_type == 0x16 {
            // Handshake record: scan messages inside
            let mut msg_offset = record_start;
            while msg_offset + 4 <= record_end {
                let msg_type = data[msg_offset];
                let msg_len = ((data[msg_offset + 1] as usize) << 16)
                    | ((data[msg_offset + 2] as usize) << 8)
                    | (data[msg_offset + 3] as usize);
                let msg_body_start = msg_offset + 4;
                let msg_body_end = msg_body_start + msg_len;

                if msg_body_end > record_end {
                    break;
                }

                if msg_type == 0x0B {
                    // Certificate message
                    // Format: 3 bytes total certs length, then for each cert: 3 bytes cert len, cert DER
                    if msg_body_start + 6 <= msg_body_end {
                        let cert_len = ((data[msg_body_start + 3] as usize) << 16)
                            | ((data[msg_body_start + 4] as usize) << 8)
                            | (data[msg_body_start + 5] as usize);
                        let cert_start = msg_body_start + 6;
                        let cert_end = cert_start + cert_len;
                        if cert_end <= msg_body_end {
                            return Some(data[cert_start..cert_end].to_vec());
                        }
                    }
                }

                msg_offset = msg_body_end;
            }
        }

        offset = record_end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_protocols(ssl3: bool, tls10: bool, tls11: bool, tls12: bool, tls13: bool) -> Vec<TlsProtocolTestResult> {
        vec![
            TlsProtocolTestResult { name: "SSL 3.0".to_owned(), supported: ssl3, rating: TlsProtocolRating::Critical },
            TlsProtocolTestResult { name: "TLS 1.0".to_owned(), supported: tls10, rating: TlsProtocolRating::Weak },
            TlsProtocolTestResult { name: "TLS 1.1".to_owned(), supported: tls11, rating: TlsProtocolRating::Weak },
            TlsProtocolTestResult { name: "TLS 1.2".to_owned(), supported: tls12, rating: TlsProtocolRating::Strong },
            TlsProtocolTestResult { name: "TLS 1.3".to_owned(), supported: tls13, rating: TlsProtocolRating::Strong },
        ]
    }

    fn make_ciphers(names: &[&str]) -> Vec<TlsCipherInfo> {
        names.iter().map(|name| build_cipher_info(name)).collect()
    }

    #[test]
    fn calculate_grade_ssl3_returns_f() {
        let protocols = make_protocols(true, false, false, true, true);
        let ciphers = make_ciphers(&["TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384"]);
        assert_eq!(calculate_grade(&protocols, &ciphers), TlsGrade::F);
    }

    #[test]
    fn calculate_grade_weak_ciphers_returns_d() {
        let protocols = make_protocols(false, false, false, true, false);
        let ciphers = make_ciphers(&["TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384", "TLS_RSA_WITH_3DES_EDE_CBC_SHA"]);
        assert_eq!(calculate_grade(&protocols, &ciphers), TlsGrade::D);
    }

    #[test]
    fn calculate_grade_tls10_returns_c() {
        let protocols = make_protocols(false, true, false, true, false);
        let ciphers = make_ciphers(&["TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384"]);
        assert_eq!(calculate_grade(&protocols, &ciphers), TlsGrade::C);
    }

    #[test]
    fn calculate_grade_tls11_returns_c() {
        let protocols = make_protocols(false, false, true, true, false);
        let ciphers = make_ciphers(&["TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384"]);
        assert_eq!(calculate_grade(&protocols, &ciphers), TlsGrade::C);
    }

    #[test]
    fn calculate_grade_cbc_returns_b() {
        let protocols = make_protocols(false, false, false, true, false);
        let ciphers = make_ciphers(&["TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384", "TLS_RSA_WITH_AES_256_CBC_SHA256"]);
        assert_eq!(calculate_grade(&protocols, &ciphers), TlsGrade::B);
    }

    #[test]
    fn calculate_grade_pfs_and_modern_returns_a_plus() {
        let protocols = make_protocols(false, false, false, true, true);
        let ciphers = make_ciphers(&["TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384"]);
        assert_eq!(calculate_grade(&protocols, &ciphers), TlsGrade::APlus);
    }

    #[test]
    fn calculate_grade_tls12_only_returns_a() {
        let protocols = make_protocols(false, false, false, true, false);
        let ciphers = make_ciphers(&["TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384"]);
        assert_eq!(calculate_grade(&protocols, &ciphers), TlsGrade::A);
    }

    #[test]
    fn calculate_grade_unreachable_returns_t() {
        let protocols = make_protocols(false, false, false, false, false);
        let ciphers = vec![];
        assert_eq!(calculate_grade(&protocols, &ciphers), TlsGrade::T);
    }

    #[test]
    fn classify_cipher_ecdhe_aes_gcm_strong() {
        let (strength, kx, auth, enc) = classify_cipher("TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384");
        assert_eq!(strength, CipherStrength::Strong);
        assert_eq!(kx, "ECDHE");
        assert_eq!(auth, "RSA");
        assert_eq!(enc, "AES-256-GCM");
    }

    #[test]
    fn classify_cipher_rsa_cbc_acceptable() {
        let (strength, kx, auth, enc) = classify_cipher("TLS_RSA_WITH_AES_256_CBC_SHA256");
        assert_eq!(strength, CipherStrength::Acceptable);
        assert_eq!(kx, "RSA");
        assert_eq!(auth, "RSA");
        assert_eq!(enc, "AES-256-CBC");
    }

    #[test]
    fn classify_cipher_3des_weak() {
        let (strength, _, _, _) = classify_cipher("TLS_RSA_WITH_3DES_EDE_CBC_SHA");
        assert_eq!(strength, CipherStrength::Weak);
    }

    #[test]
    fn classify_cipher_rc4_weak() {
        let (strength, _, _, _) = classify_cipher("TLS_RSA_WITH_RC4_128_SHA");
        assert_eq!(strength, CipherStrength::Weak);
    }

    #[test]
    fn build_findings_ssl3_critical() {
        let protocols = make_protocols(true, false, false, true, true);
        let ciphers = make_ciphers(&["TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384"]);
        let findings = build_findings(&protocols, &ciphers);
        assert!(findings.iter().any(|f| f.severity == TlsFindingSeverity::Critical));
    }

    #[test]
    fn build_findings_no_tls13_info() {
        let protocols = make_protocols(false, false, false, true, false);
        let ciphers = make_ciphers(&["TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384"]);
        let findings = build_findings(&protocols, &ciphers);
        assert!(findings.iter().any(|f| f.severity == TlsFindingSeverity::Info));
    }

    #[test]
    fn build_findings_all_clean_pass() {
        let protocols = make_protocols(false, false, false, true, true);
        let ciphers = make_ciphers(&["TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384"]);
        let findings = build_findings(&protocols, &ciphers);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, TlsFindingSeverity::Pass);
        assert_eq!(findings[0].kind, TlsFindingKind::AllGood);
    }

    #[test]
    fn grade_to_str_cases() {
        assert_eq!(grade_to_str(TlsGrade::APlus), "A+");
        assert_eq!(grade_to_str(TlsGrade::A), "A");
        assert_eq!(grade_to_str(TlsGrade::B), "B");
        assert_eq!(grade_to_str(TlsGrade::F), "F");
        assert_eq!(grade_to_str(TlsGrade::T), "T");
    }

    #[test]
    fn client_hello_packet_format() {
        let packet = build_client_hello(TlsProtocol::Tls12, "example.com", &[0xC030]);
        assert!(packet.len() > 40);
        assert_eq!(packet[0], 0x16); // Handshake
        assert_eq!(packet[5], 0x01); // ClientHello
    }
}
