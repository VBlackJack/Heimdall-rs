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

//! Message authentication codes as the C# `HmacComputer`, `HmacAlgorithmCatalog` and
//! `HmacVerifier` (`Heimdall.Core/Hashing`) compute and compare them: HMAC over SHA-256,
//! SHA-384, SHA-512, SHA-1 or MD5, written in lower-case hexadecimal or in Base64.

use data_encoding::{BASE64, HEXLOWER};
use sealvault::mac;

use super::hash_computer::HashAlgorithm;

/// How a code is written, as the C# `HmacOutputFormat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HmacOutputFormat {
    /// Lower-case hexadecimal.
    #[default]
    Hex,
    /// Base64, standard and padded.
    Base64,
}

/// The digests an HMAC is offered over, in the C# `HmacAlgorithmCatalog.SupportedKinds`
/// order (`HmacAlgorithmCatalog.cs:27-34`): the first is the one chosen when the tool opens.
pub const HMAC_ALGORITHMS: [HashAlgorithm; 5] = [
    HashAlgorithm::Sha256,
    HashAlgorithm::Sha384,
    HashAlgorithm::Sha512,
    HashAlgorithm::Sha1,
    HashAlgorithm::Md5,
];

/// An HMAC asked over a digest it is not offered over, as the C# `NotSupportedException`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not supported for HMAC")]
pub struct UnsupportedHmac(pub HashAlgorithm);

/// The name of the HMAC over `kind`, as the C# `HmacAlgorithmCatalog.DisplayName`
/// (`HmacAlgorithmCatalog.cs:48-56`); `None` for SHA3-256, which it is not offered over.
#[must_use]
pub const fn display_name(kind: HashAlgorithm) -> Option<&'static str> {
    match kind {
        HashAlgorithm::Md5 => Some("HMAC-MD5"),
        HashAlgorithm::Sha1 => Some("HMAC-SHA1"),
        HashAlgorithm::Sha256 => Some("HMAC-SHA256"),
        HashAlgorithm::Sha384 => Some("HMAC-SHA384"),
        HashAlgorithm::Sha512 => Some("HMAC-SHA512"),
        HashAlgorithm::Sha3_256 => None,
    }
}

/// The HMAC of `data` under `key` over `kind`, as the C# `HmacComputer.Compute`
/// (`HmacComputer.cs:22-43`): a key of any length, as .NET's `HMAC` takes it.
///
/// # Errors
///
/// [`UnsupportedHmac`] over SHA3-256.
pub fn compute(kind: HashAlgorithm, key: &[u8], data: &[u8]) -> Result<Vec<u8>, UnsupportedHmac> {
    let algorithm = mac::Algorithm::over(kind.sealed()).ok_or(UnsupportedHmac(kind))?;
    Ok(mac::compute(algorithm, key, data).as_bytes().to_vec())
}

/// `code` written as `format` says, as the C# `HmacComputer.Format` (`HmacComputer.cs:45-61`).
#[must_use]
pub fn format(code: &[u8], format: HmacOutputFormat) -> String {
    match format {
        HmacOutputFormat::Hex => HEXLOWER.encode(code),
        HmacOutputFormat::Base64 => BASE64.encode(code),
    }
}

/// The way `candidate` writes `code`, as the C# `HmacVerifier.Verify` (`HmacVerifier.cs:22-45`):
/// trimmed, compared whatever its case with the hexadecimal, then with the Base64; `None`
/// for a blank candidate or no match.
///
/// As the C#, the Base64 is compared whatever its case too, though its case carries bits.
#[must_use]
pub fn verify(code: &[u8], candidate: &str) -> Option<HmacOutputFormat> {
    let candidate = candidate.trim();
    if candidate.is_empty() {
        return None;
    }
    [HmacOutputFormat::Hex, HmacOutputFormat::Base64]
        .into_iter()
        .find(|written| format(code, *written).eq_ignore_ascii_case(candidate))
}
