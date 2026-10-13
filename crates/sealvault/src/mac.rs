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

//! HMAC (RFC 2104) over the digests of [`crate::hash`], computed or checked in constant
//! time.
//!
//! HMAC-MD5 and HMAC-SHA1 remain sound as MACs and are what TOTP (RFC 6238) and older
//! formats ask for; they are here for that interoperability.

use std::fmt;

use hmac::{Hmac, KeyInit, Mac};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::hash::{self, MAX_OUTPUT_LEN};

/// An HMAC algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Algorithm {
    /// HMAC-MD5, RFC 2104: interoperability only.
    HmacMd5,
    /// HMAC-SHA1, RFC 2104: TOTP's default.
    HmacSha1,
    /// HMAC-SHA256, RFC 4231.
    HmacSha256,
    /// HMAC-SHA384, RFC 4231.
    HmacSha384,
    /// HMAC-SHA512, RFC 4231.
    HmacSha512,
}

impl Algorithm {
    /// The digest underneath.
    #[must_use]
    pub const fn hash(self) -> hash::Algorithm {
        match self {
            Self::HmacMd5 => hash::Algorithm::Md5,
            Self::HmacSha1 => hash::Algorithm::Sha1,
            Self::HmacSha256 => hash::Algorithm::Sha256,
            Self::HmacSha384 => hash::Algorithm::Sha384,
            Self::HmacSha512 => hash::Algorithm::Sha512,
        }
    }

    /// The HMAC over `algorithm`; `None` for a digest HMAC is not offered with (SHA3-256).
    #[must_use]
    pub const fn over(algorithm: hash::Algorithm) -> Option<Self> {
        match algorithm {
            hash::Algorithm::Md5 => Some(Self::HmacMd5),
            hash::Algorithm::Sha1 => Some(Self::HmacSha1),
            hash::Algorithm::Sha256 => Some(Self::HmacSha256),
            hash::Algorithm::Sha384 => Some(Self::HmacSha384),
            hash::Algorithm::Sha512 => Some(Self::HmacSha512),
            hash::Algorithm::Sha3_256 => None,
        }
    }

    /// Bytes of the tag.
    #[must_use]
    pub const fn tag_len(self) -> usize {
        self.hash().output_len()
    }
}

/// A computed tag, wiped when dropped: a one-time code is cut from it, and a tag a key
/// never sent may be what an attacker wants.
#[derive(Zeroize)]
pub struct Tag {
    bytes: [u8; MAX_OUTPUT_LEN],
    len: usize,
}

impl Tag {
    fn from_slice(output: &[u8]) -> Self {
        let mut bytes = [0; MAX_OUTPUT_LEN];
        bytes[..output.len()].copy_from_slice(output);
        Self {
            bytes,
            len: output.len(),
        }
    }

    /// The tag's bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Tag(..)")
    }
}

impl Drop for Tag {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl ZeroizeOnDrop for Tag {}

/// The tag of `data` under `key` by `algorithm`. HMAC takes a key of any length.
#[must_use]
pub fn compute(algorithm: Algorithm, key: &[u8], data: &[u8]) -> Tag {
    match algorithm {
        Algorithm::HmacMd5 => tag_of::<Hmac<md5::Md5>>(key, data),
        Algorithm::HmacSha1 => tag_of::<Hmac<sha1::Sha1>>(key, data),
        Algorithm::HmacSha256 => tag_of::<Hmac<sha2::Sha256>>(key, data),
        Algorithm::HmacSha384 => tag_of::<Hmac<sha2::Sha384>>(key, data),
        Algorithm::HmacSha512 => tag_of::<Hmac<sha2::Sha512>>(key, data),
    }
}

/// Whether `tag` is the tag of `data` under `key` by `algorithm`, compared in constant time.
/// A tag cut short is refused, never compared on its prefix.
#[must_use]
pub fn verify(algorithm: Algorithm, key: &[u8], data: &[u8], tag: &[u8]) -> bool {
    match algorithm {
        Algorithm::HmacMd5 => verifies::<Hmac<md5::Md5>>(key, data, tag),
        Algorithm::HmacSha1 => verifies::<Hmac<sha1::Sha1>>(key, data, tag),
        Algorithm::HmacSha256 => verifies::<Hmac<sha2::Sha256>>(key, data, tag),
        Algorithm::HmacSha384 => verifies::<Hmac<sha2::Sha384>>(key, data, tag),
        Algorithm::HmacSha512 => verifies::<Hmac<sha2::Sha512>>(key, data, tag),
    }
}

fn keyed<M: Mac + KeyInit>(key: &[u8], data: &[u8]) -> M {
    let mut mac = <M as KeyInit>::new_from_slice(key).expect("an HMAC takes a key of any length");
    Mac::update(&mut mac, data);
    mac
}

fn tag_of<M: Mac + KeyInit>(key: &[u8], data: &[u8]) -> Tag {
    Tag::from_slice(&keyed::<M>(key, data).finalize().into_bytes())
}

fn verifies<M: Mac + KeyInit>(key: &[u8], data: &[u8], tag: &[u8]) -> bool {
    keyed::<M>(key, data).verify_slice(tag).is_ok()
}

#[cfg(test)]
mod tests {
    use zeroize::ZeroizeOnDrop;

