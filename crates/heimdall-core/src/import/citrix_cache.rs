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
//! Only the applications the user is subscribed to are read, as the C# `ServerState` filter
//! does (v2026.102301): a resource whose `ServerState` is present and not `Subscribed` is a
//! catalog entry, skipped; a legacy store with no `ServerState` imports as before. The files
//! are untrusted input: size, node and application counts are capped, and nothing in a file
//! is ever run from here.
//!
//! Each application carries the line `SelfService.exe` launches it with, pre-authenticated:
//! a secret, never shown in a log.

use std::io::Read;
use std::path::{Path, PathBuf};

use roxmltree::{Document, Node, ParsingOptions};
use zeroize::Zeroizing;

/// The cache's folder under the local application data, as the C#.
#[cfg(windows)]
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
const SERVER_STATE: &str = "ServerState";

/// The `ServerState` of an application the user is subscribed to, as the C#.
const SUBSCRIBED: &str = "Subscribed";

/// The switch of a launch line that names the resource, as the C# `CitrixIdSwitch`.
const CITRIX_ID_SWITCH: &str = "-CitrixID";

/// The most a cache file may weigh: a store's cache is far smaller, and the file is read
/// whole into memory.
pub const MAX_CACHE_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// The most nodes a cache file may hold once parsed.
const MAX_XML_NODES: u32 = 2_000_000;

/// The most applications read from one cache file.
pub const MAX_APPS_PER_FILE: usize = 20_000;

/// The most applications kept from all the files of one scan.
pub const MAX_APPS_TOTAL: usize = 50_000;

/// The most cache files read in one scan.
pub const MAX_CACHE_FILES: usize = 256;

/// The longest store file-name prefix accepted.
pub const MAX_STORE_PREFIX_CHARS: usize = 64;

/// The only file name the `SelfService.exe` override may have.
pub const SELF_SERVICE_FILE_NAME: &str = "SelfService.exe";

/// Byte order marks a cache file may start with.
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
const UTF16_LE_BOM: [u8; 2] = [0xFF, 0xFE];
const UTF16_BE_BOM: [u8; 2] = [0xFE, 0xFF];

/// A published application found in the cache.
#[derive(Clone, PartialEq, Eq)]
pub struct CachedApp {
    /// The stable resource identifier of its launch line (`-CitrixID`), as the C#
    /// `CitrixId`: what a second import matches an application saved before by.
    pub citrix_id: Option<String>,
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
            .field("citrix_id", &self.citrix_id)
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
    /// A file held more than [`MAX_APPS_PER_FILE`] applications: the rest is not read.
    FileTruncated {
        /// The file's name.
        file: String,
    },
    /// The scan reached [`MAX_APPS_TOTAL`] applications: the rest is not read.
    TotalCapReached,
    /// Applications repeating the Citrix identifier of an earlier one were skipped, the
    /// first kept, as the C# `CitrixImportMerger`.
    DuplicateIds {
        /// How many were skipped.
        count: usize,
    },
    /// A setting saved for the scan was not valid and is ignored.
    SettingIgnored(CitrixSetting),
    /// A cache file could not be read.
    Unreadable {
        /// The file's name.
        file: String,
        /// Why.
        detail: String,
    },
}

/// A Citrix Workspace setting, named for the warning that says one was not valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CitrixSetting {
    /// The `SelfService.exe` path.
    SelfServicePath,
    /// The cache folder.
    CacheDirectory,
    /// The store file-name prefix.
    StorePrefix,
}

/// The applications of every cache file, and what was said on the way.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CacheScan {
    /// The applications, file after file.
    pub apps: Vec<CachedApp>,
    /// What was said.
    pub warnings: Vec<CacheWarning>,
}

/// Where this account's cache is: `Citrix\SelfService` under the local application data
/// folder Windows says, never the one the environment names. `None` off Windows, where
/// Citrix Workspace keeps no such cache.
#[must_use]
pub fn cache_folder() -> Option<PathBuf> {
    #[cfg(windows)]
    return crate::paths::local_app_data()
        .map(|root| CACHE_FOLDER.iter().fold(root, |path, part| path.join(part)));
    #[cfg(not(windows))]
    return None;
}

