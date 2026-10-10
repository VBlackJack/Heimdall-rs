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

//! UUIDs as the C# `UuidGenerator` (`Heimdall.Core/Identifiers/UuidGenerator.cs`) makes
//! and writes them: version 4, random; version 7, the time in milliseconds then random
//! bits; written as .NET's "D" (hyphens) or "N" (none) format, in lower or upper case.

use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

/// Bytes of a UUID.
pub const UUID_BYTES: usize = 16;

/// Bytes of a version 7 UUID's timestamp, its first.
const TIMESTAMP_BYTES: usize = 6;

/// The byte holding the version, in its high bits.
const VERSION_BYTE: usize = 6;

/// The byte holding the variant, in its high bits.
const VARIANT_BYTE: usize = 8;

/// What a version byte keeps of its random bits.
const VERSION_RANDOM_MASK: u8 = 0x0F;

/// What a variant byte keeps of its random bits.
const VARIANT_RANDOM_MASK: u8 = 0x3F;

/// The RFC 9562 variant, `10` in the variant byte's high bits.
const VARIANT_RFC: u8 = 0x80;

/// The version nibble of version 4, in the version byte's high bits.
const VERSION_4: u8 = 0x40;

/// The version nibble of version 7, in the version byte's high bits.
const VERSION_7: u8 = 0x70;

/// Bits in a byte.
const BYTE_BITS: u32 = 8;

/// Where the hyphens of the "D" format go: after these bytes.
const HYPHEN_AFTER: [usize; 4] = [3, 5, 7, 9];

/// Characters of the "D" format.
const HYPHENATED_LENGTH: usize = 36;

/// The hyphen of the "D" format.
const HYPHEN: char = '-';

/// Hexadecimal digits of a byte.
const DIGITS_PER_BYTE: usize = 2;

/// The base the digits are written in.
const HEX_RADIX: u32 = 16;

/// Which UUID is made, as the C# `UuidVersion`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UuidVersion {
    /// Version 4: random.
    #[default]
    V4,
    /// Version 7: the time, then random bits.
    V7,
}

/// How a UUID is written, as the C# `UuidFormat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UuidFormat {
    /// In upper case.
    pub uppercase: bool,
    /// With its four hyphens.
    pub with_hyphens: bool,
}

impl Default for UuidFormat {
    /// Lower case with hyphens, as the C# `UuidFormat.Default`.
    fn default() -> Self {
        Self {
            uppercase: false,
            with_hyphens: true,
        }
    }
}

/// A UUID, its bytes in the order it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Uuid(pub [u8; UUID_BYTES]);

/// Why no UUID could be made: the system gave no random bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the system's random number generator failed")]
pub struct NoRandomness;

/// A new UUID of `version`, as the C# `UuidGenerator.Generate` (`UuidGenerator.cs:34-39`).
///
/// # Errors
///
/// [`NoRandomness`] when the system gives no random bytes.
pub fn generate(version: UuidVersion) -> Result<Uuid, NoRandomness> {
    let mut bytes = [0_u8; UUID_BYTES];
    sealvault::random::fill(&mut bytes).map_err(|_| NoRandomness)?;
    let version_bits = match version {
        // As .NET's `Guid.NewGuid`: random but its version and variant.
        UuidVersion::V4 => VERSION_4,
        // As the C# `GenerateV7` (`UuidGenerator.cs:48-63`): the Unix time in
        // milliseconds, big-endian, in the first six bytes.
        UuidVersion::V7 => {
            let millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_millis());
            for (index, byte) in bytes.iter_mut().take(TIMESTAMP_BYTES).enumerate() {
                let shift = BYTE_BITS * u32::try_from(TIMESTAMP_BYTES - 1 - index).unwrap_or(0);
                *byte = u8::try_from((millis >> shift) & u128::from(u8::MAX)).unwrap_or(0);
            }
            VERSION_7
        }
    };
    bytes[VERSION_BYTE] = (bytes[VERSION_BYTE] & VERSION_RANDOM_MASK) | version_bits;
    bytes[VARIANT_BYTE] = (bytes[VARIANT_BYTE] & VARIANT_RANDOM_MASK) | VARIANT_RFC;
    Ok(Uuid(bytes))
}

/// `uuid` written as `format` says, as the C# `UuidGenerator.Format`
/// (`UuidGenerator.cs:41-46`): .NET's "D" with hyphens, "N" without, upper-cased when asked.
#[must_use]
pub fn format(uuid: Uuid, format: UuidFormat) -> String {
    let mut text = String::with_capacity(HYPHENATED_LENGTH);
    for (index, byte) in uuid.0.iter().enumerate() {
        // Writing to a string cannot fail.
        let _ = if format.uppercase {
            write!(text, "{byte:02X}")
        } else {
            write!(text, "{byte:02x}")
        };
        if format.with_hyphens && HYPHEN_AFTER.contains(&index) {
            text.push(HYPHEN);
        }
    }
    text
}

/// The Unix time in milliseconds a version 7 `uuid` holds, as the C# tests read it back.
#[must_use]
pub fn timestamp_millis(uuid: Uuid) -> u64 {
    uuid.0
        .iter()
        .take(TIMESTAMP_BYTES)
        .fold(0, |millis, byte| (millis << BYTE_BITS) | u64::from(*byte))
}

/// The UUID `text` writes, in either format and case; `None` when it is not one.
#[must_use]
pub fn parse(text: &str) -> Option<Uuid> {
    let digits: String = text.chars().filter(|c| *c != HYPHEN).collect();
    if digits.len() != UUID_BYTES * DIGITS_PER_BYTE || !digits.is_ascii() {
        return None;
    }
    let mut bytes = [0_u8; UUID_BYTES];
    for (byte, pair) in bytes
        .iter_mut()
        .zip(digits.as_bytes().chunks(DIGITS_PER_BYTE))
    {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, HEX_RADIX).ok()?;
    }
    Some(Uuid(bytes))
}
