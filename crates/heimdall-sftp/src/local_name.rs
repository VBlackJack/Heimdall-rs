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

//! Server-supplied names turned into local file names, or refused.
//!
//! A download writes files named by the server. A hostile or careless server can name an
//! entry `..`, `a/b`, `C:x`, `aux`, `name:stream` or `x.`, and each of those, used as is,
//! writes outside the target folder, onto a device, into an alternate data stream, or over
//! another file. Every name crosses [`LocalName::from_remote`] before it touches the disk,
//! and every folder's names go through [`FolderNames`], which refuses two names the local
//! file system would take for the same file.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt::Write as _;

use thiserror::Error;

/// Longest name: 255 bytes on Unix file systems, 255 UTF-16 units on Windows.
const MAX_NAME_LENGTH: usize = 255;

/// Device names Windows reserves in every folder, whatever the extension.
const WINDOWS_RESERVED: [&str; 30] = [
    "CON",
    "PRN",
    "AUX",
    "NUL",
    "COM0",
    "COM1",
    "COM2",
    "COM3",
    "COM4",
    "COM5",
    "COM6",
    "COM7",
    "COM8",
    "COM9",
    "LPT0",
    "LPT1",
    "LPT2",
    "LPT3",
    "LPT4",
    "LPT5",
    "LPT6",
    "LPT7",
    "LPT8",
    "LPT9",
    "COM\u{b9}",
    "COM\u{b2}",
    "COM\u{b3}",
    "LPT\u{b9}",
    "LPT\u{b2}",
    "LPT\u{b3}",
];

/// Console names Windows also reserves.
const WINDOWS_CONSOLE: [&str; 2] = ["CONIN$", "CONOUT$"];

/// Characters Windows refuses in a name, besides control characters.
const WINDOWS_FORBIDDEN: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// The file system rules a name must satisfy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rules {
    /// Linux, macOS and other Unix systems: any bytes but `/` and NUL.
    Unix,
    /// Windows: no reserved characters, device names, trailing dot or space.
    Windows,
}

impl Rules {
    /// The rules of the system this runs on.
    #[must_use]
    pub fn native() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }

    /// Whether names differing only in letter case are the same file. Case-insensitive
    /// also covers macOS's default file system, so Unix errs on the safe side there.
    fn folds_case(self) -> bool {
        match self {
            Self::Windows => true,
            Self::Unix => cfg!(target_os = "macos"),
        }
    }
}

/// Why a server name cannot be a local file name.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LocalNameError {
    /// Empty, `.` or `..`.
    #[error("not a file name")]
    NotAName,
    /// Holds a path separator: the name would reach another folder.
    #[error("contains a path separator")]
    Separator,
    /// Holds a NUL or another control character.
    #[error("contains a control character")]
    Control,
    /// Holds a character Windows refuses, such as `:` (alternate data streams).
    #[error("contains {0:?}")]
    Forbidden(char),
    /// A Windows device name, such as `aux` or `com1.txt`.
    #[error("a reserved device name")]
    Reserved,
    /// Ends with a dot or a space, which Windows silently strips.
    #[error("ends with a dot or a space")]
    TrailingDotOrSpace,
    /// Longer than the file system allows.
    #[error("too long")]
    TooLong,
}

/// A name usable in a local folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalName {
    /// The name for the local file system.
    pub name: OsString,
    /// The server's bytes could not be kept as they are (not UTF-8 on Windows) and were
    /// escaped: the user must be told the file got another name.
    pub escaped: bool,
}

impl LocalName {
    /// The local name for `remote`, a single name as the server listed it.
    ///
    /// # Errors
    ///
    /// [`LocalNameError`] when the name cannot be used safely.
    pub fn from_remote(remote: &[u8], rules: Rules) -> Result<Self, LocalNameError> {
        if remote.is_empty() || remote == b"." || remote == b".." {
            return Err(LocalNameError::NotAName);
        }
        if remote.contains(&b'/') {
            return Err(LocalNameError::Separator);
        }
        if remote.contains(&0) {
            return Err(LocalNameError::Control);
        }
        match rules {
            Rules::Unix => unix_name(remote),
            Rules::Windows => windows_name(remote),
        }
    }
}

