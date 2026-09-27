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

//! Folder paths, as the C# Heimdall writes them: names joined by `/`, each trimmed, the
//! empty ones dropped. A folder holds every folder whose path continues its own after a `/`.
//! Two folders at the same level cannot share a name whatever its case, as the C# rename
//! refuses; paths themselves are compared as written.

use thiserror::Error;

/// Why a folder cannot be named or moved as asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FolderError {
    /// No name, or one holding a `/`.
    #[error("a folder needs a name without '/'")]
    InvalidName,
    /// A folder of that name is already at that level.
    #[error("a folder with this name already exists at the same level")]
    Collision,
    /// A folder cannot go inside itself.
    #[error("a folder cannot be moved into itself")]
    IntoItself,
    /// There is no such folder.
    #[error("no such folder")]
    Missing,
}

/// The parts of `path`: trimmed, the empty ones dropped.
#[must_use]
pub fn parts(path: &str) -> Vec<&str> {
    path.split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

/// `path` written the one way: its parts joined by `/`; empty for no folder.
#[must_use]
pub fn normal(path: &str) -> String {
    parts(path).join("/")
}

/// The folder holding `path`, empty at the top.
#[must_use]
pub fn parent(path: &str) -> String {
    let parts = parts(path);
    parts[..parts.len().saturating_sub(1)].join("/")
}

/// The last part of `path`: the folder's own name.
#[must_use]
pub fn name(path: &str) -> String {
    parts(path)
        .last()
        .map_or_else(String::new, |name| (*name).to_owned())
}

/// Whether `path` is `folder` or inside it.
#[must_use]
pub fn is_within(path: &str, folder: &str) -> bool {
    let (path, folder) = (normal(path), normal(folder));
    !folder.is_empty()
        && (path == folder
            || path
                .strip_prefix(&folder)
                .is_some_and(|rest| rest.starts_with('/')))
}

/// `path` with `folder`, which holds it, replaced by `renamed`.
#[must_use]
pub fn relabel(path: &str, folder: &str, renamed: &str) -> String {
    let (path, folder) = (normal(path), normal(folder));
    match path.strip_prefix(&folder) {
        Some(rest) if is_within(&path, &folder) => format!("{}{rest}", normal(renamed)),
        _ => path,
    }
}

/// `name` joined under `parent`, checked to be one folder name.
///
/// # Errors
///
/// [`FolderError::InvalidName`] when `name` is empty once trimmed, or holds a `/`.
pub fn child(parent: &str, name: &str) -> Result<String, FolderError> {
    let name = name.trim();
    if name.is_empty() || name.contains('/') {
        return Err(FolderError::InvalidName);
    }
    let parent = normal(parent);
    Ok(if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    })
}

/// Whether two paths name the same folder for a collision: whatever the case.
#[must_use]
pub fn same_folder(a: &str, b: &str) -> bool {
    normal(a).to_lowercase() == normal(b).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_written_one_way() {
        assert_eq!(normal(" Prod / Web /"), "Prod/Web");
        assert_eq!(normal("//"), "");
        assert_eq!(parent("Prod/Web/Front"), "Prod/Web");
        assert_eq!(parent("Prod"), "");
        assert_eq!(name(" Prod / Web "), "Web");
        assert_eq!(name(""), "");
    }

    #[test]
    fn a_folder_holds_itself_and_what_continues_its_path() {
        assert!(is_within("Prod/Web", "Prod"));
        assert!(is_within("Prod", " Prod "));
        assert!(
            !is_within("Production", "Prod"),
            "a longer name is another folder"
        );
        assert!(!is_within("Prod", "Prod/Web"));
        assert!(!is_within("Prod", ""), "no folder holds nothing");
        assert!(!is_within("", ""), "not even no folder");
        assert_eq!(
            relabel("Prod/Web/Front", "Prod/Web", "Live/Site"),
            "Live/Site/Front"
        );
        assert_eq!(relabel("Prod/Web", "Prod/Web", "Live"), "Live");
        assert_eq!(relabel("Production", "Prod", "Live"), "Production");
    }

    #[test]
    fn a_child_is_one_name_under_its_parent() {
        assert_eq!(child("Prod", " Web "), Ok("Prod/Web".to_owned()));
        assert_eq!(child("", "Prod"), Ok("Prod".to_owned()));
        assert_eq!(child("Prod", " "), Err(FolderError::InvalidName));
        assert_eq!(child("Prod", "a/b"), Err(FolderError::InvalidName));
        assert!(same_folder("prod/web", " Prod / Web "));
        assert!(!same_folder("Prod", "Prod/Web"));
    }
}
