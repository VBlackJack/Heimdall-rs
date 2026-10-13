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

//! A PIN hash as a settings file already holds it: it must keep verifying, whatever
//! computes the hash now.
//!
//! The salt and hash were computed outside this crate, by OpenSSL's Argon2id (Python
//! `cryptography` 46) at the argon2 crate's default cost (19456 KiB, 2 passes, 1 lane,
//! version 0x13, 32 bytes), the cost `PinHash` has always used.
//!
//! Proof that master's code verifies it: this file, copied unchanged into a checkout of
//! master at 9d12109, before the PIN's cryptography moved into sealvault, passes there
//! with `cargo test -p heimdall-core --locked --test pin_fixture` (1 passed).

use heimdall_core::pin::PinHash;

/// The PIN the fixture was made from.
const PIN: &str = "4817";

/// The salt, in Base64, as the settings file holds it.
const SALT: &str = "W+GjyQ99LkSGsBk6z1LX4Q==";

/// The hash, in Base64, as the settings file holds it.
const HASH: &str = "Bvb/+5slEQdj9PmzEs0u/uQEVuAqm9yDbGoWjWarqWc=";

#[test]
fn a_saved_pin_hash_still_verifies_and_nothing_else_does() {
    let saved = PinHash::saved(SALT.to_owned(), HASH.to_owned());
    assert!(saved.verify(PIN));
    assert!(!saved.verify("4818"));
    assert!(!saved.verify(""));
    let other_salt = PinHash::saved("AAAAAAAAAAAAAAAAAAAAAA==".to_owned(), HASH.to_owned());
    assert!(!other_salt.verify(PIN));
}
