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

//! From a request target to a file under the shared folder, or a refusal.
//!
//! The C# server canonicalised the path and checked the prefix. This one is stricter: the
//! target is percent-decoded once, every segment is checked for what could leave the folder
//! or name something other than a file (parent references, drive letters and alternate data
//! streams, UNC and device names, control characters), and then every component is looked
//! at on disk: a symbolic link or a junction anywhere under the root is refused, whatever it
//! points to.

use std::fs::Metadata;
use std::path::{Path, PathBuf};

/// Why a target does not name something to serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    /// 400: the target is not a path the server reads.
    Malformed,
    /// 403: the path leaves the folder, or goes through a link.
    Forbidden,
    /// 403: the path goes through a link of any kind (symbolic link, junction, or another
    /// reparse point such as a cloud placeholder or a deduplicated file). Refused as
    /// [`Self::Forbidden`] is, but told apart so that the answer and the log can say why:
    /// the C# server followed such links, this one never does.
    Link,
    /// 404: nothing is there.
    NotFound,
}

impl PathError {
    /// The status code that answers it.
    #[must_use]
    pub fn status(self) -> u16 {
        match self {
            Self::Malformed => 400,
            Self::Forbidden | Self::Link => 403,
            Self::NotFound => 404,
        }
    }
}

/// Names Windows reads as devices, whatever their extension.
const RESERVED_NAMES: [&str; 6] = ["CON", "PRN", "AUX", "NUL", "COM", "LPT"];

/// Characters Windows does not allow in a name, besides the colon and the separators.
const WINDOWS_FORBIDDEN: &str = "*?\"<>|";

/// Windows' reparse point attribute: a link, a junction, or a cloud placeholder.
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// A target split into its path and its query, both as written.
#[must_use]
pub fn split_target(target: &str) -> (&str, Option<&str>) {
    match target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (target, None),
    }
}

/// `text` with its `%XX` escapes decoded, once; `None` for a bad escape or text that is not
/// UTF-8 afterwards. A `+` stays a plus.
#[must_use]
pub fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        if byte == b'%' {
            let high = hex_value(*bytes.get(index + 1)?)?;
            let low = hex_value(*bytes.get(index + 2)?)?;
            out.push(high << 4 | low);
            index += 3;
        } else {
            out.push(byte);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_value(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

/// The value of the `token` parameter of a query, decoded.
#[must_use]
pub fn query_token(query: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == "token")
        .and_then(|(_, value)| percent_decode(value))
}

/// Whether `name` may be a segment of a served path.
///
/// The same rule filters a folder's listing, so that it never offers what a request for it
/// would refuse.
#[must_use]
pub fn segment_allowed(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }
    if name.ends_with(['.', ' ']) {
        return false;
    }
    if name
        .chars()
        .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':') || WINDOWS_FORBIDDEN.contains(c))
    {
        return false;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = RESERVED_NAMES.iter().any(|reserved| {
        stem == *reserved
            || (matches!(*reserved, "COM" | "LPT")
                && stem.strip_prefix(reserved).is_some_and(|rest| {
                    rest.len() == 1 && rest.bytes().all(|b| (b'1'..=b'9').contains(&b))
                }))
    });
    !reserved
}

/// The segments of the path of a target, below the shared folder.
///
/// # Errors
///
/// The path is not absolute, does not decode, or has a segment that [`segment_allowed`]
/// refuses (an empty one included, so `//host/share` is not read as a UNC path).
pub fn segments(path: &str) -> Result<Vec<String>, PathError> {
    let rest = path.strip_prefix('/').ok_or(PathError::Malformed)?;
    let decoded = percent_decode(rest).ok_or(PathError::Malformed)?;
    if decoded.is_empty() {
        return Ok(Vec::new());
    }
    // One trailing slash names a folder; any other empty segment is refused.
    let trimmed = decoded.strip_suffix('/').unwrap_or(&decoded);
    let mut out = Vec::new();
    for segment in trimmed.split('/') {
        if !segment_allowed(segment) {
            return Err(PathError::Forbidden);
        }
        out.push(segment.to_owned());
    }
    Ok(out)
}

/// Whether `metadata` is a link of any kind: a symbolic link, a junction, or any other
/// reparse point.
#[must_use]
pub fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// What a path names.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// The file or folder, below the canonical root.
    pub path: PathBuf,
    /// What is there.
    pub metadata: Metadata,
}

/// The file or folder `segments` name under `root`, which must be canonical.
///
/// Every component is looked at without following links; a link is refused, and so is a
/// result that does not canonicalise to a place under the root (a defence beside the first
/// check, for what a link-free walk could still miss).
///
/// # Errors
///
/// Nothing is there, or the path goes through a link or leaves the root.
pub fn resolve(root: &Path, segments: &[String]) -> Result<Resolved, PathError> {
    let mut current = root.to_path_buf();
    let mut metadata = std::fs::symlink_metadata(&current).map_err(|_| PathError::NotFound)?;
    for segment in segments {
        if !metadata.is_dir() {
            return Err(PathError::NotFound);
        }
        current.push(segment);
        metadata = std::fs::symlink_metadata(&current).map_err(|_| PathError::NotFound)?;
        if is_link(&metadata) {
            return Err(PathError::Link);
        }
    }
    let canonical = std::fs::canonicalize(&current).map_err(|_| PathError::NotFound)?;
    if !canonical.starts_with(root) {
        return Err(PathError::Forbidden);
    }
    Ok(Resolved {
        path: current,
        metadata,
    })
}

