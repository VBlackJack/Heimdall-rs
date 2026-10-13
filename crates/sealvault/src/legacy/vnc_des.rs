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

//! The response of VNC Authentication, and nothing else.
//!
//! This module exists only because RFB 3.8 section 7.2.2, VNC Authentication (RFC 6143
//! section 7.2.2), mandates it: the server sends a 16-byte challenge, and the client
//! answers with it encrypted by single DES in ECB mode, keyed by the password cut or padded
//! with zeros to 8 bytes, each byte's bits in reverse order (the historical key schedule of
//! the original VNC, which the RFC leaves implicit and every server applies).
//!
//! It is not to be used for anything else. DES is broken, the key is at most 8 bytes of a
//! password, the server is not authenticated, and a recorded exchange can be brute-forced
//! offline. A VNC client speaks it because most VNC servers offer nothing better; anything
//! that chooses its own cryptography takes it from the rest of this crate.

use des::Des;
use des::cipher::{Array, BlockCipherEncrypt, KeyInit};
use zeroize::Zeroizing;

/// Bytes of the challenge and of the response.
pub const CHALLENGE_LEN: usize = 16;

/// Bytes of the password that count; the protocol ignores the rest.
pub const PASSWORD_BYTES: usize = 8;

/// The response to `challenge` for `password`.
#[must_use]
pub fn response(password: &[u8], challenge: &[u8; CHALLENGE_LEN]) -> [u8; CHALLENGE_LEN] {
    let mut key = Zeroizing::new([0; PASSWORD_BYTES]);
    for (slot, byte) in key.iter_mut().zip(password) {
        *slot = byte.reverse_bits();
    }
    // Borrowed, not copied: the only copy of the key is the one wiped above, and the
    // cipher's key schedule is wiped when it drops (des's zeroize feature).
    let cipher = Des::new((&*key).into());
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
    use super::{CHALLENGE_LEN, response};

    /// The RFB vector: computed outside this crate, `openssl enc -des-ecb -nopad` with the
    /// key bit-reversed by hand, over the challenge 00 11 22 .. ff, for "Secret12".
    const CHALLENGE: [u8; CHALLENGE_LEN] = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff,
    ];
    const EXPECTED: [u8; CHALLENGE_LEN] = [
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

    /// The first block of the ECB example of FIPS PUB 81, "Now is t" under the key
    /// 01 23 45 67 89 ab cd ef giving 3f a4 0e 8a 98 4d 48 15 (checked again with OpenSSL):
    /// the password is that key with each byte's bits reversed, so the bit reversal and the
    /// cipher are checked apart.
    #[test]
    fn the_cipher_underneath_is_plain_des_on_the_bit_reversed_password() {
        let password = [0x80, 0xc4, 0xa2, 0xe6, 0x91, 0xd5, 0xb3, 0xf7];
        let challenge = *b"Now is tNow is t";
        let block = [0x3f, 0xa4, 0x0e, 0x8a, 0x98, 0x4d, 0x48, 0x15];
        assert_eq!(
            response(&password, &challenge).as_slice(),
            [block, block].concat()
        );
    }

    #[test]
    fn a_short_password_is_padded_with_zeros() {
        assert_eq!(
            response(b"pw", &CHALLENGE),
            response(b"pw\0\0\0\0\0\0", &CHALLENGE)
        );
        assert_eq!(response(b"", &CHALLENGE), response(&[0; 8], &CHALLENGE));
    }
}
