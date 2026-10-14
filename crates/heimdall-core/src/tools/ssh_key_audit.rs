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

//! The SSH Key Auditor engine, as the C# `SshKeyAuditEngine` (`SshKeyAuditEngine.cs`):
//! pure parsing and security assessment for SSH keys. Supports OpenSSH (public and private),
//! PEM (PKCS#1, PKCS#8, legacy DSA, legacy EC), and SPKI formats.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::module_name_repetitions,
    clippy::redundant_closure_for_method_calls,
    clippy::too_many_lines
)]

use sealvault::hash::{Algorithm, Hasher};

/// Minimum acceptable RSA key size in bits.
pub const MIN_RSA_KEY_SIZE: usize = 2048;

/// RSA key size considered strong in bits.
pub const STRONG_RSA_KEY_SIZE: usize = 3072;

/// ECDSA curve size considered strong in bits.
pub const STRONG_ECDSA_KEY_SIZE: usize = 384;

/// Standard Ed25519 key size in bits.
pub const ED25519_KEY_SIZE: usize = 256;

/// Maximum size in bytes accepted for an SSH key input or file (1 MB).
pub const MAX_KEY_FILE_SIZE: usize = 1_048_576;

const OPENSSH_RSA_PREFIX: &str = "ssh-rsa";
const OPENSSH_DSA_PREFIX: &str = "ssh-dss";
const OPENSSH_ED25519_PREFIX: &str = "ssh-ed25519";
const OPENSSH_ECDSA_P256_PREFIX: &str = "ecdsa-sha2-nistp256";
const OPENSSH_ECDSA_P384_PREFIX: &str = "ecdsa-sha2-nistp384";
const OPENSSH_ECDSA_P521_PREFIX: &str = "ecdsa-sha2-nistp521";

const PEM_BEGIN_OPENSSH_PRIVATE: &str = "-----BEGIN OPENSSH PRIVATE KEY-----";
const PEM_BEGIN_RSA_PRIVATE: &str = "-----BEGIN RSA PRIVATE KEY-----";
const PEM_BEGIN_DSA_PRIVATE: &str = "-----BEGIN DSA PRIVATE KEY-----";
const PEM_BEGIN_EC_PRIVATE: &str = "-----BEGIN EC PRIVATE KEY-----";
const PEM_BEGIN_PRIVATE: &str = "-----BEGIN PRIVATE KEY-----";
const PEM_BEGIN_ENCRYPTED_PRIVATE: &str = "-----BEGIN ENCRYPTED PRIVATE KEY-----";
const PEM_BEGIN_PUBLIC: &str = "-----BEGIN PUBLIC KEY-----";
const PEM_BEGIN_RSA_PUBLIC: &str = "-----BEGIN RSA PUBLIC KEY-----";

const PROC_TYPE_ENCRYPTED: &str = "Proc-Type: 4,ENCRYPTED";
const OPENSSH_KDF_BCRYPT: &str = "bcrypt";
const OPENSSH_MAGIC: &[u8] = b"openssh-key-v1\0";

const OID_ED25519: &[u8] = &[0x06, 0x03, 0x2B, 0x65, 0x70];
const OID_RSA_ENCRYPTION: &[u8] = &[
    0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01,
];
const OID_DSA: &[u8] = &[0x06, 0x07, 0x2A, 0x86, 0x48, 0xCE, 0x38, 0x04, 0x01];
const OID_EC_PUBLIC_KEY: &[u8] = &[0x06, 0x07, 0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01];
const OID_SECP256R1: &[u8] = &[0x06, 0x08, 0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07];
const OID_SECP384R1: &[u8] = &[0x06, 0x05, 0x2B, 0x81, 0x04, 0x00, 0x22];
const OID_SECP521R1: &[u8] = &[0x06, 0x05, 0x2B, 0x81, 0x04, 0x00, 0x23];

/// Overall security rating for an SSH key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityRating {
    Strong,
    Acceptable,
    Weak,
    Deprecated,
}

/// Severity level for an individual audit finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingSeverity {
    Pass,
    Warning,
    Fail,
}

/// Category of an audit finding for UI presentation and localization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    Ed25519Strong,
    RsaStrong(usize),
    RsaOk,
    RsaWeak(usize),
    DsaDeprecated,
    EcdsaStrong(usize),
    EcdsaOk,
    UnencryptedPrivateKey,
    EncryptedPrivateKey,
    OldPemFormat,
}

