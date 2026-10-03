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

//! Files copied on this side, offered to the server through the clipboard (MS-RDPECLIP
//! file streams), as mstsc offers them: the files and folders Explorer copied, walked into
//! one entry each, then their bytes read when the server asks, off the session's task.
//!
//! Links are not followed: a copy holds what the user sees, never what a link points to.

use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};

use ironrdp::cliprdr::pdu::{
    ClipboardFileAttributes, FileContentsFlags, FileContentsRequest, FileContentsResponse,
    FileDescriptor,
};

/// Most bytes one copy offers, all its files together.
pub const MAX_COPY_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Most files and folders one copy offers.
pub const MAX_COPY_ENTRIES: usize = 10_000;

/// Longest name a file list carries, its folders included, in UTF-16 units: its field
/// holds 260 with the terminating null.
const MAX_WIRE_NAME: usize = 259;

/// Most bytes one answer carries: a server asks for less, and more is not read.
const MAX_CHUNK: u32 = 4 * 1024 * 1024;

/// Separator of folders in a file list's names.
const WIRE_SEPARATOR: char = '\\';

/// Why the files copied on this side are not offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyRefusal {
    /// More than [`MAX_COPY_ENTRIES`] files and folders.
    TooManyEntries,
    /// More than [`MAX_COPY_BYTES`] in all.
    TooLarge,
}

/// How much one copy may offer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    /// Files and folders.
    pub(crate) entries: usize,
    /// Bytes, all files together.
    pub(crate) bytes: u64,
}

/// The limits of a copy: [`MAX_COPY_ENTRIES`] and [`MAX_COPY_BYTES`].
pub(crate) const COPY_LIMITS: Limits = Limits {
    entries: MAX_COPY_ENTRIES,
    bytes: MAX_COPY_BYTES,
};

/// A file or folder offered, where the server's index for it points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// Where it is on this side.
    pub(crate) path: PathBuf,
    /// A folder: it has no bytes of its own.
    pub(crate) directory: bool,
}

/// Files walked, ready to offer: what the server is told of each, and where each is, at
/// the same index.
#[derive(Debug, Default)]
pub(crate) struct FileList {
    /// What the server is told.
    pub(crate) descriptors: Vec<FileDescriptor>,
    /// Where each one is.
    pub(crate) entries: Vec<Entry>,
}

/// Walks `paths`, the files and folders copied, into one entry for each file and folder
/// they hold, a folder before its content. What cannot be offered is left out: a link,
/// what is neither a file nor a folder, a name the file list cannot carry, and what
/// cannot be read.
///
/// # Errors
///
/// More than `limits` allow: the copy is refused whole, not cut.
pub(crate) fn walk(paths: &[PathBuf], limits: Limits) -> Result<FileList, CopyRefusal> {
    let mut walk = Walk {
        list: FileList::default(),
        bytes: 0,
        limits,
    };
    for path in paths {
        walk.add(path, None)?;
    }
    Ok(walk.list)
}

struct Walk {
    list: FileList,
    bytes: u64,
    limits: Limits,
}

impl Walk {
    /// Adds `path`, inside the folder named `parent` in the file list, and what it holds.
    fn add(&mut self, path: &Path, parent: Option<&str>) -> Result<(), CopyRefusal> {
        let Ok(metadata) = std::fs::symlink_metadata(path) else {
            return Ok(());
        };
        let directory = metadata.is_dir();
        if metadata.file_type().is_symlink() || !(directory || metadata.is_file()) {
            return Ok(());
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Ok(());
        };
        if !carried(parent, name) {
            return Ok(());
        }
        if self.list.entries.len() >= self.limits.entries {
            return Err(CopyRefusal::TooManyEntries);
        }
        let size = if directory { 0 } else { metadata.len() };
        self.bytes = self.bytes.saturating_add(size);
        if self.bytes > self.limits.bytes {
            return Err(CopyRefusal::TooLarge);
        }
        self.list
            .descriptors
            .push(descriptor(name, parent, &metadata));
        self.list.entries.push(Entry {
            path: path.to_path_buf(),
            directory,
        });
        if directory {
            let inside = match parent {
                Some(parent) => format!("{parent}{WIRE_SEPARATOR}{name}"),
                None => name.to_owned(),
            };
            let Ok(read) = std::fs::read_dir(path) else {
                return Ok(());
            };
            let mut children: Vec<PathBuf> = read
                .filter_map(|child| child.ok().map(|child| child.path()))
                .collect();
            children.sort();
            for child in children {
                self.add(&child, Some(&inside))?;
            }
        }
        Ok(())
    }
}

/// Whether a file list carries `name` inside the folder `parent`: not empty, no separator
/// nor drive in it, and short enough with its folders. `IronRDP` drops what it cannot
/// carry, which would shift every index after it; so it is left out here first.
fn carried(parent: Option<&str>, name: &str) -> bool {
    let length =
        parent.map_or(0, |parent| parent.encode_utf16().count() + 1) + name.encode_utf16().count();
    !name.is_empty() && !name.contains([WIRE_SEPARATOR, '/', ':']) && length <= MAX_WIRE_NAME
}

