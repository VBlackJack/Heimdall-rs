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

//! Saved passwords: what one is for, and how it is written in a vault.
//!
//! A password is saved together with the server it was accepted by: protocol, host, port and
//! account. It is given back only to that same server. A profile whose host was changed after
//! the password was saved no longer matches, so the password is never sent to the new host,
//! and a gateway on the way, being another server, never receives it.

use zeroize::Zeroizing;

use crate::profile::ProfileId;

/// Prefix of the vault entries holding a profile's password.
const PASSWORD_ENTRY_PREFIX: &str = "password/";

/// Version of the entry encoding.
const ENCODING_VERSION: u8 = 1;

/// What a password opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialProtocol {
    /// An SSH account.
    Ssh,
    /// An RDP account.
    Rdp,
    /// A VNC server, which has no account.
    Vnc,
    /// An FTP account.
    Ftp,
    /// A `WinRM` account, handed to `PowerShell` for `Enter-PSSession`.
    WinRm,
}

impl CredentialProtocol {
    fn code(self) -> u8 {
        match self {
            Self::Ssh => 1,
            Self::Rdp => 2,
            Self::Vnc => 3,
            Self::Ftp => 4,
            Self::WinRm => 5,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Ssh),
            2 => Some(Self::Rdp),
            3 => Some(Self::Vnc),
            4 => Some(Self::Ftp),
            5 => Some(Self::WinRm),
            // A protocol of a later build: the entry reads as nothing, as an older build
            // reads this one's `WinRM` entries.
            _ => None,
        }
    }
}

/// The server a password is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// Protocol.
    pub protocol: CredentialProtocol,
    /// Host name or address, as the profile gives it.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Account; `None` where the protocol has none.
    pub username: Option<String>,
}

impl Endpoint {
    /// Whether `other` is the same server and account. Host names are compared without case,
    /// as DNS does; the account exactly.
    #[must_use]
    pub fn is(&self, other: &Self) -> bool {
        self.protocol == other.protocol
            && self.port == other.port
            && self.host.eq_ignore_ascii_case(&other.host)
            && self.username == other.username
    }
}

/// A password and the server it is for.
#[derive(Clone, PartialEq, Eq)]
pub struct SavedPassword {
    /// Server.
    pub endpoint: Endpoint,
    /// Password.
    pub password: Zeroizing<String>,
}

impl std::fmt::Debug for SavedPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SavedPassword")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

/// The account an RDP password is for: `user` in `domain` when the profile names one, as
/// `DOMAIN\user`. The one spelling both saving a password and giving it back use.
#[must_use]
pub fn rdp_account(domain: Option<&str>, user: &str) -> String {
    match domain.filter(|domain| !domain.is_empty()) {
        Some(domain) => format!("{domain}\\{user}"),
        None => user.to_owned(),
    }
}

/// Name of the vault entry holding `profile`'s password.
#[must_use]
pub fn password_entry(profile: &ProfileId) -> String {
    format!("{PASSWORD_ENTRY_PREFIX}{}", profile.as_str())
}

/// `saved` as vault entry bytes: a version, the protocol, then each text with its length.
#[must_use]
pub fn encode(saved: &SavedPassword) -> Zeroizing<Vec<u8>> {
    let endpoint = &saved.endpoint;
    let mut bytes = Zeroizing::new(Vec::new());
    bytes.push(ENCODING_VERSION);
    bytes.push(endpoint.protocol.code());
    bytes.extend_from_slice(&endpoint.port.to_be_bytes());
    push_text(&mut bytes, &endpoint.host);
    match &endpoint.username {
        Some(username) => {
            bytes.push(1);
            push_text(&mut bytes, username);
        }
        None => bytes.push(0),
    }
    push_text(&mut bytes, &saved.password);
    bytes
}

/// The password in vault entry `bytes`; `None` for anything this version did not write.
#[must_use]
pub fn decode(bytes: &[u8]) -> Option<SavedPassword> {
    let mut reader = Reader(bytes);
    if reader.byte()? != ENCODING_VERSION {
        return None;
    }
    let protocol = CredentialProtocol::from_code(reader.byte()?)?;
    let port = u16::from_be_bytes([reader.byte()?, reader.byte()?]);
    let host = reader.text()?;
    let username = match reader.byte()? {
        0 => None,
        1 => Some(reader.text()?),
        _ => return None,
    };
    let password = Zeroizing::new(reader.text()?);
    if !reader.0.is_empty() {
        return None;
    }
    Some(SavedPassword {
        endpoint: Endpoint {
            protocol,
            host,
            port,
            username,
        },
        password,
    })
}

/// Prefix of the vault entries holding the passphrase of a profile's SSH key.
const PASSPHRASE_ENTRY_PREFIX: &str = "passphrase/";

