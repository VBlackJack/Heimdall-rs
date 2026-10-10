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

//! Owned secrets: keys, passwords, derived keys and plaintext, each wiped when dropped.
//!
//! Neither type shows its content through `Debug`, and neither is `Clone`: a copy is made
//! on purpose, with [`SecretBytes::from_slice`] or [`SecretKey::from_array`], never by
//! accident.

use std::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::Error;

/// Secret bytes of any length, wiped when dropped.
#[derive(Default, Zeroize)]
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    /// Takes `bytes` over; they are wiped when the secret is dropped.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// A copy of `bytes`, sized once so that no reallocation leaves a copy behind.
    #[must_use]
    pub fn from_slice(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }

    /// The secret's bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// How many bytes the secret holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the secret holds no byte.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for SecretBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self::new(bytes)
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes(..)")
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.zeroize();
        #[cfg(test)]
        drop_probe::record(self.0.is_empty());
    }
}

impl ZeroizeOnDrop for SecretBytes {}

/// A secret of exactly `N` bytes, such as a key. On the heap, so that moving it copies only
/// the pointer, and wiped when dropped.
#[derive(Zeroize)]
pub struct SecretKey<const N: usize>(Box<[u8; N]>);

impl<const N: usize> SecretKey<N> {
    /// A key of zeros, to be written in place.
    #[must_use]
    pub(crate) fn zeroed() -> Self {
        Self(Box::new([0; N]))
    }

    /// A copy of `bytes`.
    #[must_use]
    pub fn from_array(bytes: &[u8; N]) -> Self {
        let mut key = Self::zeroed();
        key.0.copy_from_slice(bytes);
        key
    }

    /// A copy of `bytes`, which must be `N` long.
    ///
    /// # Errors
    ///
    /// [`Error::KeyLength`] when it is not.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != N {
            return Err(Error::KeyLength {
                expected: N,
                actual: bytes.len(),
            });
        }
        let mut key = Self::zeroed();
        key.0.copy_from_slice(bytes);
        Ok(key)
    }

    /// The key's bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; N] {
        &self.0
    }

    /// The key's bytes, to be written in place.
    pub(crate) fn as_mut_bytes(&mut self) -> &mut [u8; N] {
        &mut self.0
    }
}

impl<const N: usize> fmt::Debug for SecretKey<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretKey<{N}>(..)")
    }
}

impl<const N: usize> Drop for SecretKey<N> {
    fn drop(&mut self) {
        self.zeroize();
        #[cfg(test)]
        drop_probe::record(self.0.iter().all(|byte| *byte == 0));
    }
}

impl<const N: usize> ZeroizeOnDrop for SecretKey<N> {}

/// What the last secret dropped on this thread looked like once wiped: seen from inside
/// `drop`, so the tests check the wiping without reading freed memory.
#[cfg(test)]
pub(crate) mod drop_probe {
    use std::cell::Cell;

    thread_local! {
        static WIPED: Cell<Option<bool>> = const { Cell::new(None) };
    }

    pub fn record(wiped: bool) {
        WIPED.with(|cell| cell.set(Some(wiped)));
    }

    /// Whether the last secret dropped was wiped; `None` when none was dropped since.
    pub fn take() -> Option<bool> {
        WIPED.with(Cell::take)
    }
}

#[cfg(test)]
mod tests {
    use zeroize::ZeroizeOnDrop;

    use super::{SecretBytes, SecretKey, drop_probe};
    use crate::Error;

    fn wiped_on_drop<T: ZeroizeOnDrop>() {}

    #[test]
    fn both_secret_types_promise_to_be_wiped_on_drop() {
        wiped_on_drop::<SecretBytes>();
        wiped_on_drop::<SecretKey<32>>();
    }

    #[test]
    fn a_key_is_all_zeros_when_its_drop_ends() {
        drop_probe::take();
        drop(SecretKey::from_array(&[0xa5; 32]));
        assert_eq!(drop_probe::take(), Some(true));
    }

    #[test]
    fn secret_bytes_are_wiped_and_emptied_when_dropped() {
        drop_probe::take();
        drop(SecretBytes::from_slice(b"hunter2"));
        assert_eq!(drop_probe::take(), Some(true));
    }

    #[test]
    fn debug_shows_no_content() {
        let key = SecretKey::from_array(&[0x41; 4]);
        assert_eq!(format!("{key:?}"), "SecretKey<4>(..)");
        let bytes = SecretBytes::from_slice(b"hunter2");
        let shown = format!("{bytes:?}");
        assert_eq!(shown, "SecretBytes(..)");
        assert!(!shown.contains("hunter2"));
    }

    #[test]
    fn a_key_of_the_wrong_length_is_refused() {
        assert_eq!(
            SecretKey::<32>::from_slice(&[0; 31]).err(),
            Some(Error::KeyLength {
                expected: 32,
                actual: 31
            })
        );
        let key = SecretKey::<4>::from_slice(&[1, 2, 3, 4]).expect("four bytes");
        assert_eq!(key.as_bytes(), &[1, 2, 3, 4]);
    }
}
