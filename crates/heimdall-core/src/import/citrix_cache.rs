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

//! Citrix Workspace's local cache of published applications, read as the C#
//! `CitrixCacheScanner` reads it: the `*_Cache.xml` files of `%LocalAppData%\Citrix\
//! SelfService`, one per store, each `resource` element found by its local name whatever
//! its namespace. A DTD is refused.
//!
//! Each application carries the line `SelfService.exe` launches it with, pre-authenticated:
//! a secret, never shown in a log.

use std::path::{Path, PathBuf};

use roxmltree::{Document, Node};
use zeroize::Zeroizing;

/// The variable naming this account's local application data folder.
const LOCAL_APP_DATA: &str = "LOCALAPPDATA";

/// The cache's folder under the local application data, as the C#.
const CACHE_FOLDER: [&str; 2] = ["Citrix", "SelfService"];

/// The end of a cache file's name, compared without case as Windows matches `*_Cache.xml`.
const CACHE_FILE_SUFFIX: &str = "_cache.xml";

/// The folder the applications of a category are filed under, as the C#.
const GROUP_ROOT: &str = "Citrix";

/// The elements a resource is read from, as the C# names them.
const RESOURCE: &str = "resource";
const FRIENDLY_NAME: &str = "FriendlyName";
const CATEGORY: &str = "Category";
const LAUNCH_COMMAND_LINE: &str = "LaunchCommandLine";
const ICA_LAUNCH_URL: &str = "icaLaunchUrl";

/// Byte order marks a cache file may start with.
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
const UTF16_LE_BOM: [u8; 2] = [0xFF, 0xFE];
const UTF16_BE_BOM: [u8; 2] = [0xFE, 0xFF];

/// A published application found in the cache.
#[derive(Clone, PartialEq, Eq)]
pub struct CachedApp {
    /// Its name, as the C# `FriendlyName`.
    pub name: String,
    /// Its category, as the C# `Category`: a folder path, `\`-separated.
    pub category: Option<String>,
    /// The `StoreFront` it is published on, `scheme://host`.
    pub store_url: Option<String>,
    /// The arguments `SelfService.exe` launches it with, as the C# `LaunchCommandLine`.
    pub launch_line: Zeroizing<String>,
}

impl CachedApp {
    /// The folder it is filed in: `Citrix/<Category>`, backslashes as `/`, as the C#; none
    /// without a category.
    #[must_use]
    pub fn group(&self) -> Option<String> {
        self.category
            .as_deref()
            .map(|category| format!("{GROUP_ROOT}/{}", category.replace('\\', "/")))
    }
}

impl std::fmt::Debug for CachedApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The launch line opens a session without a password: never shown.
        f.debug_struct("CachedApp")
            .field("name", &self.name)
            .field("category", &self.category)
            .field("store_url", &self.store_url)
            .finish_non_exhaustive()
    }
}

/// What a scan says besides the applications, as the C# warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheWarning {
    /// There is no cache folder: Citrix Workspace never ran for this account.
    FolderMissing,
    /// The folder holds no cache file: no store was connected to.
    NoCacheFiles,
    /// A cache file could not be read.
    Unreadable {
        /// The file's name.
        file: String,
        /// Why.
        detail: String,
    },
}

/// The applications of every cache file, and what was said on the way.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CacheScan {
    /// The applications, file after file.
    pub apps: Vec<CachedApp>,
    /// What was said.
    pub warnings: Vec<CacheWarning>,
}

/// Where this account's cache is: `%LocalAppData%\Citrix\SelfService`.
#[must_use]
pub fn cache_folder() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os(LOCAL_APP_DATA)?);
    Some(CACHE_FOLDER.iter().fold(root, |path, part| path.join(part)))
}

/// Scans this account's cache.
#[must_use]
pub fn scan() -> CacheScan {
    match cache_folder() {
        Some(folder) => scan_folder(&folder),
        None => CacheScan {
            apps: Vec::new(),
            warnings: vec![CacheWarning::FolderMissing],
        },
    }
}

/// Scans the cache files of `folder`, in the order of their names.
#[must_use]
pub fn scan_folder(folder: &Path) -> CacheScan {
    let mut scan = CacheScan::default();
    let Ok(entries) = std::fs::read_dir(folder) else {
        scan.warnings.push(CacheWarning::FolderMissing);
        return scan;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_cache_file(path))
        .collect();
    files.sort();
    if files.is_empty() {
        scan.warnings.push(CacheWarning::NoCacheFiles);
    }
    for file in files {
        let read = std::fs::read(&file)
            .map_err(|error| error.to_string())
            .and_then(|bytes| decode(&bytes))
            .and_then(|text| parse(&text));
        match read {
            Ok(apps) => scan.apps.extend(apps),
            Err(detail) => scan.warnings.push(CacheWarning::Unreadable {
                file: file
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                detail,
            }),
        }
    }
    scan
}