/// The passphrase of an SSH key file, and the file it unlocks. It never leaves this computer:
/// it is given back only to unlock that same file, for the profile or gateway it is saved
/// with.
#[derive(Clone, PartialEq, Eq)]
pub struct SavedPassphrase {
    /// The key file, as the profile names it.
    pub key_path: String,
    /// Passphrase.
    pub passphrase: Zeroizing<String>,
}

impl std::fmt::Debug for SavedPassphrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SavedPassphrase")
            .field("key_path", &self.key_path)
            .finish_non_exhaustive()
    }
}

/// Name of the vault entry holding the passphrase of `profile`'s key.
#[must_use]
pub fn passphrase_entry(profile: &ProfileId) -> String {
    format!("{PASSPHRASE_ENTRY_PREFIX}{}", profile.as_str())
}

/// `saved` as vault entry bytes: a version, then the key file and the passphrase, each with
/// its length.
#[must_use]
pub fn encode_passphrase(saved: &SavedPassphrase) -> Zeroizing<Vec<u8>> {
    let mut bytes = Zeroizing::new(Vec::new());
    bytes.push(ENCODING_VERSION);
    push_text(&mut bytes, &saved.key_path);
    push_text(&mut bytes, &saved.passphrase);
    bytes
}

/// The passphrase in vault entry `bytes`; `None` for anything this version did not write.
#[must_use]
pub fn decode_passphrase(bytes: &[u8]) -> Option<SavedPassphrase> {
    let mut reader = Reader(bytes);
    if reader.byte()? != ENCODING_VERSION {
        return None;
    }
    let key_path = reader.text()?;
    let passphrase = Zeroizing::new(reader.text()?);
    reader.0.is_empty().then_some(SavedPassphrase {
        key_path,
        passphrase,
    })
}

/// Prefix of the vault entries holding a Citrix profile's launch line.
const CITRIX_LAUNCH_ENTRY_PREFIX: &str = "citrix-launch/";

/// Name of the vault entry holding the line Citrix Workspace's `SelfService.exe` launches
/// `profile`'s application with, read from its cache: pre-authenticated, it is kept as a
/// password is, as the C# vault-encrypts its `CitrixLaunchCommandLine`.
#[must_use]
pub fn citrix_launch_entry(profile: &ProfileId) -> String {
    format!("{CITRIX_LAUNCH_ENTRY_PREFIX}{}", profile.as_str())
}

/// `line` as vault entry bytes: a version, then the line with its length.
#[must_use]
pub fn encode_citrix_launch(line: &str) -> Zeroizing<Vec<u8>> {
    let mut bytes = Zeroizing::new(Vec::new());
    bytes.push(ENCODING_VERSION);
    push_text(&mut bytes, line);
    bytes
}

/// The launch line in vault entry `bytes`; `None` for anything this version did not write.
#[must_use]
pub fn decode_citrix_launch(bytes: &[u8]) -> Option<Zeroizing<String>> {
    let mut reader = Reader(bytes);
    if reader.byte()? != ENCODING_VERSION {
        return None;
    }
    let line = Zeroizing::new(reader.text()?);
    reader.0.is_empty().then_some(line)
}

