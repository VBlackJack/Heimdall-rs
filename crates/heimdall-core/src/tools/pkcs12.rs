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

//! A PFX (PKCS #12, RFC 7292) holding one certificate and its private key, as the C#
//! `CertificateGenerator.BuildPfx` exports one (`CertificateGenerator.cs:124-144`) through
//! .NET's `X509Certificate2.Export(Pfx, password)`.
//!
//! The file is written as OpenSSL 3 writes one by default: the key in a shrouded key bag,
//! encrypted with PBES2 (PBKDF2-HMAC-SHA256 and AES-256-CBC), the certificate in a bag of
//! its own, both tagged with the same local key identifier so the key is found with its
//! certificate, and the whole sealed by an HMAC-SHA256 whose key RFC 7292's own derivation
//! makes from the password. An empty password is a password like any other, as the C#
//! allows it.

use hmac::{Hmac, KeyInit as _, Mac as _};
use pkcs8::der::Encode as _;
use pkcs8::der::asn1::OctetStringRef;
use pkcs8::pkcs5::pbes2;
use sha1::Sha1;
use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

/// Rounds of PBKDF2 for the key and of the MAC derivation, OpenSSL's `PKCS12_DEFAULT_ITER`.
pub const PFX_ITERATIONS: u32 = 2048;

/// Bytes of each salt and of the AES vector.
const SALT_BYTES: usize = 16;

/// The PFX version, 3.
const PFX_VERSION: u32 = 3;

/// ASN.1 tags written here.
const TAG_INTEGER: u8 = 0x02;
const TAG_OCTET_STRING: u8 = 0x04;
const TAG_NULL: u8 = 0x05;
const TAG_OID: u8 = 0x06;
const TAG_SEQUENCE: u8 = 0x30;
const TAG_SET: u8 = 0x31;
const TAG_EXPLICIT_0: u8 = 0xa0;

/// `id-data`, 1.2.840.113549.1.7.1.
const OID_DATA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x01];
/// `pkcs8ShroudedKeyBag`, 1.2.840.113549.1.12.10.1.2.
const OID_SHROUDED_KEY_BAG: &[u8] = &[
    0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x0c, 0x0a, 0x01, 0x02,
];
/// `certBag`, 1.2.840.113549.1.12.10.1.3.
const OID_CERT_BAG: &[u8] = &[
    0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x0c, 0x0a, 0x01, 0x03,
];
/// `x509Certificate`, 1.2.840.113549.1.9.22.1.
const OID_X509_CERTIFICATE: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x09, 0x16, 0x01];
/// `localKeyId`, 1.2.840.113549.1.9.21.
const OID_LOCAL_KEY_ID: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x09, 0x15];
/// `id-sha256`, 2.16.840.1.101.3.4.2.1.
const OID_SHA256: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];

/// The purpose byte of RFC 7292's derivation that makes a MAC key.
const MAC_KEY_ID: u8 = 3;

/// SHA-256's output, the `u` of RFC 7292 appendix B.
const HASH_BYTES: usize = 32;

/// SHA-256's block, the `v` of RFC 7292 appendix B.
const BLOCK_BYTES: usize = 64;

/// Why no PFX was written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PfxError {
    /// The system's generator could not be read.
    #[error("the system's random generator could not be read")]
    Randomness,
    /// The key could not be encrypted or encoded.
    #[error("{0}")]
    Encoding(String),
}

/// A PFX holding `certificate_der` and `private_key_pkcs8`, sealed with `password`.
///
/// # Errors
///
/// [`PfxError`] when the system's generator cannot be read or the key not encrypted.
pub fn build(
    certificate_der: &[u8],
    private_key_pkcs8: &[u8],
    password: &str,
) -> Result<Vec<u8>, PfxError> {
    let key_id = Sha1::digest(certificate_der);
    let key_bag = shrouded_key_bag(private_key_pkcs8, password, &key_id)?;
    let cert_bag = certificate_bag(certificate_der, &key_id);
    let auth_safe = sequence(&[
        data_content_info(&sequence(&[cert_bag])),
        data_content_info(&sequence(&[key_bag])),
    ]);
    let mut mac_salt = [0_u8; SALT_BYTES];
    sealvault::random::fill(&mut mac_salt).map_err(|_| PfxError::Randomness)?;
    let mac = auth_safe_mac(&auth_safe, password, &mac_salt, PFX_ITERATIONS);
    let mac_data = sequence(&[
        sequence(&[
            sequence(&[tlv(TAG_OID, OID_SHA256), tlv(TAG_NULL, &[])]),
            tlv(TAG_OCTET_STRING, &mac),
        ]),
        tlv(TAG_OCTET_STRING, &mac_salt),
        integer(PFX_ITERATIONS),
    ]);
    Ok(sequence(&[
        integer(PFX_VERSION),
        data_content_info(&auth_safe),
        mac_data,
    ]))
}