/// Scans the cache of this account, or the folder `directory` names when it is not blank,
/// reading only the files whose name starts with `prefix` when it is not blank. A
/// directory or a prefix that [`cache_directory_valid`] or [`normalize_store_prefix`]
/// refuses is ignored, as the C# ignores an invalid prefix: the default is scanned.
#[must_use]
pub fn scan(directory: &str, prefix: &str) -> CacheScan {
    let folder = if directory.trim().is_empty() || !cache_directory_valid(directory) {
        cache_folder()
    } else {
        Some(PathBuf::from(directory.trim()))
    };
    let prefix = normalize_store_prefix(prefix);
    match folder {
        Some(folder) => scan_folder(&folder, prefix.as_deref()),
        None => CacheScan {
            apps: Vec::new(),
            warnings: vec![CacheWarning::FolderMissing],
        },
    }
}

/// Scans the cache files of `folder`, in the order of their names, those whose name starts
/// with `prefix` (compared without case) when there is one.
#[must_use]
pub fn scan_folder(folder: &Path, prefix: Option<&str>) -> CacheScan {
    let mut scan = CacheScan::default();
    let Ok(entries) = std::fs::read_dir(folder) else {
        scan.warnings.push(CacheWarning::FolderMissing);
        return scan;
    };
    let prefix = prefix.map(str::to_ascii_lowercase);
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_cache_file(path, prefix.as_deref()))
        .collect();
    files.sort();
    files.truncate(MAX_CACHE_FILES);
    if files.is_empty() {
        scan.warnings.push(CacheWarning::NoCacheFiles);
    }
    for file in files {
        let name = || {
            file.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let read = read_capped(&file)
            .and_then(|bytes| decode(&bytes))
            .and_then(|text| parse_bounded(&text));
        match read {
            Ok((apps, truncated)) => {
                if truncated {
                    scan.warnings
                        .push(CacheWarning::FileTruncated { file: name() });
                }
                let room = MAX_APPS_TOTAL.saturating_sub(scan.apps.len());
                if apps.len() > room {
                    scan.warnings.push(CacheWarning::TotalCapReached);
                }
                scan.apps.extend(apps.into_iter().take(room));
            }
            Err(detail) => scan.warnings.push(CacheWarning::Unreadable {
                file: file
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                detail,
            }),
        }
    }
    drop_duplicate_ids(&mut scan);
    scan
}

/// Keeps the first application of each Citrix identifier, as the C# merger does, and says
/// how many were skipped.
fn drop_duplicate_ids(scan: &mut CacheScan) {
    let mut seen = std::collections::HashSet::new();
    let before = scan.apps.len();
    scan.apps.retain(|app| {
        app.citrix_id
            .as_ref()
            .is_none_or(|id| seen.insert(id.clone()))
    });
    let count = before - scan.apps.len();
    if count > 0 {
        scan.warnings.push(CacheWarning::DuplicateIds { count });
    }
}

/// The bytes of `path`, refused when over [`MAX_CACHE_FILE_BYTES`]: never read past the cap.
fn read_capped(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_CACHE_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if u64::try_from(bytes.len()).is_ok_and(|len| len > MAX_CACHE_FILE_BYTES) {
        return Err(format!("larger than {MAX_CACHE_FILE_BYTES} bytes"));
    }
    Ok(bytes)
}

/// Whether `path` is named as a cache file: `<prefix>*_Cache.xml`, `prefix` being lowercase.
fn is_cache_file(path: &Path, prefix: Option<&str>) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|name| {
            name.ends_with(CACHE_FILE_SUFFIX)
                && prefix.is_none_or(|prefix| name.starts_with(prefix))
        })
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
    parse_bounded(text).map(|(apps, _)| apps)
}

/// [`parse`], and whether the file held more than [`MAX_APPS_PER_FILE`] applications.
///
/// # Errors
///
/// Why the file is not XML, or holds a DTD.
pub fn parse_bounded(text: &str) -> Result<(Vec<CachedApp>, bool), String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // A DTD is refused, which no entity can be expanded without; the node count is capped.
    let options = ParsingOptions {
        allow_dtd: false,
        nodes_limit: MAX_XML_NODES,
        ..ParsingOptions::default()
    };
    let document =
        Document::parse_with_options(text, options).map_err(|error| error.to_string())?;
    let mut store_url = None;
    let mut apps = Vec::new();
    let mut truncated = false;
    for resource in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == RESOURCE)
    {
        // A catalog entry the user never subscribed to is not theirs to import; a store
        // whose cache holds no state at all is a legacy one, imported as before.
        if value(resource, SERVER_STATE)
            .is_some_and(|state| !state.eq_ignore_ascii_case(SUBSCRIBED))
        {
            continue;
        }
        let (Some(name), Some(launch_line)) = (
            value(resource, FRIENDLY_NAME),
            value(resource, LAUNCH_COMMAND_LINE),
        ) else {
            continue;
        };
        if store_url.is_none() {
            store_url = value(resource, ICA_LAUNCH_URL).and_then(|url| store_of(&url));
        }
        if apps.len() >= MAX_APPS_PER_FILE {
            truncated = true;
            break;
        }
        apps.push(CachedApp {
            citrix_id: extract_citrix_id(&launch_line),
            name,
            category: value(resource, CATEGORY),
            store_url: None,
            launch_line: Zeroizing::new(launch_line),
        });
    }
    for app in &mut apps {
        app.store_url.clone_from(&store_url);
    }
    Ok((apps, truncated))
}

