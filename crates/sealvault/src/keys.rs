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

//! Key pairs: RSA (public exponent 65537, primes from the operating system's generator) and
//! Ed25519 (RFC 8032), held as PKCS#8 (RFC 5958; RFC 8410 for Ed25519), with their OpenSSH
//! public line and SHA-256 fingerprint, and their PKCS#8 written in PEM, plain or encrypted
//! with PBES2 (RFC 8018: PBKDF2-HMAC-SHA256, AES-256-CBC).
//!
//! The PKCS#8 of a pair is a secret: held in a [`SecretBytes`], never shown by `Debug`. The
//! RSA and Ed25519 key structures the libraries build while making one are their own, and
//! wiped by them only as far as they wipe themselves.

use std::convert::Infallible;
use std::fmt;

use getrandom::rand_core::{TryCryptoRng, TryRng};
use pkcs8::der::Decode as _;
use pkcs8::der::EncodePem as _;
use pkcs8::der::pem::LineEnding;
use pkcs8::pkcs5::pbes2;
use pkcs8::{EncryptedPrivateKeyInfoRef, PrivateKeyInfoRef};
use rsa::pkcs8::EncodePrivateKey as _;
use ssh_key::public::{Ed25519PublicKey, KeyData, RsaPublicKey};
use ssh_key::{HashAlg, PublicKey};
use zeroize::Zeroizing;

use crate::Error;
use crate::hash::SHA256_LEN;
use crate::random;
use crate::secret::SecretBytes;

/// Bytes of an Ed25519 seed.
const ED25519_SEED_LEN: usize = 32;

/// The PKCS#8 encoding of an Ed25519 key before its 32-byte seed, as RFC 8410 writes it:
/// version 0, the algorithm 1.3.101.112, the seed in an octet string inside the octet string.
const ED25519_PKCS8_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

/// Bytes of PBKDF2 salt, as .NET draws them.
const PBES2_SALT_LEN: usize = 16;

/// Bytes of an AES-CBC initialisation vector.
const PBES2_IV_LEN: usize = 16;

/// What an RSA generation says when the system's generator failed under it.
const GENERATOR_FAILED: &str = "the system's random generator could not be read";

/// A key pair's algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAlgorithm {
    /// RSA of this many bits.
    Rsa(usize),
    /// Ed25519.
    Ed25519,
}

/// A key pair: its PKCS#8, wiped on drop, and its public half.
pub struct KeyPair {
    algorithm: KeyAlgorithm,
    pkcs8: SecretBytes,
    public: KeyData,
}

impl fmt::Debug for KeyPair {
    /// The private key is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyPair")
            .field("algorithm", &self.algorithm)
            .finish_non_exhaustive()
    }
}

impl KeyPair {
    /// A new RSA key of `bits`, its primes drawn from the operating system's generator,
    /// public exponent 65537.
    ///
    /// # Errors
    ///
    /// [`Error::Randomness`] when the generator could not be read; [`Error::Encoding`]
    /// when the size is refused or the key cannot be encoded.
    pub fn rsa(bits: usize) -> Result<Self, Error> {
        Self::rsa_with(bits, Watched::default())
    }

    fn rsa_with(bits: usize, mut random: Watched) -> Result<Self, Error> {
        let key = rsa::RsaPrivateKey::new(&mut random, bits).map_err(encoding)?;
        if random.failed {
            return Err(Error::Randomness(GENERATOR_FAILED.to_owned()));
        }
        let public = RsaPublicKey::try_from(&key.to_public_key()).map_err(encoding)?;
        let der = key.to_pkcs8_der().map_err(encoding)?;
        Ok(Self {
            algorithm: KeyAlgorithm::Rsa(bits),
            pkcs8: SecretBytes::from_slice(der.as_bytes()),
            public: KeyData::Rsa(public),
        })
    }

    /// A new Ed25519 key, its seed drawn from the operating system's generator.
    ///
    /// # Errors
    ///
    /// [`Error::Randomness`].
    pub fn ed25519() -> Result<Self, Error> {
        let seed: Zeroizing<[u8; ED25519_SEED_LEN]> = Zeroizing::new(random::array()?);
        let pair = ssh_key::private::Ed25519Keypair::from_seed(&seed);
        let public: Ed25519PublicKey = pair.public;
        let mut der = Vec::with_capacity(ED25519_PKCS8_PREFIX.len() + ED25519_SEED_LEN);
        der.extend_from_slice(&ED25519_PKCS8_PREFIX);
        der.extend_from_slice(seed.as_ref());
        Ok(Self {
            algorithm: KeyAlgorithm::Ed25519,
            pkcs8: SecretBytes::new(der),
            public: KeyData::Ed25519(public),
        })
    }

    /// The algorithm.
    #[must_use]
    pub fn algorithm(&self) -> KeyAlgorithm {
        self.algorithm
    }

