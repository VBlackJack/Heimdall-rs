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

//! Message digests, one-shot or streamed.
//!
//! MD5 and SHA-1 are here for interoperability only (checksums a user compares, HMAC-SHA1
//! for TOTP, formats that name them): they resist no collision attack, and
//! [`Algorithm::is_collision_resistant`] says so for anything that must choose.

use std::fmt;

use sha2::Digest as _;

/// Bytes of the longest digest, SHA-512's.
pub const MAX_OUTPUT_LEN: usize = 64;

/// Bytes of a SHA-256 digest.
pub const SHA256_LEN: usize = 32;

/// Bytes of an MD5 digest.
const MD5_LEN: usize = 16;

/// Bytes of a SHA-1 digest.
const SHA1_LEN: usize = 20;

/// Bytes of a SHA-384 digest.
const SHA384_LEN: usize = 48;

/// A digest algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Algorithm {
    /// MD5, RFC 1321: interoperability only.
    Md5,
    /// SHA-1, FIPS 180-4: interoperability only.
    Sha1,
    /// SHA-256, FIPS 180-4.
    Sha256,
    /// SHA-384, FIPS 180-4.
    Sha384,
    /// SHA-512, FIPS 180-4.
    Sha512,
    /// SHA3-256, FIPS 202.
    Sha3_256,
}

impl Algorithm {
    /// Bytes of the digest.
    #[must_use]
    pub const fn output_len(self) -> usize {
        match self {
            Self::Md5 => MD5_LEN,
            Self::Sha1 => SHA1_LEN,
            Self::Sha256 | Self::Sha3_256 => SHA256_LEN,
            Self::Sha384 => SHA384_LEN,
            Self::Sha512 => MAX_OUTPUT_LEN,
        }
    }

    /// Whether no practical collision is known: false for MD5 and SHA-1.
    #[must_use]
    pub const fn is_collision_resistant(self) -> bool {
        !matches!(self, Self::Md5 | Self::Sha1)
    }
}

/// A computed digest. Not a secret: it may be shown, compared and copied.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Digest {
    bytes: [u8; MAX_OUTPUT_LEN],
    len: usize,
}

impl Digest {
    fn from_slice(output: &[u8]) -> Self {
        let mut bytes = [0; MAX_OUTPUT_LEN];
        bytes[..output.len()].copy_from_slice(output);
        Self {
            bytes,
            len: output.len(),
        }
    }

    /// The digest's bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

impl AsRef<[u8]> for Digest {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Digest(")?;
        for byte in self.as_bytes() {
            write!(f, "{byte:02x}")?;
        }
        f.write_str(")")
    }
}

/// A digest computed over data given in pieces.
#[derive(Clone)]
pub struct Hasher(State);

#[derive(Clone)]
enum State {
    Md5(md5::Md5),
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
    Sha384(sha2::Sha384),
    Sha512(sha2::Sha512),
    Sha3_256(sha3::Sha3_256),
}

impl Hasher {
    /// A digest by `algorithm`, over nothing yet.
    #[must_use]
    pub fn new(algorithm: Algorithm) -> Self {
        Self(match algorithm {
            Algorithm::Md5 => State::Md5(md5::Md5::new()),
            Algorithm::Sha1 => State::Sha1(sha1::Sha1::new()),
            Algorithm::Sha256 => State::Sha256(sha2::Sha256::new()),
            Algorithm::Sha384 => State::Sha384(sha2::Sha384::new()),
            Algorithm::Sha512 => State::Sha512(sha2::Sha512::new()),
            Algorithm::Sha3_256 => State::Sha3_256(sha3::Sha3_256::new()),
        })
    }

    /// The algorithm.
    #[must_use]
    pub fn algorithm(&self) -> Algorithm {
        match self.0 {
            State::Md5(_) => Algorithm::Md5,
            State::Sha1(_) => Algorithm::Sha1,
            State::Sha256(_) => Algorithm::Sha256,
            State::Sha384(_) => Algorithm::Sha384,
            State::Sha512(_) => Algorithm::Sha512,
            State::Sha3_256(_) => Algorithm::Sha3_256,
        }
    }

