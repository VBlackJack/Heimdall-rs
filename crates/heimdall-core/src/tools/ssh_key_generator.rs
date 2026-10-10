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

//! The SSH Key Generator's engine, as the C# `SshKeyGeneratorView` makes a pair
//! (`SshKeyGeneratorView.xaml.cs:257-562`) and `SshKeyFileWriter` writes it
//! (`SshKeyFileWriter.cs:29-61`): RSA 2048, RSA 4096 or Ed25519; the public key as an
//! OpenSSH line, `type base64 comment`; the private key in PKCS#8 PEM, encrypted with
//! PBES2 (PBKDF2-HMAC-SHA256, 600,000 rounds, AES-256-CBC) when a passphrase is given; the
//! SHA-256 fingerprint as OpenSSH prints it.
//!
//! The private key is a secret: held in memory wiped when dropped, never written out by
//! `Debug`, and written to disk only where the user saves it, readable by them alone.

use std::fmt;
use std::io;
use std::path::Path;

use sealvault::keys::KeyPair;
use zeroize::Zeroizing;

use super::pkcs8_pem::{self, PemError};
use super::private_file;

/// Rounds of PBKDF2 a passphrase is stretched with, as the C# `PbeIterationCount`.
pub const PBE_ITERATION_COUNT: u32 = 600_000;

/// Bits of an RSA key of the first size, as the C# `Rsa2048KeySize`.
pub const RSA_2048_BITS: usize = 2048;

/// Bits of an RSA key of the second size, as the C# `Rsa4096KeySize`.
pub const RSA_4096_BITS: usize = 4096;

/// A carriage return, which a key brought from elsewhere may end its lines with.
const CARRIAGE_RETURN: char = '\r';

/// What ends every line of a key file, as the C# `SshKeyFileWriter.LineFeed`.
const LINE_FEED: &str = "\n";

/// The extension of a public key file, as the C# `PublicKeyFileExtension`.
pub const PUBLIC_KEY_EXTENSION: &str = "pub";

/// The extension of a private key file, as the C# `PrivateKeyFileExtension`.
pub const PRIVATE_KEY_EXTENSION: &str = "pem";

/// The user's name, as .NET's `Environment.UserName` reads it.
#[cfg(windows)]
const USER_VARIABLE: &str = "USERNAME";
#[cfg(not(windows))]
const USER_VARIABLE: &str = "USER";

/// This computer's name on Windows, as .NET's `Environment.MachineName` reads it.
#[cfg(windows)]
const HOST_VARIABLE: &str = "COMPUTERNAME";

/// Where Linux says the host name.
#[cfg(unix)]
const HOSTNAME_FILE: &str = "/proc/sys/kernel/hostname";

/// The host name a shell exports, elsewhere.
#[cfg(not(windows))]
const HOST_VARIABLE: &str = "HOSTNAME";

/// The comment a new key starts with, `user@host`, as the C# `Initialize` writes
/// `Environment.UserName@Environment.MachineName`; what is unknown left empty.
#[must_use]
pub fn default_comment() -> String {
    let user = std::env::var(USER_VARIABLE).unwrap_or_default();
    #[cfg(unix)]
    let file_host = std::fs::read_to_string(HOSTNAME_FILE)
        .ok()
        .map(|host| host.trim().to_owned())
        .filter(|host| !host.is_empty());
    #[cfg(not(unix))]
    let file_host: Option<String> = None;
    let host = file_host
        .or_else(|| std::env::var(HOST_VARIABLE).ok())
        .unwrap_or_default();
    format!("{user}@{host}")
}

/// A key's algorithm, in the order of the C# algorithm box (`AlgorithmIndex*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum SshKeyAlgorithm {
    /// RSA, 2048 bits: the box's first entry.
    #[default]
    Rsa2048,
    /// RSA, 4096 bits.
    Rsa4096,
    /// Ed25519.
    Ed25519,
}

impl SshKeyAlgorithm {
    /// Every algorithm, in the box's order.
    pub const ALL: [Self; 3] = [Self::Rsa2048, Self::Rsa4096, Self::Ed25519];

    /// The name a key file of this algorithm is offered under, as the C#'s `id_rsa` and
    /// `id_ed25519`.
    #[must_use]
    pub const fn file_stem(self) -> &'static str {
        match self {
            Self::Rsa2048 | Self::Rsa4096 => "id_rsa",
            Self::Ed25519 => "id_ed25519",
        }
    }
}

/// Why no key was made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SshKeyError {
    /// The system's generator could not be read.
    #[error("the system's random generator could not be read")]
    Randomness,
    /// The key could not be made or written.
    #[error("{0}")]
    Encoding(String),
}

impl From<sealvault::Error> for SshKeyError {
    fn from(error: sealvault::Error) -> Self {
        match error {
            sealvault::Error::Randomness(_) => Self::Randomness,
            other => Self::Encoding(other.to_string()),
        }
    }
}

impl From<PemError> for SshKeyError {
    fn from(error: PemError) -> Self {
        match error {
            PemError::Randomness => Self::Randomness,
            PemError::Encoding(detail) => Self::Encoding(detail),
        }
    }
}