fn unix_name(remote: &[u8]) -> Result<LocalName, LocalNameError> {
    if remote.len() > MAX_NAME_LENGTH {
        return Err(LocalNameError::TooLong);
    }
    Ok(LocalName {
        name: os_from_unix_bytes(remote),
        escaped: false,
    })
}

#[cfg(unix)]
fn os_from_unix_bytes(bytes: &[u8]) -> OsString {
    use std::os::unix::ffi::OsStringExt as _;
    OsString::from_vec(bytes.to_vec())
}

#[cfg(not(unix))]
fn os_from_unix_bytes(bytes: &[u8]) -> OsString {
    // Only reached when Unix rules are checked on another system, in tests.
    OsString::from(String::from_utf8_lossy(bytes).into_owned())
}

/// Valid UTF-8 kept; every other byte and every `%` written `%XX`, so two non-UTF-8 names
/// never escape to the same text. A UTF-8 name keeps its `%` (`50%.txt` stays as it is),
/// so it can coincide with an escaped one: [`FolderNames`] refuses the second.
fn escaped_text(bytes: &[u8]) -> (String, bool) {
    let mut text = String::with_capacity(bytes.len());
    let mut escaped = false;
    for chunk in bytes.utf8_chunks() {
        for character in chunk.valid().chars() {
            if character == '%' {
                text.push_str("%25");
            } else {
                text.push(character);
            }
        }
        for byte in chunk.invalid() {
            escaped = true;
            let _ = write!(text, "%{byte:02X}");
        }
    }
    (text, escaped)
}

fn windows_name(remote: &[u8]) -> Result<LocalName, LocalNameError> {
    let (text, escaped) = if std::str::from_utf8(remote).is_ok() {
        (String::from_utf8_lossy(remote).into_owned(), false)
    } else {
        escaped_text(remote)
    };
    if text.chars().any(char::is_control) {
        return Err(LocalNameError::Control);
    }
    if text.contains('\\') || text.contains('/') {
        return Err(LocalNameError::Separator);
    }
    if let Some(character) = text.chars().find(|c| WINDOWS_FORBIDDEN.contains(c)) {
        return Err(LocalNameError::Forbidden(character));
    }
    if text.ends_with('.') || text.ends_with(' ') {
        return Err(LocalNameError::TrailingDotOrSpace);
    }
    let stem = text.split('.').next().unwrap_or_default().trim_end();
    let stem = stem.to_uppercase();
    let reserved = WINDOWS_RESERVED.contains(&stem.as_str());
    if reserved
        || WINDOWS_CONSOLE
            .iter()
            .any(|name| text.eq_ignore_ascii_case(name))
    {
        return Err(LocalNameError::Reserved);
    }
    if text.encode_utf16().count() > MAX_NAME_LENGTH {
        return Err(LocalNameError::TooLong);
    }
    Ok(LocalName {
        name: OsString::from(text),
        escaped,
    })
}

/// Two remote names that would be the same local file.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0:?} names the same local file as another entry")]
pub struct Collision(pub OsString);

/// The local names already given in one folder.
#[derive(Debug)]
pub struct FolderNames {
    rules: Rules,
    taken: HashSet<String>,
}

impl FolderNames {
    /// An empty folder under `rules`.
    #[must_use]
    pub fn new(rules: Rules) -> Self {
        Self {
            rules,
            taken: HashSet::new(),
        }
    }

    fn key(&self, name: &LocalName) -> String {
        let text = name.name.to_string_lossy();
        if self.rules.folds_case() {
            text.to_lowercase()
        } else {
            text.into_owned()
        }
    }

