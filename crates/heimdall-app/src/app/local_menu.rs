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

//! The entries of the local file browser's menu its Files pane does not have, as the C#
//! `LocalFileBrowserView` offers them: "Open With", "Open in Editor", "Copy", "Paste" and
//! "Properties".
//!
//! - "Open With" shows the system's "Open with" chooser for one regular file, as the C#
//!   does through `rundll32.exe shell32.dll,OpenAs_RunDLL`. The chooser can run the file:
//!   one that would run is asked about first, as Open asks. Other systems have no such
//!   chooser, and their menu does not offer it.
//! - "Open in Editor" opens one regular file, whatever its kind, in the external editor
//!   set, as a text file opens: nothing is watched or sent.
//! - "Copy" and "Paste" go through the system's clipboard on Windows, as the C# does: the
//!   files chosen put there as Explorer's Copy puts them, and the files found there, copied
//!   in Heimdall or in Explorer, pasted into the folder shown as [`crate::local_paste`]
//!   says. Elsewhere, where no clipboard holds copied files, Heimdall holds them itself, to
//!   be pasted into a local file browser or uploaded by a Files tab's "Paste". The C#
//!   browser has no "Cut": none is offered.
//! - "Properties" shows what the file system says of the entry chosen alone, as
//!   [`crate::local_properties`] reads it.

use std::path::PathBuf;

use super::{App, Dialog, Effect, Notice};
use crate::external_edit::editor;
use crate::files::{EntryKind, FileOperation, FilesError, Side};
use crate::ids::TabId;
use crate::local_open;
use crate::text::visible_text;

/// Whether the system's clipboard holds copied files, Explorer's and Heimdall's alike: on
/// Windows alone. Elsewhere, Heimdall holds what its local file browser copies.
const SYSTEM_FILE_LIST: bool = cfg!(windows);

impl App {
    /// Whether `tab` is a local file browser: this computer's files alone.
    pub(super) fn is_local_browser_files(&self, tab: TabId) -> bool {
        self.tab(tab)
            .and_then(|tab| tab.files.as_deref())
            .is_some_and(|files| files.local_only)
    }

    /// Whether entry `index` of `tab` is offered "Open With" and "Open in Editor", as the
    /// C# offers them: a regular file of the local file browser, chosen alone.
    #[must_use]
    pub fn offers_local_file_entry(&self, tab: TabId, index: usize) -> bool {
        self.tab(tab)
            .and_then(|tab| tab.files.as_deref())
            .filter(|files| files.local_only)
            .is_some_and(|files| {
                files
                    .local
                    .entries
                    .get(index)
                    .is_some_and(|entry| entry.kind == EntryKind::File)
                    && files.local.chosen().len() <= 1
            })
    }

    /// The full path of entry `index` of local file browser `tab`, with its name, when it
    /// is offered "Open With" and "Open in Editor".
    fn offered_local_file(&self, tab: TabId, index: usize) -> Option<(PathBuf, String)> {
        if !self.offers_local_file_entry(tab, index) {
            return None;
        }
        let local = &self.tab(tab)?.files.as_deref()?.local;
        let entry = local.entries.get(index)?;
        Some((
            local.path.join(&entry.name),
            entry.name.to_string_lossy().into_owned(),
        ))
    }

    /// "Open With" on entry `index` of local file browser `tab`: the system's chooser for
    /// it; asked first, its full path shown, when the file would run.
    pub(super) fn open_with(&mut self, tab: TabId, index: usize) -> Vec<Effect> {
        let Some((file, name)) = self.offered_local_file(tab, index) else {
            return Vec::new();
        };
        if local_open::runnable(&name, &file) {
            self.dialog = Some(Dialog::ConfirmOpenRunnable {
                tab,
                shown: visible_text(&file.to_string_lossy()),
                file,
                chooser: true,
            });
            return Vec::new();
        }
        vec![Effect::OpenWithChooser { tab, file }]
    }

    /// "Open in Editor" on entry `index` of local file browser `tab`: the file in the
    /// external editor set.
    pub(super) fn open_in_editor(&mut self, tab: TabId, index: usize) -> Vec<Effect> {
        match self.offered_local_file(tab, index) {
            Some((file, _)) => self.edit_local_file(tab, file),
            None => Vec::new(),
        }
    }

    /// Local `file` of `tab` in the external editor set, nothing watched; an editor
    /// refused is said on the pane.
    pub(super) fn edit_local_file(&mut self, tab: TabId, file: PathBuf) -> Vec<Effect> {
        match editor(&self.settings.external_editor) {
            Ok(editor) => vec![Effect::LaunchEditor { tab, editor, file }],
            Err(refused) => {
                if let Some(files) = self.files_mut(tab) {
                    files.local.error = Some(super::files_edit::editor_error(refused));
                }
                Vec::new()
            }
        }
    }

