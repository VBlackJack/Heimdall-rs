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

//! The offer to migrate from the legacy PowerShell Heimdall, `RDPManager`, as the C# makes it
//! at start (`App.xaml.cs:1253-1371`): its folder found walking up from the program's, its two
//! files read once, fingerprinted as the C# `LegacyMigrationDecisionPolicy` does so that an
//! offer declined is not made again for the same files, and converted with
//! [`heimdall_core::import::rdpmanager::convert`].
//!
//! The files are read as `PowerShell` writes them: UTF-16LE or UTF-8, with a byte order mark
//! or not. A larger one than [`MAX_FILE_BYTES`] is refused, as is anything else than a file.
//! What is imported is what was read for the offer: files changed after it are offered again
//! at the next start.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use heimdall_core::import::rdpmanager::{
    self, Conversion, ConversionError, MAX_FILE_BYTES, find_installation,
};

use crate::text_codec;

/// The version of the offer, as the C# `CurrentOfferVersion`: an offer declined at an older
/// version is made again.
pub const OFFER_VERSION: u32 = 1;

/// What the fingerprint of the legacy files starts with, as the C# `FingerprintDomain`.
const FINGERPRINT_DOMAIN: &[u8] = b"Heimdall.LegacyMigrationOffer.v1";

/// An offer to migrate the legacy installation found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// The legacy folder, shown whole in the offer.
    pub source: PathBuf,
    /// The fingerprint of its two files, upper-case hexadecimal.
    pub fingerprint: String,
    /// What they give.
    pub conversion: Conversion,
}

/// Why the legacy files were not offered.
#[derive(Debug)]
pub enum OfferProblem {
    /// Something else than a file has the name of one.
    NotAFile(PathBuf),
    /// A file is larger than [`MAX_FILE_BYTES`].
    TooLarge(PathBuf),
    /// The system refused to read a file.
    Unreadable(PathBuf, io::Error),
    /// A file's byte order mark names an encoding its bytes do not follow.
    Undecodable(PathBuf),
    /// The files are not those of the legacy application.
    Unconverted(ConversionError),
}

impl std::fmt::Display for OfferProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAFile(path) => write!(f, "'{}' is not a file", path.display()),
            Self::TooLarge(path) => write!(
                f,
                "file '{}' exceeds {MAX_FILE_BYTES} bytes",
                path.display()
            ),
            Self::Unreadable(path, error) => {
                write!(f, "I/O error reading '{}': {error}", path.display())
            }
            Self::Undecodable(path) => write!(f, "decoding error in '{}'", path.display()),
            Self::Unconverted(error) => error.fmt(f),
        }
    }
}

/// The fingerprint of the legacy `settings` and `servers` bytes, as the C#
/// `CreateOfferAsync`: SHA-256 of the domain, then for each file its length as a big-endian
/// 64-bit signed integer and its bytes; upper-case hexadecimal.
#[must_use]
pub fn fingerprint(settings: &[u8], servers: &[u8]) -> String {
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    context.update(FINGERPRINT_DOMAIN);
    for bytes in [settings, servers] {
        let length = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
        context.update(&length.to_be_bytes());
        context.update(bytes);
    }
    context
        .finish()
        .as_ref()
        .iter()
        .fold(String::new(), |mut hex, byte| {
            // Writing to a string does not fail.
            let _ = write!(hex, "{byte:02X}");
            hex
        })
}

/// The bytes of the file at `path`, at most [`MAX_FILE_BYTES`].
fn read_capped(path: &Path) -> Result<Vec<u8>, OfferProblem> {
    let file =
        File::open(path).map_err(|error| OfferProblem::Unreadable(path.to_owned(), error))?;
    // What was opened, not only what the name pointed at a moment before.
    let metadata = file
        .metadata()
        .map_err(|error| OfferProblem::Unreadable(path.to_owned(), error))?;
    if !metadata.is_file() {
        return Err(OfferProblem::NotAFile(path.to_owned()));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| OfferProblem::Unreadable(path.to_owned(), error))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
        return Err(OfferProblem::TooLarge(path.to_owned()));
    }
    Ok(bytes)
}

/// The text of `bytes`, read from `path`, as .NET reads a file `PowerShell` wrote.
fn text_of(path: &Path, bytes: &[u8]) -> Result<String, OfferProblem> {
    text_codec::decode_as_read_all_text(bytes)
        .map_err(|_| OfferProblem::Undecodable(path.to_owned()))
}

/// The offer for the legacy installation `installation`.
///
/// # Errors
///
/// [`OfferProblem`] when a file cannot be read, or is not the legacy application's.
pub fn offer(installation: &Path) -> Result<Offer, OfferProblem> {
    let settings_path = rdpmanager::settings_file(installation);
    let servers_path = rdpmanager::servers_file(installation);
    let settings = read_capped(&settings_path)?;
    let servers = read_capped(&servers_path)?;
    let conversion = rdpmanager::convert(
        &text_of(&settings_path, &settings)?,
        &text_of(&servers_path, &servers)?,
    )
    .map_err(OfferProblem::Unconverted)?;
    Ok(Offer {
        source: installation.to_owned(),
        fingerprint: fingerprint(&settings, &servers),
        conversion,
    })
}

