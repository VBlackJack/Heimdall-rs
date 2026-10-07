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

//! "Paste" in the local file browser, as the C# `LocalFileBrowserViewModel.PasteFilesAsync`:
//! the files and folders copied, in Heimdall or in Explorer, copied into the folder the
//! browser shows.
//!
//! Where the C# asks about each name in the way, this paste never replaces anything, as
//! the Files tab's own paste on a server: an entry whose name is taken is copied under its
//! first free copy name ("a (copy).txt"), so pasting into the folder an entry is in makes a
//! copy of it beside it. As the C#, a folder is never pasted into itself or into one of its
//! own folders, and a link or a junction pasted is refused rather than copied through.
//! Inside a folder copied, links and special files are left out, as the paste across
//! servers leaves them. The first failure stops the rest.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::files::FilesError;
use crate::text::server_text;

/// How many copy names ("a (copy).txt", "a (copy 2).txt", ...) are tried before the paste
/// gives up on an entry.
const COPY_NAME_TRIES: usize = 1000;

/// Copies each of `sources`, files and folders of this computer, into `folder`, one after
/// another, as the module says.
///
/// # Errors
///
/// [`FilesError::PasteIntoItself`] for a folder pasted into itself or one of its own
/// folders; [`FilesError::PasteLink`] for a link or a junction; [`FilesError::NotAFile`]
/// for a device, a pipe or a socket; [`FilesError::TooLarge`] for a folder holding more
/// entries than a transfer walks; [`FilesError::Exists`] when no copy name is free;
/// [`FilesError::Local`] for what the file system refused. The first one stops the rest.
pub fn paste(sources: &[PathBuf], folder: &Path) -> Result<(), FilesError> {
    sources
        .iter()
        .try_for_each(|source| paste_one(source, folder))
}

/// Copies `source` into `folder`.
fn paste_one(source: &Path, folder: &Path) -> Result<(), FilesError> {
    let name = source.file_name().ok_or(FilesError::NotAFile)?;
    let shown = || server_text(&name.to_string_lossy());
    let metadata = source.symlink_metadata().map_err(|error| local(&error))?;
    let kind = metadata.file_type();
    // A junction is a link to Rust's standard library too: refused alike.
    if kind.is_symlink() {
        return Err(FilesError::PasteLink { name: shown() });
    }
    if kind.is_dir() {
        if is_same_or_inside(source, folder)? {
            return Err(FilesError::PasteIntoItself { name: shown() });
        }
    } else if !kind.is_file() {
        return Err(FilesError::NotAFile);
    }
    let target = free_target(folder, name)?;
    if kind.is_dir() {
        copy_folder(source, &target)
    } else {
        copy_file(source, &target, &metadata)
    }
}

/// Whether `folder` is folder `source` itself or one of the folders inside it, each taken
/// as the file system resolves it.
fn is_same_or_inside(source: &Path, folder: &Path) -> Result<bool, FilesError> {
    let source = source.canonicalize().map_err(|error| local(&error))?;
    let folder = folder.canonicalize().map_err(|error| local(&error))?;
    Ok(folder.starts_with(source))
}

/// Where entry `name` goes in `folder`: under its own name while nothing has it, else under
/// its first free copy name.
fn free_target(folder: &Path, name: &OsStr) -> Result<PathBuf, FilesError> {
    let taken = |candidate: &OsStr| folder.join(candidate).symlink_metadata().is_ok();
    if !taken(name) {
        return Ok(folder.join(name));
    }
    copy_candidates(name)
        .into_iter()
        .find(|candidate| !taken(candidate))
        .map(|free| folder.join(free))
        .ok_or(FilesError::Exists)
}

/// Copies regular file `source`, of `metadata`, to `target`, where nothing is: its content,
/// its modification time and its permissions. A copy that fails halfway is removed.
fn copy_file(source: &Path, target: &Path, metadata: &fs::Metadata) -> Result<(), FilesError> {
    let mut from = fs::File::open(source).map_err(|error| local(&error))?;
    // Made new: never one already there, whatever came between the name chosen and now.
    let mut to = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|error| local(&error))?;
    if let Err(error) = io::copy(&mut from, &mut to) {
        drop(to);
        let _ = fs::remove_file(target);
        return Err(local(&error));
    }
    // What the system cannot carry over leaves the copy as it is: its content is there.
    if let Ok(modified) = metadata.modified() {
        let _ = to.set_modified(modified);
    }
    drop(to);
    let _ = fs::set_permissions(target, metadata.permissions());
    Ok(())
}

/// Copies folder `source` whole to `target`, made where nothing is, folder by folder: its
/// folders and regular files, never its links nor its special files; as many entries as a
/// transfer walks at most.
fn copy_folder(source: &Path, target: &Path) -> Result<(), FilesError> {
    fs::create_dir(target).map_err(|error| local(&error))?;
    let mut pending = vec![(source.to_owned(), target.to_owned())];
    let mut walked = 0_usize;
    while let Some((from, to)) = pending.pop() {
        for entry in fs::read_dir(&from).map_err(|error| local(&error))? {
            let entry = entry.map_err(|error| local(&error))?;
            walked += 1;
            if walked > heimdall_sftp::tree::MAX_ENTRIES {
                return Err(FilesError::TooLarge);
            }
            // Not followed: a link is a link here.
            let kind = entry.file_type().map_err(|error| local(&error))?;
            let into = to.join(entry.file_name());
            if kind.is_dir() {
                fs::create_dir(&into).map_err(|error| local(&error))?;
                pending.push((entry.path(), into));
            } else if kind.is_file() {
                let metadata = entry.metadata().map_err(|error| local(&error))?;
                copy_file(&entry.path(), &into, &metadata)?;
            }
        }
    }
    Ok(())
}

/// What the file system said, as a Files error.
fn local(error: &io::Error) -> FilesError {
    if error.kind() == io::ErrorKind::AlreadyExists {
        FilesError::Exists
    } else {
        FilesError::Local {
            detail: error.to_string(),
        }
    }
}

/// The first copy names of `name` ("a (copy).txt", "a (copy 2).txt", ...), as many as
/// are tried.
#[cfg(unix)]
fn copy_candidates(name: &OsStr) -> Vec<OsString> {
    use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
    heimdall_files::conflict::copy_names(name.as_bytes())
        .take(COPY_NAME_TRIES)
        .map(OsString::from_vec)
        .collect()
}

/// The first copy names of `name` ("a (copy).txt", "a (copy 2).txt", ...), as many as
/// are tried; none for a name that is not Unicode, which they are made from.
#[cfg(not(unix))]
fn copy_candidates(name: &OsStr) -> Vec<OsString> {
    let Some(name) = name.to_str() else {
        return Vec::new();
    };
    heimdall_files::conflict::copy_names(name.as_bytes())
        .take(COPY_NAME_TRIES)
        .filter_map(|bytes| String::from_utf8(bytes).ok())
        .map(OsString::from)
        .collect()
}
