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
//! certificates, each copied or forgotten from the Settings page; and the FTPS certificates,
//! which the C# keeps out of sight in its settings file, and the VNC ones, which the C# never
//! asks about, listed and forgotten the same way.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use heimdall_core::profile::display_address;
use heimdall_rdp::{KnownRdpHost, KnownRdpHosts};
use heimdall_ssh::{KnownHostEntry, KnownHosts, KnownHostsError};

use super::{App, Dialog, Effect, Notice};

/// The folder OpenSSH keeps its files in, under the home folder.
const OPENSSH_FOLDER: &str = ".ssh";

/// The file of the keys OpenSSH trusts, in that folder.
const OPENSSH_KNOWN_HOSTS: &str = "known_hosts";

/// The format of the C# "Trusted since", "First seen" and "Last seen" columns: the day and
/// the minute, as its general format, in an order every language reads.
const TRUSTED_SINCE_FORMAT: &str = "%Y-%m-%d %H:%M";

/// `time` in this computer's time, as the C# lists of trusted keys show a date.
#[must_use]
pub fn local_date_time(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(time)
        .format(TRUSTED_SINCE_FORMAT)
        .to_string()
}

/// A key trusted for a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustedKey {
    /// An SSH host key.
    Ssh(KnownHostEntry),
    /// The key of an RDP server's certificate.
    Rdp(KnownRdpHost),
    /// The key of an FTPS server's certificate, pinned as an RDP one, in a file of its own.
    Ftps(KnownRdpHost),
    /// The key of a VNC server's certificate, pinned as an FTPS one, in a file of its own.
    Vnc(KnownRdpHost),
}

impl TrustedKey {
    /// The server, as `host:port`.
    #[must_use]
    pub fn address(&self) -> String {
        match self {
            Self::Ssh(entry) => display_address(&entry.host, entry.port),
            Self::Rdp(entry) | Self::Ftps(entry) | Self::Vnc(entry) => {
                display_address(&entry.host, entry.port)
            }
        }
    }

