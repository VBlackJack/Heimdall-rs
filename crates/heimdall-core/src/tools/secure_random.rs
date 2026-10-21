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
            sealvault::random::fill(self.pool.as_mut()).ok()?;
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
        below_from(bound, || self.next_u32())
    }

    /// One element of `items`, each as likely; `None` when there is none.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        let index = self.below(items.len())?;
        items.get(index)
    }

    /// `bytes` filled from the system's generator; `false` when it cannot be read.
    pub fn fill(&mut self, bytes: &mut [u8]) -> bool {
        sealvault::random::fill(bytes).is_ok()
    }
}

/// A value in `0..bound` from `next_u32`, refusing the draws past the last whole multiple
/// of `bound`. Split from [`SecureRandom::below`] so the refusal can be tested with a
/// scripted source instead of by statistics on the system's generator.
fn below_from(bound: usize, mut next_u32: impl FnMut() -> Option<u32>) -> Option<usize> {
    let bound = u64::try_from(bound).ok().filter(|bound| *bound > 0)?;
    let span = 1_u64 << 32;
    if bound > span {
        return None;
    }
    // The draws at or past `zone` would favour the first values: they are refused.
    let zone = span - span % bound;
    loop {
        let drawn = u64::from(next_u32()?);
        if drawn < zone {
            return usize::try_from(drawn % bound).ok();
        }
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

    /// A fixed-seed xorshift32: the same sequence on every run, so a statistical check over
    /// it passes or fails for good instead of by chance.
    fn xorshift(mut state: u32) -> impl FnMut() -> Option<u32> {
        move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            Some(state)
        }
    }

    #[test]
    fn draws_are_spread_evenly_over_their_range() {
        // A chi-squared test over ten values and two hundred thousand draws of a fixed-seed
        // source: nine degrees of freedom, 27.88 being the one-in-a-thousand critical value
        // for a fair source. The sequence is fixed, so the outcome never varies between runs.
        const DRAWS: usize = 200_000;
        const VALUES: usize = 10;
        const CRITICAL: f64 = 27.88;
        const SEED: u32 = 0x9E37_79B9;
        let mut source = xorshift(SEED);
        let mut counts = [0_u32; VALUES];
        for _ in 0..DRAWS {
            counts[below_from(VALUES, &mut source).expect("drawn")] += 1;
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
        // Three does not divide 2^32: its zone ends at 2^32 - 1, so the single draw
        // 0xFFFF_FFFF, which a bare modulo would fold onto 0, is refused and the next one used.
        let mut script = [u32::MAX, 0, 1, 2, 3, u32::MAX - 1].into_iter();
        let mut source = || script.next();
        let drawn: Vec<usize> = (0..5)
            .map(|_| below_from(3, &mut source).expect("drawn"))
            .collect();
        assert_eq!(drawn, [0, 1, 2, 0, 2]);
        // Nothing is refused when the range divides the draw: every value is kept as is.
        let mut script = [u32::MAX, 0].into_iter();
        assert_eq!(below_from(2, || script.next()), Some(1));
        assert_eq!(below_from(2, || script.next()), Some(0));
    }

    #[test]
    fn a_source_that_cannot_be_read_ends_the_draw() {
        assert_eq!(below_from(3, || None), None);
        assert_eq!(below_from(0, || Some(1)), None);
        assert_eq!(below_from(1 << 33, || Some(1)), None);
    }

    #[test]
    fn production_draws_come_from_the_system_generator() {
        // No statistics: two independent 32-byte reads of the system's generator equal each
        // other with probability 2^-256, and an all-zero pool (never filled) would be caught.
        let mut first = [0_u8; 32];
        let mut second = [0_u8; 32];
        let mut random = SecureRandom::new();
        assert!(random.fill(&mut first));
        assert!(random.fill(&mut second));
        assert_ne!(first, second);
        assert_ne!(first, [0_u8; 32]);
        // `below` reads the pool from the same source: a fresh generator draws 64 times a
        // range of 2^32 and does not return one constant (2^-1984 for a fair source).
        let draws: Vec<usize> = (0..64)
            .map(|_| random.below(1 << 32).expect("drawn"))
            .collect();
        assert!(draws.iter().any(|draw| *draw != draws[0]));
    }

    #[test]
    fn its_pool_is_never_written_out() {
        let mut random = SecureRandom::new();
        let _ = random.below(10);
        assert_eq!(format!("{random:?}"), "SecureRandom(..)");
    }
}
