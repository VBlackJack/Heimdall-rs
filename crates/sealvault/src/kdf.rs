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

//! Derived keys: from passwords by Argon2id (RFC 9106), version 0x13; from secrets that
//! are already random (a signature, a shared secret) by HKDF-SHA256 (RFC 5869).

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
    /// The least any derivation may cost: 19 MiB, 2 passes, 1 lane, OWASP's minimum for
    /// Argon2id (Password Storage Cheat Sheet) and the argon2 crate's default, which the
    /// application PIN's hash uses. [`argon2id`] refuses anything cheaper.
    pub const MINIMUM: Self = Self {
        memory_kib: 19 * 1024,
        iterations: 2,
        lanes: 1,
    };

    /// The least a vault may ask for: 64 MiB, 3 passes, 1 lane. What a new vault gets.
    pub const FLOOR: Self = Self {
        memory_kib: 64 * 1024,
        iterations: 3,
        lanes: 1,
    };

    /// The most any derivation may cost, and so the most a vault may ask for: beyond, a file
    /// or a caller could make it exhaust the machine. [`argon2id`] refuses anything dearer.
    pub const CEILING: Self = Self {
        memory_kib: 4 * 1024 * 1024,
        iterations: 64,
        lanes: 16,
    };

    /// Whether each value lies between [`KdfParams::FLOOR`] and [`KdfParams::CEILING`]: the
    /// vault's own, stricter, bounds.
    #[must_use]
    pub fn is_acceptable(&self) -> bool {
        self.lies_between(&Self::FLOOR, &Self::CEILING)
    }

    /// Whether each value lies between [`KdfParams::MINIMUM`] and [`KdfParams::CEILING`]:
    /// what [`argon2id`] accepts.
    #[must_use]
    pub fn is_within_bounds(&self) -> bool {
        self.lies_between(&Self::MINIMUM, &Self::CEILING)
    }

    fn lies_between(&self, low: &Self, high: &Self) -> bool {
        (low.memory_kib..=high.memory_kib).contains(&self.memory_kib)
            && (low.iterations..=high.iterations).contains(&self.iterations)
            && (low.lanes..=high.lanes).contains(&self.lanes)
    }
}

