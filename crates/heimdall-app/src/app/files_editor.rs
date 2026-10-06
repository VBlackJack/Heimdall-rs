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

//! "Edit" in a Files tab, the integrated editor: the server's file opened in place of the
//! lists, saved only over the file it was opened from, closed with a question when its
//! text is not saved.

use super::{App, Dialog, Effect, FilesMessage};
use crate::files::{EntryKind, FilesError};
use crate::ids::{EditorId, TabId};
use crate::integrated_edit::{EditorNotice, INTEGRATED_EDIT_LIMIT, IntegratedEdit};
use crate::text_codec::{TextEncoding, encode};

impl App {
    /// A message of the integrated editor.
    pub(super) fn editor_message(&mut self, message: FilesMessage) -> Vec<Effect> {
        let message = self.to_editor_tab(message);
        match message {
            FilesMessage::EditIntegrated { tab } => self.edit_integrated(tab, false),
            FilesMessage::EditorOpened { tab, id, result } => {
                self.editor_opened(tab, id, result);
                Vec::new()
            }
            FilesMessage::EditorChanged { tab, id, dirty } => {
                if let Some(edit) = self.editor_mut(tab, id) {
                    edit.dirty = dirty;
                    if dirty && edit.notice == Some(EditorNotice::Saved) {
                        edit.notice = None;
                    }
                }
                Vec::new()
            }
            FilesMessage::EditorSave {
                tab,
                id,
                text,
                overwrite,
            } => self.editor_save(tab, id, &text, overwrite),
            FilesMessage::EditorSaved {
                tab,
                id,
                result,
                dirty,
            } => {
                if let Some(edit) = self.editor_mut(tab, id) {
                    edit.saving = false;
                    edit.dirty = dirty;
                    edit.notice = Some(match result {
                        Ok(fingerprint) => {
                            if let Some((_, opened)) = edit.opened.as_mut() {
                                *opened = fingerprint;
                            }
                            EditorNotice::Saved
                        }
                        Err(FilesError::ChangedOnServer) => EditorNotice::ChangedOnServer,
                        Err(error) => EditorNotice::Failed(error),
                    });
                }
                Vec::new()
            }
            FilesMessage::EditorClose { tab, id } => {
                self.editor_close(tab, id);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// `message`, addressed to the tab its editor is in now: a reconnection moves the
    /// editor to a new tab while a read or a save for it runs.
    fn to_editor_tab(&self, mut message: FilesMessage) -> FilesMessage {
        if let FilesMessage::EditorOpened { tab, id, .. }
        | FilesMessage::EditorChanged { tab, id, .. }
        | FilesMessage::EditorSave { tab, id, .. }
        | FilesMessage::EditorSaved { tab, id, .. }
        | FilesMessage::EditorClose { tab, id } = &mut message
            && let Some(found) = self.tabs.iter().find(|found| {
                found
                    .files
                    .as_ref()
                    .and_then(|files| files.editor.as_ref())
                    .is_some_and(|edit| edit.id == *id)
            })
        {
            *tab = found.id;
        }
        message
    }

    /// The integrated editor `id` of `tab`, while it is the one open.
    fn editor_mut(&mut self, tab: TabId, id: EditorId) -> Option<&mut IntegratedEdit> {
        self.files_mut(tab)?
            .editor
            .as_mut()
            .filter(|edit| edit.id == id)
    }

    /// Whether Open on the server's entry `index` opens it in the integrated editor, as the
    /// C# does: a file the editor takes, over SFTP or FTP alike. Larger, it is downloaded.
    pub(super) fn opens_in_editor(&self, tab: TabId, index: usize) -> bool {
        let Some(files) = self.tab(tab).and_then(|found| found.files.as_ref()) else {
            return false;
        };
        files.editor.is_none()
            && files.client.is_some()
            && files.remote.entries.get(index).is_some_and(|entry| {
                entry.kind == EntryKind::File
                    && entry.size.is_none_or(|size| size <= INTEGRATED_EDIT_LIMIT)
            })
    }

    /// "Edit", or Open (`from_open`): the server's selected file read for the integrated
    /// editor, unless one is open already.
    pub(super) fn edit_integrated(&mut self, tab: TabId, from_open: bool) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        if files.editor.is_some() {
            return Vec::new();
        }
        let Some(entry) = files
            .remote
            .selected
            .and_then(|index| files.remote.entries.get(index))
        else {
            return Vec::new();
        };
        if entry.kind != EntryKind::File {
            files.remote.error = Some(FilesError::NotAFile);
            return Vec::new();
        }
        if entry.size.is_some_and(|size| size > INTEGRATED_EDIT_LIMIT) {
            files.remote.error = Some(FilesError::TooLargeForEditor);
            return Vec::new();
        }
        let Some(client) = files.client.clone() else {
            return Vec::new();
        };
        let remote = files.remote.path.join(&entry.name);
        let id = EditorId::fresh();
        files.editor = Some(IntegratedEdit {
            id,
            remote: remote.clone(),
            name: entry.label.clone(),
            opened: None,
            dirty: false,
            saving: false,
            notice: None,
            from_open,
        });
        vec![Effect::OpenEditor {
            tab,
            id,
            client,
            remote,
        }]
    }

    /// The file was read, or why not: a file not opened says so on the lists.
    fn editor_opened(
        &mut self,
        tab: TabId,
        id: EditorId,
        result: Result<(TextEncoding, heimdall_files::Fingerprint), FilesError>,
    ) {
        let Some(files) = self.files_mut(tab) else {
            return;
        };
        if files.editor.as_ref().is_none_or(|edit| edit.id != id) {
            return;
        }
        match result {
            Ok((encoding, fingerprint)) => {
                if let Some(edit) = files.editor.as_mut() {
                    edit.opened = Some((encoding, fingerprint));
                    if encoding == TextEncoding::Latin1 {
                        edit.notice = Some(EditorNotice::Latin1);
                    }
                }
            }
            Err(error) => {
                let Some(edit) = files.editor.take() else {
                    return;
                };
                // Opened by Open: a file that is not text is offered for download, as the C#.
                if edit.from_open && error == FilesError::LooksBinary {
                    self.dialog = Some(Dialog::ConfirmDownloadBinary {
                        tab,
                        name: edit.name,
                        remote: edit.remote,
                    });
                } else {
                    files.remote.error = Some(error);
                }
            }
        }
    }

    /// Downloads the server's file `remote`, still listed, into the local folder shown.
    pub(super) fn download_remote(
        &mut self,
        tab: TabId,
        remote: &heimdall_files::RemotePath,
    ) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let path = files.remote.path.clone();
        let Some(index) = files
            .remote
            .entries
            .iter()
            .position(|entry| path.join(&entry.name) == *remote)
        else {
            return Vec::new();
        };
        files.remote.select_only(Some(index));
        self.start_transfer(tab, crate::files::Direction::Download)
    }

    /// Saves `text`, stored as the file was, only over the file opened unless `overwrite`.
    fn editor_save(
        &mut self,
        tab: TabId,
        id: EditorId,
        text: &str,
        overwrite: bool,
    ) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let client = files.client.clone();
        let Some(edit) = files.editor.as_mut().filter(|edit| edit.id == id) else {
            return Vec::new();
        };
        let Some((encoding, fingerprint)) = edit.opened else {
            return Vec::new();
        };
        if edit.saving {
            edit.notice = Some(EditorNotice::SaveRunning);
            return Vec::new();
        }
        let Some(client) = client else {
            edit.notice = Some(EditorNotice::SessionEnded);
            return Vec::new();
        };
        let bytes = match encode(text, encoding) {
            Ok(bytes) => bytes,
            Err(at) => {
                edit.notice = Some(EditorNotice::Unencodable(at));
                return Vec::new();
            }
        };
        edit.saving = true;
        edit.notice = None;
        vec![Effect::SaveEditor {
            tab,
            id,
            client,
            remote: edit.remote.clone(),
            bytes,
            expected: (!overwrite).then_some(fingerprint),
        }]
    }

    /// Closes the integrated editor: not while it saves, and asked first when its text is
    /// not saved.
    fn editor_close(&mut self, tab: TabId, id: EditorId) {
        // Another question on screen is answered first: never replaced.
        if self.dialog.is_some() {
            return;
        }
        let Some(files) = self.files_mut(tab) else {
            return;
        };
        let Some(edit) = files.editor.as_mut().filter(|edit| edit.id == id) else {
            return;
        };
        if edit.saving {
            edit.notice = Some(EditorNotice::SaveRunning);
        } else if edit.dirty {
            let name = edit.name.clone();
            self.dialog = Some(Dialog::ConfirmDiscardEditor { tab, name });
        } else {
            files.editor = None;
        }
    }
}
