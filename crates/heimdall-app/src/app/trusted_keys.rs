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

//! The keys trusted for servers, as the C# Settings lists them: SSH host keys and RDP
//! certificates, each copied or forgotten from the Settings page.

use heimdall_core::profile::display_address;
use heimdall_rdp::{KnownRdpHost, KnownRdpHosts};
use heimdall_ssh::{KnownHostEntry, KnownHosts};

use super::{App, Dialog, Effect, Notice};

/// A key trusted for a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustedKey {
    /// An SSH host key.
    Ssh(KnownHostEntry),
    /// The key of an RDP server's certificate.
    Rdp(KnownRdpHost),
}

impl TrustedKey {
    /// The server, as `host:port`.
    #[must_use]
    pub fn address(&self) -> String {
        match self {
            Self::Ssh(entry) => display_address(&entry.host, entry.port),
            Self::Rdp(entry) => display_address(&entry.host, entry.port),
        }
    }

    /// Its SHA-256 fingerprint.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        match self {
            Self::Ssh(entry) => entry.fingerprint.clone(),
            Self::Rdp(entry) => entry.fingerprint.to_string(),
        }
    }
}

/// The keys trusted, as last read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustedKeys {
    /// SSH host keys, in the order of their file.
    pub ssh: Vec<KnownHostEntry>,
    /// RDP certificates, in the order of their file.
    pub rdp: Vec<KnownRdpHost>,
    /// Why a file could not be read, when one could not: its list is then empty.
    pub unreadable: Option<String>,
}

/// A change from the lists of trusted keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustedKeysMessage {
    /// Read the files again: the page is shown, or asked to.
    Refresh,
    /// Copy the whole fingerprint of a key.
    CopyFingerprint(TrustedKey),
    /// Ask whether to forget a key.
    RequestForget(TrustedKey),
    /// Import the keys of another `known_hosts` file, as the C# "Trusted SSH hosts...".
    Import(super::HostKeysMessage),
}

impl App {
    /// The keys trusted, as last read.
    #[must_use]
    pub fn trusted_keys(&self) -> &TrustedKeys {
        &self.trusted_keys
    }

    /// Applies a change from the lists of trusted keys.
    pub(super) fn trusted_keys_message(&mut self, message: &TrustedKeysMessage) -> Vec<Effect> {
        match message {
            TrustedKeysMessage::Refresh => {
                self.read_trusted_keys();
                Vec::new()
            }
            TrustedKeysMessage::CopyFingerprint(key) => {
                self.tell(Notice::FingerprintCopied(key.address()));
                vec![Effect::WriteClipboard(key.fingerprint())]
            }
            TrustedKeysMessage::RequestForget(key) => {
                self.dialog = Some(Dialog::ForgetTrustedKey(key.clone()));
                Vec::new()
            }
            TrustedKeysMessage::Import(message) => self.hostkeys_message(message.clone()),
        }
    }

    /// Reads both files of trusted keys.
    pub(super) fn read_trusted_keys(&mut self) {
        let mut unreadable = Vec::new();
        let ssh = KnownHosts::new(&self.config.known_hosts)
            .entries()
            .unwrap_or_else(|error| {
                unreadable.push(error.to_string());
                Vec::new()
            });
        let rdp_file = self.known_rdp_hosts();
        let rdp = KnownRdpHosts::new(&rdp_file)
            .entries()
            .unwrap_or_else(|error| {
                unreadable.push(format!("{}: {error}", rdp_file.display()));
                Vec::new()
            });
        self.trusted_keys = TrustedKeys {
            ssh,
            rdp,
            unreadable: (!unreadable.is_empty()).then(|| unreadable.join("\n")),
        };
    }

    /// Forgets `key`, as confirmed: an SSH server's keys, or that one RDP certificate. The
    /// next connection to it asks again.
    pub(super) fn forget_trusted_key(&mut self, key: &TrustedKey) {
        let forgotten = match key {
            TrustedKey::Ssh(entry) => KnownHosts::new(&self.config.known_hosts)
                .forget(&entry.host, entry.port)
                .map_err(|error| error.to_string()),
            TrustedKey::Rdp(entry) => KnownRdpHosts::new(self.known_rdp_hosts())
                .forget_key(&entry.host, entry.port, &entry.fingerprint)
                .map_err(|error| error.to_string()),
        };
        self.read_trusted_keys();
        match forgotten {
            Ok(_) => self.tell(match key {
                TrustedKey::Ssh(_) => Notice::HostKeyRemoved(key.address()),
                TrustedKey::Rdp(_) => Notice::CertificateForgotten(key.address()),
            }),
            Err(detail) => self.dialog = Some(Dialog::StoreError { detail }),
        }
    }
}
