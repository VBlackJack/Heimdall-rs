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

//! Against the real store of the machine running the test, when asked to: it writes to the
//! user's credential store, so it runs only with `HEIMDALL_LIVE_KEYRING` set. It removes
//! what it wrote.

use heimdall_keyring::SystemKeyring;

/// Environment variable enabling the test.
const LIVE_VARIABLE: &str = "HEIMDALL_LIVE_KEYRING";

#[test]
fn a_secret_goes_in_comes_back_whole_and_leaves() {
    if std::env::var_os(LIVE_VARIABLE).is_none() {
        return;
    }
    let service = format!("heimdall-rs-test-{}", std::process::id());
    let keyring = SystemKeyring::open(&service).expect("a store on this machine");
    let name = "password/profile-1";
    // Bytes that are not UTF-8, and a NUL: the store keeps secrets as bytes.
    let secret = b"p\xe4ss\0w\xf6rd".to_vec();

    assert_eq!(keyring.get(name).expect("get"), None, "nothing yet");
    keyring.set(name, &secret).expect("set");
    assert_eq!(
        keyring.get(name).expect("get").map(|bytes| bytes.to_vec()),
        Some(secret.clone())
    );
    keyring.set(name, b"replaced").expect("replace");
    assert_eq!(
        keyring.get(name).expect("get").map(|bytes| bytes.to_vec()),
        Some(b"replaced".to_vec())
    );
    assert!(keyring.remove(name).expect("remove"));
    assert_eq!(keyring.get(name).expect("get"), None, "gone");
    assert!(!keyring.remove(name).expect("remove again"), "nothing left");
}