/// A single security finding from an SSH key audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshKeyAuditFinding {
    pub severity: FindingSeverity,
    pub kind: FindingKind,
}

/// Result of parsing and auditing an SSH key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshKeyAuditResult {
    pub algorithm: String,
    pub key_size: usize,
    pub fingerprint: String,
    pub format: String,
    pub is_private_key: bool,
    pub is_encrypted: bool,
    pub rating: SecurityRating,
    pub findings: Vec<SshKeyAuditFinding>,
}

/// Parses an SSH key (any supported format) and returns the security audit result,
/// or `None` if the input is empty, oversized, or not a recognized key format.
#[must_use]
pub fn audit(key_text: &str) -> Option<SshKeyAuditResult> {
    let trimmed = key_text.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_KEY_FILE_SIZE {
        return None;
    }

    let first_line = trimmed.lines().next().unwrap_or("").trim();

    if is_openssh_public_prefix(first_line) {
        return parse_openssh_public_key(first_line);
    }

    if trimmed.contains(PEM_BEGIN_OPENSSH_PRIVATE) {
        return parse_openssh_private_key(trimmed);
    }

    if trimmed.contains(PEM_BEGIN_RSA_PRIVATE) {
        return parse_pkcs1_rsa_private_key(trimmed);
    }

    if trimmed.contains(PEM_BEGIN_DSA_PRIVATE) {
        return parse_legacy_dsa_private_key(trimmed);
    }

    if trimmed.contains(PEM_BEGIN_EC_PRIVATE) {
        return parse_legacy_ec_private_key(trimmed);
    }

    if trimmed.contains(PEM_BEGIN_ENCRYPTED_PRIVATE) {
        return parse_pkcs8_private_key(trimmed, true);
    }

    if trimmed.contains(PEM_BEGIN_PRIVATE) {
        return parse_pkcs8_private_key(trimmed, false);
    }

    if trimmed.contains(PEM_BEGIN_PUBLIC) {
        return parse_spki_public_key(trimmed);
    }

    if trimmed.contains(PEM_BEGIN_RSA_PUBLIC) {
        return parse_pkcs1_rsa_public_key(trimmed);
    }

    None
}

fn is_openssh_public_prefix(line: &str) -> bool {
    line.starts_with("ssh-rsa ")
        || line.starts_with("ssh-dss ")
        || line.starts_with("ssh-ed25519 ")
        || line.starts_with("ecdsa-sha2-nistp256 ")
        || line.starts_with("ecdsa-sha2-nistp384 ")
        || line.starts_with("ecdsa-sha2-nistp521 ")
}

fn parse_openssh_public_key(line: &str) -> Option<SshKeyAuditResult> {
    let mut parts = line.split_whitespace();
    let _prefix = parts.next()?;
    let b64 = parts.next()?;
    let blob = data_encoding::BASE64.decode(b64.as_bytes()).ok()?;
    parse_openssh_blob(&blob, false, false, "OpenSSH")
}

fn parse_openssh_blob(
    blob: &[u8],
    is_private: bool,
    is_encrypted: bool,
    format: &str,
) -> Option<SshKeyAuditResult> {
    let mut reader = WireReader::new(blob);
    let blob_algorithm = reader.read_string()?;

    let (algorithm, key_size) = match blob_algorithm.as_str() {
        OPENSSH_RSA_PREFIX => {
            let _exponent = reader.read_bytes()?;
            let modulus = reader.read_bytes()?;
            ("RSA".to_owned(), get_mpint_bit_length(modulus))
        }
        OPENSSH_DSA_PREFIX => {
            let p = reader.read_bytes()?;
            ("DSA".to_owned(), get_mpint_bit_length(p))
        }
        OPENSSH_ED25519_PREFIX => ("Ed25519".to_owned(), ED25519_KEY_SIZE),
        OPENSSH_ECDSA_P256_PREFIX => ("ECDSA".to_owned(), 256),
        OPENSSH_ECDSA_P384_PREFIX => ("ECDSA".to_owned(), 384),
        OPENSSH_ECDSA_P521_PREFIX => ("ECDSA".to_owned(), 521),
        _ => return None,
    };

    let fingerprint = compute_sha256_fingerprint(blob);
    let findings = build_findings(&algorithm, key_size, is_private, is_encrypted, false);
    let rating = determine_rating(&algorithm, key_size);

    Some(SshKeyAuditResult {
        algorithm,
        key_size,
        fingerprint,
        format: format.to_owned(),
        is_private_key: is_private,
        is_encrypted,
        rating,
        findings,
    })
}

