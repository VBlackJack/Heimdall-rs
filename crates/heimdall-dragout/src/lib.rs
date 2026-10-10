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

//! Files of this computer dragged out of the window, to Explorer or another application, as
//! the C# Files tab's local list drags them (`LocalFileBrowserView.xaml.cs:612-630`: the
//! paths as a file drop, copy only).
//!
//! On Windows the shell does the whole drag: it makes the data object of the files from
//! their item identifiers, supplies the drag source and the drag image, and runs the drag
//! loop. Nothing here implements a COM object; the calls to the shell are the only unsafe
//! code, in one module. Elsewhere, dragging out is not supported.

use std::path::{Path, PathBuf};

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "the shell's drag and drop is only reached through its COM functions"
)]
mod shell;

/// Whether files can be dragged out of the window on this system.
pub const SUPPORTED: bool = cfg!(windows);

/// How a drag out ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOutcome {
    /// The files were dropped where a copy of them was taken.
    Copied,
    /// The drag was given up, or dropped where nothing took them.
    Cancelled,
}

/// Why files could not be dragged out.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DragError {
    /// This system has no drag out.
    #[error("dragging files out of the window is not supported on this system")]
    Unsupported,
    /// No file was given.
    #[error("no file to drag")]
    Empty,
    /// A path is not a full path of this computer, from the root of its drive.
    #[error("not a full path: {0}")]
    InvalidPath(PathBuf),
    /// The files are not all in one folder: the shell drags the items of one folder.
    #[error("the files dragged are not all in the same folder")]
    SeveralFolders,
    /// No window was given to own the drag.
    #[error("no window to drag from")]
    NoWindow,
    /// The thread is not in a single-threaded COM apartment, as a drag needs.
    #[error("the calling thread is not in a single-threaded COM apartment")]
    NotSingleThreaded,
    /// The shell refused, with this code and this message.
    #[error("the shell refused ({code:#010x}): {message}")]
    Shell {
        /// The `HRESULT`.
        code: i32,
        /// What the system says of it.
        message: String,
    },
}

/// The one folder `paths` are in: the shell drags the items of one folder, as Explorer's
/// own selection always is.
///
/// # Errors
///
/// [`DragError::Empty`] without a path, [`DragError::InvalidPath`] for one that is not a
/// full path, [`DragError::SeveralFolders`] when they are not all in the same folder, a
/// root counting as in no folder.
pub fn common_folder(paths: &[PathBuf]) -> Result<&Path, DragError> {
    let mut folder = None;
    for path in paths {
        if !path.is_absolute() {
            return Err(DragError::InvalidPath(path.clone()));
        }
        let parent = path.parent().ok_or(DragError::SeveralFolders)?;
        match folder {
            None => folder = Some(parent),
            Some(seen) if seen == parent => {}
            Some(_) => return Err(DragError::SeveralFolders),
        }
    }
    folder.ok_or(DragError::Empty)
}

/// Drags `paths` out of the window `window`, a Win32 window handle, as a copy, until the
/// files are dropped or the drag is given up; the system's drag loop runs meanwhile, and
/// the window's messages are dispatched from it.
///
/// To be called on the thread that owns `window`, in a single-threaded COM apartment, the
/// left mouse button held: the event loop's thread, whose window library initialised OLE.
/// This function never initialises COM itself.
///
/// # Errors
///
/// Those of [`common_folder`]; [`DragError::NoWindow`] for a null handle;
/// [`DragError::NotSingleThreaded`] off a single-threaded apartment;
/// [`DragError::Shell`] when the shell refuses a path or the drag;
/// [`DragError::Unsupported`] on a system other than Windows.
pub fn drag_files(window: isize, paths: &[PathBuf]) -> Result<DragOutcome, DragError> {
    common_folder(paths)?;
    if window == 0 {
        return Err(DragError::NoWindow);
    }
    platform_drag(window, paths)
}

#[cfg(windows)]
fn platform_drag(window: isize, paths: &[PathBuf]) -> Result<DragOutcome, DragError> {
    shell::drag(window, paths)
}

#[cfg(not(windows))]
fn platform_drag(_window: isize, _paths: &[PathBuf]) -> Result<DragOutcome, DragError> {
    Err(DragError::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn the_files_of_one_folder_have_it_in_common() {
        let folder = root().join("folder");
        let paths = [folder.join("a.txt"), folder.join("b")];
        assert_eq!(common_folder(&paths), Ok(folder.as_path()));
    }

    #[test]
    fn files_of_two_folders_are_refused() {
        let paths = [root().join("one").join("a"), root().join("two").join("b")];
        assert_eq!(common_folder(&paths), Err(DragError::SeveralFolders));
        let nested = [root().join("a"), root().join("a").join("b")];
        assert_eq!(common_folder(&nested), Err(DragError::SeveralFolders));
    }

    #[test]
    fn nothing_a_partial_path_or_a_root_is_refused() {
        assert_eq!(common_folder(&[]), Err(DragError::Empty));
        let partial = PathBuf::from("relative").join("a");
        let refused = Err(DragError::InvalidPath(partial.clone()));
        assert_eq!(common_folder(&[partial]), refused);
        let top = root()
            .ancestors()
            .last()
            .map(Path::to_path_buf)
            .expect("a root");
        assert_eq!(common_folder(&[top]), Err(DragError::SeveralFolders));
    }

    #[test]
    fn no_window_is_refused_before_the_system_is_asked() {
        let paths = [root().join("a")];
        assert_eq!(drag_files(0, &paths), Err(DragError::NoWindow));
        assert_eq!(drag_files(1, &[]), Err(DragError::Empty));
    }

    /// The `[lints]` table of a manifest; the workspace's own when `workspace`.
    fn lints(manifest: &str, workspace: bool) -> toml::Table {
        let table: toml::Table = manifest.parse().expect("a manifest");
        let holder = if workspace {
            table["workspace"].as_table().expect("a workspace").clone()
        } else {
            table
        };
        holder["lints"].as_table().expect("a lints table").clone()
    }

    #[test]
    fn the_lints_are_the_workspace_s_but_for_unsafe_code() {
        let workspace = lints(include_str!("../../../Cargo.toml"), true);
        let mut own = lints(include_str!("../Cargo.toml"), false);
        let mut expected = workspace;
        // Denied rather than forbidden, so that the shell's module can allow it.
        assert_eq!(
            expected["rust"]["unsafe_code"].as_str(),
            Some("forbid"),
            "the workspace forbids unsafe code"
        );
        expected["rust"]
            .as_table_mut()
            .expect("rust lints")
            .insert("unsafe_code".to_owned(), "deny".into());
        // Every unsafe block explained.
        let added = own["clippy"]
            .as_table_mut()
            .expect("clippy lints")
            .remove("undocumented_unsafe_blocks");
        assert_eq!(added.as_ref().and_then(toml::Value::as_str), Some("deny"));
        assert_eq!(
            own, expected,
            "a lint added to the workspace is added here too"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_than_windows_dragging_out_is_unsupported() {
        const { assert!(!SUPPORTED) };
        let paths = [root().join("a")];
        assert_eq!(drag_files(1, &paths), Err(DragError::Unsupported));
    }
}