    /// "Copy" in local file browser `tab`: the entries chosen, whatever their kind, as the
    /// C# copies them; on the system's clipboard on Windows, else held by Heimdall. What
    /// was cut or copied before gives way, as on a clipboard.
    pub(super) fn copy_local(&mut self, tab: TabId) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab).filter(|files| files.local_only) else {
            return Vec::new();
        };
        let local = &files.local;
        let paths: Vec<PathBuf> = local
            .chosen()
            .into_iter()
            .filter_map(|index| local.entries.get(index))
            .map(|entry| local.path.join(&entry.name))
            .collect();
        if paths.is_empty() {
            return Vec::new();
        }
        self.files_clipboard = None;
        self.tell(Notice::FilesCopied(paths.len()));
        if SYSTEM_FILE_LIST {
            self.local_copied.clear();
            vec![Effect::WriteFileList(paths)]
        } else {
            self.local_copied = paths;
            Vec::new()
        }
    }

    /// Whether `tab` can paste copied files of this computer, when that is what its
    /// "Paste" does: `None` when its "Paste" is about the server's entries held. A local
    /// file browser pastes on Windows whatever the clipboard holds, read when pasting, and
    /// elsewhere the files Heimdall holds; a connected Files tab uploads those.
    pub(super) fn pastes_local_files(&self, tab: TabId) -> Option<bool> {
        if self.is_local_browser_files(tab) {
            return Some(SYSTEM_FILE_LIST || !self.local_copied.is_empty());
        }
        (self.files_clipboard.is_none() && !self.local_copied.is_empty())
            .then(|| self.files_connected(tab))
    }

    /// "Paste" of files of this computer in `tab`, when that is what it does: into the
    /// folder a local file browser shows, the system's clipboard read first on Windows;
    /// or uploaded into the server's folder shown. `None` when its "Paste" is about the
    /// server's entries held.
    pub(super) fn paste_local_files(&mut self, tab: TabId) -> Option<Vec<Effect>> {
        if self.is_local_browser_files(tab) {
            if SYSTEM_FILE_LIST {
                return Some(vec![Effect::ReadExplorerFiles { tab }]);
            }
            let paths = self.local_copied.clone();
            return Some(self.paste_into_browser(tab, paths));
        }
        if self.pastes_local_files(tab) != Some(true) {
            return None;
        }
        let paths = self.local_copied.clone();
        Some(self.upload_paths(tab, &paths))
    }

    /// The files copied in Explorer, read: none is said; in a local file browser they are
    /// pasted into the folder it shows, elsewhere uploaded into the server's folder shown.
    pub(super) fn explorer_files_read(&mut self, tab: TabId, paths: Vec<PathBuf>) -> Vec<Effect> {
        if paths.is_empty() {
            self.tell(Notice::ExplorerHoldsNoFiles);
            Vec::new()
        } else if self.is_local_browser_files(tab) {
            self.paste_into_browser(tab, paths)
        } else {
            self.upload_paths(tab, &paths)
        }
    }

    /// Copies `paths`, files and folders of this computer, into the folder local file
    /// browser `tab` shows, as [`crate::local_paste::paste`] does; the folder is listed
    /// again once done.
    pub(super) fn paste_into_browser(&mut self, tab: TabId, paths: Vec<PathBuf>) -> Vec<Effect> {
        if paths.is_empty() {
            return Vec::new();
        }
        let Some(files) = self.files_mut(tab).filter(|files| files.local_only) else {
            return Vec::new();
        };
        let folder = files.local.path.clone();
        vec![Effect::FileOperation {
            tab,
            side: Side::Local,
            operation: Box::new(FileOperation::LocalPaste {
                sources: paths,
                folder,
            }),
        }]
    }

    /// "Properties" in local file browser `tab`: what the file system says of the entry
    /// chosen alone, as the C# shows it; what could not be read is said on the pane.
    pub(super) fn show_local_properties(&mut self, tab: TabId) {
        let Some(files) = self.files_mut(tab).filter(|files| files.local_only) else {
            return;
        };
        let local = &files.local;
        if local.chosen().len() > 1 {
            return;
        }
        let Some(path) = local
            .selected
            .and_then(|index| local.entries.get(index))
            .map(|entry| local.path.join(&entry.name))
        else {
            return;
        };
        match crate::local_properties::read(&path) {
            Ok(properties) => {
                self.dialog = Some(Dialog::LocalFileProperties(Box::new(properties)));
            }
            Err(error) => {
                files.local.error = Some(FilesError::Local {
                    detail: error.to_string(),
                });
            }
        }
    }
}