fn parse_openssh_private_key(pem: &str) -> Option<SshKeyAuditResult> {
    let b64 = extract_pem_base64(pem, "OPENSSH PRIVATE KEY")?;
    let data = data_encoding::BASE64.decode(b64.as_bytes()).ok()?;
    if data.len() < 15 || &data[..15] != OPENSSH_MAGIC {
        return None;
    }

    let mut reader = WireReader::new(&data[15..]);
    let cipher_name = reader.read_string()?;
    let kdf_name = reader.read_string()?;
    let _kdf_options = reader.read_bytes()?;
    let num_keys = reader.read_u32()?;
    if num_keys < 1 {
        return None;
    }

    let pub_key_blob = reader.read_bytes()?;
    let is_encrypted = cipher_name != "none" || kdf_name == OPENSSH_KDF_BCRYPT;

    parse_openssh_blob(pub_key_blob, true, is_encrypted, "OpenSSH")
}

fn parse_pkcs1_rsa_private_key(pem: &str) -> Option<SshKeyAuditResult> {
    let is_encrypted = pem.contains(PROC_TYPE_ENCRYPTED);
    let is_old_format = true;

    if is_encrypted {
        return Some(SshKeyAuditResult {
            algorithm: "RSA".to_owned(),
            key_size: 0,
            fingerprint: String::new(),
            format: "PEM (PKCS#1)".to_owned(),
            is_private_key: true,
            is_encrypted: true,
            rating: SecurityRating::Acceptable,
            findings: build_findings("RSA", 0, true, true, is_old_format),
        });
    }

    let b64 = extract_pem_base64(pem, "RSA PRIVATE KEY")?;
    let der = data_encoding::BASE64.decode(b64.as_bytes()).ok()?;

    let (modulus, exponent) = extract_rsa_modulus_and_exponent_from_pkcs1(&der)?;
    let key_size = get_mpint_bit_length(modulus);
    let public_blob = build_rsa_public_key_blob(exponent, modulus);
    let fingerprint = compute_sha256_fingerprint(&public_blob);
    let rating = determine_rating("RSA", key_size);

    Some(SshKeyAuditResult {
        algorithm: "RSA".to_owned(),
        key_size,
        fingerprint,
        format: "PEM (PKCS#1)".to_owned(),
        is_private_key: true,
        is_encrypted: false,
        rating,
        findings: build_findings("RSA", key_size, true, false, is_old_format),
    })
}

fn parse_legacy_dsa_private_key(pem: &str) -> Option<SshKeyAuditResult> {
    let is_encrypted = pem.contains(PROC_TYPE_ENCRYPTED);

    if is_encrypted {
        return Some(SshKeyAuditResult {
            algorithm: "DSA".to_owned(),
            key_size: 0,
            fingerprint: String::new(),
            format: "PEM (Legacy DSA)".to_owned(),
            is_private_key: true,
            is_encrypted: true,
            rating: SecurityRating::Deprecated,
            findings: build_findings("DSA", 0, true, true, true),
        });
    }

    let b64 = extract_pem_base64(pem, "DSA PRIVATE KEY")?;
    let der = data_encoding::BASE64.decode(b64.as_bytes()).ok()?;
    let key_size = extract_dsa_key_size_from_pkcs1(&der).unwrap_or(0);
    let rating = determine_rating("DSA", key_size);

    Some(SshKeyAuditResult {
        algorithm: "DSA".to_owned(),
        key_size,
        fingerprint: String::new(),
        format: "PEM (Legacy DSA)".to_owned(),
        is_private_key: true,
        is_encrypted: false,
        rating,
        findings: build_findings("DSA", key_size, true, false, true),
    })
}