/// What the server is told of `name`, inside the folder `parent`.
fn descriptor(name: &str, parent: Option<&str>, metadata: &std::fs::Metadata) -> FileDescriptor {
    let mut attributes = if metadata.is_dir() {
        ClipboardFileAttributes::DIRECTORY
    } else {
        ClipboardFileAttributes::ARCHIVE
    };
    if metadata.is_file() && metadata.permissions().readonly() {
        attributes |= ClipboardFileAttributes::READONLY;
    }
    let mut descriptor = FileDescriptor::new(name).with_attributes(attributes);
    if metadata.is_file() {
        descriptor = descriptor.with_file_size(metadata.len());
    }
    let written = crate::drives::filetime(metadata.modified().ok());
    if let Ok(written) = u64::try_from(written)
        && written > 0
    {
        descriptor = descriptor.with_last_write_time(written);
    }
    if let Some(parent) = parent {
        descriptor = descriptor.with_relative_path(parent);
    }
    descriptor
}

/// The answer to the server's `request` for `entry`, read from the disk now: its size, or
/// a range of its bytes. Blocking: run off the session's task.
pub(crate) fn answer(
    request: &FileContentsRequest,
    entry: Option<&Entry>,
) -> FileContentsResponse<'static> {
    let stream = request.stream_id;
    let Some(entry) = entry else {
        return FileContentsResponse::new_error(stream);
    };
    if request.flags.contains(FileContentsFlags::SIZE) {
        if entry.directory {
            return FileContentsResponse::new_size_response(stream, 0);
        }
        return match std::fs::symlink_metadata(&entry.path) {
            Ok(metadata) if metadata.is_file() => {
                FileContentsResponse::new_size_response(stream, metadata.len())
            }
            _ => FileContentsResponse::new_error(stream),
        };
    }
    if request.flags.contains(FileContentsFlags::RANGE) && !entry.directory {
        return match read_range(&entry.path, request.position, request.requested_size) {
            Ok(data) => FileContentsResponse::new_data_response(stream, data),
            Err(_) => FileContentsResponse::new_error(stream),
        };
    }
    FileContentsResponse::new_error(stream)
}

