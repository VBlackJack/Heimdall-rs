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

//! RSA keys drawn from the system's generator, as .NET's `RSA.Create(keySize)` makes them
//! for the SSH Key and Certificate generators: public exponent 65537, the size asked.

use getrandom::SysRng;
use getrandom::rand_core::UnwrapErr;

/// Bytes read from the system's generator before a key is made, to learn that it works.
const PROBE_BYTES: usize = 16;

/// A new RSA key of `bits`, its primes drawn from the system's generator.
///
/// # Errors
///
/// What went wrong, said: the system's generator unreadable, or the size refused.
pub fn generate(bits: usize) -> Result<rsa::RsaPrivateKey, String> {
    // The generator is read once first: the draws of the key itself cannot report an error,
    // so a generator that does not work is refused here rather than met there.
    let mut probe = zeroize::Zeroizing::new([0_u8; PROBE_BYTES]);
    getrandom::fill(probe.as_mut()).map_err(|error| error.to_string())?;
    rsa::RsaPrivateKey::new(&mut UnwrapErr(SysRng), bits).map_err(|error| error.to_string())
}
