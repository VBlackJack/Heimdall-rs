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
//!
//! The key generation takes a generator that cannot fail. The system's can, so it is read
//! through [`Watched`]: a failure is noted rather than raised, the draws go on from a stand-in
//! so the generation still ends, and the key made is thrown away and the failure returned.
//! Nothing panics, and no key drawn even partly from the stand-in is ever handed out.

use std::convert::Infallible;

use getrandom::rand_core::{TryCryptoRng, TryRng};
use sha2::{Digest as _, Sha256};

/// What the generation says when the system's generator failed under it.
const GENERATOR_FAILED: &str = "the system's random generator could not be read";

/// The system's generator, its failures noted.
#[derive(Default)]
struct Watched {
    failed: bool,
    /// The stand-in's counter, used once the system's generator failed.
    counter: u64,
}

impl Watched {
    /// `dest` filled from the system's generator, or from the stand-in once it failed.
    fn fill(&mut self, dest: &mut [u8]) {
        if !self.failed && getrandom::fill(dest).is_ok() {
            return;
        }
        self.failed = true;
        // Different bytes on every draw, so a search for primes still ends; what it makes is
        // never used.
        for chunk in dest.chunks_mut(Sha256::output_size()) {
            self.counter += 1;
            let block = Sha256::digest(self.counter.to_le_bytes());
            chunk.copy_from_slice(&block[..chunk.len()]);
        }
    }
}

impl TryRng for Watched {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Infallible> {
        let mut bytes = [0; 4];
        self.fill(&mut bytes);
        Ok(u32::from_le_bytes(bytes))
    }

    fn try_next_u64(&mut self) -> Result<u64, Infallible> {
        let mut bytes = [0; 8];
        self.fill(&mut bytes);
        Ok(u64::from_le_bytes(bytes))
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Infallible> {
        self.fill(dest);
        Ok(())
    }
}

impl TryCryptoRng for Watched {}

/// A new RSA key of `bits`, its primes drawn from the system's generator.
///
/// # Errors
///
/// What went wrong, said: the system's generator unreadable, or the size refused.
pub fn generate(bits: usize) -> Result<rsa::RsaPrivateKey, String> {
    generate_with(bits, Watched::default())
}

/// [`generate`] with `random`, a test's generator that fails on its first draw.
fn generate_with(bits: usize, mut random: Watched) -> Result<rsa::RsaPrivateKey, String> {
    let key = rsa::RsaPrivateKey::new(&mut random, bits).map_err(|error| error.to_string())?;
    if random.failed {
        return Err(GENERATOR_FAILED.to_owned());
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failing_generator_gives_an_error_and_no_key() {
        let failing = Watched {
            failed: true,
            counter: 0,
        };
        assert_eq!(
            generate_with(2048, failing).err().as_deref(),
            Some(GENERATOR_FAILED)
        );
    }

    #[test]
    fn the_system_generator_makes_a_key_of_the_size_asked() {
        use rsa::traits::PublicKeyParts as _;
        let key = generate(2048).expect("made");
        assert_eq!(key.size() * 8, 2048);
    }
}
