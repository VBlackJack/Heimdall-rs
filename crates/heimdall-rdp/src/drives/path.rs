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

//! The paths a server names on a shared drive, checked before any reaches the file system.
//!
//! A server sends `\dir\file`: backslashes, relative to the drive's root, one leading
//! backslash. Whatever else could leave the drive or name something that is not a file is
//! refused: `..` and `.`, an empty component (`\\`, a UNC prefix), a drive letter or a
//! stream (`:`), a forward slash, a trailing dot or space (Windows drops them, so `x.` would
//! be `x`), control and reserved characters, and the DOS device names (`CON`, `COM1`...),
//! which Windows opens as devices whatever folder they are in.

use std::path::{Path, PathBuf};

/// Longest component, in UTF-16 units, as NTFS allows.
const MAX_COMPONENT: usize = 255;

/// Longest path, in UTF-16 units, as Windows' long paths allow.
const MAX_PATH: usize = 32_767;

/// Characters no component may hold: reserved by Windows, or meaningful in a path.
const RESERVED: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Names Windows opens as devices, with any extension.
const DEVICE_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Why a path is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refused {
    /// Not a path on the drive: it could leave it, or names a device.
    Invalid,
}

/// The components of `path`, a server's path on a drive, once checked. The drive's root is
/// no component.
pub(crate) fn components(path: &str) -> Result<Vec<&str>, Refused> {
    if path.encode_utf16().count() > MAX_PATH {
        return Err(Refused::Invalid);
    }
    let relative = path.strip_prefix('\\').unwrap_or(path);
    if relative.is_empty() {
        return Ok(Vec::new());
    }
    relative
        .split('\\')
        .map(|component| {
            if is_component(component) {
                Ok(component)
            } else {
                Err(Refused::Invalid)
            }
        })
        .collect()
}

/// Whether `name` may be one component of a path on a drive.
pub(crate) fn is_component(name: &str) -> bool {
    // `.` and `..` end with a dot, refused below with any name that does.
    if name.is_empty() {
        return false;
    }
    if name.encode_utf16().count() > MAX_COMPONENT {
        return false;
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return false;
    }
    if name
        .chars()
        .any(|c| c.is_control() || RESERVED.contains(&c))
    {
        return false;
    }
    let stem = name.split('.').next().unwrap_or(name);
    !DEVICE_NAMES
        .iter()
        .any(|device| device.eq_ignore_ascii_case(stem.trim_end()))
}

/// `path`, a server's path, under `root`, the drive's folder on this side.
pub(crate) fn resolve(root: &Path, path: &str) -> Result<PathBuf, Refused> {
    let mut resolved = root.to_path_buf();
    for component in components(path)? {
        resolved.push(component);
    }
    Ok(resolved)
}

/// A directory query's path split into the folder to list and the pattern of the names to
/// find in it: `\dir\*.txt` is `\dir` and `*.txt`. The pattern may hold `*` and `?`; the
/// folder is checked as any path.
pub(crate) fn query(root: &Path, path: &str) -> Result<(PathBuf, String), Refused> {
    let (folder, pattern) = path.rsplit_once('\\').unwrap_or(("", path));
    let checked = pattern.replace(['*', '?'], "x");
    if pattern.is_empty() || (pattern != "*" && !is_component(&checked)) {
        return Err(Refused::Invalid);
    }
    Ok((resolve(root, folder)?, pattern.to_owned()))
}

/// Whether `name` matches `pattern`, as Windows compares names: whatever the case, `*` for
/// any run of characters and `?` for one.
pub(crate) fn matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().flat_map(char::to_lowercase).collect();
    let name: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
    // Two cursors, and where to resume after the last `*`.
    let (mut p, mut n) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, n));
                p += 1;
            }
            Some(&c) if c == '?' || c == name[n] => {
                p += 1;
                n += 1;
            }
            _ => match star {
                Some((star_p, star_n)) => {
                    p = star_p + 1;
                    n = star_n + 1;
                    star = Some((star_p, star_n + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_stays_on_its_drive() {
        let root = Path::new("root");
        assert_eq!(resolve(root, "").expect("root"), root);
        assert_eq!(resolve(root, "\\").expect("root"), root);
        assert_eq!(
            resolve(root, "\\Docs\\report.txt").expect("file"),
            root.join("Docs").join("report.txt")
        );
        for refused in [
            "\\..\\secret",
            "\\Docs\\..\\..\\x",
            "\\.\\x",
            "\\\\server\\share",
            "\\Docs\\\\x",
            "\\C:\\Windows",
            "\\file.txt:stream",
            "\\a/b",
            "\\trailing.",
            "\\trailing ",
            "\\CON",
            "\\con.txt",
            "\\Docs\\LPT1.log",
            "\\bad\u{1}name",
            "\\star*",
            "\\question?",
            "\\pipe|x",
        ] {
            assert_eq!(resolve(root, refused), Err(Refused::Invalid), "{refused:?}");
        }
        assert!(
            resolve(root, "\\CONFIG.sys").is_ok(),
            "only whole device names"
        );
        assert!(resolve(root, "\\.hidden").is_ok());
    }

    #[test]
    fn a_path_too_long_is_refused() {
        let root = Path::new("root");
        assert!(resolve(root, &format!("\\{}", "a".repeat(MAX_COMPONENT))).is_ok());
        assert!(resolve(root, &format!("\\{}", "a".repeat(MAX_COMPONENT + 1))).is_err());
        let deep = format!("\\{}", vec!["a"; MAX_PATH / 2 + 1].join("\\"));
        assert!(resolve(root, &deep).is_err());
    }

    #[test]
    fn a_query_is_a_checked_folder_and_a_pattern() {
        let root = Path::new("root");
        assert_eq!(
            query(root, "\\Docs\\*").expect("all"),
            (root.join("Docs"), "*".to_owned())
        );
        assert_eq!(
            query(root, "\\*.txt").expect("some"),
            (root.to_path_buf(), "*.txt".to_owned())
        );
        assert_eq!(
            query(root, "\\Docs\\report.txt").expect("one"),
            (root.join("Docs"), "report.txt".to_owned())
        );
        assert!(query(root, "\\..\\*").is_err());
        assert!(query(root, "\\Docs\\").is_err());
        assert!(query(root, "\\Docs\\a:b*").is_err());
    }

    #[test]
    fn patterns_match_as_windows_does() {
        for (pattern, name, found) in [
            ("*", "anything", true),
            ("*", "", true),
            ("*.txt", "Report.TXT", true),
            ("*.txt", "report.txt.bak", false),
            ("re?ort*", "report 2026.pdf", true),
            ("re?ort", "reort", false),
            ("a*b*c", "axxbyyc", true),
            ("a*b*c", "axxbyy", false),
            ("report.txt", "REPORT.txt", true),
            ("é*", "École", true),
        ] {
            assert_eq!(matches(pattern, name), found, "{pattern:?} {name:?}");
        }
    }
}