/// Up to `wanted` bytes of the file at `path` from `position`, [`MAX_CHUNK`] at most;
/// fewer at its end.
fn read_range(path: &Path, position: u64, wanted: u32) -> std::io::Result<Vec<u8>> {
    // Still the file walked, not a link put in its place since.
    if !std::fs::symlink_metadata(path)?.is_file() {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(position))?;
    let wanted = wanted.min(MAX_CHUNK);
    let mut data = Vec::with_capacity(usize::try_from(wanted).unwrap_or_default());
    file.take(u64::from(wanted)).read_to_end(&mut data)?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ironrdp::cliprdr::pdu::{ClipboardFileAttributes, FileContentsFlags, FileContentsRequest};

    use super::{COPY_LIMITS, CopyRefusal, Entry, Limits, MAX_CHUNK, MAX_WIRE_NAME, answer, walk};

    fn request(
        flags: FileContentsFlags,
        position: u64,
        requested_size: u32,
    ) -> FileContentsRequest {
        FileContentsRequest {
            stream_id: 7,
            index: 0,
            flags,
            position,
            requested_size,
            data_id: None,
        }
    }

    #[test]
    fn a_folder_is_offered_before_its_content_each_named_from_the_copy() {
        let dir = tempfile::tempdir().expect("dir");
        let folder = dir.path().join("logs");
        std::fs::create_dir_all(folder.join("old")).expect("folders");
        std::fs::write(folder.join("a.txt"), b"abc").expect("a");
        std::fs::write(folder.join("old").join("b.txt"), b"hello").expect("b");
        let single = dir.path().join("notes.md");
        std::fs::write(&single, b"x").expect("notes");

        let list = walk(&[folder.clone(), single.clone()], COPY_LIMITS).expect("walked");
        let names: Vec<(Option<&str>, &str)> = list
            .descriptors
            .iter()
            .map(|file| (file.relative_path.as_deref(), file.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                (None, "logs"),
                (Some("logs"), "a.txt"),
                (Some("logs"), "old"),
                (Some("logs\\old"), "b.txt"),
                (None, "notes.md"),
            ]
        );
        assert_eq!(list.entries.len(), list.descriptors.len(), "one entry each");
        assert_eq!(
            list.entries[3],
            Entry {
                path: folder.join("old").join("b.txt"),
                directory: false
            }
        );
        let folder_attributes = list.descriptors[0].attributes.expect("attributes");
        assert!(folder_attributes.contains(ClipboardFileAttributes::DIRECTORY));
        assert_eq!(list.descriptors[0].file_size, None, "a folder has no size");
        assert_eq!(list.descriptors[3].file_size, Some(5));
        assert!(list.descriptors[3].last_write_time.is_some());
    }

    #[test]
    fn a_copy_beyond_its_limits_is_refused_whole() {
        let dir = tempfile::tempdir().expect("dir");
        let paths: Vec<PathBuf> = (0..3)
            .map(|index| {
                let path = dir.path().join(format!("{index}.bin"));
                std::fs::write(&path, [0_u8; 10]).expect("file");
                path
            })
            .collect();
        let few = Limits {
            entries: 2,
            bytes: 1_000,
        };
        assert_eq!(
            walk(&paths, few).map(|list| list.entries.len()),
            Err(CopyRefusal::TooManyEntries)
        );
        let small = Limits {
            entries: 100,
            bytes: 25,
        };
        assert_eq!(
            walk(&paths, small).map(|list| list.entries.len()),
            Err(CopyRefusal::TooLarge)
        );
        assert_eq!(
            walk(&paths, COPY_LIMITS).map(|list| list.entries.len()),
            Ok(3)
        );
    }

    #[test]
    fn a_name_the_file_list_cannot_carry_is_left_out_with_its_content() {
        let dir = tempfile::tempdir().expect("dir");
        let long = dir.path().join("d".repeat(200));
        std::fs::create_dir(&long).expect("long folder");
        // 200 + 1 + 60 runs past the field's 259 units.
        let inside = long.join("f".repeat(60));
        std::fs::create_dir(&inside).expect("inside");
        std::fs::write(inside.join("x"), b"x").expect("deep file");
        std::fs::write(long.join("kept.txt"), b"k").expect("kept");

        let list = walk(&[long], COPY_LIMITS).expect("walked");
        let names: Vec<&str> = list
            .descriptors
            .iter()
            .map(|file| file.name.as_str())
            .collect();
        assert_eq!(names, ["d".repeat(200).as_str(), "kept.txt"]);
        assert!(list.descriptors.iter().all(|file| {
            file.relative_path.as_ref().map_or(0, |path| path.len() + 1) + file.name.len()
                <= MAX_WIRE_NAME
        }));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_not_followed() {
        let dir = tempfile::tempdir().expect("dir");
        let secret = dir.path().join("secret");
        std::fs::write(&secret, b"s").expect("secret");
        let folder = dir.path().join("shared");
        std::fs::create_dir(&folder).expect("folder");
        std::os::unix::fs::symlink(&secret, folder.join("link")).expect("link");
        std::os::unix::fs::symlink(&secret, dir.path().join("top-link")).expect("link");

        let list = walk(&[folder, dir.path().join("top-link")], COPY_LIMITS).expect("walked");
        let names: Vec<&str> = list
            .descriptors
            .iter()
            .map(|file| file.name.as_str())
            .collect();
        assert_eq!(names, ["shared"]);
    }

    #[test]
    fn the_server_gets_a_files_size_and_its_bytes_by_range() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("data.bin");
        std::fs::write(&path, b"0123456789").expect("file");
        let file = Entry {
            path,
            directory: false,
        };

        let size = answer(&request(FileContentsFlags::SIZE, 0, 8), Some(&file));
        assert_eq!(size.stream_id(), 7);
        assert_eq!(size.data_as_size().expect("a size"), 10);

        let range = answer(&request(FileContentsFlags::RANGE, 3, 4), Some(&file));
        assert_eq!(range.data(), b"3456");
        let end = answer(&request(FileContentsFlags::RANGE, 8, 100), Some(&file));
        assert_eq!(end.data(), b"89", "fewer at its end");
        let past = answer(&request(FileContentsFlags::RANGE, 50, 4), Some(&file));
        assert!(
            !past.is_error() && past.data().is_empty(),
            "nothing past its end"
        );
    }

    #[test]
    fn a_range_is_read_up_to_its_chunk() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("big.bin");
        let size = usize::try_from(MAX_CHUNK).expect("fits") + 10;
        std::fs::write(&path, vec![1_u8; size]).expect("file");
        let file = Entry {
            path,
            directory: false,
        };
        let range = answer(&request(FileContentsFlags::RANGE, 0, u32::MAX), Some(&file));
        assert_eq!(
            range.data().len(),
            usize::try_from(MAX_CHUNK).expect("fits")
        );
    }

    #[test]
    fn what_is_gone_a_folders_bytes_and_an_unknown_index_are_errors() {
        let dir = tempfile::tempdir().expect("dir");
        let gone = Entry {
            path: dir.path().join("gone"),
            directory: false,
        };
        assert!(answer(&request(FileContentsFlags::SIZE, 0, 8), Some(&gone)).is_error());
        assert!(answer(&request(FileContentsFlags::RANGE, 0, 8), Some(&gone)).is_error());

        let folder = Entry {
            path: dir.path().to_path_buf(),
            directory: true,
        };
        let size = answer(&request(FileContentsFlags::SIZE, 0, 8), Some(&folder));
        assert_eq!(size.data_as_size().expect("a size"), 0);
        assert!(answer(&request(FileContentsFlags::RANGE, 0, 8), Some(&folder)).is_error());

        assert!(answer(&request(FileContentsFlags::SIZE, 0, 8), None).is_error());
    }
}
