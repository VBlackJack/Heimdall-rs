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

//! Authenticated encryption: AES-256-GCM (NIST SP 800-38D), a 96-bit nonce, a 128-bit tag
//! after the ciphertext.
//!
//! A nonce must never seal twice under one key. [`seal`] therefore takes a [`FreshNonce`]
//! by value, and the only way to make one is [`FreshNonce::random`]: its bytes can be read
//! first, to be written beside the ciphertext or authenticated with it, but the nonce
//! itself is spent by the sealing. [`open`] takes plain bytes, as read back.

use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Aead, KeyInit, Nonce, Payload};

use crate::Error;
use crate::random;
use crate::secret::{SecretBytes, SecretKey};

/// Bytes of a key.
pub const KEY_LEN: usize = 32;

/// Bytes of a nonce.
pub const NONCE_LEN: usize = 12;

/// Bytes of the tag that follows the ciphertext.
pub const TAG_LEN: usize = 16;

/// An AES-256-GCM key.
pub type Key = SecretKey<KEY_LEN>;

/// A nonce drawn from the system's generator, spent by the one sealing it is given to.
#[derive(Debug, PartialEq, Eq)]
pub struct FreshNonce([u8; NONCE_LEN]);

impl FreshNonce {
    /// A nonce from the operating system's generator.
    ///
    /// # Errors
    ///
    /// [`Error::Randomness`].
    pub fn random() -> Result<Self, Error> {
        random::array().map(Self)
    }

    /// The nonce's bytes, to be stored or authenticated before the sealing spends it.
    #[must_use]
    pub fn bytes(&self) -> [u8; NONCE_LEN] {
        self.0
    }

    /// A chosen nonce, for known-answer tests only.
    #[cfg(test)]
    pub(crate) fn fixed(bytes: [u8; NONCE_LEN]) -> Self {
        Self(bytes)
    }
}

/// `plain` sealed under `key` and `nonce`, `authenticated` bound to it without being
/// encrypted: the ciphertext, then the tag.
///
/// # Errors
///
/// [`Error::Unauthentic`] when the cipher refuses the input, which only a message past
/// AES-GCM's limit of about 64 GiB can cause.
#[expect(
    clippy::needless_pass_by_value,
    reason = "taken by value so that a nonce is spent by the one sealing it serves"
)]
pub fn seal(
    key: &Key,
    nonce: FreshNonce,
    plain: &[u8],
    authenticated: &[u8],
) -> Result<Vec<u8>, Error> {
    Aes256Gcm::new(key.as_bytes().into())
        .encrypt(
            &Nonce::<Aes256Gcm>::from(nonce.0),
            Payload {
                msg: plain,
                aad: authenticated,
            },
        )
        .map_err(|_| Error::Unauthentic)
}

/// The plaintext of `sealed`, opened with `key` and `nonce` and checked against
/// `authenticated`, in a buffer wiped on drop.
///
/// # Errors
///
/// [`Error::Unauthentic`] for a wrong key, nonce or authenticated data, a changed byte or a
/// cut message: deliberately one answer.
pub fn open(
    key: &Key,
    nonce: &[u8; NONCE_LEN],
    sealed: &[u8],
    authenticated: &[u8],
) -> Result<SecretBytes, Error> {
    Aes256Gcm::new(key.as_bytes().into())
        .decrypt(
            &Nonce::<Aes256Gcm>::from(*nonce),
            Payload {
                msg: sealed,
                aad: authenticated,
            },
        )
        .map(SecretBytes::new)
        .map_err(|_| Error::Unauthentic)
}

#[cfg(test)]
mod tests {
    use super::{FreshNonce, Key, NONCE_LEN, TAG_LEN, open, seal};
    use crate::Error;
    use crate::test_hex::hex;

    fn zero_key() -> Key {
        Key::from_array(&[0; 32])
    }

    /// Test cases 13 and 14 of the original GCM specification, the vectors NIST's GCM
    /// validation reuses: a key and a nonce of zeros, over nothing and over 16 zeros.
    #[test]
    fn the_gcm_specification_vectors_are_met() {
        let empty = seal(&zero_key(), FreshNonce::fixed([0; NONCE_LEN]), b"", b"").expect("sealed");
        assert_eq!(empty, hex("530f8afbc74536b9a963b4f1c4cb738b"));
        let zeros = seal(
            &zero_key(),
            FreshNonce::fixed([0; NONCE_LEN]),
            &[0; 16],
            b"",
        )
        .expect("sealed");
        assert_eq!(
            zeros,
            hex("cea7403d4d606b6e074ec5d3baf39d18d0d1c8a799996bf0265b98b5d48ab919")
        );
    }

    /// Authenticated data counts: computed outside this crate with OpenSSL (Python
    /// `cryptography`), "sealvault" sealed with "context" authenticated.
    #[test]
    fn authenticated_data_is_bound_as_openssl_binds_it() {
        let sealed = seal(
            &zero_key(),
            FreshNonce::fixed([0; NONCE_LEN]),
            b"sealvault",
            b"context",
        )
        .expect("sealed");
        assert_eq!(
            sealed,
            hex("bdc221513b011e027396e8538730265aa5faa17a44a4180c8f")
        );
    }

    #[test]
    fn what_is_sealed_opens_and_nothing_else_does() {
        let key = Key::from_array(&[7; 32]);
        let nonce = FreshNonce::random().expect("random");
        let bytes = nonce.bytes();
        let sealed = seal(&key, nonce, b"secret", b"aad").expect("sealed");
        assert_eq!(sealed.len(), b"secret".len() + TAG_LEN);
        let plain = open(&key, &bytes, &sealed, b"aad").expect("opened");
        assert_eq!(plain.as_bytes(), b"secret");

        let unauthentic = |result: Result<_, Error>| matches!(result, Err(Error::Unauthentic));
        assert!(unauthentic(open(&key, &bytes, &sealed, b"aaD")));
        assert!(unauthentic(open(
            &Key::from_array(&[8; 32]),
            &bytes,
            &sealed,
            b"aad"
        )));
        let mut other_nonce = bytes;
        other_nonce[0] ^= 1;
        assert!(unauthentic(open(&key, &other_nonce, &sealed, b"aad")));
        let mut changed = sealed.clone();
        changed[0] ^= 1;
        assert!(unauthentic(open(&key, &bytes, &changed, b"aad")));
        assert!(unauthentic(open(
            &key,
            &bytes,
            &sealed[..sealed.len() - 1],
            b"aad"
        )));
    }

    #[test]
    fn two_fresh_nonces_differ() {
        let first = FreshNonce::random().expect("random");
        let second = FreshNonce::random().expect("random");
        assert_ne!(first, second);
    }
}
