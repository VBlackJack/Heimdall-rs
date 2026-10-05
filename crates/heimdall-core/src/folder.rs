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

/// A folder's colour, from the C# `BadgeColorPalette`: shown on the folder, and on every
/// folder in it without a colour of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FolderColor {
    /// `#3B82F6`.
    Blue,
    /// `#22C55E`.
    Green,
    /// `#EF4444`.
    Red,
    /// `#F59E0B`.
    Amber,
    /// `#8B5CF6`.
    Purple,
    /// `#EC4899`.
    Pink,
    /// `#06B6D4`.
    Cyan,
    /// `#F97316`.
    Orange,
}

impl FolderColor {
    /// Every colour, in the C# palette's order.
    pub const ALL: [Self; 8] = [
        Self::Blue,
        Self::Green,
        Self::Red,
        Self::Amber,
        Self::Purple,
        Self::Pink,
        Self::Cyan,
        Self::Orange,
    ];

    /// Its red, green and blue, as the C# palette's.
    #[must_use]
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Blue => (0x3B, 0x82, 0xF6),
            Self::Green => (0x22, 0xC5, 0x5E),
            Self::Red => (0xEF, 0x44, 0x44),
            Self::Amber => (0xF5, 0x9E, 0x0B),
            Self::Purple => (0x8B, 0x5C, 0xF6),
            Self::Pink => (0xEC, 0x48, 0x99),
            Self::Cyan => (0x06, 0xB6, 0xD4),
            Self::Orange => (0xF9, 0x73, 0x16),
        }
    }

    /// The name the profile file keeps it under.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Red => "red",
            Self::Amber => "amber",
            Self::Purple => "purple",
            Self::Pink => "pink",
            Self::Cyan => "cyan",
            Self::Orange => "orange",
        }
    }

    /// The colour the profile file names `name`, whatever its case; `None` for another.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|color| color.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The colour the C# keeps as `hex`, `#RRGGBB` whatever its case; `None` for one out of
    /// its palette.
    #[must_use]
    pub fn from_hex(hex: &str) -> Option<Self> {
        let digits = hex.trim().strip_prefix('#')?;
        let value = u32::from_str_radix(digits, 16)
            .ok()
            .filter(|_| digits.len() == 6)?;
        let [_, red, green, blue] = value.to_be_bytes();
        Self::ALL
            .into_iter()
            .find(|color| color.rgb() == (red, green, blue))
    }
}

#[cfg(test)]
mod color_tests {
    use super::FolderColor;

    #[test]
    fn a_colour_is_kept_by_name_and_read_from_the_csharp_hex() {
        for color in FolderColor::ALL {
            assert_eq!(FolderColor::named(color.name()), Some(color));
        }
        assert_eq!(FolderColor::named(" Blue "), Some(FolderColor::Blue));
        assert_eq!(FolderColor::named("teal"), None);
        assert_eq!(FolderColor::from_hex("#3b82f6"), Some(FolderColor::Blue));
        assert_eq!(FolderColor::from_hex("#F97316"), Some(FolderColor::Orange));
        assert_eq!(FolderColor::from_hex("#123456"), None, "out of the palette");
        assert_eq!(FolderColor::from_hex("3B82F6"), None);
        assert_eq!(FolderColor::from_hex("#3B82F"), None);
    }
}
