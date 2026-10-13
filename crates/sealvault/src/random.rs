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

//! Random bytes from the operating system's generator, the only source this crate uses.

use crate::Error;
use crate::secret::SecretKey;

/// `buffer` filled from the operating system's generator.
///
/// # Errors
///
/// [`Error::Randomness`] when the system gives no random bytes.
pub fn fill(buffer: &mut [u8]) -> Result<(), Error> {
    getrandom::fill(buffer).map_err(|error| Error::Randomness(error.to_string()))
}

/// `N` random bytes, for values that are not secret: a salt, a nonce, a token compared
/// once.
///
/// # Errors
///
/// [`Error::Randomness`].
pub fn array<const N: usize>() -> Result<[u8; N], Error> {
    let mut bytes = [0; N];
    fill(&mut bytes)?;
    Ok(bytes)
}

/// A random key of `N` bytes, written straight into its wiped home on the heap.
///
/// # Errors
///
/// [`Error::Randomness`].
pub fn secret_key<const N: usize>() -> Result<SecretKey<N>, Error> {
    let mut key = SecretKey::zeroed();
    fill(key.as_mut_bytes())?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::{array, fill, secret_key};

    /// Bytes drawn in each test: two equal draws of this size have odds of 2^-256.
    const DRAWN: usize = 32;

    #[test]
    fn two_draws_differ_and_are_not_all_zeros() {
        let first = array::<DRAWN>().expect("random");
        let second = array::<DRAWN>().expect("random");
        assert_ne!(first, second);
        assert_ne!(first, [0; DRAWN]);
    }

    #[test]
    fn a_secret_key_is_filled() {
        let key = secret_key::<DRAWN>().expect("random");
        assert_ne!(key.as_bytes(), &[0; DRAWN]);
    }

    #[test]
    fn an_empty_buffer_is_fine() {
        fill(&mut []).expect("nothing to fill");
    }
}
