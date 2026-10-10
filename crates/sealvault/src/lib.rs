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

//! Heimdall's cryptography, behind one API, and the vault built on it.
//!
//! Every primitive comes from an audited implementation (the `RustCrypto` crates) and is
//! reached only through these modules, so that what the application may use is listed in
//! one place:
//!
//! - [`hash`]: SHA-2, SHA3-256, and MD5 and SHA-1 for interoperability only.
//! - [`mac`]: HMAC, computed or verified in constant time.
//! - [`aead`]: AES-256-GCM, each sealing spending a fresh random nonce.
//! - [`kdf`]: keys derived from passwords, by Argon2id.
//! - [`random`]: the operating system's generator.
//! - [`compare`]: equality of secrets in constant time.
//! - [`secret`]: the owned secrets the others hand out, wiped when dropped.
//! - [`legacy`]: what an old protocol mandates and nothing else may use.
//! - [`Vault`]: a file of named secrets sealed with a password.
//!
//! Secrets this crate creates (keys, derived keys, plaintext, tags) come back in types that
//! are wiped when dropped and never show their content through `Debug`. Without `unsafe`,
//! copies the compiler makes on the stack, swap and crash dumps are out of reach: keep
//! secrets briefly.

pub mod aead;
pub mod compare;
pub mod hash;
pub mod kdf;
pub mod legacy;
pub mod mac;
pub mod random;
pub mod secret;

mod body;
mod error;
mod format;
mod vault;

pub use crate::error::Error;
pub use crate::kdf::KdfParams;
pub use crate::vault::{DATA_KEY_LEN, Vault, VaultError, backup_path};

/// Hexadecimal written in the tests' known answers, read back as bytes.
#[cfg(test)]
pub(crate) mod test_hex {
    /// Bits of one hexadecimal digit.
    const NIBBLE_BITS: u32 = 4;

    /// Base of a hexadecimal digit.
    const RADIX: u32 = 16;

    /// The bytes `text` spells in hexadecimal, two digits each.
    pub fn hex(text: &str) -> Vec<u8> {
        let digits: Vec<u8> = text
            .chars()
            .map(|digit| {
                u8::try_from(digit.to_digit(RADIX).expect("a hexadecimal digit")).expect("< 16")
            })
            .collect();
        assert!(digits.len().is_multiple_of(2), "two digits a byte");
        digits
            .chunks(2)
            .map(|pair| (pair[0] << NIBBLE_BITS) | pair[1])
            .collect()
    }
}