fn parse_legacy_ec_private_key(pem: &str) -> Option<SshKeyAuditResult> {
    let is_encrypted = pem.contains(PROC_TYPE_ENCRYPTED);

    if is_encrypted {
        return Some(SshKeyAuditResult {
            algorithm: "ECDSA".to_owned(),
            key_size: 0,
            fingerprint: String::new(),
            format: "PEM (Legacy EC)".to_owned(),
            is_private_key: true,
            is_encrypted: true,
            rating: SecurityRating::Acceptable,
            findings: build_findings("ECDSA", 0, true, true, true),
        });
    }

    let b64 = extract_pem_base64(pem, "EC PRIVATE KEY")?;
    let der = data_encoding::BASE64.decode(b64.as_bytes()).ok()?;
    let key_size = detect_ec_key_size_from_der(&der).unwrap_or(0);
    let rating = determine_rating("ECDSA", key_size);

    Some(SshKeyAuditResult {
        algorithm: "ECDSA".to_owned(),
        key_size,
        fingerprint: String::new(),
        format: "PEM (Legacy EC)".to_owned(),
        is_private_key: true,
        is_encrypted: false,
        rating,
        findings: build_findings("ECDSA", key_size, true, false, true),
    })
}

fn parse_pkcs8_private_key(pem: &str, is_encrypted: bool) -> Option<SshKeyAuditResult> {
    if is_encrypted {
        return Some(SshKeyAuditResult {
            algorithm: "Unknown".to_owned(),
            key_size: 0,
            fingerprint: String::new(),
            format: "PEM (PKCS#8, encrypted)".to_owned(),
            is_private_key: true,
            is_encrypted: true,
            rating: SecurityRating::Acceptable,
            findings: build_findings("Unknown", 0, true, true, false),
        });
    }

    let b64 = extract_pem_base64(pem, "PRIVATE KEY")?;
    let der = data_encoding::BASE64.decode(b64.as_bytes()).ok()?;

    if contains_sequence(&der, OID_ED25519) {
        let rating = determine_rating("Ed25519", ED25519_KEY_SIZE);
        return Some(SshKeyAuditResult {
            algorithm: "Ed25519".to_owned(),
            key_size: ED25519_KEY_SIZE,
            fingerprint: String::new(),
            format: "PEM (PKCS#8)".to_owned(),
            is_private_key: true,
            is_encrypted: false,
            rating,
            findings: build_findings("Ed25519", ED25519_KEY_SIZE, true, false, false),
        });
    }

    if contains_sequence(&der, OID_RSA_ENCRYPTION) {
        let key_size = extract_rsa_key_size_from_pkcs8(&der).unwrap_or(0);
        let rating = determine_rating("RSA", key_size);
        return Some(SshKeyAuditResult {
            algorithm: "RSA".to_owned(),
            key_size,
            fingerprint: String::new(),
            format: "PEM (PKCS#8)".to_owned(),
            is_private_key: true,
            is_encrypted: false,
            rating,
            findings: build_findings("RSA", key_size, true, false, false),
        });
    }

    if contains_sequence(&der, OID_EC_PUBLIC_KEY) {
        let key_size = detect_ec_key_size_from_der(&der).unwrap_or(0);
        let rating = determine_rating("ECDSA", key_size);
        return Some(SshKeyAuditResult {
            algorithm: "ECDSA".to_owned(),
            key_size,
            fingerprint: String::new(),
            format: "PEM (PKCS#8)".to_owned(),
            is_private_key: true,
            is_encrypted: false,
            rating,
            findings: build_findings("ECDSA", key_size, true, false, false),
        });
    }

    if contains_sequence(&der, OID_DSA) {
        let key_size = extract_dsa_key_size_from_pkcs8(&der).unwrap_or(0);
        let rating = determine_rating("DSA", key_size);
        return Some(SshKeyAuditResult {
            algorithm: "DSA".to_owned(),
            key_size,
            fingerprint: String::new(),
            format: "PEM (PKCS#8)".to_owned(),
            is_private_key: true,
            is_encrypted: false,
            rating,
            findings: build_findings("DSA", key_size, true, false, false),
        });
    }

    None
}