    use super::{Algorithm, Tag, compute, verify};
    use crate::hash;
    use crate::test_hex::hex;

    /// Test case 1 of RFC 4231 (and of RFC 2202 for MD5 and SHA-1, with a 16-byte key for
    /// MD5): the key 0x0b repeated, over "Hi There".
    const CASE_1: [(Algorithm, usize, &str); 5] = [
        (Algorithm::HmacMd5, 16, "9294727a3638bb1c13f48ef8158bfc9d"),
        (
            Algorithm::HmacSha1,
            20,
            "b617318655057264e28bc0b6fb378c8ef146be00",
        ),
        (
            Algorithm::HmacSha256,
            20,
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
        ),
        (
            Algorithm::HmacSha384,
            20,
            "afd03944d84895626b0825f4ab46907f15f9dadbe4101ec682aa034c7cebc59c\
             faea9ea9076ede7f4af152e8b2fa9cb6",
        ),
        (
            Algorithm::HmacSha512,
            20,
            "87aa7cdea5ef619d4ff0b4241a1d6cb02379f4e2ce4ec2787ad0b30545e17cde\
             daa833b7d6b8a702038b274eaea3f4e4be9d914eeb61f1702e696c203a126854",
        ),
    ];

    /// Test case 2 of RFC 4231: the key "Jefe".
    const CASE_2_SHA256: &str = "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843";

    #[test]
    fn the_rfc_4231_and_2202_vectors_are_met() {
        for (algorithm, key_len, expected) in CASE_1 {
            let key = vec![0x0b; key_len];
            let tag = compute(algorithm, &key, b"Hi There");
            assert_eq!(tag.as_bytes(), hex(expected), "{algorithm:?}");
            assert_eq!(tag.as_bytes().len(), algorithm.tag_len());
            assert!(verify(algorithm, &key, b"Hi There", &hex(expected)));
        }
        let tag = compute(
            Algorithm::HmacSha256,
            b"Jefe",
            b"what do ya want for nothing?",
        );
        assert_eq!(tag.as_bytes(), hex(CASE_2_SHA256));
    }

    #[test]
    fn a_wrong_or_cut_tag_does_not_verify() {
        let key = [0x0b; 20];
        let mut tag = hex(CASE_1[2].2);
        assert!(!verify(Algorithm::HmacSha256, &key, b"Hi there", &tag));
        assert!(!verify(
            Algorithm::HmacSha256,
            &key,
            b"Hi There",
            &tag[..tag.len() - 1]
        ));
        assert!(!verify(Algorithm::HmacSha256, &key, b"Hi There", &[]));
        tag[0] ^= 1;
        assert!(!verify(Algorithm::HmacSha256, &key, b"Hi There", &tag));
    }

    #[test]
    fn each_hmac_names_its_digest_and_back() {
        for (algorithm, _, _) in CASE_1 {
            assert_eq!(Algorithm::over(algorithm.hash()), Some(algorithm));
        }
        assert_eq!(Algorithm::over(hash::Algorithm::Sha3_256), None);
    }

    #[test]
    fn a_tag_is_wiped_on_drop_and_never_shown() {
        fn wiped_on_drop<T: ZeroizeOnDrop>() {}
        wiped_on_drop::<Tag>();
        let tag = compute(Algorithm::HmacSha256, b"k", b"m");
        assert_eq!(format!("{tag:?}"), "Tag(..)");
    }
}