/// `O_NOFOLLOW` of the platforms std gives no portable name for: opening a final component
/// that is a symbolic link then fails instead of following it.
#[cfg(all(
    target_os = "linux",
    any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "riscv64",
        target_arch = "s390x",
        target_arch = "loongarch64"
    )
))]
const OPEN_NO_FOLLOW: i32 = 0o400_000;
#[cfg(all(
    target_os = "linux",
    any(
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "powerpc64"
    )
))]
const OPEN_NO_FOLLOW: i32 = 0o100_000;
#[cfg(any(target_os = "macos", target_os = "freebsd"))]
const OPEN_NO_FOLLOW: i32 = 0x100;

/// `FILE_FLAG_OPEN_REPARSE_POINT`: open a link itself, not what it points to.
#[cfg(windows)]
const OPEN_REPARSE_POINT: u32 = 0x0020_0000;

/// Whether two looks at a file are of the same file as far as std can tell: the same size
/// and modification time, and on Unix the same device and inode.
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if a.dev() != b.dev() || a.ino() != b.ino() {
            return false;
        }
    }
    // A folder changes whenever a file comes or goes: only a file is held to its size and time.
    a.is_dir() == b.is_dir()
        && (a.is_dir() || (a.len() == b.len() && a.modified().ok() == b.modified().ok()))
}

/// Checks, after a file or folder was opened or read, that the path still names what it
/// named when it was resolved, and still lies under the root. This narrows the window
/// between checking a path and using it (std has no `openat`): a component swapped for a
/// link, or the file replaced, in between is refused.
fn recheck(root: &Path, resolved: &Resolved, opened: Option<&Metadata>) -> Result<(), PathError> {
    let after = std::fs::symlink_metadata(&resolved.path).map_err(|_| PathError::NotFound)?;
    if is_link(&after) || opened.is_some_and(is_link) {
        return Err(PathError::Link);
    }
    if !same_file(&after, &resolved.metadata)
        || opened.is_some_and(|opened| !same_file(opened, &resolved.metadata))
    {
        return Err(PathError::Forbidden);
    }
    let canonical = std::fs::canonicalize(&resolved.path).map_err(|_| PathError::NotFound)?;
    if canonical.starts_with(root) {
        Ok(())
    } else {
        Err(PathError::Forbidden)
    }
}

/// Opens the file `resolved` names without following a link in its last component, then
/// checks it again with [`recheck`].
///
/// Residual risk: a local user able to write in the shared folder can still race the
/// intermediate folders between [`resolve`] and the open; the checks after the open catch a
/// swap that is still in place, not one undone in time.
///
/// # Errors
///
/// The file cannot be opened, or is not the one resolved.
pub fn open_file(root: &Path, resolved: &Resolved) -> Result<std::fs::File, PathError> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(any(
        all(
            target_os = "linux",
            any(
                target_arch = "x86",
                target_arch = "x86_64",
                target_arch = "riscv64",
                target_arch = "s390x",
                target_arch = "loongarch64",
                target_arch = "aarch64",
                target_arch = "arm",
                target_arch = "powerpc64"
            )
        ),
        target_os = "macos",
        target_os = "freebsd"
    ))]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(OPEN_NO_FOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.custom_flags(OPEN_REPARSE_POINT);
    }
    let file = options
        .open(&resolved.path)
        .map_err(|_| PathError::NotFound)?;
    let opened = file.metadata().map_err(|_| PathError::NotFound)?;
    if !opened.is_file() {
        return Err(PathError::NotFound);
    }
    recheck(root, resolved, Some(&opened))?;
    Ok(file)
}