/// The key bag: the key encrypted with PBES2, tagged with `key_id`.
fn shrouded_key_bag(
    private_key_pkcs8: &[u8],
    password: &str,
    key_id: &[u8],
) -> Result<Vec<u8>, PfxError> {
    let mut salt = Zeroizing::new([0_u8; SALT_BYTES]);
    let mut iv = [0_u8; SALT_BYTES];
    sealvault::random::fill(salt.as_mut()).map_err(|_| PfxError::Randomness)?;
    sealvault::random::fill(&mut iv).map_err(|_| PfxError::Randomness)?;
    let encoding = |error: &dyn std::fmt::Display| PfxError::Encoding(error.to_string());
    let parameters =
        pbes2::Parameters::generate_pbkdf2_sha256_aes256cbc(PFX_ITERATIONS, &*salt, iv)
            .map_err(|error| encoding(&error))?;
    let encrypted = parameters
        .encrypt(password.as_bytes(), private_key_pkcs8)
        .map_err(|error| encoding(&error))?;
    let info = pkcs8::EncryptedPrivateKeyInfoRef {
        encryption_algorithm: parameters.into(),
        encrypted_data: OctetStringRef::new(&encrypted).map_err(|error| encoding(&error))?,
    }
    .to_der()
    .map_err(|error| encoding(&error))?;
    Ok(safe_bag(OID_SHROUDED_KEY_BAG, &info, key_id))
}

/// The certificate bag of `certificate_der`, tagged with `key_id`.
fn certificate_bag(certificate_der: &[u8], key_id: &[u8]) -> Vec<u8> {
    let cert_bag = sequence(&[
        tlv(TAG_OID, OID_X509_CERTIFICATE),
        tlv(TAG_EXPLICIT_0, &tlv(TAG_OCTET_STRING, certificate_der)),
    ]);
    safe_bag(OID_CERT_BAG, &cert_bag, key_id)
}

/// A `SafeBag` of type `bag_id` holding `value`, its local key identifier `key_id`.
fn safe_bag(bag_id: &[u8], value: &[u8], key_id: &[u8]) -> Vec<u8> {
    let attribute = sequence(&[
        tlv(TAG_OID, OID_LOCAL_KEY_ID),
        tlv(TAG_SET, &tlv(TAG_OCTET_STRING, key_id)),
    ]);
    sequence(&[
        tlv(TAG_OID, bag_id),
        tlv(TAG_EXPLICIT_0, value),
        tlv(TAG_SET, &attribute),
    ])
}

/// A `ContentInfo` of type `id-data` holding `content` in an octet string.
fn data_content_info(content: &[u8]) -> Vec<u8> {
    sequence(&[
        tlv(TAG_OID, OID_DATA),
        tlv(TAG_EXPLICIT_0, &tlv(TAG_OCTET_STRING, content)),
    ])
}

/// The HMAC-SHA256 of `auth_safe` under the key RFC 7292 derives from `password`.
fn auth_safe_mac(auth_safe: &[u8], password: &str, salt: &[u8], iterations: u32) -> Vec<u8> {
    let key = derive_key(password, salt, iterations, MAC_KEY_ID, HASH_BYTES);
    let mut mac = <Hmac<Sha256>>::new_from_slice(&key).expect("HMAC takes a key of any size");
    mac.update(auth_safe);
    mac.finalize().into_bytes().to_vec()
}

/// `password` as RFC 7292 appendix B.1 hands it to the derivation: big-endian UTF-16,
/// ended by two zero bytes.
fn bmp_password(password: &str) -> Zeroizing<Vec<u8>> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(password.len() * 2 + 2));
    for unit in password.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

/// `source` repeated to the next whole number of blocks, as RFC 7292 appendix B.2 fills
/// `S` and `P`; nothing when `source` is empty.
fn fill_blocks(source: &[u8]) -> Zeroizing<Vec<u8>> {
    if source.is_empty() {
        return Zeroizing::new(Vec::new());
    }
    let length = source.len().div_ceil(BLOCK_BYTES) * BLOCK_BYTES;
    Zeroizing::new(source.iter().copied().cycle().take(length).collect())
}

