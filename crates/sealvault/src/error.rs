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

//! The one error of the cryptographic API.

/// Why a cryptographic operation failed. Deliberately coarse: an opening that fails says
/// nothing of which byte, key or tag was wrong.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The operating system gave no random bytes.
    #[error("no random bytes: {0}")]
    Randomness(String),
    /// A key of the wrong length was given.
    #[error("a key of {actual} bytes where {expected} are expected")]
    KeyLength {
        /// Bytes the primitive takes.
        expected: usize,
        /// Bytes it was given.
        actual: usize,
    },
    /// Sealed data that the key, the nonce and the authenticated data do not open: a wrong
    /// key, a changed byte or a cut message, without saying which.
    #[error("the sealed data cannot be opened")]
    Unauthentic,
    /// Key derivation parameters the algorithm refuses.
    #[error("the key derivation parameters are refused")]
    KdfParameters,
    /// A key, certificate or container that could not be made, encoded or read, as the
    /// library underneath said it.
    #[error("{0}")]
    Encoding(String),
}