/// A key pair made.
pub struct GeneratedSshKey {
    /// The public key as an OpenSSH line.
    pub public_key: String,
    /// The private key in PKCS#8 PEM, encrypted when a passphrase was given.
    pub private_key_pem: Zeroizing<String>,
    /// The SHA-256 fingerprint, `SHA256:` then base64 without padding.
    pub fingerprint: String,
    /// The algorithm it was made with.
    pub algorithm: SshKeyAlgorithm,
}

impl fmt::Debug for GeneratedSshKey {
    /// The private key is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeneratedSshKey")
            .field("public_key", &self.public_key)
            .field("fingerprint", &self.fingerprint)
            .field("algorithm", &self.algorithm)
            .finish_non_exhaustive()
    }
}

/// A key pair of `algorithm`, its public line ended by `comment` once trimmed, its private
/// key encrypted with `passphrase` unless it is empty, as the C# `GenerateKeyPairAsync`.
///
/// # Errors
///
/// [`SshKeyError`] when the system's generator cannot be read or the key not encoded.
pub fn generate(
    algorithm: SshKeyAlgorithm,
    comment: &str,
    passphrase: &str,
) -> Result<GeneratedSshKey, SshKeyError> {
    let pair = match algorithm {
        SshKeyAlgorithm::Rsa2048 => KeyPair::rsa(RSA_2048_BITS)?,
        SshKeyAlgorithm::Rsa4096 => KeyPair::rsa(RSA_4096_BITS)?,
        SshKeyAlgorithm::Ed25519 => KeyPair::ed25519()?,
    };
    let public_key = pair.openssh_public(comment.trim())?;
    let private_key_pem = if passphrase.is_empty() {
        pkcs8_pem::private_key_pem(pair.pkcs8_der())?
    } else {
        pkcs8_pem::encrypted_private_key_pem(pair.pkcs8_der(), passphrase, PBE_ITERATION_COUNT)?
    };
    Ok(GeneratedSshKey {
        public_key,
        private_key_pem,
        fingerprint: pair.openssh_fingerprint(),
        algorithm,
    })
}

/// `text` with its line breaks made line feeds, as the C# `NormalizeLineEndings`.
fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", LINE_FEED).replace('\r', LINE_FEED)
}

/// What a public key file holds: the line, trimmed at its end, then one line feed, as the
/// C# `WritePublicKey`.
#[must_use]
pub fn public_key_file_text(line: &str) -> String {
    normalize_line_endings(line.trim_end()) + LINE_FEED
}

/// Writes the public `line` at `path`: UTF-8 without a byte order mark, one line feed, as
/// the C# `SshKeyFileWriter.WritePublicKey`.
///
/// # Errors
///
/// The write's error.
pub fn write_public_key(path: &Path, line: &str) -> io::Result<()> {
    std::fs::write(path, public_key_file_text(line))
}

