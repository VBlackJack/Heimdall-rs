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

//! Keys and certificates written in PEM as .NET writes them: `ExportPkcs8PrivateKeyPem`,
//! `ExportEncryptedPkcs8PrivateKeyPem` with PBES2 (PBKDF2-HMAC-SHA256 and AES-256-CBC), and
//! `ExportCertificatePem`, lines of 64 characters ended by a line feed.

use pkcs8::der::pem::{self, LineEnding};
use pkcs8::der::{Decode as _, EncodePem as _};
use pkcs8::pkcs5::pbes2;
use pkcs8::{EncryptedPrivateKeyInfoRef, PrivateKeyInfoRef};
use zeroize::Zeroizing;

/// The label of a private key in PKCS#8.
const PRIVATE_KEY_LABEL: &str = "PRIVATE KEY";

/// The label of a certificate.
const CERTIFICATE_LABEL: &str = "CERTIFICATE";

/// Bytes of salt for PBKDF2, as .NET draws them.
const SALT_BYTES: usize = 16;

/// Bytes of an AES-CBC initialisation vector.
const IV_BYTES: usize = 16;

/// Why a key could not be written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PemError {
    /// The system's generator could not be read for the salt or the vector.
    #[error("the system's random generator could not be read")]
    Randomness,
    /// The encoding failed.
    #[error("{0}")]
    Encoding(String),
}

/// The PKCS#8 `der` of a private key in PEM, as .NET's `ExportPkcs8PrivateKeyPem`.
///
/// # Errors
///
/// [`PemError::Encoding`] when it cannot be written.
pub fn private_key_pem(der: &[u8]) -> Result<Zeroizing<String>, PemError> {
    pem::encode_string(PRIVATE_KEY_LABEL, LineEnding::LF, der)
        .map(Zeroizing::new)
        .map_err(|error| PemError::Encoding(error.to_string()))
}

/// The PKCS#8 `der` of a private key encrypted with `passphrase`, in PEM, as .NET's
/// `ExportEncryptedPkcs8PrivateKeyPem` with `PbeParameters(Aes256Cbc, SHA256, iterations)`:
/// PBES2, PBKDF2-HMAC-SHA256 over `iterations` rounds and a random salt, AES-256-CBC.
///
/// # Errors
///
/// [`PemError::Randomness`] without a salt; [`PemError::Encoding`] when it cannot be
/// written.
pub fn encrypted_private_key_pem(
    der: &[u8],
    passphrase: &str,
    iterations: u32,
) -> Result<Zeroizing<String>, PemError> {
    // The key is checked to be PKCS#8 before it is encrypted.
    PrivateKeyInfoRef::from_der(der).map_err(|error| PemError::Encoding(error.to_string()))?;
    let mut salt = Zeroizing::new([0_u8; SALT_BYTES]);
    let mut iv = [0_u8; IV_BYTES];
    getrandom::fill(salt.as_mut()).map_err(|_| PemError::Randomness)?;
    getrandom::fill(&mut iv).map_err(|_| PemError::Randomness)?;
    let parameters = pbes2::Parameters::generate_pbkdf2_sha256_aes256cbc(iterations, &*salt, iv)
        .map_err(|error| PemError::Encoding(error.to_string()))?;
    let encrypted = parameters
        .encrypt(passphrase.as_bytes(), der)
        .map_err(|error| PemError::Encoding(error.to_string()))?;
    let data = pkcs8::der::asn1::OctetStringRef::new(&encrypted)
        .map_err(|error| PemError::Encoding(error.to_string()))?;
    EncryptedPrivateKeyInfoRef {
        encryption_algorithm: parameters.into(),
        encrypted_data: data,
    }
    .to_pem(LineEnding::LF)
    .map(Zeroizing::new)
    .map_err(|error| PemError::Encoding(error.to_string()))
}

/// The DER of a certificate in PEM, as .NET's `ExportCertificatePem`.
///
/// # Errors
///
/// [`PemError::Encoding`] when it cannot be written.
pub fn certificate_pem(der: &[u8]) -> Result<String, PemError> {
    pem::encode_string(CERTIFICATE_LABEL, LineEnding::LF, der)
        .map_err(|error| PemError::Encoding(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An Ed25519 key of RFC 8410, section 10.3.
    const ED25519_PKCS8: [u8; 48] = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20, 0xd4, 0xee, 0x72, 0xdb, 0xf9, 0x13, 0x58, 0x4a, 0xd5, 0xb6, 0xd8, 0xf1, 0xf7, 0x69,
        0xf8, 0xad, 0x3a, 0xfe, 0x7c, 0x28, 0xcb, 0xf1, 0xd4, 0xfb, 0xe0, 0x97, 0xa8, 0x8f, 0x44,
        0x75, 0x58, 0x42,
    ];

    #[test]
    fn a_key_in_pem_is_the_rfc_7468_text_of_its_der() {
        let pem = private_key_pem(&ED25519_PKCS8).expect("written");
        assert_eq!(
            pem.as_str(),
            "-----BEGIN PRIVATE KEY-----\n\
             MC4CAQAwBQYDK2VwBCIEINTuctv5E1hK1bbY8fdp+K06/nwoy/HU++CXqI9EdVhC\n\
             -----END PRIVATE KEY-----\n"
        );
    }

    #[test]
    fn an_encrypted_key_reads_back_with_its_passphrase_only() {
        let pem = encrypted_private_key_pem(&ED25519_PKCS8, "s3cret", 1000).expect("written");
        assert!(pem.starts_with("-----BEGIN ENCRYPTED PRIVATE KEY-----\n"));
        let (label, der) = pem::decode_vec(pem.as_bytes()).expect("pem");
        assert_eq!(label, "ENCRYPTED PRIVATE KEY");
        let info = EncryptedPrivateKeyInfoRef::from_der(&der).expect("der");
        let scheme = info.encryption_algorithm.pbes2().expect("PBES2");
        let kdf = scheme.kdf.pbkdf2().expect("PBKDF2");
        assert_eq!(kdf.iteration_count, 1000);
        assert!(matches!(
            scheme.encryption,
            pbes2::EncryptionScheme::Aes256Cbc { .. }
        ));
        assert_eq!(
            info.decrypt("s3cret").expect("decrypted").as_bytes(),
            ED25519_PKCS8
        );
        assert!(info.decrypt("wrong").is_err());
    }
}
