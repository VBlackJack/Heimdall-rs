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
//! password. And the Plain authentication of `VeNCrypt`: a user name and a password as they
//! are, sent here only inside TLS, once the server's certificate is trusted.
//!
//! VNC Authentication is weak by design and the client cannot fix it: the key is the password
//! cut to 8 bytes, the server is not authenticated, and a recorded exchange can be
//! brute-forced offline. It is spoken because most VNC servers offer nothing better.

use des::Des;
use des::cipher::{Array, BlockCipherEncrypt, KeyInit};
use zeroize::Zeroizing;

/// Length of the challenge and of the response.
pub const CHALLENGE_LENGTH: usize = 16;

/// Bytes of the password that count; the rest is ignored by the protocol.
pub const PASSWORD_BYTES: usize = 8;

/// Longest Plain user name sent, in bytes.
pub const MAX_PLAIN_USERNAME: usize = 1024;

/// Longest Plain password sent, in bytes.
pub const MAX_PLAIN_PASSWORD: usize = 1024;

/// Bytes of each length Plain sends before the user name and the password.
const PLAIN_LENGTH_BYTES: usize = 4;

/// A Plain credential longer than the client sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TooLong {
    /// The user name.
    Username,
    /// The password.
    Password,
}

impl std::fmt::Display for TooLong {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Username => "user name",
            Self::Password => "password",
        })
    }
}

/// What Plain sends for `username` and `password`: both lengths in 32 bits, then both, as
/// they are. Refused past [`MAX_PLAIN_USERNAME`] or [`MAX_PLAIN_PASSWORD`] bytes, before
/// anything is built.
///
/// # Errors
///
/// [`TooLong`], naming which.
pub fn plain(username: &[u8], password: &[u8]) -> Result<Zeroizing<Vec<u8>>, TooLong> {
    let length = |bytes: &[u8], limit: usize, which: TooLong| {
        u32::try_from(bytes.len())
            .ok()
            .filter(|_| bytes.len() <= limit)
            .ok_or(which)
    };
    let username_length = length(username, MAX_PLAIN_USERNAME, TooLong::Username)?;
    let password_length = length(password, MAX_PLAIN_PASSWORD, TooLong::Password)?;
    // Sized once, so no copy of the password is left behind by a reallocation.
    let mut sent = Zeroizing::new(Vec::with_capacity(
        2 * PLAIN_LENGTH_BYTES + username.len() + password.len(),
    ));
    sent.extend_from_slice(&username_length.to_be_bytes());
    sent.extend_from_slice(&password_length.to_be_bytes());
    sent.extend_from_slice(username);
    sent.extend_from_slice(password);
    Ok(sent)
}

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
    fn plain_sends_both_lengths_then_both_as_they_are() {
        let sent = plain(b"admin", "p\u{e9}".as_bytes()).expect("short");
        assert_eq!(
            sent.as_slice(),
            [
                0, 0, 0, 5, 0, 0, 0, 3, b'a', b'd', b'm', b'i', b'n', b'p', 0xc3, 0xa9
            ]
        );
        assert!(plain(b"", b"").is_ok(), "empty is a length like another");
    }

    #[test]
    fn plain_refuses_a_credential_past_its_bound() {
        let at_bound = vec![b'u'; MAX_PLAIN_USERNAME];
        assert!(plain(&at_bound, &[b'p'; MAX_PLAIN_PASSWORD]).is_ok());
        let past = vec![b'u'; MAX_PLAIN_USERNAME + 1];
        assert_eq!(plain(&past, b"").err(), Some(TooLong::Username));
        let past = vec![b'p'; MAX_PLAIN_PASSWORD + 1];
        assert_eq!(plain(b"admin", &past).err(), Some(TooLong::Password));
    }

    #[test]
    fn only_the_first_eight_bytes_of_the_password_count() {
        assert_eq!(response(b"Secret12 and more", &CHALLENGE), EXPECTED);
        assert_ne!(response(b"Secret1", &CHALLENGE), EXPECTED);
    }
}