/// The offer for the legacy installation found walking up from `start`, the program's
/// folder; `None` when there is none, or it cannot be offered, which the log says.
#[must_use]
pub fn detect(start: &Path) -> Option<Offer> {
    let installation = find_installation(start)?;
    log::info!(
        "Legacy Heimdall installation found at '{}'.",
        installation.display()
    );
    match offer(&installation) {
        Ok(offer) => Some(offer),
        Err(problem) => {
            log::warn!("Legacy migration not offered: {problem}.");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use heimdall_core::import::rdpmanager::{LEGACY_APP_FOLDER_NAME, servers_file, settings_file};

    use super::*;

    /// A legacy installation under `root` holding these bytes.
    fn installed(root: &Path, settings: &[u8], servers: &[u8]) -> PathBuf {
        let folder = root.join(LEGACY_APP_FOLDER_NAME);
        fs::create_dir_all(settings_file(&folder).parent().expect("config")).expect("config");
        fs::write(settings_file(&folder), settings).expect("settings");
        fs::write(servers_file(&folder), servers).expect("servers");
        folder
    }

    fn utf16le_with_bom(text: &str) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    }

    /// Computed apart from this code, by the C# algorithm: SHA-256 of
    /// "Heimdall.LegacyMigrationOffer.v1", then for each file its length as a big-endian
    /// `Int64` and its bytes.
    #[test]
    fn the_fingerprint_is_the_csharp_one() {
        assert_eq!(
            fingerprint(b"{}", b"[]"),
            "0A2FDB3E6D8394501096B3E0DF72257E8F2F49674DECE72ACDD8A1434A60D120"
        );
        assert_eq!(
            fingerprint(
                b"\xEF\xBB\xBF{\"DefaultTheme\":\"Tarn\"}",
                &utf16le_with_bom("[]")
            ),
            "11DB821CEFBAD6EB0B018CFD187346EFEA2F1AACCE06BA7DC4CD89BBCDBE466B"
        );
        assert_eq!(
            fingerprint(b"", b""),
            "0BE5847013CCB7760D3C7A5259A6939A15B1B0C68E68C346829643529D895DDB"
        );
        // The length is part of it: the same bytes split otherwise are other files.
        assert_ne!(fingerprint(b"{}[", b"]"), fingerprint(b"{}", b"[]"));
    }

    #[test]
    fn files_written_by_powershell_are_read_whatever_their_byte_order_mark() {
        let root = tempfile::tempdir().expect("dir");
        let servers = r#"[{"Id": "é-1", "DisplayName": "Serveur é", "RemoteServer": "10.0.0.1", "ConnectionType": "SSH"}]"#;
        let mut settings = b"\xEF\xBB\xBF".to_vec();
        settings.extend_from_slice(br#"{"DefaultTheme": "Tarn"}"#);
        let folder = installed(root.path(), &settings, &utf16le_with_bom(servers));
        let made = offer(&folder).expect("offered");
        assert_eq!(made.source, folder);
        assert_eq!(made.conversion.report.profiles.len(), 1);
        assert_eq!(made.conversion.report.profiles[0].name, "Serveur é");
        assert_eq!(
            made.fingerprint,
            fingerprint(&settings, &utf16le_with_bom(servers))
        );
        // UTF-16LE settings, UTF-8 servers without a mark, a single server alone.
        let alone = r#"{"Id": "solo", "DisplayName": "Solo", "RemoteServer": "10.0.0.2", "ConnectionType": "SSH"}"#;
        let folder = installed(
            root.path(),
            &utf16le_with_bom(r#"{"DefaultTheme": "Tarn"}"#),
            alone.as_bytes(),
        );
        let made = offer(&folder).expect("offered");
        assert_eq!(made.conversion.examined, 1);
        assert_eq!(made.conversion.report.profiles[0].name, "Solo");
    }

    #[test]
    fn an_oversize_file_a_folder_or_a_broken_mark_is_not_offered() {
        let root = tempfile::tempdir().expect("dir");
        let folder = installed(root.path(), b"{}", b"[]");
        let file = File::options()
            .write(true)
            .open(servers_file(&folder))
            .expect("open");
        file.set_len(MAX_FILE_BYTES + 1).expect("sized");
        drop(file);
        assert!(matches!(offer(&folder), Err(OfferProblem::TooLarge(_))));

        // A UTF-16 file cut in the middle of a character.
        fs::write(servers_file(&folder), [0xFF, 0xFE, 0x5B]).expect("written");
        assert!(matches!(offer(&folder), Err(OfferProblem::Undecodable(_))));

        fs::write(servers_file(&folder), b"{ broken").expect("written");
        assert!(matches!(offer(&folder), Err(OfferProblem::Unconverted(_))));

        fs::remove_file(settings_file(&folder)).expect("removed");
        fs::create_dir_all(settings_file(&folder)).expect("folder");
        assert!(offer(&folder).is_err());
    }

    #[test]
    fn detection_walks_up_from_the_folder_it_is_given() {
        let root = tempfile::tempdir().expect("dir");
        let start = root.path().join("tools").join("heimdall");
        fs::create_dir_all(&start).expect("start");
        assert_eq!(detect(&start), None);
        let folder = installed(root.path(), b"{}", b"[]");
        let found = detect(&start).expect("found");
        assert_eq!(found.source, folder);
        assert_eq!(found.fingerprint, fingerprint(b"{}", b"[]"));
        // Found but not readable as the legacy files: not offered.
        fs::write(settings_file(&folder), b"[]").expect("written");
        assert_eq!(detect(&start), None);
    }
}
