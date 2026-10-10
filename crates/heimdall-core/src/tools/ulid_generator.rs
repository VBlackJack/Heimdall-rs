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

//! ULIDs as the C# `UlidGenerator` (`Heimdall.Core/Identifiers/UlidGenerator.cs`) makes
//! them: the Unix time in milliseconds on 48 bits, then 80 random bits, written in 26
//! characters of Crockford's base 32. Not monotonic: two ULIDs of the same millisecond are
//! ordered by their random bits, as the C#'s.

use std::time::{SystemTime, UNIX_EPOCH};

use super::uuid_generator::NoRandomness;

/// Characters of a ULID, as the C# `TextLength`.
pub const TEXT_LENGTH: usize = 26;

/// Random bytes of a ULID, as the C# `RandomByteCount`.
pub const RANDOM_BYTE_COUNT: usize = 10;

/// Crockford's base 32: no I, L, O or U, as the C# `Alphabet`.
pub const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The latest time a ULID holds, 48 bits of milliseconds, as the C# `MaxTimestamp`.
pub const MAX_TIMESTAMP: u64 = 0xFFFF_FFFF_FFFF;

/// Characters of the time.
const TIMESTAMP_CHARS: usize = 10;

/// Characters of each half of the random bits.
const HALF_CHARS: usize = 8;

/// Bytes of each half of the random bits: 40 bits.
const HALF_BYTES: usize = 5;

/// Bits a character writes.
const BITS_PER_CHAR: u32 = 5;

/// What a character keeps of its bits.
const CHAR_MASK: u64 = 0x1F;

/// Bits in a byte.
const BYTE_BITS: u32 = 8;

/// Why a ULID could not be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UlidError {
    /// The time is past what 48 bits hold, as the C# `ArgumentOutOfRangeException`.
    #[error("the time is out of a ULID's range")]
    TimestampOutOfRange,
}

/// A new ULID, as the C# `UlidGenerator.Generate` (`UlidGenerator.cs:28-32`): this
/// moment's Unix time in milliseconds and ten random bytes.
///
/// # Errors
///
/// [`NoRandomness`] when the system gives no random bytes.
pub fn generate() -> Result<String, NoRandomness> {
    let mut random = [0_u8; RANDOM_BYTE_COUNT];
    sealvault::random::fill(&mut random).map_err(|_| NoRandomness)?;
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        });
    // A clock past the year 10889 is the only way out of range: written as the latest.
    Ok(encode(millis.min(MAX_TIMESTAMP), &random).unwrap_or_default())
}

/// The ULID of `timestamp_ms` and `random`, as the C# `Encode` (`UlidGenerator.cs:34-75`):
/// the time in its first ten characters, each half of the random bits in eight.
///
/// # Errors
///
/// [`UlidError::TimestampOutOfRange`] past [`MAX_TIMESTAMP`].
pub fn encode(timestamp_ms: u64, random: &[u8; RANDOM_BYTE_COUNT]) -> Result<String, UlidError> {
    if timestamp_ms > MAX_TIMESTAMP {
        return Err(UlidError::TimestampOutOfRange);
    }
    let mut text = String::with_capacity(TEXT_LENGTH);
    push_chars(&mut text, timestamp_ms, TIMESTAMP_CHARS);
    let (upper, lower) = random.split_at(HALF_BYTES);
    push_chars(&mut text, half(upper), HALF_CHARS);
    push_chars(&mut text, half(lower), HALF_CHARS);
    Ok(text)
}

/// The time in milliseconds the first ten characters of `ulid` hold, as the C# tests read
/// it back; `None` for a character out of the alphabet.
#[must_use]
pub fn timestamp_ms(ulid: &str) -> Option<u64> {
    ulid.bytes()
        .take(TIMESTAMP_CHARS)
        .try_fold(0_u64, |value, byte| {
            let digit = ALPHABET.iter().position(|known| *known == byte)?;
            Some((value << BITS_PER_CHAR) | u64::try_from(digit).ok()?)
        })
}

/// Five bytes as a 40-bit number, big-endian.
fn half(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0, |value, byte| (value << BYTE_BITS) | u64::from(*byte))
}

/// The last `count` characters of `value` in base 32, the most significant first.
fn push_chars(text: &mut String, value: u64, count: usize) {
    for index in (0..count).rev() {
        let shift = BITS_PER_CHAR * u32::try_from(index).unwrap_or(0);
        let digit = usize::try_from((value >> shift) & CHAR_MASK).unwrap_or(0);
        text.push(char::from(ALPHABET[digit]));
    }
}