/// RFC 7292 appendix B.2's derivation over SHA-256: `length` bytes for purpose `id` from
/// `password` and `salt` over `iterations` rounds.
fn derive_key(
    password: &str,
    salt: &[u8],
    iterations: u32,
    id: u8,
    length: usize,
) -> Zeroizing<Vec<u8>> {
    let diversifier = [id; BLOCK_BYTES];
    let salt_blocks = fill_blocks(salt);
    let password_blocks = fill_blocks(&bmp_password(password));
    // Made at its size at once: a vector that grows leaves its old buffer unwiped.
    let mut input = Zeroizing::new(Vec::with_capacity(
        salt_blocks.len() + password_blocks.len(),
    ));
    input.extend_from_slice(&salt_blocks);
    input.extend_from_slice(&password_blocks);
    let mut output = Zeroizing::new(Vec::with_capacity(length.div_ceil(HASH_BYTES) * HASH_BYTES));
    while output.len() < length {
        let mut block: Zeroizing<[u8; HASH_BYTES]> = Zeroizing::new(
            Sha256::new()
                .chain_update(diversifier)
                .chain_update(&*input)
                .finalize()
                .into(),
        );
        for _ in 1..iterations {
            *block = Sha256::digest(*block).into();
        }
        output.extend_from_slice(&*block);
        if output.len() >= length {
            break;
        }
        // B, the block repeated to v bytes, plus one, added to each v-byte block of I.
        let repeated: Zeroizing<Vec<u8>> =
            Zeroizing::new(block.iter().copied().cycle().take(BLOCK_BYTES).collect());
        for chunk in input.chunks_mut(BLOCK_BYTES) {
            let mut carry = 1_u16;
            for (byte, add) in chunk.iter_mut().zip(repeated.iter()).rev() {
                let sum = u16::from(*byte) + u16::from(*add) + carry;
                *byte = sum.to_le_bytes()[0];
                carry = sum >> 8;
            }
        }
    }
    output.truncate(length);
    output
}

/// A DER element of `tag` holding `content`.
fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut element = vec![tag];
    let length = content.len();
    if length < 0x80 {
        element.push(length.to_be_bytes()[std::mem::size_of::<usize>() - 1]);
    } else {
        let bytes = length.to_be_bytes();
        let start = bytes
            .iter()
            .position(|byte| *byte != 0)
            .unwrap_or(bytes.len() - 1);
        let count = bytes.len() - start;
        element.push(0x80 | count.to_be_bytes()[std::mem::size_of::<usize>() - 1]);
        element.extend_from_slice(&bytes[start..]);
    }
    element.extend_from_slice(content);
    element
}

/// A DER `SEQUENCE` of `elements`, each already encoded.
fn sequence(elements: &[Vec<u8>]) -> Vec<u8> {
    tlv(TAG_SEQUENCE, &elements.concat())
}

/// A DER `INTEGER` of `value`.
fn integer(value: u32) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let start = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len() - 1);
    let mut content = Vec::with_capacity(bytes.len() + 1);
    if bytes[start] & 0x80 != 0 {
        content.push(0);
    }
    content.extend_from_slice(&bytes[start..]);
    tlv(TAG_INTEGER, &content)
}

#[cfg(test)]
pub(crate) mod tests {
    use pkcs8::der::Decode as _;

    use super::*;

    /// One DER element read: its tag, its content, what follows it.
    pub(crate) fn read(bytes: &[u8]) -> (u8, &[u8], &[u8]) {
        let tag = bytes[0];
        let (length, header) = if bytes[1] < 0x80 {
            (usize::from(bytes[1]), 2)
        } else {
            let count = usize::from(bytes[1] & 0x7f);
            let length = bytes[2..2 + count]
                .iter()
                .fold(0_usize, |length, byte| length << 8 | usize::from(*byte));
            (length, 2 + count)
        };
        (
            tag,
            &bytes[header..header + length],
            &bytes[header + length..],
        )
    }

    /// The elements of a constructed content, in order.
    pub(crate) fn children(mut content: &[u8]) -> Vec<(u8, &[u8])> {
        let mut found = Vec::new();
        while !content.is_empty() {
            let (tag, inner, rest) = read(content);
            found.push((tag, inner));
            content = rest;
        }
        found
    }

