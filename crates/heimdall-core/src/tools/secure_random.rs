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

//! Uniform draws from the system's generator, as the C# `RandomNumberGenerator.GetInt32`
//! the password generator draws every character with: the system's CSPRNG only, and an
//! unbiased draw, a value past the last whole multiple of the range being drawn again
//! rather than folded onto the first ones.

use std::fmt;

use zeroize::Zeroizing;

/// Bytes read from the system's generator at a time: a password of 128 characters drawn
/// ten thousand times is a million draws, which would otherwise be a million system calls.
const POOL_BYTES: usize = 256;

/// Bytes one draw takes.
const DRAW_BYTES: usize = 4;

/// The system's generator, read through a pool wiped when it is dropped.
pub struct SecureRandom {
    pool: Zeroizing<[u8; POOL_BYTES]>,
    /// The next unread byte of the pool; [`POOL_BYTES`] when it is spent.
    next: usize,
}

impl fmt::Debug for SecureRandom {
    /// What is drawn is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecureRandom(..)")
    }
}

impl Default for SecureRandom {
    fn default() -> Self {
        Self::new()
    }
}

impl SecureRandom {
    /// A generator whose pool is filled on its first draw.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pool: Zeroizing::new([0; POOL_BYTES]),
            next: POOL_BYTES,
        }
    }

    /// Four bytes of the system's generator; `None` when it cannot be read.
    fn next_u32(&mut self) -> Option<u32> {
        if self.next + DRAW_BYTES > POOL_BYTES {
            getrandom::fill(self.pool.as_mut()).ok()?;
            self.next = 0;
        }
        let mut bytes = [0; DRAW_BYTES];
        bytes.copy_from_slice(&self.pool[self.next..self.next + DRAW_BYTES]);
        self.pool[self.next..self.next + DRAW_BYTES].fill(0);
        self.next += DRAW_BYTES;
        Some(u32::from_le_bytes(bytes))
    }

    /// A value in `0..bound`, each one as likely as the others, as the C#
    /// `RandomNumberGenerator.GetInt32(bound)`: a draw past the last whole multiple of
    /// `bound` is drawn again. `None` when `bound` is zero, past `u32::MAX`, or the system's
    /// generator cannot be read.
    pub fn below(&mut self, bound: usize) -> Option<usize> {
        let bound = u64::try_from(bound).ok().filter(|bound| *bound > 0)?;
        let span = 1_u64 << 32;
        if bound > span {
            return None;
        }
        // The draws at or past `zone` would favour the first values: they are refused.
        let zone = span - span % bound;
        loop {
            let drawn = u64::from(self.next_u32()?);
            if drawn < zone {
                return usize::try_from(drawn % bound).ok();
            }
        }
    }

    /// One element of `items`, each as likely; `None` when there is none.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        let index = self.below(items.len())?;
        items.get(index)
    }

    /// `bytes` filled from the system's generator; `false` when it cannot be read.
    pub fn fill(&mut self, bytes: &mut [u8]) -> bool {
        getrandom::fill(bytes).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draw_stays_in_its_range_and_an_empty_range_has_none() {
        let mut random = SecureRandom::new();
        for bound in [1, 2, 3, 7, 10, 26, 94, 1000] {
            for _ in 0..200 {
                assert!(random.below(bound).expect("drawn") < bound);
            }
        }
        assert_eq!(random.below(0), None);
        assert_eq!(random.below(1), Some(0));
        assert!(random.pick::<u8>(&[]).is_none());
    }

    #[test]
    fn draws_are_spread_evenly_over_their_range() {
        // A chi-squared test over ten values and two hundred thousand draws: with nine
        // degrees of freedom, 27.88 is passed one time in a thousand by a fair source.
        const DRAWS: usize = 200_000;
        const VALUES: usize = 10;
        const CRITICAL: f64 = 27.88;
        let mut random = SecureRandom::new();
        let mut counts = [0_u32; VALUES];
        for _ in 0..DRAWS {
            counts[random.below(VALUES).expect("drawn")] += 1;
        }
        let expected = f64::from(u32::try_from(DRAWS / VALUES).expect("small"));
        let chi: f64 = counts
            .iter()
            .map(|count| (f64::from(*count) - expected).powi(2) / expected)
            .sum();
        assert!(chi < CRITICAL, "chi-squared {chi} over {counts:?}");
    }

    #[test]
    fn a_range_that_does_not_divide_the_draw_is_not_biased_toward_its_start() {
        // Three does not divide 2^32: a modulo without refusal favours 0. Over three hundred
        // thousand draws each value has to stay within a fraction of a percent of a third.
        const DRAWS: u32 = 300_000;
        let mut random = SecureRandom::new();
        let mut counts = [0_u32; 3];
        for _ in 0..DRAWS {
            counts[random.below(3).expect("drawn")] += 1;
        }
        for count in counts {
            let share = f64::from(count) / f64::from(DRAWS);
            assert!((share - 1.0 / 3.0).abs() < 0.005, "{counts:?}");
        }
    }

    #[test]
    fn its_pool_is_never_written_out() {
        let mut random = SecureRandom::new();
        let _ = random.below(10);
        assert_eq!(format!("{random:?}"), "SecureRandom(..)");
    }
}
