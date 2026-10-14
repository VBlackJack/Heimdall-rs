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

//! Remote paths as the server's bytes.
//!
//! SFTP version 3 names are byte strings: a server may hold names in Latin-1 or any other
//! encoding. A path is kept as the exact bytes the server sent, so a file listed can always
//! be opened, renamed and deleted; text is only ever a display form.

use std::fmt;
use std::fmt::Write as _;

/// Separator of remote path components.
const SEPARATOR: u8 = b'/';

/// A path on the server, as bytes.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct RemotePath(Vec<u8>);

impl RemotePath {
    /// A path from the server's bytes.
    #[must_use]
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// The exact bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Whether the path starts at the root.
    #[must_use]
    pub fn is_absolute(&self) -> bool {
        self.0.first() == Some(&SEPARATOR)
    }

    /// `self` followed by one more component.
    #[must_use]
    pub fn join(&self, name: &[u8]) -> Self {
        let mut bytes = self.0.clone();
        if !bytes.is_empty() && bytes.last() != Some(&SEPARATOR) {
            bytes.push(SEPARATOR);
        }
        bytes.extend_from_slice(name);
        Self(bytes)
    }

    /// The directory holding `self`; the root is its own parent.
    #[must_use]
    pub fn parent(&self) -> Self {
        let trimmed = trim_trailing_separators(&self.0);
        match trimmed.iter().rposition(|&byte| byte == SEPARATOR) {
            Some(0) => Self(vec![SEPARATOR]),
            Some(index) => Self(trim_trailing_separators(&trimmed[..index]).to_vec()),
            None if self.is_absolute() => Self(vec![SEPARATOR]),
            None => Self(b".".to_vec()),
        }
    }

    /// The last component, if any.
    #[must_use]
    pub fn file_name(&self) -> Option<&[u8]> {
        let trimmed = trim_trailing_separators(&self.0);
        let start = trimmed
            .iter()
            .rposition(|&byte| byte == SEPARATOR)
            .map_or(0, |index| index + 1);
        let name = &trimmed[start..];
        (!name.is_empty()).then_some(name)
    }

    /// Text to show: valid UTF-8 as is, any other byte as `\xNN`, so two different names
    /// never display the same and nothing is silently replaced.
    #[must_use]
    pub fn display(&self) -> String {
        display_bytes(&self.0)
    }
}

impl From<&str> for RemotePath {
    fn from(text: &str) -> Self {
        Self(text.as_bytes().to_vec())
    }
}

impl fmt::Debug for RemotePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RemotePath({:?})", self.display())
    }
}

fn trim_trailing_separators(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .rposition(|&byte| byte != SEPARATOR)
        .map_or(usize::from(!bytes.is_empty()), |index| index + 1);
    &bytes[..end]
}

/// `bytes` as text: valid UTF-8 kept, every other byte and every backslash escaped.
#[must_use]
pub fn display_bytes(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        for character in chunk.valid().chars() {
            if character == '\\' {
                text.push_str("\\\\");
            } else {
                text.push(character);
            }
        }
        for byte in chunk.invalid() {
            let _ = write!(text, "\\x{byte:02X}");
        }
    }
    text
}

/// Whether `name`, an entry's name as a server listed it, is one clean path segment that
/// may enter a listing, as the C# `SftpPathGuard.IsValidChildName` that every C# listing
/// (`SftpBrowser`, `FtpBrowser`, `SudoDirectoryListing`) passes its names through: not blank,
/// not `.` or `..` once trimmed, no `/` or `\`, no control character. A hostile server's
/// `../x` or `a/b` would otherwise be joined to a folder and reach outside it.
///
/// Stricter than the C# on one point: a control character at either end, which the C#
/// trims away before looking, is refused too; `a\n` names no file a user can type.
#[must_use]
pub fn is_child_name(name: &[u8]) -> bool {
    let text = String::from_utf8_lossy(name);
    let trimmed = text.trim();
    !trimmed.is_empty()
        && trimmed != "."
        && trimmed != ".."
        && !text
            .chars()
            .any(|character| matches!(character, '/' | '\\') || character.is_control())
}

#[cfg(test)]
mod tests {
    use super::{RemotePath, display_bytes, is_child_name};

    #[test]
    fn a_listed_name_is_one_clean_segment_or_left_out() {
        for name in [
            &b""[..],
            b" ",
            b"\t",
            b".",
            b"..",
            b" .. ",
            b"../x",
            b"a/b",
            b"/",
            b"/etc/passwd",
            b"a\\b",
            b"..\\x",
            b"a\0b",
            b"a\nb",
            b"a\n",
            b"a\x1b[2J",
            b"a\x7f",
            "a\u{85}".as_bytes(),
        ] {
            assert!(!is_child_name(name), "{}", display_bytes(name));
        }
        for name in [
            &b"a"[..],
            b" a",
            b"a b ",
            b"...",
            b".hidden",
            b"C:x",
            b"caf\xE9",
            "\u{e9}t\u{e9}".as_bytes(),
        ] {
            assert!(is_child_name(name), "{}", display_bytes(name));
        }
    }

    #[test]
    fn a_listed_name_a_download_writes_never_leaves_its_folder() {
        use std::path::{Component, Path};

        use crate::local_name::{LocalName, Rules};

        // What a hostile server may list, past the listing's guard or not: a download's
        // local name (`LocalName`, the tree and folder downloads' only way to a local
        // path) is either refused or one plain component of the target folder.
        let target = Path::new("target");
        for name in [
            &b"../x"[..],
            b"..",
            b"a/b",
            b"/etc/passwd",
            b"..\\x",
            b"C:x",
            b"C:\\x",
            b"\\\\server\\share",
            b"a\0b",
            b"x",
            b" a",
        ] {
            for rules in [Rules::Unix, Rules::Windows] {
                let Ok(local) = LocalName::from_remote(name, rules) else {
                    continue;
                };
                let joined = target.join(&local.name);
                assert_eq!(joined.parent(), Some(target), "{}", display_bytes(name));
                assert!(
                    matches!(
                        Path::new(&local.name).components().collect::<Vec<_>>()[..],
                        [Component::Normal(_)]
                    ),
                    "{}",
                    display_bytes(name)
                );
            }
        }
    }

    #[test]
    fn a_latin1_name_is_kept_byte_for_byte_and_shown_escaped() {
        // "cafe" with a Latin-1 e acute: not UTF-8.
        let path = RemotePath::from("/srv").join(b"caf\xE9.txt");
        assert_eq!(path.as_bytes(), b"/srv/caf\xE9.txt");
        assert_eq!(path.display(), "/srv/caf\\xE9.txt");
        assert_eq!(path.file_name(), Some(&b"caf\xE9.txt"[..]));
    }

    #[test]
    fn a_backslash_is_escaped_so_displays_never_collide() {
        assert_ne!(display_bytes(b"a\\xE9"), display_bytes(b"a\xE9"));
    }

    #[test]
    fn parents_and_names() {
        assert_eq!(RemotePath::from("/a/b/").parent(), RemotePath::from("/a"));
        assert_eq!(RemotePath::from("/a").parent(), RemotePath::from("/"));
        assert_eq!(RemotePath::from("/").parent(), RemotePath::from("/"));
        assert_eq!(RemotePath::from("a").parent(), RemotePath::from("."));
        assert_eq!(RemotePath::from("/").file_name(), None);
        assert_eq!(RemotePath::from("/").join(b"x"), RemotePath::from("/x"));
    }
}