fn parse_spki_public_key(pem: &str) -> Option<SshKeyAuditResult> {
    let b64 = extract_pem_base64(pem, "PUBLIC KEY")?;
    let der = data_encoding::BASE64.decode(b64.as_bytes()).ok()?;

    if contains_sequence(&der, OID_ED25519) {
        let rating = determine_rating("Ed25519", ED25519_KEY_SIZE);
        return Some(SshKeyAuditResult {
            algorithm: "Ed25519".to_owned(),
            key_size: ED25519_KEY_SIZE,
            fingerprint: String::new(),
            format: "PEM (SPKI)".to_owned(),
            is_private_key: false,
            is_encrypted: false,
            rating,
            findings: build_findings("Ed25519", ED25519_KEY_SIZE, false, false, false),
        });
    }

    if contains_sequence(&der, OID_RSA_ENCRYPTION) {
        if let Some((modulus, exponent)) = extract_rsa_from_spki(&der) {
            let key_size = get_mpint_bit_length(modulus);
            let public_blob = build_rsa_public_key_blob(exponent, modulus);
            let fingerprint = compute_sha256_fingerprint(&public_blob);
            let rating = determine_rating("RSA", key_size);
            return Some(SshKeyAuditResult {
                algorithm: "RSA".to_owned(),
                key_size,
                fingerprint,
                format: "PEM (SPKI)".to_owned(),
                is_private_key: false,
                is_encrypted: false,
                rating,
                findings: build_findings("RSA", key_size, false, false, false),
            });
        }
    }

    if contains_sequence(&der, OID_EC_PUBLIC_KEY) {
        let key_size = detect_ec_key_size_from_der(&der).unwrap_or(0);
        let rating = determine_rating("ECDSA", key_size);
        return Some(SshKeyAuditResult {
            algorithm: "ECDSA".to_owned(),
            key_size,
            fingerprint: String::new(),
            format: "PEM (SPKI)".to_owned(),
            is_private_key: false,
            is_encrypted: false,
            rating,
            findings: build_findings("ECDSA", key_size, false, false, false),
        });
    }

    if contains_sequence(&der, OID_DSA) {
        let key_size = extract_dsa_key_size_from_spki(&der).unwrap_or(0);
        let rating = determine_rating("DSA", key_size);
        return Some(SshKeyAuditResult {
            algorithm: "DSA".to_owned(),
            key_size,
            fingerprint: String::new(),
            format: "PEM (SPKI)".to_owned(),
            is_private_key: false,
            is_encrypted: false,
            rating,
            findings: build_findings("DSA", key_size, false, false, false),
        });
    }

    None
}

fn parse_pkcs1_rsa_public_key(pem: &str) -> Option<SshKeyAuditResult> {
    let b64 = extract_pem_base64(pem, "RSA PUBLIC KEY")?;
    let der = data_encoding::BASE64.decode(b64.as_bytes()).ok()?;
    let (modulus, exponent) = extract_rsa_modulus_and_exponent_from_pkcs1_pub(&der)?;
    let key_size = get_mpint_bit_length(modulus);
    let public_blob = build_rsa_public_key_blob(exponent, modulus);
    let fingerprint = compute_sha256_fingerprint(&public_blob);
    let rating = determine_rating("RSA", key_size);

    Some(SshKeyAuditResult {
        algorithm: "RSA".to_owned(),
        key_size,
        fingerprint,
        format: "PEM (PKCS#1)".to_owned(),
        is_private_key: false,
        is_encrypted: false,
        rating,
        findings: build_findings("RSA", key_size, false, false, false),
    })
}

#[must_use]
pub fn determine_rating(algorithm: &str, key_size: usize) -> SecurityRating {
    match algorithm {
        "DSA" => SecurityRating::Deprecated,
        "Ed25519" => SecurityRating::Strong,
        "RSA" if key_size > 0 && key_size < MIN_RSA_KEY_SIZE => SecurityRating::Weak,
        "RSA" if key_size >= STRONG_RSA_KEY_SIZE => SecurityRating::Strong,
        "RSA" if key_size >= MIN_RSA_KEY_SIZE => SecurityRating::Acceptable,
        "ECDSA" if key_size >= STRONG_ECDSA_KEY_SIZE => SecurityRating::Strong,
        "ECDSA" if key_size > 0 => SecurityRating::Acceptable,
        _ => SecurityRating::Acceptable,
    }
}