    /// The private key's PKCS#8 DER.
    #[must_use]
    pub fn pkcs8_der(&self) -> &[u8] {
        self.pkcs8.as_bytes()
    }

    /// The public key as an OpenSSH line, `type base64 comment`.
    ///
    /// # Errors
    ///
    /// [`Error::Encoding`] when it cannot be written.
    pub fn openssh_public(&self, comment: &str) -> Result<String, Error> {
        PublicKey::new(self.public.clone(), comment.to_owned())
            .to_openssh()
            .map_err(encoding)
    }

    /// The public key's SHA-256 fingerprint as OpenSSH prints it: `SHA256:` then Base64
    /// without padding.
    #[must_use]
    pub fn openssh_fingerprint(&self) -> String {
        PublicKey::new(self.public.clone(), String::new())
            .fingerprint(HashAlg::Sha256)
            .to_string()
    }
}

/// `der` in PEM under `label`, lines of 64 characters ended by a line feed (RFC 7468).
///
/// # Errors
///
/// [`Error::Encoding`] when it cannot be written.
pub fn pem(label: &str, der: &[u8]) -> Result<String, Error> {
    pkcs8::der::pem::encode_string(label, LineEnding::LF, der).map_err(encoding)
}

/// The PKCS#8 `der` encrypted with `passphrase` and written in PEM, as .NET's
/// `ExportEncryptedPkcs8PrivateKeyPem` with `PbeParameters(Aes256Cbc, SHA256, iterations)`:
/// PBES2, PBKDF2-HMAC-SHA256 over `iterations` rounds and a random salt, AES-256-CBC under
/// a random vector.
///
/// # Errors
///
/// [`Error::Randomness`] without a salt or vector; [`Error::Encoding`] when `der` is not
/// PKCS#8 or the result cannot be written.
pub fn encrypted_pkcs8_pem(
    der: &[u8],
    passphrase: &[u8],
    iterations: u32,
) -> Result<Zeroizing<String>, Error> {
    // The key is checked to be PKCS#8 before it is encrypted.
    PrivateKeyInfoRef::from_der(der).map_err(encoding)?;
    let salt: Zeroizing<[u8; PBES2_SALT_LEN]> = Zeroizing::new(random::array()?);
    let iv: [u8; PBES2_IV_LEN] = random::array()?;
    let parameters = pbes2::Parameters::generate_pbkdf2_sha256_aes256cbc(iterations, &*salt, iv)
        .map_err(encoding)?;
    let encrypted = parameters.encrypt(passphrase, der).map_err(encoding)?;
    let data = pkcs8::der::asn1::OctetStringRef::new(&encrypted).map_err(encoding)?;
    EncryptedPrivateKeyInfoRef {
        encryption_algorithm: parameters.into(),
        encrypted_data: data,
    }
    .to_pem(LineEnding::LF)
    .map(Zeroizing::new)
    .map_err(encoding)
}

/// A library's failure, said.
fn encoding(error: impl fmt::Display) -> Error {
    Error::Encoding(error.to_string())
}

/// The system's generator, its failures noted, for the RSA generation, which takes a
/// generator that cannot fail: a failure is noted rather than raised, the draws go on from
/// a stand-in so the generation still ends, and the key made is thrown away. Nothing
/// panics, and no key drawn even partly from the stand-in is ever handed out.
#[derive(Default)]
struct Watched {
    failed: bool,
    /// The stand-in's counter, used once the system's generator failed.
    counter: u64,
}

impl Watched {
    /// `dest` filled from the system's generator, or from the stand-in once it failed.
    fn fill(&mut self, dest: &mut [u8]) {
        if !self.failed && random::fill(dest).is_ok() {
            return;
        }
        self.failed = true;
        // Different bytes on every draw, so a search for primes still ends; what it makes is
        // never used.
        for chunk in dest.chunks_mut(SHA256_LEN) {
            self.counter += 1;
            let block = crate::hash::sha256(&self.counter.to_le_bytes());
            chunk.copy_from_slice(&block[..chunk.len()]);
        }
    }
}

impl TryRng for Watched {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Infallible> {
        let mut bytes = [0; 4];
        self.fill(&mut bytes);
        Ok(u32::from_le_bytes(bytes))
    }

    fn try_next_u64(&mut self) -> Result<u64, Infallible> {
        let mut bytes = [0; 8];
        self.fill(&mut bytes);
        Ok(u64::from_le_bytes(bytes))
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Infallible> {
        self.fill(dest);
        Ok(())
    }
}

impl TryCryptoRng for Watched {}

#[cfg(test)]
mod tests {
    use pkcs8::der::Decode as _;
    use pkcs8::der::pem as der_pem;
    use pkcs8::pkcs5::pbes2;
    use pkcs8::{EncryptedPrivateKeyInfoRef, PrivateKeyInfoRef};
    use rsa::pkcs8::DecodePrivateKey as _;
    use rsa::traits::PublicKeyParts as _;

