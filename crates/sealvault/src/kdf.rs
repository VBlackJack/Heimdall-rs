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

//! Keys derived from passwords: Argon2id (RFC 9106), version 0x13.

use crate::Error;
use crate::secret::SecretKey;

/// The cost of Argon2id.
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

/// The key of `N` bytes that `password` gives under `params` and `salt`, by Argon2id
/// version 0x13 with neither secret nor associated data. The cost is the caller's to bound
/// before calling: [`KdfParams::is_acceptable`] for a cost read from a file.
///
/// # Errors
///
/// [`Error::KdfParameters`] when Argon2id refuses `params`, the salt or the length.
pub fn argon2id<const N: usize>(
    password: &[u8],
    salt: &[u8],
    params: &KdfParams,
) -> Result<SecretKey<N>, Error> {
    let argon = argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(params.memory_kib, params.iterations, params.lanes, Some(N))
            .map_err(|_| Error::KdfParameters)?,
    );
    let mut key = SecretKey::zeroed();
    argon
        .hash_password_into(password, salt, key.as_mut_bytes())
        .map_err(|_| Error::KdfParameters)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::{KdfParams, argon2id};
    use crate::Error;
    use crate::test_hex::hex;

    /// A cost small enough for a unit test.
    const CHEAP: KdfParams = KdfParams {
        memory_kib: 32,
        iterations: 3,
        lanes: 4,
    };

    /// Argon2id of the password 32 bytes of 0x01 and the salt 16 bytes of 0x02 at
    /// [`CHEAP`], 32 bytes: the inputs of RFC 9106 section 5.3 without its secret and
    /// associated data, which this API does not take. Computed outside this crate by
    /// OpenSSL's Argon2id (Python `cryptography` 46), which gives the RFC's own tag
    /// `0d640df5...e01e659` when the secret and associated data are added back.
    const EXPECTED: &str = "03aab965c12001c9d7d0d2de33192c0494b684bb148196d73c1df1acaf6d0c2e";

    #[test]
    fn the_derivation_matches_the_reference_implementation() {
        let key = argon2id::<32>(&[0x01; 32], &[0x02; 16], &CHEAP).expect("derived");
        assert_eq!(key.as_bytes().to_vec(), hex(EXPECTED));
    }

    #[test]
    fn the_same_inputs_give_the_same_key_and_another_salt_another() {
        let first = argon2id::<32>(b"pw", b"saltsaltsaltsalt", &CHEAP).expect("derived");
        let again = argon2id::<32>(b"pw", b"saltsaltsaltsalt", &CHEAP).expect("derived");
        let other = argon2id::<32>(b"pw", b"saltsaltsaltsalT", &CHEAP).expect("derived");
        assert_eq!(first.as_bytes(), again.as_bytes());
        assert_ne!(first.as_bytes(), other.as_bytes());
    }

    #[test]
    fn parameters_argon2_refuses_are_an_error() {
        let no_lane = KdfParams { lanes: 0, ..CHEAP };
        assert_eq!(
            argon2id::<32>(b"pw", b"saltsaltsaltsalt", &no_lane).err(),
            Some(Error::KdfParameters)
        );
        assert_eq!(
            argon2id::<32>(b"pw", b"short", &CHEAP).err(),
            Some(Error::KdfParameters),
            "a salt under 8 bytes"
        );
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