/// Whether `path` is named as a cache file.
fn is_cache_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().ends_with(CACHE_FILE_SUFFIX))
}

/// The text of a cache file: UTF-8, or UTF-16 by its byte order mark.
fn decode(bytes: &[u8]) -> Result<String, String> {
    let utf16 = |rest: &[u8], unit: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| unit(*pair))
            .collect();
        String::from_utf16(&units).map_err(|error| error.to_string())
    };
    if let Some(rest) = bytes.strip_prefix(&UTF16_LE_BOM) {
        utf16(rest, u16::from_le_bytes)
    } else if let Some(rest) = bytes.strip_prefix(&UTF16_BE_BOM) {
        utf16(rest, u16::from_be_bytes)
    } else {
        let rest = bytes.strip_prefix(&UTF8_BOM).unwrap_or(bytes);
        String::from_utf8(rest.to_vec()).map_err(|error| error.to_string())
    }
}

/// The applications of one cache file, as the C# `ParseCacheFile`: each `resource` with a
/// name and a launch line. Every application of the file is given the `StoreFront` of the
/// first launch address in it, where the C# gives it from that resource on: one file is one
/// store.
///
/// # Errors
///
/// Why the file is not XML, or holds a DTD.
pub fn parse(text: &str) -> Result<Vec<CachedApp>, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let document = Document::parse(text).map_err(|error| error.to_string())?;
    let mut store_url = None;
    let mut apps = Vec::new();
    for resource in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == RESOURCE)
    {
        let (Some(name), Some(launch_line)) = (
            value(resource, FRIENDLY_NAME),
            value(resource, LAUNCH_COMMAND_LINE),
        ) else {
            continue;
        };
        if store_url.is_none() {
            store_url = value(resource, ICA_LAUNCH_URL).and_then(|url| store_of(&url));
        }
        apps.push(CachedApp {
            name,
            category: value(resource, CATEGORY),
            store_url: None,
            launch_line: Zeroizing::new(launch_line),
        });
    }
    for app in &mut apps {
        app.store_url.clone_from(&store_url);
    }
    Ok(apps)
}

/// The text of `parent`'s child element `name`, whatever its namespace, trimmed: all of its
/// text, as the C# `XElement.Value`. `None` when there is none or it is blank.
fn value(parent: Node<'_, '_>, name: &str) -> Option<String> {
    let element = parent
        .children()
        .find(|child| child.is_element() && child.tag_name().name() == name)?;
    let text: String = element
        .descendants()
        .filter(Node::is_text)
        .filter_map(|node| node.text())
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// `scheme://host` of the address `url`, as the C# builds it from a `Uri`: no account, no
/// port, no path. `None` when it is not an absolute address.
fn store_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let scheme_valid = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !scheme_valid {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host_port)| host_port);
    let host = if host_port.starts_with('[') {
        // An IPv6 address keeps its brackets, as the C# `Uri.Host` does.
        &host_port[..=host_port.find(']')?]
    } else {
        host_port.split(':').next().unwrap_or_default()
    };
    (!host.is_empty()).then(|| {
        format!(
            "{}://{}",
            scheme.to_ascii_lowercase(),
            host.to_ascii_lowercase()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_is_its_scheme_and_host() {
        assert_eq!(
            store_of("https://Store.Lab:8443/Citrix/Store/resources?x=1").as_deref(),
            Some("https://store.lab")
        );
        assert_eq!(
            store_of("https://user@store.lab/").as_deref(),
            Some("https://store.lab")
        );
        assert_eq!(
            store_of("http://[fe80::1]:80/x").as_deref(),
            Some("http://[fe80::1]")
        );
        assert_eq!(store_of("store.lab/Citrix"), None);
        assert_eq!(store_of("https:///x"), None);
        assert_eq!(store_of("1http://store.lab"), None);
    }

    #[test]
    fn a_file_is_read_whatever_its_encoding() {
        let text = "<a>\u{e9}</a>";
        let mut utf8 = UTF8_BOM.to_vec();
        utf8.extend_from_slice(text.as_bytes());
        assert_eq!(decode(&utf8).as_deref(), Ok(text));
        let mut le = UTF16_LE_BOM.to_vec();
        le.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode(&le).as_deref(), Ok(text));
        let mut be = UTF16_BE_BOM.to_vec();
        be.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
        assert_eq!(decode(&be).as_deref(), Ok(text));
        assert!(decode(&[0xC3]).is_err());
    }

    #[test]
    fn a_launch_line_never_shows_in_debug_output() {
        let app = CachedApp {
            name: "Excel".to_owned(),
            category: None,
            store_url: None,
            launch_line: Zeroizing::new("-qlaunch secret".to_owned()),
        };
        assert!(!format!("{app:?}").contains("secret"));
    }
}