fn push_text(bytes: &mut Vec<u8>, text: &str) {
    // A text longer than 4 GiB cannot be typed into a password field.
    let length = u32::try_from(text.len()).unwrap_or(u32::MAX);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(&text.as_bytes()[..length as usize]);
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        let (&first, rest) = self.0.split_first()?;
        self.0 = rest;
        Some(first)
    }

    fn text(&mut self) -> Option<String> {
        let length = u32::from_be_bytes([self.byte()?, self.byte()?, self.byte()?, self.byte()?]);
        let length = usize::try_from(length).ok()?;
        if self.0.len() < length {
            return None;
        }
        let (text, rest) = self.0.split_at(length);
        self.0 = rest;
        String::from_utf8(text.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint() -> Endpoint {
        Endpoint {
            protocol: CredentialProtocol::Ssh,
            host: "Web.Lab".to_owned(),
            port: 22,
            username: Some("admin".to_owned()),
        }
    }

    fn saved() -> SavedPassword {
        SavedPassword {
            endpoint: endpoint(),
            password: Zeroizing::new("p\u{e4}ss w\u{f6}rd".to_owned()),
        }
    }

    #[test]
    fn an_entry_reads_back_as_written() {
        assert_eq!(decode(&encode(&saved())), Some(saved()));
        let vnc = SavedPassword {
            endpoint: Endpoint {
                protocol: CredentialProtocol::Vnc,
                username: None,
                ..endpoint()
            },
            password: Zeroizing::new(String::new()),
        };
        assert_eq!(decode(&encode(&vnc)), Some(vnc));
    }

    #[test]
    fn a_damaged_entry_reads_as_nothing() {
        let bytes = encode(&saved());
        for cut in 0..bytes.len() {
            assert_eq!(decode(&bytes[..cut]), None, "cut at {cut}");
        }
        let mut longer = bytes.to_vec();
        longer.push(0);
        assert_eq!(decode(&longer), None, "trailing byte");
        let mut version = bytes.to_vec();
        version[0] = 2;
        assert_eq!(decode(&version), None, "unknown version");
        let mut protocol = bytes.to_vec();
        protocol[1] = 9;
        assert_eq!(decode(&protocol), None, "unknown protocol");
    }

    #[test]
    fn a_winrm_entry_reads_back_and_an_unknown_protocol_reads_as_nothing() {
        let winrm = SavedPassword {
            endpoint: Endpoint {
                protocol: CredentialProtocol::WinRm,
                host: "dc01.lab".to_owned(),
                port: 5985,
                username: Some("LAB\\admin".to_owned()),
            },
            password: Zeroizing::new("s3cret".to_owned()),
        };
        let bytes = encode(&winrm);
        assert_eq!(bytes[1], 5, "the next free code");
        assert_eq!(decode(&bytes), Some(winrm));
        // What an older build does with it: a code it does not know reads as nothing.
        for unknown in [0, 6, u8::MAX] {
            let mut later = bytes.to_vec();
            later[1] = unknown;
            assert_eq!(decode(&later), None, "code {unknown}");
        }
    }

    #[test]
    fn the_same_server_is_the_same_whatever_the_case_of_its_name() {
        let other = Endpoint {
            host: "web.lab".to_owned(),
            ..endpoint()
        };
        assert!(endpoint().is(&other));
    }

    #[test]
    fn another_host_port_account_or_protocol_is_another_server() {
        let differs = [
            Endpoint {
                host: "web2.lab".to_owned(),
                ..endpoint()
            },
            Endpoint {
                port: 2222,
                ..endpoint()
            },
            Endpoint {
                username: Some("Admin".to_owned()),
                ..endpoint()
            },
            Endpoint {
                username: None,
                ..endpoint()
            },
            Endpoint {
                protocol: CredentialProtocol::Rdp,
                ..endpoint()
            },
        ];
        for other in differs {
            assert!(!endpoint().is(&other), "{other:?}");
        }
    }

    #[test]
    fn a_password_never_shows_in_debug_output() {
        let shown = format!("{:?}", saved());
        assert!(!shown.contains("p\u{e4}ss"), "{shown}");
    }

    #[test]
    fn an_rdp_account_carries_its_domain() {
        assert_eq!(rdp_account(Some("CORP"), "admin"), "CORP\\admin");
        assert_eq!(rdp_account(Some(""), "admin"), "admin");
        assert_eq!(rdp_account(None, "admin"), "admin");
    }

    #[test]
    fn entries_are_named_by_profile() {
        assert_eq!(password_entry(&ProfileId::new("a1")), "password/a1");
    }

    #[test]
    fn a_passphrase_reads_back_with_its_key_file_and_nothing_else_does() {
        let saved = SavedPassphrase {
            key_path: "C:/keys/id_ed25519".to_owned(),
            passphrase: Zeroizing::new("correct horse".to_owned()),
        };
        let bytes = encode_passphrase(&saved);
        assert_eq!(decode_passphrase(&bytes), Some(saved.clone()));
        assert_eq!(passphrase_entry(&ProfileId::new("web")), "passphrase/web");
        assert_ne!(
            passphrase_entry(&ProfileId::new("web")),
            password_entry(&ProfileId::new("web"))
        );
        let mut longer = bytes.to_vec();
        longer.push(0);
        assert_eq!(decode_passphrase(&longer), None, "trailing bytes");
        assert_eq!(
            decode_passphrase(&bytes[..bytes.len() - 1]),
            None,
            "cut short"
        );
        assert_eq!(decode_passphrase(&[]), None);
        assert!(
            !format!("{saved:?}").contains("correct horse"),
            "never in a log"
        );
    }

    #[test]
    fn a_citrix_launch_line_reads_back_and_nothing_else_does() {
        let line = "-qlaunch \"Excel 2024\" -s store";
        let bytes = encode_citrix_launch(line);
        assert_eq!(
            decode_citrix_launch(&bytes).as_deref().map(String::as_str),
            Some(line)
        );
        assert_eq!(citrix_launch_entry(&ProfileId::new("c")), "citrix-launch/c");
        assert_eq!(decode_citrix_launch(&bytes[..bytes.len() - 1]), None);
        let mut longer = bytes.to_vec();
        longer.push(0);
        assert_eq!(decode_citrix_launch(&longer), None);
    }
}