#[must_use]
pub fn build_findings(
    algorithm: &str,
    key_size: usize,
    is_private: bool,
    is_encrypted: bool,
    is_old_format: bool,
) -> Vec<SshKeyAuditFinding> {
    let mut findings = Vec::new();

    match algorithm {
        "Ed25519" => findings.push(SshKeyAuditFinding {
            severity: FindingSeverity::Pass,
            kind: FindingKind::Ed25519Strong,
        }),
        "RSA" if key_size >= STRONG_RSA_KEY_SIZE => findings.push(SshKeyAuditFinding {
            severity: FindingSeverity::Pass,
            kind: FindingKind::RsaStrong(key_size),
        }),
        "RSA" if key_size >= MIN_RSA_KEY_SIZE => findings.push(SshKeyAuditFinding {
            severity: FindingSeverity::Warning,
            kind: FindingKind::RsaOk,
        }),
        "RSA" if key_size > 0 => findings.push(SshKeyAuditFinding {
            severity: FindingSeverity::Fail,
            kind: FindingKind::RsaWeak(key_size),
        }),
        "DSA" => findings.push(SshKeyAuditFinding {
            severity: FindingSeverity::Fail,
            kind: FindingKind::DsaDeprecated,
        }),
        "ECDSA" if key_size >= STRONG_ECDSA_KEY_SIZE => findings.push(SshKeyAuditFinding {
            severity: FindingSeverity::Pass,
            kind: FindingKind::EcdsaStrong(key_size),
        }),
        "ECDSA" if key_size > 0 => findings.push(SshKeyAuditFinding {
            severity: FindingSeverity::Warning,
            kind: FindingKind::EcdsaOk,
        }),
        _ => {}
    }

    if is_private {
        if is_encrypted {
            findings.push(SshKeyAuditFinding {
                severity: FindingSeverity::Pass,
                kind: FindingKind::EncryptedPrivateKey,
            });
        } else {
            findings.push(SshKeyAuditFinding {
                severity: FindingSeverity::Warning,
                kind: FindingKind::UnencryptedPrivateKey,
            });
        }
    }

    if is_old_format && is_private {
        findings.push(SshKeyAuditFinding {
            severity: FindingSeverity::Warning,
            kind: FindingKind::OldPemFormat,
        });
    }

    findings
}

fn compute_sha256_fingerprint(blob: &[u8]) -> String {
    let mut hasher = Hasher::new(Algorithm::Sha256);
    hasher.update(blob);
    let digest = hasher.finalize();
    let b64 = data_encoding::BASE64.encode(digest.as_bytes());
    let trimmed = b64.trim_end_matches('=');
    format!("SHA256:{trimmed}")
}

fn build_rsa_public_key_blob(exponent: &[u8], modulus: &[u8]) -> Vec<u8> {
    let mut blob = Vec::new();
    write_openssh_string(&mut blob, OPENSSH_RSA_PREFIX);
    write_openssh_mpint(&mut blob, exponent);
    write_openssh_mpint(&mut blob, modulus);
    blob
}

fn write_openssh_string(buf: &mut Vec<u8>, s: &str) {
    write_openssh_bytes(buf, s.as_bytes());
}

fn write_openssh_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    let len = bytes.len() as u32;
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(bytes);
}

fn write_openssh_mpint(buf: &mut Vec<u8>, val: &[u8]) {
    let needs_padding = !val.is_empty() && (val[0] & 0x80) != 0;
    let len = (val.len() + if needs_padding { 1 } else { 0 }) as u32;
    buf.extend_from_slice(&len.to_be_bytes());
    if needs_padding {
        buf.push(0x00);
    }
    buf.extend_from_slice(val);
}

#[must_use]
pub fn get_mpint_bit_length(mpint: &[u8]) -> usize {
    let mut offset = 0;
    while offset < mpint.len() && mpint[offset] == 0 {
        offset += 1;
    }
    if offset >= mpint.len() {
        return 0;
    }
    let byte_count = mpint.len() - offset;
    let mut top_byte = mpint[offset];
    let mut top_bits = 0;
    while top_byte > 0 {
        top_bits += 1;
        top_byte >>= 1;
    }
    ((byte_count - 1) * 8) + top_bits
}

fn extract_pem_base64(pem: &str, label: &str) -> Option<String> {
    let begin_marker = format!("-----BEGIN {label}-----");
    let end_marker = format!("-----END {label}-----");

    let begin_idx = pem.find(&begin_marker)?;
    let body_start = begin_idx + begin_marker.len();
    let end_idx = pem[body_start..].find(&end_marker)?;

    let body = &pem[body_start..body_start + end_idx];
    let mut result = String::with_capacity(body.len());
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.contains(':') {
            continue;
        }
        result.push_str(trimmed);
    }
    Some(result)
}

