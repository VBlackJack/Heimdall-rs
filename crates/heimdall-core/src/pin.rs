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

//! The application PIN, as the C# Heimdall's: 4 to 8 digits asked before anything else at
//! start. Only a hash of it is kept, salted, by Argon2id where the C# one uses PBKDF2.
//!
//! The PIN keeps a person at the keyboard out of the application; it encrypts nothing. The
//! passwords are the vault's to protect.

use data_encoding::BASE64;
use zeroize::Zeroizing;

/// Fewest digits a PIN has, as the C# rule.
pub const MIN_PIN_DIGITS: usize = 4;
/// Most digits a PIN has, as the C# rule.
pub const MAX_PIN_DIGITS: usize = 8;
/// Bytes of random salt, as the C# one.
const SALT_BYTES: usize = 16;
/// Bytes of hash kept.
const HASH_BYTES: usize = 32;

/// Why a new PIN is refused, checked in the C# order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinProblem {
    /// Fewer than [`MIN_PIN_DIGITS`] characters.
    TooShort,
    /// More than [`MAX_PIN_DIGITS`] characters.
    TooLong,
    /// A character that is not an ASCII digit.
    NotDigits,
}

/// Why a new PIN is refused; `None` when it is taken.
#[must_use]
pub fn pin_problem(pin: &str) -> Option<PinProblem> {
    let length = pin.chars().count();
    if length < MIN_PIN_DIGITS {
        Some(PinProblem::TooShort)
    } else if length > MAX_PIN_DIGITS {
        Some(PinProblem::TooLong)
    } else if !pin.chars().all(|c| c.is_ascii_digit()) {
        Some(PinProblem::NotDigits)
    } else {
        None
    }
}

/// A PIN as kept: its salt and its hash, written in Base64.
#[derive(Clone, PartialEq, Eq)]
pub struct PinHash {
    salt: String,
    hash: String,
}

impl std::fmt::Debug for PinHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PinHash(..)")
    }
}

impl PinHash {
    /// Hashes `pin` with a new salt.
    ///
    /// # Errors
    ///
    /// Why no random salt could be drawn, or the hash made.
    pub fn new(pin: &str) -> Result<Self, String> {
        let mut salt = [0; SALT_BYTES];
        getrandom::fill(&mut salt).map_err(|error| error.to_string())?;
        let hash = derive(pin, &salt)?;
        Ok(Self {
            salt: BASE64.encode(&salt),
            hash: BASE64.encode(hash.as_slice()),
        })
    }

    /// The PIN kept as `salt` and `hash`, as the settings file holds them. What is not
    /// Base64 of the right length is kept as it is: no PIN is then taken, so a damaged
    /// file keeps the application closed rather than open.
    #[must_use]
    pub fn saved(salt: String, hash: String) -> Self {
        Self { salt, hash }
    }

    /// The salt, as the settings file holds it.
    #[must_use]
    pub fn salt(&self) -> &str {
        &self.salt
    }

    /// The hash, as the settings file holds it.
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Whether `pin` is the PIN kept, compared in constant time.
    #[must_use]
    pub fn verify(&self, pin: &str) -> bool {
        let (Ok(salt), Ok(kept)) = (
            BASE64.decode(self.salt.as_bytes()),
            BASE64.decode(self.hash.as_bytes()),
        ) else {
            return false;
        };
        // Compared whole: a shorter hash kept would match on its length alone. The salt's
        // length proves nothing: a hash matches only the salt it was made with.
        if kept.len() != HASH_BYTES {
            return false;
        }
        derive(pin, &salt).is_ok_and(|typed| {
            typed
                .iter()
                .zip(&kept)
                .fold(0_u8, |differ, (a, b)| differ | (a ^ b))
                == 0
        })
    }
}

/// The hash of `pin` with `salt`, by Argon2id at the crate's default cost.
fn derive(pin: &str, salt: &[u8]) -> Result<Zeroizing<[u8; HASH_BYTES]>, String> {
    let mut hash = Zeroizing::new([0; HASH_BYTES]);
    argon2::Argon2::default()
        .hash_password_into(pin.as_bytes(), salt, hash.as_mut_slice())
        .map_err(|error| error.to_string())?;
    Ok(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pin_is_four_to_eight_digits() {
        assert_eq!(pin_problem(""), Some(PinProblem::TooShort));
        assert_eq!(pin_problem("123"), Some(PinProblem::TooShort));
        assert_eq!(pin_problem("1234"), None);
        assert_eq!(pin_problem("12345678"), None);
        assert_eq!(pin_problem("123456789"), Some(PinProblem::TooLong));
        assert_eq!(pin_problem("12a4"), Some(PinProblem::NotDigits));
        assert_eq!(pin_problem("12 34"), Some(PinProblem::NotDigits));
        // A digit of another script is not an ASCII one, as C# `IsAsciiDigit`.
        assert_eq!(pin_problem("12\u{0663}4"), Some(PinProblem::NotDigits));
    }

    #[test]
    fn the_length_is_checked_before_the_digits_and_counted_in_characters() {
        assert_eq!(pin_problem("ab"), Some(PinProblem::TooShort));
        assert_eq!(pin_problem("abcdefghi"), Some(PinProblem::TooLong));
        // Five characters of two bytes each: within the rule, not long by the bytes.
        assert_eq!(
            pin_problem("\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}"),
            Some(PinProblem::NotDigits)
        );
    }

    #[test]
    fn the_pin_kept_is_taken_and_no_other() {
        let kept = PinHash::new("2468").expect("hash");
        assert!(kept.verify("2468"));
        assert!(!kept.verify("2469"));
        assert!(!kept.verify("24680"));
        assert!(!kept.verify(""));
    }

    #[test]
    fn each_pin_has_its_own_salt_and_nothing_of_the_pin_is_kept() {
        let first = PinHash::new("2468").expect("hash");
        let second = PinHash::new("2468").expect("hash");
        assert_ne!(first.salt(), second.salt());
        assert_ne!(first.hash(), second.hash());
        assert!(!first.hash().contains("2468") && !first.salt().contains("2468"));
    }

    #[test]
    fn a_pin_read_back_from_the_file_is_taken() {
        let kept = PinHash::new("13579").expect("hash");
        let read = PinHash::saved(kept.salt().to_owned(), kept.hash().to_owned());
        assert!(read.verify("13579"));
    }

    #[test]
    fn a_damaged_pin_takes_nothing() {
        let kept = PinHash::new("2468").expect("hash");
        let short_hash = &kept.hash()[..kept.hash().len() - 4];
        let short_salt = &kept.salt()[..kept.salt().len() - 4];
        for damaged in [
            PinHash::saved(kept.salt().to_owned(), "not base64!".to_owned()),
            PinHash::saved(kept.salt().to_owned(), short_hash.to_owned()),
            PinHash::saved(short_salt.to_owned(), kept.hash().to_owned()),
            PinHash::saved(String::new(), String::new()),
        ] {
            assert!(!damaged.verify("2468"));
        }
    }
}