/// The key of `N` bytes that `password` gives under `params` and `salt`, by Argon2id
/// version 0x13 with neither secret nor associated data. A cost outside
/// [`KdfParams::MINIMUM`] and [`KdfParams::CEILING`] is refused before anything is
/// allocated; a vault checks its stricter [`KdfParams::is_acceptable`] first.
///
/// # Errors
///
/// [`Error::KdfParameters`] for a cost out of bounds, or when Argon2id refuses `params`,
/// the salt or the length.
pub fn argon2id<const N: usize>(
    password: &[u8],
    salt: &[u8],
    params: &KdfParams,
) -> Result<SecretKey<N>, Error> {
    if !params.is_within_bounds() {
        return Err(Error::KdfParameters);
    }
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

/// Bytes HKDF-SHA256 can give at most: 255 blocks of SHA-256.
pub const HKDF_SHA256_MAX_LEN: usize = 255 * crate::hash::SHA256_LEN;

/// The key of `N` bytes HKDF-SHA256 (RFC 5869) gives from the input keying material `ikm`,
/// `salt` and `info`: extract, then expand. For a secret that is already random; a
/// password goes through [`argon2id`] instead.
///
/// # Errors
///
/// [`Error::KeyLength`] when `N` is past [`HKDF_SHA256_MAX_LEN`].
pub fn hkdf_sha256<const N: usize>(
    ikm: &[u8],
    salt: &[u8],
    info: &[u8],
) -> Result<SecretKey<N>, Error> {
    let mut key = SecretKey::zeroed();
    hkdf::Hkdf::<sha2::Sha256>::new(Some(salt), ikm)
        .expand(info, key.as_mut_bytes())
        .map_err(|_| Error::KeyLength {
            expected: HKDF_SHA256_MAX_LEN,
            actual: N,
        })?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::{HKDF_SHA256_MAX_LEN, KdfParams, argon2id, hkdf_sha256};
    use crate::Error;
    use crate::test_hex::hex;

    /// Salt of the tests that need any valid one.
    const SALT: &[u8] = b"saltsaltsaltsalt";

    /// Argon2id of the password 32 bytes of 0x01 and the salt 16 bytes of 0x02 (the inputs
    /// of RFC 9106 section 5.3, without its secret and associated data, which this API does
    /// not take) at [`KdfParams::MINIMUM`], 32 bytes. Computed outside this crate by
    /// OpenSSL's Argon2id (Python `cryptography` 46), which also gives the RFC's own tag
    /// `0d640df5...e01e659` on the RFC's full inputs and cost.
    const EXPECTED: &str = "551d2b516a3d92963b2cd1e8fdc1725129e15824dfb6c8d9bb8a599ffcabfc1c";

    #[test]
    fn the_derivation_matches_the_reference_implementation() {
        let key = argon2id::<32>(&[0x01; 32], &[0x02; 16], &KdfParams::MINIMUM).expect("derived");
        assert_eq!(key.as_bytes().to_vec(), hex(EXPECTED));
    }

    #[test]
    fn the_same_inputs_give_the_same_key_and_another_salt_another() {
        let first = argon2id::<32>(b"pw", SALT, &KdfParams::MINIMUM).expect("derived");
        let again = argon2id::<32>(b"pw", SALT, &KdfParams::MINIMUM).expect("derived");
        let other =
            argon2id::<32>(b"pw", b"saltsaltsaltsalT", &KdfParams::MINIMUM).expect("derived");
        assert_eq!(first.as_bytes(), again.as_bytes());
        assert_ne!(first.as_bytes(), other.as_bytes());
    }

    #[test]
    fn a_cost_below_the_minimum_or_above_the_ceiling_is_refused_before_deriving() {
        let refused = |change: fn(&mut KdfParams), from: KdfParams| {
            let mut params = from;
            change(&mut params);
            argon2id::<32>(b"pw", SALT, &params).err() == Some(Error::KdfParameters)
        };
        assert!(refused(|p| p.memory_kib -= 1, KdfParams::MINIMUM));
        assert!(refused(|p| p.iterations -= 1, KdfParams::MINIMUM));
        assert!(refused(|p| p.lanes = 0, KdfParams::MINIMUM));
        // Above the ceiling: refused at once, nothing is allocated.
        assert!(refused(|p| p.memory_kib += 1, KdfParams::CEILING));
        assert!(refused(|p| p.iterations += 1, KdfParams::CEILING));
        assert!(refused(|p| p.lanes += 1, KdfParams::CEILING));
        assert!(refused(|p| p.memory_kib = u32::MAX, KdfParams::MINIMUM));
    }

    #[test]
    fn a_salt_argon2_refuses_is_an_error() {
        assert_eq!(
            argon2id::<32>(b"pw", b"short", &KdfParams::MINIMUM).err(),
            Some(Error::KdfParameters),
            "a salt under 8 bytes"
        );
    }

    /// RFC 5869 appendix A.1: the basic test case with SHA-256.
    #[test]
    fn hkdf_meets_rfc_5869_test_case_1() {
        let salt: Vec<u8> = (0x00..=0x0c).collect();
        let info: Vec<u8> = (0xf0..=0xf9).collect();
        let okm = hkdf_sha256::<42>(&[0x0b; 22], &salt, &info).expect("derived");
        assert_eq!(
            okm.as_bytes().to_vec(),
            hex(
                "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf\
                 34007208d5b887185865"
            )
        );
    }

    /// RFC 5869 appendix A.3: an empty salt and an empty info.
    #[test]
    fn hkdf_meets_rfc_5869_test_case_3() {
        let okm = hkdf_sha256::<42>(&[0x0b; 22], &[], &[]).expect("derived");
        assert_eq!(
            okm.as_bytes().to_vec(),
            hex(
                "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d\
                 9d201395faa4b61a96c8"
            )
        );
    }

    #[test]
    fn hkdf_refuses_more_than_255_blocks() {
        assert!(hkdf_sha256::<HKDF_SHA256_MAX_LEN>(b"ikm", b"salt", b"info").is_ok());
        assert_eq!(
            hkdf_sha256::<{ HKDF_SHA256_MAX_LEN + 1 }>(b"ikm", b"salt", b"info").err(),
            Some(Error::KeyLength {
                expected: HKDF_SHA256_MAX_LEN,
                actual: HKDF_SHA256_MAX_LEN + 1
            })
        );
    }

    #[test]
    fn the_bounds_nest_the_minimum_under_the_vault_floor() {
        assert!(KdfParams::MINIMUM.is_within_bounds());
        assert!(!KdfParams::MINIMUM.is_acceptable(), "too cheap for a vault");
        assert!(KdfParams::FLOOR.is_within_bounds());
        assert!(KdfParams::CEILING.is_within_bounds());
        // The argon2 crate's default cost, the application PIN's, stays derivable.
        let pin = KdfParams {
            memory_kib: 19_456,
            iterations: 2,
            lanes: 1,
        };
        assert!(pin.is_within_bounds());
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