fn contains_sequence(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn detect_ec_key_size_from_der(der: &[u8]) -> Option<usize> {
    if contains_sequence(der, OID_SECP256R1) {
        Some(256)
    } else if contains_sequence(der, OID_SECP384R1) {
        Some(384)
    } else if contains_sequence(der, OID_SECP521R1) {
        Some(521)
    } else {
        None
    }
}

// ── Simple ASN.1 DER helpers ──────────────────────────────────────────

fn read_der_tlv<'a>(input: &'a [u8]) -> Option<(u8, &'a [u8], &'a [u8])> {
    if input.is_empty() {
        return None;
    }
    let tag = input[0];
    let mut idx = 1;
    if idx >= input.len() {
        return None;
    }
    let len_byte = input[idx];
    idx += 1;
    let length = if (len_byte & 0x80) == 0 {
        len_byte as usize
    } else {
        let num_octets = (len_byte & 0x7F) as usize;
        if num_octets == 0 || num_octets > 4 || idx + num_octets > input.len() {
            return None;
        }
        let mut l = 0usize;
        for &b in &input[idx..idx + num_octets] {
            l = (l << 8) | (b as usize);
        }
        idx += num_octets;
        l
    };
    if idx + length > input.len() {
        return None;
    }
    let value = &input[idx..idx + length];
    let rest = &input[idx + length..];
    Some((tag, value, rest))
}

fn extract_rsa_modulus_and_exponent_from_pkcs1<'a>(der: &'a [u8]) -> Option<(&'a [u8], &'a [u8])> {
    let (tag, seq_body, _) = read_der_tlv(der)?;
    if tag != 0x30 {
        return None;
    }
    // version INTEGER
    let (t_ver, _, rest1) = read_der_tlv(seq_body)?;
    if t_ver != 0x02 {
        return None;
    }
    // modulus INTEGER
    let (t_mod, modulus, rest2) = read_der_tlv(rest1)?;
    if t_mod != 0x02 {
        return None;
    }
    // publicExponent INTEGER
    let (t_exp, exponent, _) = read_der_tlv(rest2)?;
    if t_exp != 0x02 {
        return None;
    }
    Some((modulus, exponent))
}

fn extract_rsa_modulus_and_exponent_from_pkcs1_pub<'a>(
    der: &'a [u8],
) -> Option<(&'a [u8], &'a [u8])> {
    let (tag, seq_body, _) = read_der_tlv(der)?;
    if tag != 0x30 {
        return None;
    }
    // modulus INTEGER
    let (t_mod, modulus, rest1) = read_der_tlv(seq_body)?;
    if t_mod != 0x02 {
        return None;
    }
    // publicExponent INTEGER
    let (t_exp, exponent, _) = read_der_tlv(rest1)?;
    if t_exp != 0x02 {
        return None;
    }
    Some((modulus, exponent))
}

fn extract_rsa_key_size_from_pkcs8(der: &[u8]) -> Option<usize> {
    // Look for embedded PKCS#1 structure in OCTET STRING
    let mut remaining = der;
    while let Some((tag, val, rest)) = read_der_tlv(remaining) {
        if tag == 0x04 {
            // OCTET STRING: might contain PKCS#1
            if let Some((modulus, _)) = extract_rsa_modulus_and_exponent_from_pkcs1(val) {
                return Some(get_mpint_bit_length(modulus));
            }
        }
        remaining = rest;
    }
    None
}

fn extract_dsa_key_size_from_pkcs1(der: &[u8]) -> Option<usize> {
    let (tag, seq_body, _) = read_der_tlv(der)?;
    if tag != 0x30 {
        return None;
    }
    // version INTEGER
    let (_, _, rest1) = read_der_tlv(seq_body)?;
    // p INTEGER
    let (t_p, p, _) = read_der_tlv(rest1)?;
    if t_p == 0x02 {
        Some(get_mpint_bit_length(p))
    } else {
        None
    }
}

fn extract_dsa_key_size_from_pkcs8(der: &[u8]) -> Option<usize> {
    // In DSA PKCS#8, AlgorithmIdentifier parameters holds SEQUENCE { p, q, g }
    let mut remaining = der;
    while let Some((tag, val, rest)) = read_der_tlv(remaining) {
        if tag == 0x30 && val.len() >= 4 {
            if let Some((_, p, _)) = read_der_tlv(val) {
                let bits = get_mpint_bit_length(p);
                if bits >= 512 {
                    return Some(bits);
                }
            }
        }
        remaining = rest;
    }
    None
}

fn extract_rsa_from_spki<'a>(der: &'a [u8]) -> Option<(&'a [u8], &'a [u8])> {
    // SubjectPublicKeyInfo: SEQUENCE { algorithm AlgorithmIdentifier, subjectPublicKey BIT STRING }
    let (tag, seq_body, _) = read_der_tlv(der)?;
    if tag != 0x30 {
        return None;
    }
    let (_, _, rest1) = read_der_tlv(seq_body)?; // AlgorithmIdentifier
    let (t_bits, bit_string, _) = read_der_tlv(rest1)?; // BIT STRING
    if t_bits != 0x03 || bit_string.is_empty() {
        return None;
    }
    // skip unused bits count (first byte of BIT STRING)
    let rsa_pub = &bit_string[1..];
    extract_rsa_modulus_and_exponent_from_pkcs1_pub(rsa_pub)
}