    /// The certificate and the key of `pfx` opened with `password`, its MAC checked first;
    /// `None` when the MAC does not match, as a reader refuses a wrong password.
    pub(crate) fn open(pfx: &[u8], password: &str) -> Option<(Vec<u8>, Vec<u8>)> {
        let (_, pfx, _) = read(pfx);
        let parts = children(pfx);
        assert_eq!(parts[0], (TAG_INTEGER, &[3_u8][..]));
        let info = children(parts[1].1);
        assert_eq!(info[0].1, OID_DATA);
        let (_, auth_safe, _) = read(info[1].1);
        let auth_safe = read(auth_safe).1;
        let mac_data = children(parts[2].1);
        let digest = children(mac_data[0].1);
        let salt = mac_data[1].1;
        assert_eq!(mac_data[2].1, [0x08, 0x00], "2048 rounds");
        // The MAC is computed over the AuthenticatedSafe's encoding, its header included.
        let (_, wrapped, _) = read(info[1].1);
        let expected = auth_safe_mac(wrapped, password, salt, PFX_ITERATIONS);
        if digest[1].1 != expected.as_slice() {
            return None;
        }
        let safes = children(auth_safe);
        let cert_bag = open_bag(safes[0].1);
        assert_eq!(cert_bag[0].1, OID_CERT_BAG);
        let cert = children(read(cert_bag[1].1).1);
        let certificate = read(cert[1].1).1.to_vec();
        let key_bag = open_bag(safes[1].1);
        assert_eq!(key_bag[0].1, OID_SHROUDED_KEY_BAG);
        let encrypted = pkcs8::EncryptedPrivateKeyInfoRef::from_der(cert_bag_value(&key_bag))
            .expect("EncryptedPrivateKeyInfo");
        let key = encrypted.decrypt(password).ok()?.as_bytes().to_vec();
        Some((certificate, key))
    }

    /// The elements of the one bag in a `ContentInfo` of data.
    fn open_bag(content_info: &[u8]) -> Vec<(u8, &[u8])> {
        let parts = children(content_info);
        let (_, octets, _) = read(parts[1].1);
        let contents = read(octets).1;
        let (_, bag, _) = read(contents);
        children(bag)
    }

    /// The DER of the value inside a bag's `[0]`.
    fn cert_bag_value<'a>(bag: &[(u8, &'a [u8])]) -> &'a [u8] {
        bag[1].1
    }

    #[test]
    fn der_lengths_take_the_short_and_the_long_forms() {
        assert_eq!(tlv(TAG_OCTET_STRING, &[1, 2]), [0x04, 0x02, 1, 2]);
        let long = tlv(TAG_OCTET_STRING, &[0; 200]);
        assert_eq!(long[..3], [0x04, 0x81, 200]);
        let longer = tlv(TAG_OCTET_STRING, &[0; 300]);
        assert_eq!(longer[..4], [0x04, 0x82, 0x01, 0x2c]);
        assert_eq!(integer(3), [0x02, 0x01, 0x03]);
        assert_eq!(integer(2048), [0x02, 0x02, 0x08, 0x00]);
        assert_eq!(integer(128), [0x02, 0x02, 0x00, 0x80]);
    }

    #[test]
    fn the_rfc_7292_derivation_hashes_its_filled_blocks() {
        // RFC 7292 B.2 with SHA-256, one round, 32 bytes: SHA-256 of D, S and P, each filled
        // to a 64-byte block, P being the password in BMP with its two zero bytes.
        let salt = [0_u8, 1, 2, 3, 4, 5, 6, 7];
        let key = derive_key("secret", &salt, 1, MAC_KEY_ID, 32);
        let mut input = vec![MAC_KEY_ID; 64];
        input.extend(salt.iter().copied().cycle().take(64));
        let bmp = bmp_password("secret");
        input.extend(bmp.iter().copied().cycle().take(64));
        assert_eq!(key.as_slice(), Sha256::digest(&input).as_slice());
        assert_eq!(
            bmp.as_slice(),
            [0, b's', 0, b'e', 0, b'c', 0, b'r', 0, b'e', 0, b't', 0, 0]
        );
    }

    #[test]
    fn a_pfx_opens_with_its_password_only_and_holds_its_certificate_and_key() {
        let certificate = b"certificate DER".to_vec();
        // The RFC 8410 Ed25519 key.
        let key = [
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20, 0xd4, 0xee, 0x72, 0xdb, 0xf9, 0x13, 0x58, 0x4a, 0xd5, 0xb6, 0xd8, 0xf1,
            0xf7, 0x69, 0xf8, 0xad, 0x3a, 0xfe, 0x7c, 0x28, 0xcb, 0xf1, 0xd4, 0xfb, 0xe0, 0x97,
            0xa8, 0x8f, 0x44, 0x75, 0x58, 0x42,
        ];
        let pfx = build(&certificate, &key, "secret").expect("built");
        let (opened_certificate, opened_key) = open(&pfx, "secret").expect("opened");
        assert_eq!(opened_certificate, certificate);
        assert_eq!(opened_key, key);
        assert!(open(&pfx, "").is_none(), "a wrong password is refused");
        let empty = build(&certificate, &key, "").expect("built");
        assert!(open(&empty, "").is_some(), "an empty password is one");
        assert!(open(&empty, "secret").is_none());
    }
}
