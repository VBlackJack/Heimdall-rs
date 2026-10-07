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

//! What a file or folder of this computer is, as the C# local file browser's Properties
//! shows it: its name and path, its kind, its size, when it was created, modified and last
//! read, and its attributes. All of it is what the standard library reads of the file
//! system; the shell's own Properties sheet is not opened.

use std::io;
use std::path::Path;
use std::time::SystemTime;

use crate::files::{EntryKind, local_kind};
use crate::text::server_text;

/// The Windows attribute of a file the system's file manager hides.
#[cfg(windows)]
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;

/// What an entry of the local file browser is, as its Properties dialog shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalProperties {
    /// Its name, made safe.
    pub name: String,
    /// Its whole path, made safe.
    pub path: String,
    /// Its kind, a link not followed.
    pub kind: EntryKind,
    /// Its size, for a regular file.
    pub size: Option<u64>,
    /// When it was made, where the file system keeps it.
    pub created: Option<SystemTime>,
    /// When it was last written.
    pub modified: Option<SystemTime>,
    /// When it was last read, where the file system keeps it.
    pub accessed: Option<SystemTime>,
    /// It cannot be written to.
    pub read_only: bool,
    /// Windows hides it: its hidden attribute, on Windows alone.
    pub hidden: Option<bool>,
    /// Where it points, for a link, made safe.
    pub link_target: Option<String>,
}

/// What entry `path` of this computer is, a link not followed.
///
/// # Errors
///
/// What the file system said when it could not be read.
pub fn read(path: &Path) -> io::Result<LocalProperties> {
    let metadata = path.symlink_metadata()?;
    let kind = local_kind(metadata.file_type());
    let name = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy();
    let link_target = if kind == EntryKind::Link {
        std::fs::read_link(path)
            .ok()
            .map(|target| server_text(&target.to_string_lossy()))
    } else {
        None
    };
    // Windows' hidden attribute; there is no such attribute elsewhere.
    #[cfg(windows)]
    let hidden = {
        use std::os::windows::fs::MetadataExt as _;
        Some(metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
    };
    #[cfg(not(windows))]
    let hidden = None;
    Ok(LocalProperties {
        name: server_text(&name),
        path: server_text(&path.to_string_lossy()),
        kind,
        size: (kind == EntryKind::File).then_some(metadata.len()),
        created: metadata.created().ok(),
        modified: metadata.modified().ok(),
        accessed: metadata.accessed().ok(),
        read_only: metadata.permissions().readonly(),
        hidden,
        link_target,
    })
}
