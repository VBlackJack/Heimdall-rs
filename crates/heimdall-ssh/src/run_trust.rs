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

//! Host keys trusted for this run of the application only, as the C# Heimdall's "Trust this
//! session" leaves them: held in memory, never written to `known_hosts`, gone when the
//! application ends. A key trusted so counts as recorded for its server.

use std::sync::{Arc, Mutex, PoisonError};

use russh::keys::PublicKey;

/// The keys trusted for this run; clones share them.
#[derive(Debug, Clone, Default)]
pub struct RunTrust(Arc<Mutex<Vec<(String, u16, PublicKey)>>>);

impl RunTrust {
    /// Trusts `key` for `host` on `port` until the application ends.
    pub fn trust(&self, host: &str, port: u16, key: PublicKey) {
        let mut keys = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if !keys.iter().any(|(known, known_port, known_key)| {
            known == host && *known_port == port && *known_key == key
        }) {
            keys.push((host.to_owned(), port, key));
        }
    }

    /// The keys trusted for `host` on `port`.
    #[must_use]
    pub fn keys(&self, host: &str, port: u16) -> Vec<PublicKey> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(known, known_port, _)| known == host && *known_port == port)
            .map(|(_, _, key)| key.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = include_str!("../tests/fixtures/hostkeys/host-ed25519.pub");

    fn key() -> PublicKey {
        PublicKey::from_openssh(KEY.trim()).expect("key")
    }

    #[test]
    fn a_key_is_trusted_for_its_server_only_and_shared_by_clones() {
        let trust = RunTrust::default();
        let shared = trust.clone();
        assert!(trust.keys("web.lab", 22).is_empty());
        shared.trust("web.lab", 22, key());
        shared.trust("web.lab", 22, key());
        assert_eq!(
            trust.keys("web.lab", 22),
            [key()],
            "once, seen by the clone"
        );
        assert!(trust.keys("web.lab", 2222).is_empty(), "another port");
        assert!(trust.keys("db.lab", 22).is_empty(), "another host");
    }
}
