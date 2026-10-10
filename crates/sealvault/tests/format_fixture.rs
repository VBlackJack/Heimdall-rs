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

//! A vault file written by the first release of the format, before the cryptography moved
//! behind the crate's modules: it must open, byte for byte, with every later build.

use std::fs;

use sealvault::{DATA_KEY_LEN, Vault};

const PASSWORD: &[u8] = b"fixture password";

/// The whole file, header then sealed body, as that build wrote it.
const FILE_HEX: &str = "\
    53564c540100010000010003000000010000009724b888a8f50100bbe94fb3e3d79f2c01ad4b5027\
    7c5d8403584c80f514cd0150834b790a4f2135d6f64ec2715316fe054f5319e803195ca65a6b1f9a\
    afb69445acfe4f30739f8ef80f2fa30ee85212d918a3ef17dc8f031e4c5655263c8f15e6bffdfdb5\
    d980c82b02e274bb0e39e53b45c625274d6c4e266e026557b2ccefbb91666786cf3bb645d54338d6\
    46164daaea3934a8e13da1e2f1";

/// Its data key, as [`Vault::data_key`] gave it then.
const DATA_KEY_HEX: &str = "34ef7b0dc2c20224ac868002a19acfb563e2717206d1404ebcd71fd2afab87d9";

/// Where the body's nonce starts in the file, as the format lays it out.
const BODY_NONCE: usize = 96;

/// Bits of one hexadecimal digit.
const NIBBLE_BITS: u32 = 4;

/// Base of a hexadecimal digit.
const RADIX: u32 = 16;

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

fn check(vault: &Vault) {
    assert_eq!(vault.names().collect::<Vec<_>>(), ["rdp/dc", "ssh/web"]);
    assert_eq!(vault.get("ssh/web"), Some(&b"hunter2"[..]));
    assert_eq!(vault.get("rdp/dc"), Some(&b"P@ss w0rd"[..]));
}

#[test]
fn a_file_of_the_first_format_opens_with_its_password_and_its_data_key() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("fixture.svlt");
    fs::write(&path, hex(FILE_HEX)).expect("written");

    let vault = Vault::open(&path, PASSWORD).expect("opens with the password");
    check(&vault);
    assert_eq!(vault.data_key().to_vec(), hex(DATA_KEY_HEX));

    let key: [u8; DATA_KEY_LEN] = hex(DATA_KEY_HEX).try_into().expect("32 bytes");
    check(&Vault::open_with_data_key(&path, &key).expect("opens with the data key"));
}

#[test]
fn saved_again_it_keeps_its_header_and_still_opens() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("fixture.svlt");
    let original = hex(FILE_HEX);
    fs::write(&path, &original).expect("written");

    Vault::open(&path, PASSWORD)
        .expect("opens")
        .save()
        .expect("saved");
    let saved = fs::read(&path).expect("read");
    // Everything up to the body's nonce is unchanged: same cost, salt and wrapping.
    assert_eq!(saved[..BODY_NONCE], original[..BODY_NONCE]);
    assert_eq!(saved.len(), original.len());
    assert_ne!(saved, original, "a new nonce seals the body again");
    check(&Vault::open(&path, PASSWORD).expect("opens again"));
}
