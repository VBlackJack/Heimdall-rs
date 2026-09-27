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

//! The header of a vault file, fixed at 108 bytes, integers little-endian:
//!
//! | Bytes | Field |
//! |---|---|
//! | 4 | magic `SVLT` |
//! | 2 | format version, 1 |
//! | 1 | key derivation, 1 for Argon2id version 0x13 |
//! | 4 | Argon2id memory, KiB |
//! | 4 | Argon2id iterations |
//! | 4 | Argon2id lanes |
//! | 16 | salt |
//! | 1 | how the data key is wrapped, 1 for a password |
//! | 12 | nonce of the wrapping |
//! | 48 | the wrapped data key and its tag |
//! | 12 | nonce of the body |
//!
//! The sealed body follows, its tag last.

/// Bytes of a key.
pub const KEY_LEN: usize = 32;

/// Bytes of a nonce.
pub const NONCE_LEN: usize = 12;

/// Bytes of a salt.
pub const SALT_LEN: usize = 16;

/// Bytes of an AES-GCM tag.
const TAG_LEN: usize = 16;

/// Bytes of the wrapped data key.
const WRAPPED_LEN: usize = KEY_LEN + TAG_LEN;

const MAGIC: [u8; 4] = *b"SVLT";
const VERSION: u16 = 1;
const KDF_ARGON2ID: u8 = 1;
const WRAP_BY_PASSWORD: u8 = 1;

/// Bytes of the header up to and including the wrapping's nonce: what the wrapping
/// authenticates.
const WRAP_PREFIX_LEN: usize = 4 + 2 + 1 + 4 + 4 + 4 + SALT_LEN + 1 + NONCE_LEN;

/// Bytes of the whole header.
pub const HEADER_LEN: usize = WRAP_PREFIX_LEN + WRAPPED_LEN + NONCE_LEN;

/// The cost of Argon2id, as the file states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory, in KiB.
    pub memory_kib: u32,
    /// Passes over the memory.
    pub iterations: u32,
    /// Parallel lanes.
    pub lanes: u32,
}

impl KdfParams {
    /// The least a vault may ask for: 64 MiB, 3 passes, 1 lane. What a new vault gets.
    pub const FLOOR: Self = Self {
        memory_kib: 64 * 1024,
        iterations: 3,
        lanes: 1,
    };

    /// The most a vault may ask for: beyond, a file could make opening it exhaust the
    /// machine.
    pub const CEILING: Self = Self {
        memory_kib: 4 * 1024 * 1024,
        iterations: 64,
        lanes: 16,
    };

    /// Whether each value lies between [`KdfParams::FLOOR`] and [`KdfParams::CEILING`].
    #[must_use]
    pub fn is_acceptable(&self) -> bool {
        (Self::FLOOR.memory_kib..=Self::CEILING.memory_kib).contains(&self.memory_kib)
            && (Self::FLOOR.iterations..=Self::CEILING.iterations).contains(&self.iterations)
            && (Self::FLOOR.lanes..=Self::CEILING.lanes).contains(&self.lanes)
    }
}

/// A parsed header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub params: KdfParams,
    pub salt: [u8; SALT_LEN],
    pub wrap_nonce: [u8; NONCE_LEN],
    pub wrapped_key: Vec<u8>,
    pub body_nonce: [u8; NONCE_LEN],
}

impl Header {
    /// The header of `bytes` and the sealed body after it; `None` unless every fixed field
    /// holds what this version writes and a body with its tag follows.
    pub fn parse(bytes: &[u8]) -> Option<(Self, &[u8])> {
        if bytes.len() < HEADER_LEN + TAG_LEN {
            return None;
        }
        let (header, body) = bytes.split_at(HEADER_LEN);
        let mut reader = Reader(header);
        let fixed_ok = reader.take::<4>()? == MAGIC
            && u16::from_le_bytes(reader.take()?) == VERSION
            && reader.take::<1>()? == [KDF_ARGON2ID];
        if !fixed_ok {
            return None;
        }
        let params = KdfParams {
            memory_kib: u32::from_le_bytes(reader.take()?),
            iterations: u32::from_le_bytes(reader.take()?),
            lanes: u32::from_le_bytes(reader.take()?),
        };
        let salt = reader.take()?;
        if reader.take::<1>()? != [WRAP_BY_PASSWORD] {
            return None;
        }
        let wrap_nonce = reader.take()?;
        let wrapped_key = reader.take::<WRAPPED_LEN>()?.to_vec();
        let body_nonce = reader.take()?;
        Some((
            Self {
                params,
                salt,
                wrap_nonce,
                wrapped_key,
                body_nonce,
            },
            body,
        ))
    }

