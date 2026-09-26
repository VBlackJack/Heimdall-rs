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

//! VNC Authentication (RFC 6143 7.2.2): the challenge encrypted with DES, keyed by the
//! password.
//!
//! It is weak by design and the client cannot fix it: the key is the password cut to 8 bytes,
//! the server is not authenticated, and a recorded exchange can be brute-forced offline. It
//! is spoken because most VNC servers offer nothing better.

use des::Des;
use des::cipher::{Array, BlockCipherEncrypt, KeyInit};
use zeroize::Zeroizing;

/// Length of the challenge and of the response.
pub const CHALLENGE_LENGTH: usize = 16;

/// Bytes of the password that count; the rest is ignored by the protocol.
pub const PASSWORD_BYTES: usize = 8;

/// The response to `challenge` for `password`.
#[must_use]
pub fn response(password: &[u8], challenge: &[u8; CHALLENGE_LENGTH]) -> [u8; CHALLENGE_LENGTH] {
    // The password cut or padded with zeros to 8 bytes, each byte's bits in reverse order:
    // the historical VNC key schedule.
    let mut key = Zeroizing::new([0; PASSWORD_BYTES]);
    for (slot, byte) in key.iter_mut().zip(password) {
        *slot = byte.reverse_bits();
    }
    let cipher = Des::new(&Array::from(*key));
    let mut response = *challenge;
    for block in response.as_chunks_mut::<PASSWORD_BYTES>().0 {
        let mut array = Array::from(*block);
        cipher.encrypt_block(&mut array);
        block.copy_from_slice(&array);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Computed outside this crate: `openssl enc -des-ecb -nopad` with the key bit-reversed
    /// by hand, over the challenge 00 11 22 .. ff.
    const CHALLENGE: [u8; 16] = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff,
    ];
    const EXPECTED: [u8; 16] = [
        0xee, 0xe9, 0x81, 0xe2, 0x74, 0x19, 0x66, 0x45, 0xd7, 0x10, 0xe9, 0xd9, 0x1f, 0xf5, 0xf5,
        0xe8,
    ];

    #[test]
    fn the_response_matches_an_independent_computation() {
        assert_eq!(response(b"Secret12", &CHALLENGE), EXPECTED);
    }

    #[test]
    fn only_the_first_eight_bytes_of_the_password_count() {
        assert_eq!(response(b"Secret12 and more", &CHALLENGE), EXPECTED);
        assert_ne!(response(b"Secret1", &CHALLENGE), EXPECTED);
    }
}