    /// Records `name`.
    ///
    /// # Errors
    ///
    /// [`Collision`] when the folder already holds a name the file system takes for it.
    pub fn claim(&mut self, name: &LocalName) -> Result<(), Collision> {
        if self.taken.insert(self.key(name)) {
            Ok(())
        } else {
            Err(Collision(name.name.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FolderNames, LocalName, LocalNameError, Rules};

    fn windows(remote: &[u8]) -> Result<LocalName, LocalNameError> {
        LocalName::from_remote(remote, Rules::Windows)
    }

    fn unix(remote: &[u8]) -> Result<LocalName, LocalNameError> {
        LocalName::from_remote(remote, Rules::Unix)
    }

    #[test]
    fn names_that_leave_the_folder_are_refused_everywhere() {
        for rules in [Rules::Unix, Rules::Windows] {
            for name in [&b""[..], b".", b"..", b"a/b", b"../x", b"/etc", b"a\0b"] {
                assert!(
                    LocalName::from_remote(name, rules).is_err(),
                    "{rules:?} {name:?}"
                );
            }
        }
    }

    #[test]
    fn windows_refuses_what_it_would_misread() {
        let cases: [(&[u8], LocalNameError); 12] = [
            (b"..\\x", LocalNameError::Separator),
            (b"a\\b", LocalNameError::Separator),
            (b"C:x", LocalNameError::Forbidden(':')),
            (b"file:stream", LocalNameError::Forbidden(':')),
            (b"a?b", LocalNameError::Forbidden('?')),
            (b"x.", LocalNameError::TrailingDotOrSpace),
            (b"x ", LocalNameError::TrailingDotOrSpace),
            (b"aux", LocalNameError::Reserved),
            (b"Com1.txt", LocalNameError::Reserved),
            (b"lpt\xC2\xB9.log", LocalNameError::Reserved),
            (b"CONOUT$", LocalNameError::Reserved),
            (b"tab\there", LocalNameError::Control),
        ];
        for (name, expected) in cases {
            assert_eq!(windows(name), Err(expected), "{name:?}");
        }
        assert!(
            windows(b"auxiliary.txt").is_ok(),
            "only the exact device names"
        );
        assert!(windows(b"con.d").is_err(), "an extension does not help");
    }

    #[test]
    fn unix_keeps_what_windows_refuses() {
        for name in [&b"a:b"[..], b"x.", b"aux", b"a\\b", b"a?b"] {
            assert!(unix(name).is_ok(), "{name:?}");
        }
    }

    #[test]
    fn a_non_utf8_name_is_escaped_on_windows_and_kept_on_unix() {
        let latin1 = b"caf\xE9.txt";
        let on_windows = windows(latin1).expect("usable");
        assert_eq!(on_windows.name, "caf%E9.txt");
        assert!(on_windows.escaped);
        let with_percent = windows(b"%E9\xFF").expect("usable");
        let without = windows(b"\xE9\xFF").expect("usable");
        assert_ne!(
            with_percent.name, without.name,
            "two non-UTF-8 names never escape to the same text"
        );
        let mut folder = FolderNames::new(Rules::Windows);
        folder.claim(&on_windows).expect("first");
        let literal = windows(b"caf%E9.txt").expect("usable");
        assert!(!literal.escaped, "a UTF-8 name keeps its percent");
        assert!(
            folder.claim(&literal).is_err(),
            "the rare coincidence with an escaped name is refused, not overwritten"
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;
            let on_unix = unix(latin1).expect("usable");
            assert_eq!(on_unix.name.as_bytes(), latin1);
            assert!(!on_unix.escaped);
        }
    }

    #[test]
    fn overlong_names_are_refused() {
        assert_eq!(unix(&[b'a'; 256]), Err(LocalNameError::TooLong));
        assert!(unix(&[b'a'; 255]).is_ok());
        let wide = "\u{e9}".repeat(256);
        assert_eq!(windows(wide.as_bytes()), Err(LocalNameError::TooLong));
    }

    #[test]
    fn names_differing_only_in_case_collide_where_the_system_folds_case() {
        let mut folder = FolderNames::new(Rules::Windows);
        folder
            .claim(&windows(b"Report.txt").expect("usable"))
            .expect("first");
        assert!(
            folder
                .claim(&windows(b"report.TXT").expect("usable"))
                .is_err()
        );
        let mut exact = FolderNames::new(Rules::Unix);
        exact
            .claim(&unix(b"Report.txt").expect("usable"))
            .expect("first");
        let second = exact.claim(&unix(b"report.TXT").expect("usable"));
        assert_eq!(second.is_err(), cfg!(target_os = "macos"));
        assert!(exact.claim(&unix(b"Report.txt").expect("usable")).is_err());
    }
}