/// Checks a folder again after it was listed.
///
/// # Errors
///
/// The folder is not the one resolved any more, or left the root.
pub fn recheck_dir(root: &Path, resolved: &Resolved) -> Result<(), PathError> {
    recheck(root, resolved, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.txt"), "a").unwrap();
        std::fs::write(root.join("sub").join("b.txt"), "b").unwrap();
        (dir, root)
    }

    #[test]
    fn percent_decoding_happens_once() {
        assert_eq!(percent_decode("a%20b").as_deref(), Some("a b"));
        assert_eq!(percent_decode("%252e").as_deref(), Some("%2e"));
        assert_eq!(percent_decode("a+b").as_deref(), Some("a+b"));
        assert_eq!(percent_decode("%zz"), None);
        assert_eq!(percent_decode("%2"), None);
        assert_eq!(percent_decode("%ff"), None, "not UTF-8");
    }

    #[test]
    fn traversal_and_special_names_are_refused() {
        for path in [
            "/..",
            "/../x",
            "/sub/../a.txt",
            "/%2e%2e/x",
            "/%2E%2E%2Fx",
            "/sub/%2e%2e/a.txt",
            "/.",
            "/a.txt:stream",
            "/a.txt%3A%24DATA",
            "/C:/Windows",
            "//host/share/file",
            "/%5Chost%5Cshare",
            "/sub%5C..%5Cx",
            "/con",
            "/NUL.txt",
            "/com1",
            "/LPT9.log",
            "/a%00b",
            "/a%0Ab",
            "/name.",
            "/name ",
            "/a*b",
            "/sub//b.txt",
        ] {
            assert_eq!(segments(path), Err(PathError::Forbidden), "{path}");
        }
    }

    #[test]
    fn what_is_not_a_path_is_malformed() {
        for path in ["x", "", "http://h/x", "/%zz", "/%ff"] {
            assert_eq!(segments(path), Err(PathError::Malformed), "{path}");
        }
    }

    #[test]
    fn plain_paths_are_read() {
        assert_eq!(segments("/"), Ok(Vec::new()));
        assert_eq!(segments("/sub/"), Ok(vec!["sub".to_owned()]));
        assert_eq!(
            segments("/sub/b%20c.txt"),
            Ok(vec!["sub".to_owned(), "b c.txt".to_owned()])
        );
        assert_eq!(segments("/com10"), Ok(vec!["com10".to_owned()]));
    }

    #[test]
    fn targets_split_and_tokens_are_found() {
        assert_eq!(split_target("/a?token=x"), ("/a", Some("token=x")));
        assert_eq!(split_target("/a"), ("/a", None));
        assert_eq!(query_token("x=1&token=a%62c").as_deref(), Some("abc"));
        assert_eq!(query_token("tokens=1"), None);
    }

    #[test]
    fn files_resolve_and_missing_ones_do_not() {
        let (_dir, root) = tree();
        let file = resolve(&root, &["sub".to_owned(), "b.txt".to_owned()]).unwrap();
        assert!(file.metadata.is_file());
        assert!(resolve(&root, &[]).unwrap().metadata.is_dir());
        assert_eq!(
            resolve(&root, &["nope".to_owned()]).unwrap_err(),
            PathError::NotFound
        );
        assert_eq!(
            resolve(&root, &["a.txt".to_owned(), "x".to_owned()]).unwrap_err(),
            PathError::NotFound
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_anywhere_in_the_path_is_refused() {
        let (_dir, root) = tree();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "s").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("out")).unwrap();
        std::os::unix::fs::symlink(root.join("a.txt"), root.join("alias")).unwrap();
        for path in [
            vec!["out".to_owned(), "secret".to_owned()],
            vec!["out".to_owned()],
            vec!["alias".to_owned()],
        ] {
            assert_eq!(resolve(&root, &path).unwrap_err(), PathError::Link);
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_symlink_anywhere_in_the_path_is_refused() {
        let (_dir, root) = tree();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "s").unwrap();
        // Creating a symbolic link needs a privilege a runner may lack: nothing to check then.
        if std::os::windows::fs::symlink_dir(outside.path(), root.join("out")).is_err() {
            return;
        }
        assert_eq!(
            resolve(&root, &["out".to_owned(), "secret".to_owned()]).unwrap_err(),
            PathError::Link
        );
    }

    #[test]
    fn a_link_error_answers_403_like_a_forbidden_path() {
        assert_eq!(PathError::Link.status(), 403);
        assert_eq!(PathError::Link.status(), PathError::Forbidden.status());
    }

    #[test]
    fn a_file_replaced_after_it_was_resolved_is_refused() {
        let (_dir, root) = tree();
        let resolved = resolve(&root, &["a.txt".to_owned()]).unwrap();
        assert!(open_file(&root, &resolved).is_ok());
        std::fs::write(root.join("a.txt"), "a much longer content than before").unwrap();
        assert_eq!(
            open_file(&root, &resolved).unwrap_err(),
            PathError::Forbidden
        );
    }

    #[test]
    fn a_folder_swapped_after_it_was_resolved_is_refused() {
        let (_dir, root) = tree();
        let resolved = resolve(&root, &["sub".to_owned()]).unwrap();
        assert!(recheck_dir(&root, &resolved).is_ok());
        std::fs::remove_dir_all(root.join("sub")).unwrap();
        assert!(recheck_dir(&root, &resolved).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_file_swapped_for_a_symlink_after_it_was_resolved_is_refused() {
        let (_dir, root) = tree();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "a").unwrap();
        let resolved = resolve(&root, &["a.txt".to_owned()]).unwrap();
        std::fs::remove_file(root.join("a.txt")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret"), root.join("a.txt")).unwrap();
        assert!(open_file(&root, &resolved).is_err());
    }
}
