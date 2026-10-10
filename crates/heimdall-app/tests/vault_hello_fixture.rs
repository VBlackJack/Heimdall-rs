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

//! A Windows Hello envelope enrolled before the envelope's cryptography moved into
//! sealvault: it must still unwrap the data key and open its vault, byte for byte.
//!
//! The envelope was written by `vault_hello::enrol` of master at 9d12109 (aes-gcm, ring's
//! HKDF and random generator), with [`Fixed`] as the credential, over the data key of
//! sealvault's own format fixture, enrolled at [`ENROLLED_AT`].
//!
//! Proof that master's code reads it: this file, copied unchanged into a checkout of
//! 9d12109, passes there with
//! `cargo test -p heimdall-app --locked --test vault_hello_fixture` (3 passed: the
//! envelope reads back, unwraps the key and opens the vault, and another signature
//! unwraps nothing).

use std::fs;
use std::time::{Duration, SystemTime};

use heimdall_app::vault_hello::{self, ENVELOPE_LEN, Envelope, HelloFailure, KeyCredentials};
use zeroize::Zeroizing;

/// The envelope as master's code wrote it.
const ENVELOPE_HEX: &str = "\
    4848564501669430e8c75bc5d4fa55fe5d198ed2162b71e5708e3c07e1dd8e5b5e3349701848a777e7ba32c7\
    aee0b04ac92adb42f1c3eb9fa5d2005a568ea28de91bd9914adbfa9524869e4979ca8e1c1b63f6302f9d9a73\
    f107d59f19ba877a3d1bc6e7e1c722f56835accfe447c18cdf23b1d985000000006ac58b80427e41d8ee3788\
    86541a4fea60d966039a2ef105e52b1d881acaf343f3870303ac721c47ff9161b5e5e128eabedff9542f9657\
    800ea60fca452d1c8d";

/// The vault the envelope's data key opens: sealvault's format fixture, password
/// "fixture password", data key 34ef7b0d...afab87d9.
const VAULT_HEX: &str = "\
    53564c540100010000010003000000010000009724b888a8f50100bbe94fb3e3d79f2c01ad4b5027\
    7c5d8403584c80f514cd0150834b790a4f2135d6f64ec2715316fe054f5319e803195ca65a6b1f9a\
    afb69445acfe4f30739f8ef80f2fa30ee85212d918a3ef17dc8f031e4c5655263c8f15e6bffdfdb5\
    d980c82b02e274bb0e39e53b45c625274d6c4e266e026557b2ccefbb91666786cf3bb645d54338d6\
    46164daaea3934a8e13da1e2f1";

/// When the fixture was enrolled, in seconds since 1970.
const ENROLLED_AT: u64 = 1_791_331_200;

/// The credential's private part, from which [`Fixed`] signs.
const PRIVATE: &[u8] = b"fixture credential private part";

/// The credential's public key, whose hash the envelope holds.
const PUBLIC_KEY: &[u8] = b"fixture credential public key";

/// Bits of one hexadecimal digit.
const NIBBLE_BITS: u32 = 4;

/// Base of a hexadecimal digit.
const RADIX: u32 = 16;

/// A credential that signs deterministically, as Windows Hello's RSA PKCS#1 v1.5 does: the
/// SHA-256 of its private part and the challenge.
struct Fixed;

impl KeyCredentials for Fixed {
    fn enrolment_available(&self) -> bool {
        true
    }

    fn create(&self, _name: &str) -> Result<Vec<u8>, HelloFailure> {
        Ok(PUBLIC_KEY.to_vec())
    }

    fn open(&self, _name: &str) -> Result<Vec<u8>, HelloFailure> {
        Ok(PUBLIC_KEY.to_vec())
    }

    fn sign(&self, _name: &str, challenge: &[u8]) -> Result<Zeroizing<Vec<u8>>, HelloFailure> {
        Ok(Zeroizing::new(
            sealvault::hash::sha256(&[PRIVATE, challenge].concat()).to_vec(),
        ))
    }

    fn delete(&self, _name: &str) {}
}

/// A credential that signs anything else.
struct Other;

impl KeyCredentials for Other {
    fn enrolment_available(&self) -> bool {
        true
    }

    fn create(&self, _name: &str) -> Result<Vec<u8>, HelloFailure> {
        Ok(PUBLIC_KEY.to_vec())
    }

    fn open(&self, _name: &str) -> Result<Vec<u8>, HelloFailure> {
        Ok(PUBLIC_KEY.to_vec())
    }

    fn sign(&self, _name: &str, challenge: &[u8]) -> Result<Zeroizing<Vec<u8>>, HelloFailure> {
        Ok(Zeroizing::new(
            sealvault::hash::sha256(&[b"another private part".as_slice(), challenge].concat())
                .to_vec(),
        ))
    }

    fn delete(&self, _name: &str) {}
}

fn hex(text: &str) -> Vec<u8> {
    let digits: Vec<u8> = text
        .chars()
        .map(|digit| u8::try_from(digit.to_digit(RADIX).expect("hexadecimal")).expect("< 16"))
        .collect();
    digits
        .chunks(2)
        .map(|pair| (pair[0] << NIBBLE_BITS) | pair[1])
        .collect()
}

fn envelope() -> Envelope {
    Envelope::parse(&hex(ENVELOPE_HEX)).expect("an envelope")
}

#[test]
fn the_envelope_reads_back_byte_for_byte() {
    let bytes = hex(ENVELOPE_HEX);
    assert_eq!(bytes.len(), ENVELOPE_LEN);
    assert_eq!(envelope().to_bytes(), bytes);
    assert_eq!(
        envelope().enrolled_at(),
        SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(ENROLLED_AT))
    );
}

#[test]
fn the_envelope_unwraps_the_data_key_and_opens_its_vault() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("fixture.svlt");
    fs::write(&path, hex(VAULT_HEX)).expect("written");
    let vault = vault_hello::unlock(&Fixed, &path, &envelope()).expect("unlocked");
    assert_eq!(vault.get("ssh/web"), Some(&b"hunter2"[..]));
    assert_eq!(vault.get("rdp/dc"), Some(&b"P@ss w0rd"[..]));
}

#[test]
fn another_signature_unwraps_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("fixture.svlt");
    fs::write(&path, hex(VAULT_HEX)).expect("written");
    assert_eq!(
        vault_hello::unlock(&Other, &path, &envelope()).err(),
        Some(HelloFailure::CryptoFailure)
    );
}