    /// The header as written.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Self::wrap_authenticated_for(&self.params, &self.salt, &self.wrap_nonce);
        bytes.extend_from_slice(&self.wrapped_key);
        bytes.extend_from_slice(&self.body_nonce);
        bytes
    }

    /// The header bytes up to and including the wrapping's nonce: what the wrapping
    /// authenticates, built by the one function both writing and reading use.
    pub fn wrap_authenticated_for(
        params: &KdfParams,
        salt: &[u8; SALT_LEN],
        wrap_nonce: &[u8; NONCE_LEN],
    ) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER_LEN);
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.push(KDF_ARGON2ID);
        bytes.extend_from_slice(&params.memory_kib.to_le_bytes());
        bytes.extend_from_slice(&params.iterations.to_le_bytes());
        bytes.extend_from_slice(&params.lanes.to_le_bytes());
        bytes.extend_from_slice(salt);
        bytes.push(WRAP_BY_PASSWORD);
        bytes.extend_from_slice(wrap_nonce);
        debug_assert_eq!(bytes.len(), WRAP_PREFIX_LEN);
        bytes
    }
}

/// Reads fixed-size fields in order.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (field, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;
        Some(*field)
    }
}

#[cfg(test)]
mod tests {
    use super::{HEADER_LEN, Header, KdfParams};

    fn header() -> Header {
        Header {
            params: KdfParams::FLOOR,
            salt: [1; 16],
            wrap_nonce: [2; 12],
            wrapped_key: vec![3; 48],
            body_nonce: [4; 12],
        }
    }

    #[test]
    fn a_header_reads_back_as_written_and_is_108_bytes() {
        let bytes = header().to_bytes();
        assert_eq!(bytes.len(), HEADER_LEN);
        assert_eq!(HEADER_LEN, 108);
        let file = [bytes.as_slice(), &[0; 16]].concat();
        let (read, body) = Header::parse(&file).expect("parsed");
        assert_eq!(read, header());
        assert_eq!(body.len(), 16);
    }

    #[test]
    fn a_file_too_short_for_a_tag_or_with_another_magic_version_or_method_is_refused() {
        let good = [header().to_bytes().as_slice(), &[0; 16]].concat();
        assert!(
            Header::parse(&good[..good.len() - 1]).is_none(),
            "no room for a tag"
        );
        for index in [0, 4, 6, 7 + 12 + 16] {
            let mut changed = good.clone();
            changed[index] ^= 1;
            assert!(Header::parse(&changed).is_none(), "byte {index}");
        }
    }

    #[test]
    fn costs_outside_the_floor_and_ceiling_are_refused() {
        assert!(KdfParams::FLOOR.is_acceptable());
        assert!(KdfParams::CEILING.is_acceptable());
        let below = |change: fn(&mut KdfParams)| {
            let mut params = KdfParams::FLOOR;
            change(&mut params);
            params.is_acceptable()
        };
        assert!(!below(|p| p.memory_kib -= 1));
        assert!(!below(|p| p.iterations -= 1));
        assert!(!below(|p| p.lanes = 0));
        let above = |change: fn(&mut KdfParams)| {
            let mut params = KdfParams::CEILING;
            change(&mut params);
            params.is_acceptable()
        };
        assert!(!above(|p| p.memory_kib += 1));
        assert!(!above(|p| p.iterations += 1));
        assert!(!above(|p| p.lanes += 1));
    }
}