    /// Its SHA-256 fingerprint.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        match self {
            Self::Ssh(entry) => entry.fingerprint.clone(),
            Self::Rdp(entry) | Self::Ftps(entry) | Self::Vnc(entry) => {
                entry.fingerprint.to_string()
            }
        }
    }

    /// When an RDP, FTPS or VNC certificate was trusted, in this computer's time, as the C#
    /// "Trusted since" column; `None` for an SSH key or a certificate recorded without the
    /// time.
    #[must_use]
    pub fn trusted_since(&self) -> Option<String> {
        match self {
            Self::Ssh(_) => None,
            Self::Rdp(entry) | Self::Ftps(entry) | Self::Vnc(entry) => {
                entry.trusted.map(local_date_time)
            }
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
    /// FTPS certificates, in the order of their file.
    pub ftps: Vec<KnownRdpHost>,
    /// VNC certificates, in the order of their file.
    pub vnc: Vec<KnownRdpHost>,
    /// Why a file could not be read, when one could not: its list is then empty.
    pub unreadable: Option<String>,
}

impl TrustedKeys {
    /// How many keys are trusted for the server of `key`, `key` among them, in its list.
    #[must_use]
    pub fn keys_of_server(&self, key: &TrustedKey) -> usize {
        let (list, host, port) = match key {
            TrustedKey::Ssh(entry) => {
                return self
                    .ssh
                    .iter()
                    .filter(|other| other.host == entry.host && other.port == entry.port)
                    .count();
            }
            TrustedKey::Rdp(entry) => (&self.rdp, &entry.host, entry.port),
            TrustedKey::Ftps(entry) => (&self.ftps, &entry.host, entry.port),
            TrustedKey::Vnc(entry) => (&self.vnc, &entry.host, entry.port),
        };
        list.iter()
            .filter(|other| &other.host == host && other.port == port)
            .count()
    }
}

/// A change from the lists of trusted keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustedKeysMessage {
    /// Read the files again: the page is shown, or asked to.
    Refresh,
    /// Copy the whole fingerprint of a key.
    CopyFingerprint(TrustedKey),
    /// Show all that is known of an SSH host key, as the C# "Details" dialog.
    ShowDetails(KnownHostEntry),
    /// Ask whether to forget a key.
    RequestForget(TrustedKey),
    /// Ask whether to forget every certificate trusted for the server of a certificate: an
    /// RDP or FTPS server trusted with more than one. An SSH key is forgotten with its
    /// server's already.
    RequestForgetServer(TrustedKey),
    /// Import the keys of another `known_hosts` file, as the C# "Trusted SSH hosts...".
    Import(super::HostKeysMessage),
    /// Write the keys trusted into the user's OpenSSH `known_hosts`, as the C# "Export
    /// `known_hosts`".
    Export,
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
            TrustedKeysMessage::ShowDetails(entry) => {
                self.dialog = Some(Dialog::TrustedHostKeyDetails(Box::new(entry.clone())));
                Vec::new()
            }
            TrustedKeysMessage::RequestForget(key) => {
                self.dialog = Some(Dialog::ForgetTrustedKey(key.clone()));
                Vec::new()
            }
            TrustedKeysMessage::RequestForgetServer(key) => {
                self.dialog = Some(match key {
                    TrustedKey::Ssh(_) => Dialog::ForgetTrustedKey(key.clone()),
                    TrustedKey::Rdp(_) | TrustedKey::Ftps(_) | TrustedKey::Vnc(_) => {
                        Dialog::ForgetTrustedServer {
                            key: key.clone(),
                            count: self.trusted_keys.keys_of_server(key),
                        }
                    }
                });
                Vec::new()
            }
            TrustedKeysMessage::Import(message) => self.hostkeys_message(message.clone()),
            TrustedKeysMessage::Export => {
                self.export_known_hosts();
                Vec::new()
            }
        }
    }

    /// Writes the keys trusted into `~/.ssh/known_hosts`, as the C# export: in place of
    /// their own lines there, every other line kept; and says how it went.
    fn export_known_hosts(&mut self) {
        let Some(target) =
            std::env::home_dir().map(|home| home.join(OPENSSH_FOLDER).join(OPENSSH_KNOWN_HOSTS))
        else {
            self.tell(Notice::KnownHostsExportFailed(String::new()));
            return;
        };
        let notice = match KnownHosts::new(self.config.known_hosts.clone()).export_to(&target) {
            Ok(report) => Notice::KnownHostsExported {
                count: report.written,
                path: target.display().to_string(),
                skipped: report.skipped,
            },
            Err(error) => Notice::KnownHostsExportFailed(match error {
                KnownHostsError::Unreadable { path, source }
                | KnownHostsError::ExportFailed { path, source } => {
                    format!("{}: {source}", path.display())
                }
                _ => target.display().to_string(),
            }),
        };
        self.tell(notice);
    }

    /// Reads the four files of trusted keys.
    pub(super) fn read_trusted_keys(&mut self) {
        let mut unreadable = Vec::new();
        let ssh = KnownHosts::new(&self.config.known_hosts)
            .entries()
            .unwrap_or_else(|error| {
                unreadable.push(error.to_string());
                Vec::new()
            });
        let rdp = read_certificates(&self.known_rdp_hosts(), &mut unreadable);
        let ftps = read_certificates(&self.known_ftps_hosts(), &mut unreadable);
        let vnc = read_certificates(&self.known_vnc_hosts(), &mut unreadable);
        self.trusted_keys = TrustedKeys {
            ssh,
            rdp,
            ftps,
            vnc,
            unreadable: (!unreadable.is_empty()).then(|| unreadable.join("\n")),
        };
    }

    /// The file the certificate `key` was read from; `None` for an SSH key.
    fn certificates_file(&self, key: &TrustedKey) -> Option<PathBuf> {
        match key {
            TrustedKey::Ssh(_) => None,
            TrustedKey::Rdp(_) => Some(self.known_rdp_hosts()),
            TrustedKey::Ftps(_) => Some(self.known_ftps_hosts()),
            TrustedKey::Vnc(_) => Some(self.known_vnc_hosts()),
        }
    }

    /// Forgets `key`, as confirmed: an SSH server's keys, or that one RDP or FTPS
    /// certificate. The next connection to it asks again, unless it presents another
    /// certificate still trusted for it.
    pub(super) fn forget_trusted_key(&mut self, key: &TrustedKey) {
        let (TrustedKey::Rdp(entry) | TrustedKey::Ftps(entry) | TrustedKey::Vnc(entry), Some(file)) =
            (key, self.certificates_file(key))
        else {
            // An SSH key goes with its server's, as the C# removes it.
            self.forget_server_of(key);
            return;
        };
        let forgotten =
            KnownRdpHosts::new(file).forget_key(&entry.host, entry.port, &entry.fingerprint);
        self.read_trusted_keys();
        match forgotten {
            Ok(_) => self.tell(Notice::CertificateForgotten(key.address())),
            Err(error) => {
                self.dialog = Some(Dialog::StoreError {
                    detail: error.to_string(),
                });
            }
        }
    }

    /// Forgets every key trusted for the server of `key`, in the file it was read from, as
    /// confirmed: the next connection to it asks again.
    pub(super) fn forget_server_of(&mut self, key: &TrustedKey) {
        let forgotten = match (key, self.certificates_file(key)) {
            (
                TrustedKey::Rdp(entry) | TrustedKey::Ftps(entry) | TrustedKey::Vnc(entry),
                Some(file),
            ) => KnownRdpHosts::new(file)
                .forget(&entry.host, entry.port)
                .map_err(|error| error.to_string()),
            (TrustedKey::Ssh(entry), _) => KnownHosts::new(&self.config.known_hosts)
                .forget(&entry.host, entry.port)
                .map_err(|error| error.to_string()),
            (_, None) => return,
        };
        self.read_trusted_keys();
        match forgotten {
            Ok(_) => self.tell(match key {
                TrustedKey::Ssh(_) => Notice::HostKeyRemoved(key.address()),
                TrustedKey::Rdp(_) | TrustedKey::Ftps(_) | TrustedKey::Vnc(_) => {
                    Notice::ServerCertificatesForgotten(key.address())
                }
            }),
            Err(detail) => self.dialog = Some(Dialog::StoreError { detail }),
        }
    }
}

/// The certificates trusted in `file`; none when it cannot be read, and then why is pushed
/// onto `unreadable`.
fn read_certificates(file: &Path, unreadable: &mut Vec<String>) -> Vec<KnownRdpHost> {
    KnownRdpHosts::new(file).entries().unwrap_or_else(|error| {
        unreadable.push(format!("{}: {error}", file.display()));
        Vec::new()
    })
}