/// The resource identifier a launch line passes to `-CitrixID`, as the C#
/// `ExtractCitrixId`: the switch is a whole word, its value quoted or a bare word that does
/// not start with `-`; `None` when absent or empty.
#[must_use]
pub fn extract_citrix_id(line: &str) -> Option<String> {
    // ASCII lowercasing keeps every byte offset, so a match in it is a match in `line`.
    let lower = line.to_ascii_lowercase();
    let switch = CITRIX_ID_SWITCH.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(&switch) {
        let start = from + found;
        let end = start + switch.len();
        let left_ok = line[..start]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace);
        let right_ok = line[end..].chars().next().is_none_or(char::is_whitespace);
        if !left_ok || !right_ok {
            from = end;
            continue;
        }
        let rest = line[end..].trim_start();
        if let Some(quoted) = rest.strip_prefix('"') {
            let value = quoted.split_once('"')?.0.trim();
            return (!value.is_empty()).then(|| value.to_owned());
        }
        let token = rest.split(char::is_whitespace).next().unwrap_or_default();
        return (!token.is_empty() && !token.starts_with('-')).then(|| token.to_owned());
    }
    None
}

/// `prefix` trimmed when it is a store file-name prefix that is safe: letters, digits and
/// `-`, `_`, `.`, space; at most [`MAX_STORE_PREFIX_CHARS`] of them. `None` when blank or
/// invalid: a path separator, a wildcard, a control or invisible character, or anything else
/// that would let the scan leave the cache folder.
#[must_use]
pub fn normalize_store_prefix(prefix: &str) -> Option<String> {
    let trimmed = prefix.trim();
    let safe = !trimmed.is_empty()
        && trimmed.chars().count() <= MAX_STORE_PREFIX_CHARS
        && trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '));
    safe.then(|| trimmed.to_owned())
}

/// Whether `c` separates the steps of a path on Windows or elsewhere.
fn is_separator(c: char) -> bool {
    matches!(c, '/' | '\\')
}

/// Whether `text` can be a path typed in the Settings: no control or invisible character, an
/// absolute path of this computer (a drive or a root), never a share, and no `..` step.
fn local_absolute_path(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed
            .chars()
            .any(|c| c.is_control() || crate::invisible::is_invisible(c))
    {
        return false;
    }
    // A share is another computer's, which a file of settings could point at.
    let mut first_two = trimmed.chars().take(2);
    if first_two.next().is_some_and(is_separator) && first_two.next().is_some_and(is_separator) {
        return false;
    }
    let bytes = trimmed.as_bytes();
    let has_drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let rooted = trimmed.starts_with('/') || Path::new(trimmed).is_absolute();
    (has_drive || rooted) && !trimmed.split(['/', '\\']).any(|part| part == "..")
}

/// Whether `directory` can be the cache folder to scan, see [`local_absolute_path`].
#[must_use]
pub fn cache_directory_valid(directory: &str) -> bool {
    local_absolute_path(directory)
}

/// Whether `path` can be the `SelfService.exe` to launch: a local absolute path whose file
/// name is [`SELF_SERVICE_FILE_NAME`], compared without case, as the C#.
#[must_use]
pub fn self_service_path_valid(path: &str) -> bool {
    local_absolute_path(path)
        && path
            .trim()
            .rsplit(['/', '\\'])
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(SELF_SERVICE_FILE_NAME))
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
            citrix_id: None,
            name: "Excel".to_owned(),
            category: None,
            store_url: None,
            launch_line: Zeroizing::new("-qlaunch secret".to_owned()),
        };
        assert!(!format!("{app:?}").contains("secret"));
    }
}
