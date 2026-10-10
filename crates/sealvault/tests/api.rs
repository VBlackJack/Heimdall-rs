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

//! The cryptographic API as another crate sees it: each module reachable by its path, the
//! secrets it hands out wiped on drop and silent in `Debug`.

use sealvault::aead::{self, FreshNonce, Key};
use sealvault::secret::{SecretBytes, SecretKey};
use sealvault::{Error, KdfParams, compare, hash, kdf, legacy, mac, random};
use zeroize::ZeroizeOnDrop;

fn wiped_on_drop<T: ZeroizeOnDrop>() {}

#[test]
fn every_secret_type_handed_out_is_wiped_on_drop() {
    wiped_on_drop::<SecretBytes>();
    wiped_on_drop::<SecretKey<{ aead::KEY_LEN }>>();
    wiped_on_drop::<Key>();
    wiped_on_drop::<mac::Tag>();
}

#[test]
fn a_key_from_the_generator_seals_and_opens() {
    let key: Key = random::secret_key().expect("random");
    let nonce = FreshNonce::random().expect("random");
    let nonce_bytes = nonce.bytes();
    let sealed = aead::seal(&key, nonce, b"plain", b"context").expect("sealed");
    let plain = aead::open(&key, &nonce_bytes, &sealed, b"context").expect("opened");
    assert!(compare::equal(plain.as_bytes(), b"plain"));
    assert_eq!(
        aead::open(&key, &nonce_bytes, &sealed, b"other").err(),
        Some(Error::Unauthentic)
    );
    assert_eq!(format!("{key:?}"), "SecretKey<32>(..)");
    assert_eq!(format!("{plain:?}"), "SecretBytes(..)");
}

#[test]
fn hashes_macs_and_derivations_are_reachable() {
    assert_eq!(hash::sha256(b"abc").len(), hash::SHA256_LEN);
    assert_eq!(
        hash::digest(hash::Algorithm::Sha256, b"abc").as_bytes(),
        hash::sha256(b"abc")
    );
    let tag = mac::compute(mac::Algorithm::HmacSha256, b"key", b"data");
    assert!(mac::verify(
        mac::Algorithm::HmacSha256,
        b"key",
        b"data",
        tag.as_bytes()
    ));
    let cheap = KdfParams {
        memory_kib: 8,
        iterations: 1,
        lanes: 1,
    };
    let derived: SecretKey<16> = kdf::argon2id(b"pw", b"saltsalt", &cheap).expect("derived");
    assert_ne!(derived.as_bytes(), &[0; 16]);
}

#[test]
fn the_legacy_module_offers_the_vnc_response_only() {
    let response = legacy::vnc_des::response(b"pw", &[0; legacy::vnc_des::CHALLENGE_LEN]);
    assert_eq!(response.len(), legacy::vnc_des::CHALLENGE_LEN);
    assert_eq!(legacy::vnc_des::PASSWORD_BYTES, 8);
}
