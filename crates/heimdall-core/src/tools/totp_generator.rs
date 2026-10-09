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

//! Time-based one-time passwords as the C# `TotpGenerator` and `TotpParameters`
//! (`Heimdall.Core/Otp/TotpGenerator.cs`) make them, RFC 6238 over RFC 4226: an HMAC of the
//! number of time steps since 1970, cut down to a few digits; and the Base32 secret read as
//! the C# `Base32Codec` reads it (`Heimdall.Core/Otp/Base32Codec.cs`).

use super::hash_computer::HashAlgorithm;
use super::hmac_computer;

/// Seconds a code lasts, as the C# `TotpParameters.DefaultTimeStepSeconds`.
pub const DEFAULT_TIME_STEP_SECONDS: i64 = 30;

/// Digits of a code, as the C# `TotpParameters.DefaultDigits`.
pub const DEFAULT_DIGITS: u32 = 6;

/// The fewest digits, as the C# `TotpParameters.MinDigits`.
pub const MIN_DIGITS: u32 = 1;

/// The most, as the C# `TotpParameters.MaxDigits`.
pub const MAX_DIGITS: u32 = 9;

/// The HMAC's digest, as the C# `TotpParameters.DefaultAlgorithm`: SHA-1, RFC 6238's
/// baseline, which every authenticator reads.
pub const DEFAULT_ALGORITHM: HashAlgorithm = HashAlgorithm::Sha1;

/// The bits of the dynamically truncated value kept, as RFC 4226's `0x7FFFFFFF`.
const TRUNCATION_MASK: u32 = 0x7FFF_FFFF;

/// The bits of the last byte of the HMAC that give where the value is read.
const OFFSET_MASK: u8 = 0x0F;

/// The base the code is written in.
const DECIMAL: u32 = 10;

/// Why a code cannot be made, as the C#'s `ArgumentOutOfRangeException` and
/// `NotSupportedException`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TotpError {
    /// The digits are not from [`MIN_DIGITS`] to [`MAX_DIGITS`].
    #[error("digits must be from {MIN_DIGITS} to {MAX_DIGITS}")]
    Digits,
    /// The time step is not positive.
    #[error("the time step must be positive")]
    TimeStep,
    /// The digest is not SHA-1, SHA-256 or SHA-512.
    #[error("TOTP takes SHA-1, SHA-256 or SHA-512")]
    Algorithm,
}

/// The code of `secret` at `unix_seconds`, as the C# `TotpGenerator.Generate`
/// (`TotpGenerator.cs:30-63`): the counter `unix_seconds / step` in eight big-endian bytes,
/// its HMAC by `algorithm`, four bytes read at the offset its last byte gives, their 31 low
/// bits taken modulo 10 to the `digits`, written with its leading zeros.
///
/// # Errors
///
/// [`TotpError`] for digits out of range, a step not positive, or a digest TOTP does not
/// take.
pub fn generate(
    secret: &[u8],
    unix_seconds: i64,
    algorithm: HashAlgorithm,
    digits: u32,
    step: i64,
) -> Result<String, TotpError> {
    if !(MIN_DIGITS..=MAX_DIGITS).contains(&digits) {
        return Err(TotpError::Digits);
    }
    if step <= 0 {
        return Err(TotpError::TimeStep);
    }
    if !matches!(
        algorithm,
        HashAlgorithm::Sha1 | HashAlgorithm::Sha256 | HashAlgorithm::Sha512
    ) {
        return Err(TotpError::Algorithm);
    }
    let counter = unix_seconds / step;
    let hash = hmac_computer::compute(algorithm, secret, &counter.to_be_bytes())
        .map_err(|_| TotpError::Algorithm)?;
    let offset = usize::from(hash[hash.len() - 1] & OFFSET_MASK);
    let binary = u32::from_be_bytes([
        hash[offset],
        hash[offset + 1],
        hash[offset + 2],
        hash[offset + 3],
    ]) & TRUNCATION_MASK;
    let code = binary % DECIMAL.pow(digits);
    let width = usize::try_from(digits).unwrap_or(usize::MAX);
    Ok(format!("{code:0width$}"))
}

/// Seconds gone in the step `unix_seconds` falls in, as the C# `TotpGenerator.ElapsedInStep`.
///
/// # Errors
///
/// [`TotpError::TimeStep`] for a step not positive.
pub const fn elapsed_in_step(unix_seconds: i64, step: i64) -> Result<i64, TotpError> {
    if step <= 0 {
        return Err(TotpError::TimeStep);
    }
    Ok(unix_seconds % step)
}

/// Seconds left in it, as the C# `TotpGenerator.RemainingInStep`: a whole step at its start.
///
/// # Errors
///
/// [`TotpError::TimeStep`] for a step not positive.
pub const fn remaining_in_step(unix_seconds: i64, step: i64) -> Result<i64, TotpError> {
    if step <= 0 {
        return Err(TotpError::TimeStep);
    }
    Ok(step - unix_seconds % step)
}

/// A character that is not Base32, as the C# `FormatException` "Invalid Base32 character".
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("Invalid Base32 character: {0}")]
pub struct InvalidBase32(pub char);

/// Bits a Base32 symbol carries.
const SYMBOL_BITS: u32 = 5;

/// Bits of a byte.
const BYTE_BITS: u32 = 8;

/// The bits kept between symbols: never more than a byte and a symbol.
const BUFFER_MASK: u32 = 0xFFFF;

/// The value of the first digit symbol, `2`, in the alphabet.
const DIGITS_FIRST_VALUE: u32 = 26;

/// The padding character.
const PADDING: char = '=';

/// The bytes `text` stands for in Base32, as the C# `Base32Codec.Decode`
/// (`Base32Codec.cs:21-52`): the padding at the end dropped, either case read, the bits
/// past the last whole byte left; nothing for an empty text.
///
/// # Errors
///
/// [`InvalidBase32`] naming the first character outside `A`-`Z`, `a`-`z` and `2`-`7`.
pub fn decode_base32(text: &str) -> Result<Vec<u8>, InvalidBase32> {
    let text = text.trim_end_matches(PADDING);
    let mut output = Vec::new();
    let mut buffer: u32 = 0;
    let mut held: u32 = 0;
    for symbol in text.chars() {
        let value = symbol_value(symbol).ok_or(InvalidBase32(symbol))?;
        buffer = ((buffer << SYMBOL_BITS) | value) & BUFFER_MASK;
        held += SYMBOL_BITS;
        if held >= BYTE_BITS {
            held -= BYTE_BITS;
            // The byte is the eight bits above those still held.
            output.push((buffer >> held).to_le_bytes()[0]);
        }
    }
    Ok(output)
}

/// The value of a Base32 symbol, as the C# `CharToValue`.
fn symbol_value(symbol: char) -> Option<u32> {
    match symbol {
        'A'..='Z' => Some(u32::from(symbol) - u32::from('A')),
        'a'..='z' => Some(u32::from(symbol) - u32::from('a')),
        '2'..='7' => Some(u32::from(symbol) - u32::from('2') + DIGITS_FIRST_VALUE),
        _ => None,
    }
}