    use super::{
        ED25519_PKCS8_PREFIX, GENERATOR_FAILED, KeyAlgorithm, KeyPair, Watched,
        encrypted_pkcs8_pem, pem,
    };
    use crate::Error;

    /// Bits of the RSA keys the tests make: the smallest the generators offer.
    const RSA_BITS: usize = 2048;

    /// An Ed25519 key of RFC 8410, section 10.3.
    const ED25519_PKCS8: [u8; 48] = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20, 0xd4, 0xee, 0x72, 0xdb, 0xf9, 0x13, 0x58, 0x4a, 0xd5, 0xb6, 0xd8, 0xf1, 0xf7, 0x69,
        0xf8, 0xad, 0x3a, 0xfe, 0x7c, 0x28, 0xcb, 0xf1, 0xd4, 0xfb, 0xe0, 0x97, 0xa8, 0x8f, 0x44,
        0x75, 0x58, 0x42,
    ];

    #[test]
    fn a_failing_generator_gives_an_error_and_no_key() {
        let failing = Watched {
            failed: true,
            counter: 0,
        };
        assert_eq!(
            KeyPair::rsa_with(RSA_BITS, failing).err(),
            Some(Error::Randomness(GENERATOR_FAILED.to_owned()))
        );
    }

    #[test]
    fn an_rsa_key_has_the_size_asked_and_reads_back_as_pkcs8() {
        let pair = KeyPair::rsa(RSA_BITS).expect("made");
        assert_eq!(pair.algorithm(), KeyAlgorithm::Rsa(RSA_BITS));
        let key = rsa::RsaPrivateKey::from_pkcs8_der(pair.pkcs8_der()).expect("PKCS#8");
        assert_eq!(key.size() * 8, RSA_BITS);
        let line = pair.openssh_public("me@host").expect("line");
        assert!(line.starts_with("ssh-rsa "));
        assert!(line.ends_with(" me@host"));
    }

    #[test]
    fn an_ed25519_key_is_rfc_8410_pkcs8_and_its_public_line_parses() {
        let pair = KeyPair::ed25519().expect("made");
        assert_eq!(pair.algorithm(), KeyAlgorithm::Ed25519);
        assert_eq!(pair.pkcs8_der().len(), ED25519_PKCS8.len());
        assert!(pair.pkcs8_der().starts_with(&ED25519_PKCS8_PREFIX));
        PrivateKeyInfoRef::from_der(pair.pkcs8_der()).expect("PKCS#8");
        let line = pair.openssh_public("c").expect("line");
        let parsed = ssh_key::PublicKey::from_openssh(&line).expect("OpenSSH");
        assert_eq!(
            parsed.fingerprint(ssh_key::HashAlg::Sha256).to_string(),
            pair.openssh_fingerprint()
        );
        assert!(pair.openssh_fingerprint().starts_with("SHA256:"));
    }

    #[test]
    fn two_ed25519_keys_differ_and_debug_shows_neither() {
        let first = KeyPair::ed25519().expect("made");
        let second = KeyPair::ed25519().expect("made");
        assert_ne!(first.pkcs8_der(), second.pkcs8_der());
        assert_eq!(format!("{first:?}"), "KeyPair { algorithm: Ed25519, .. }");
    }

    #[test]
    fn a_key_in_pem_is_the_rfc_7468_text_of_its_der() {
        assert_eq!(
            pem("PRIVATE KEY", &ED25519_PKCS8).expect("written"),
            "-----BEGIN PRIVATE KEY-----\n\
             MC4CAQAwBQYDK2VwBCIEINTuctv5E1hK1bbY8fdp+K06/nwoy/HU++CXqI9EdVhC\n\
             -----END PRIVATE KEY-----\n"
        );
    }

    #[test]
    fn an_encrypted_key_reads_back_with_its_passphrase_only() {
        let text = encrypted_pkcs8_pem(&ED25519_PKCS8, b"s3cret", 1000).expect("written");
        let (label, der) = der_pem::decode_vec(text.as_bytes()).expect("pem");
        assert_eq!(label, "ENCRYPTED PRIVATE KEY");
        let info = EncryptedPrivateKeyInfoRef::from_der(&der).expect("der");
        let scheme = info.encryption_algorithm.pbes2().expect("PBES2");
        assert_eq!(scheme.kdf.pbkdf2().expect("PBKDF2").iteration_count, 1000);
        assert!(matches!(
            scheme.encryption,
            pbes2::EncryptionScheme::Aes256Cbc { .. }
        ));
        assert_eq!(
            info.decrypt("s3cret").expect("decrypted").as_bytes(),
            ED25519_PKCS8
        );
        assert!(info.decrypt("wrong").is_err());
        assert!(matches!(
            encrypted_pkcs8_pem(b"not PKCS#8", b"s3cret", 1000),
            Err(Error::Encoding(_))
        ));
    }
}