    /// `data` added after what came before.
    pub fn update(&mut self, data: &[u8]) {
        match &mut self.0 {
            State::Md5(state) => state.update(data),
            State::Sha1(state) => state.update(data),
            State::Sha256(state) => state.update(data),
            State::Sha384(state) => state.update(data),
            State::Sha512(state) => state.update(data),
            State::Sha3_256(state) => state.update(data),
        }
    }

    /// The digest of everything added.
    #[must_use]
    pub fn finalize(self) -> Digest {
        match self.0 {
            State::Md5(state) => Digest::from_slice(&state.finalize()),
            State::Sha1(state) => Digest::from_slice(&state.finalize()),
            State::Sha256(state) => Digest::from_slice(&state.finalize()),
            State::Sha384(state) => Digest::from_slice(&state.finalize()),
            State::Sha512(state) => Digest::from_slice(&state.finalize()),
            State::Sha3_256(state) => Digest::from_slice(&state.finalize()),
        }
    }
}

impl fmt::Debug for Hasher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Hasher").field(&self.algorithm()).finish()
    }
}

/// The digest of `data` by `algorithm`.
#[must_use]
pub fn digest(algorithm: Algorithm, data: &[u8]) -> Digest {
    let mut hasher = Hasher::new(algorithm);
    hasher.update(data);
    hasher.finalize()
}

/// The SHA-256 digest of `data`, as an array: fingerprints and content identities.
#[must_use]
pub fn sha256(data: &[u8]) -> [u8; SHA256_LEN] {
    sha2::Sha256::digest(data).into()
}

#[cfg(test)]
mod tests {
    use super::{Algorithm, Hasher, MAX_OUTPUT_LEN, digest, sha256};
    use crate::test_hex::hex;

    const ALL: [Algorithm; 6] = [
        Algorithm::Md5,
        Algorithm::Sha1,
        Algorithm::Sha256,
        Algorithm::Sha384,
        Algorithm::Sha512,
        Algorithm::Sha3_256,
    ];

    /// "abc" by each algorithm: RFC 1321 appendix A.5, FIPS 180-2 appendices A to D, and
    /// the SHA3-256 "abc" example NIST publishes for FIPS 202.
    const ABC: [(Algorithm, &str); 6] = [
        (Algorithm::Md5, "900150983cd24fb0d6963f7d28e17f72"),
        (Algorithm::Sha1, "a9993e364706816aba3e25717850c26c9cd0d89d"),
        (
            Algorithm::Sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            Algorithm::Sha384,
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed\
             8086072ba1e7cc2358baeca134c825a7",
        ),
        (
            Algorithm::Sha512,
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
        ),
        (
            Algorithm::Sha3_256,
            "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532",
        ),
    ];

    #[test]
    fn abc_gives_the_published_digests() {
        for (algorithm, expected) in ABC {
            assert_eq!(
                digest(algorithm, b"abc").as_bytes(),
                hex(expected),
                "{algorithm:?}"
            );
        }
        assert_eq!(sha256(b"abc").to_vec(), hex(ABC[2].1));
    }

    #[test]
    fn pieces_give_the_digest_of_the_whole() {
        for algorithm in ALL {
            let mut hasher = Hasher::new(algorithm);
            assert_eq!(hasher.algorithm(), algorithm);
            hasher.update(b"a");
            hasher.update(b"");
            hasher.update(b"bc");
            assert_eq!(
                hasher.finalize(),
                digest(algorithm, b"abc"),
                "{algorithm:?}"
            );
        }
    }

    #[test]
    fn each_digest_has_its_stated_length() {
        for algorithm in ALL {
            let len = digest(algorithm, b"").as_bytes().len();
            assert_eq!(len, algorithm.output_len(), "{algorithm:?}");
            assert!(len <= MAX_OUTPUT_LEN);
        }
    }

    #[test]
    fn only_md5_and_sha1_are_flagged_as_broken() {
        let broken: Vec<_> = ALL
            .into_iter()
            .filter(|algorithm| !algorithm.is_collision_resistant())
            .collect();
        assert_eq!(broken, [Algorithm::Md5, Algorithm::Sha1]);
    }
}