fn extract_dsa_key_size_from_spki(der: &[u8]) -> Option<usize> {
    extract_dsa_key_size_from_pkcs8(der)
}

// ── WireReader for OpenSSH binary format ──────────────────────────────

struct WireReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> WireReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn read_u32(&mut self) -> Option<u32> {
        if self.offset + 4 > self.data.len() {
            return None;
        }
        let bytes: [u8; 4] = self.data[self.offset..self.offset + 4].try_into().ok()?;
        self.offset += 4;
        Some(u32::from_be_bytes(bytes))
    }

    fn read_bytes(&mut self) -> Option<&'a [u8]> {
        let len = self.read_u32()? as usize;
        if self.offset + len > self.data.len() {
            return None;
        }
        let slice = &self.data[self.offset..self.offset + len];
        self.offset += len;
        Some(slice)
    }

    fn read_string(&mut self) -> Option<String> {
        let bytes = self.read_bytes()?;
        String::from_utf8(bytes.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ed25519_public_key_audited_strong() {
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIG5k0o7U9Z3g4V6pW7yF1g2H3j4K5l6M7n8O9p0Q1r2S test@host";
        let result = audit(key).expect("audit succeeds");
        assert_eq!(result.algorithm, "Ed25519");
        assert_eq!(result.key_size, 256);
        assert_eq!(result.rating, SecurityRating::Strong);
        assert!(!result.is_private_key);
        assert!(!result.is_encrypted);
        assert!(result.fingerprint.starts_with("SHA256:"));
        assert_eq!(result.findings[0].severity, FindingSeverity::Pass);
        assert_eq!(result.findings[0].kind, FindingKind::Ed25519Strong);
    }

    #[test]
    fn rsa_weak_key_size_fails() {
        let findings = build_findings("RSA", 1024, false, false, false);
        assert_eq!(findings[0].severity, FindingSeverity::Fail);
        assert_eq!(findings[0].kind, FindingKind::RsaWeak(1024));
        assert_eq!(determine_rating("RSA", 1024), SecurityRating::Weak);
    }

    #[test]
    fn rsa_acceptable_key_size_warns() {
        let findings = build_findings("RSA", 2048, false, false, false);
        assert_eq!(findings[0].severity, FindingSeverity::Warning);
        assert_eq!(findings[0].kind, FindingKind::RsaOk);
        assert_eq!(determine_rating("RSA", 2048), SecurityRating::Acceptable);
    }

    #[test]
    fn rsa_strong_key_size_passes() {
        let findings = build_findings("RSA", 4096, false, false, false);
        assert_eq!(findings[0].severity, FindingSeverity::Pass);
        assert_eq!(findings[0].kind, FindingKind::RsaStrong(4096));
        assert_eq!(determine_rating("RSA", 4096), SecurityRating::Strong);
    }

    #[test]
    fn dsa_key_deprecated() {
        let findings = build_findings("DSA", 1024, false, false, false);
        assert_eq!(findings[0].severity, FindingSeverity::Fail);
        assert_eq!(findings[0].kind, FindingKind::DsaDeprecated);
        assert_eq!(determine_rating("DSA", 1024), SecurityRating::Deprecated);
    }

    #[test]
    fn unencrypted_private_key_warned() {
        let findings = build_findings("Ed25519", 256, true, false, false);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[1].severity, FindingSeverity::Warning);
        assert_eq!(findings[1].kind, FindingKind::UnencryptedPrivateKey);
    }

    #[test]
    fn encrypted_private_key_passed() {
        let findings = build_findings("Ed25519", 256, true, true, false);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[1].severity, FindingSeverity::Pass);
        assert_eq!(findings[1].kind, FindingKind::EncryptedPrivateKey);
    }

    #[test]
    fn empty_or_oversized_rejected() {
        assert!(audit("").is_none());
        assert!(audit("   \n\t  ").is_none());
        let huge = "a".repeat(MAX_KEY_FILE_SIZE + 1);
        assert!(audit(&huge).is_none());
    }
}