/// Writes `private_key_pem` at `path`, its line breaks made line feeds, readable by the
/// current user alone, replacing any file there, as the C# `SshKeyFileWriter.WritePrivateKey`.
///
/// # Errors
///
/// The write's error.
pub fn write_private_key(path: &Path, private_key_pem: &str) -> io::Result<()> {
    // The PEM made here already ends its lines with line feeds: it is written as it is,
    // without a copy; only a key brought with other line breaks is copied, and wiped.
    if !private_key_pem.contains(CARRIAGE_RETURN) {
        return private_file::write_private(path, private_key_pem.as_bytes());
    }
    let text = Zeroizing::new(normalize_line_endings(private_key_pem));
    private_file::write_private(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use pkcs8::der::Decode as _;
    use rsa::pkcs8::DecodePrivateKey as _;
    use rsa::traits::PublicKeyParts as _;
    use ssh_key::public::{KeyData, RsaPublicKey};
    use ssh_key::{HashAlg, PublicKey};

    use super::*;

    /// The PKCS#8 encoding of an Ed25519 key before its 32-byte seed, as RFC 8410 writes it:
    /// version 0, the algorithm 1.3.101.112, the seed in an octet string inside the octet
    /// string.
    const ED25519_PKCS8_PREFIX: [u8; 16] = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];

    /// The private key's DER, read back from its PEM, decrypted with `passphrase` if any.
    fn private_der(pem: &str, passphrase: Option<&str>) -> Vec<u8> {
        let (label, der) = pkcs8::der::pem::decode_vec(pem.as_bytes()).expect("pem");
        match passphrase {
            None => {
                assert_eq!(label, "PRIVATE KEY");
                der
            }
            Some(passphrase) => {
                assert_eq!(label, "ENCRYPTED PRIVATE KEY");
                let info = pkcs8::EncryptedPrivateKeyInfoRef::from_der(&der).expect("der");
                info.decrypt(passphrase)
                    .expect("decrypted")
                    .as_bytes()
                    .to_vec()
            }
        }
    }

    #[test]
    fn an_ed25519_key_round_trips_through_ssh_key_and_its_pkcs8_seed() {
        let key = generate(SshKeyAlgorithm::Ed25519, "  user@host  ", "").expect("made");
        assert!(
            key.public_key
                .starts_with("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA")
        );
        assert!(
            key.public_key.ends_with(" user@host"),
            "the comment trimmed"
        );
        let parsed = PublicKey::from_openssh(&key.public_key).expect("valid OpenSSH");
        assert_eq!(parsed.comment().as_bytes(), b"user@host");
        assert_eq!(
            parsed.fingerprint(HashAlg::Sha256).to_string(),
            key.fingerprint
        );
        assert!(key.fingerprint.starts_with("SHA256:") && !key.fingerprint.ends_with('='));
        // The seed in the PKCS#8 makes the public key in the OpenSSH line.
        let der = private_der(&key.private_key_pem, None);
        assert_eq!(der.len(), 48);
        assert_eq!(der[..16], ED25519_PKCS8_PREFIX);
        let seed: [u8; 32] = der[16..].try_into().expect("seed");
        let pair = ssh_key::private::Ed25519Keypair::from_seed(&seed);
        assert_eq!(
            parsed.key_data(),
            &KeyData::Ed25519(pair.public),
            "the private key is the public one's"
        );
    }

    #[test]
    fn an_rsa_key_has_its_size_and_its_private_half_matches_its_public_line() {
        let key = generate(SshKeyAlgorithm::Rsa2048, "", "").expect("made");
        assert!(key.public_key.starts_with("ssh-rsa AAAAB3NzaC1yc2EAAAA"));
        assert_eq!(
            key.public_key.split(' ').count(),
            2,
            "no comment, no trailing field"
        );
        let parsed = PublicKey::from_openssh(&key.public_key).expect("valid OpenSSH");
        let KeyData::Rsa(public) = parsed.key_data() else {
            panic!("an RSA key");
        };
        assert_eq!(public.key_size(), 2048);
        let private = rsa::RsaPrivateKey::from_pkcs8_der(&private_der(&key.private_key_pem, None))
            .expect("PKCS#8");
        assert_eq!(private.size() * 8, 2048);
        assert_eq!(
            &RsaPublicKey::try_from(&private.to_public_key()).expect("public"),
            public
        );
    }

    #[test]
    fn an_rsa_4096_key_has_4096_bits() {
        let key = generate(SshKeyAlgorithm::Rsa4096, "c", "").expect("made");
        let parsed = PublicKey::from_openssh(&key.public_key).expect("valid OpenSSH");
        let KeyData::Rsa(public) = parsed.key_data() else {
            panic!("an RSA key");
        };
        assert_eq!(public.key_size(), 4096);
    }

    #[test]
    fn a_passphrase_encrypts_the_private_key_with_pbes2_aes_256_cbc() {
        let key = generate(SshKeyAlgorithm::Ed25519, "c", "correct horse").expect("made");
        assert!(
            key.private_key_pem
                .starts_with("-----BEGIN ENCRYPTED PRIVATE KEY-----\n")
        );
        let der = private_der(&key.private_key_pem, Some("correct horse"));
        assert_eq!(der[..16], ED25519_PKCS8_PREFIX);
    }

    #[test]
    fn the_passphrase_is_stretched_over_the_csharp_rounds() {
        let key = generate(SshKeyAlgorithm::Ed25519, "", "pw").expect("made");
        let (_, der) = pkcs8::der::pem::decode_vec(key.private_key_pem.as_bytes()).expect("pem");
        let info = pkcs8::EncryptedPrivateKeyInfoRef::from_der(&der).expect("der");
        let rounds = info
            .encryption_algorithm
            .pbes2()
            .and_then(|scheme| scheme.kdf.pbkdf2())
            .map(|kdf| kdf.iteration_count);
        assert_eq!(rounds, Some(600_000));
    }

    #[test]
    fn the_private_key_is_never_written_out_and_files_are_named_as_the_csharp() {
        let key = generate(SshKeyAlgorithm::Ed25519, "c", "").expect("made");
        let shown = format!("{key:?}");
        assert!(!shown.contains("PRIVATE KEY"), "{shown}");
        assert_eq!(SshKeyAlgorithm::Rsa4096.file_stem(), "id_rsa");
        assert_eq!(SshKeyAlgorithm::Ed25519.file_stem(), "id_ed25519");
    }

    #[test]
    fn a_public_key_file_starts_with_its_type_and_ends_with_one_line_feed() {
        // As the C# SshKeyFileWriterTests.
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("id_ed25519.pub");
        write_public_key(&path, "ssh-ed25519 AAAAC3 comment").expect("written");
        assert_eq!(
            std::fs::read(&path).expect("read"),
            b"ssh-ed25519 AAAAC3 comment\n"
        );
        assert_eq!(public_key_file_text("a b c \r\n"), "a b c\n");
    }

    #[test]
    fn a_private_key_file_has_line_feeds_only_and_replaces_what_was_there() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("id_ed25519");
        std::fs::write(&path, "stale").expect("written");
        write_private_key(
            &path,
            "-----BEGIN PRIVATE KEY-----\r\nAAAA\r\n-----END PRIVATE KEY-----\r\n",
        )
        .expect("written");
        assert_eq!(
            std::fs::read(&path).expect("read"),
            b"-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n"
        );
    }
}
