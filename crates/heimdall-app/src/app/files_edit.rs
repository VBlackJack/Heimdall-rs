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

//! "Edit with external editor" in a Files tab: the file opened, then watched while the tab
//! is open, each save sent as [`crate::external_edit`] says.

use std::path::PathBuf;

use tokio_util::sync::CancellationToken;

use super::{App, Effect, Notice};
use crate::external_edit::{EDIT_SIZE_LIMIT, EditCheck, EditSession, EditorRefused, editor};
use crate::files::{EntryKind, FilesError, Side};
use crate::ids::TabId;

impl App {
    /// A message of a file edited with the external editor.
    pub(super) fn edit_message(&mut self, message: super::FilesMessage) -> Vec<Effect> {
        use super::FilesMessage;
        match message {
            FilesMessage::EditExternal { tab } => self.edit_external(tab),
            FilesMessage::EditStarted { tab, result } => self.edit_started(tab, result),
            FilesMessage::EditorLaunched { tab, result } => {
                if let Err(error) = result {
                    self.editor_failed(tab, error);
                }
                Vec::new()
            }
            FilesMessage::EditTick => self.edit_tick(),
            FilesMessage::EditsChecked { tab, results } => self.edits_checked(tab, results),
            _ => Vec::new(),
        }
    }

    /// Whether a Files tab has files being edited, to be looked at every
    /// [`crate::external_edit::EDIT_LOOK`].
    #[must_use]
    pub fn has_edits(&self) -> bool {
        self.tabs
            .iter()
            .filter_map(|tab| tab.files.as_deref())
            .any(|files| !files.edits.is_empty())
    }

    /// Where files are edited: the user's own local folder, unless a test chose another.
    pub fn set_edit_dir(&mut self, folder: PathBuf) {
        self.edit_dir = Some(folder);
    }

    /// "Edit with external editor": the selected file of the server opened in the editor
    /// set; one already being edited is opened in it again.
    fn edit_external(&mut self, tab_id: TabId) -> Vec<Effect> {
        let setting = self.settings.external_editor.clone();
        let base = self.edit_dir.clone();
        let Some(files) = self.files_mut(tab_id) else {
            return Vec::new();
        };
        let Some(entry) = files
            .remote
            .selected
            .and_then(|index| files.remote.entries.get(index))
        else {
            return Vec::new();
        };
        if entry.kind != EntryKind::File {
            return Vec::new();
        }
        let name = entry.label.clone();
        if entry.size.is_some_and(|size| size > EDIT_SIZE_LIMIT) {
            files.remote.error = Some(FilesError::FileTooLarge);
            return Vec::new();
        }
        let remote = files.remote.path.join(&entry.name);
        let editor = match editor(&setting) {
            Ok(editor) => editor,
            Err(refused) => {
                files.remote.error = Some(match refused {
                    EditorRefused::Runs(_) => FilesError::EditorRunsFiles,
                    EditorRefused::NotFound(path) | EditorRefused::NotAProgram(path) => {
                        FilesError::EditorFailed { detail: path }
                    }
                });
                return Vec::new();
            }
        };
        if let Some(open) = files.edits.iter().find(|edit| edit.remote == remote) {
            let file = open.local.clone();
            self.tell(Notice::FilesEditing(name));
            return vec![Effect::LaunchEditor {
                tab: tab_id,
                editor,
                file,
            }];
        }
        let (Some(client), Some(base)) = (files.client.clone(), base) else {
            files.remote.error = Some(FilesError::WorkingFolderUnprotected);
            return Vec::new();
        };
        vec![Effect::StartEdit {
            tab: tab_id,
            client,
            remote,
            editor,
            base,
            cancel: CancellationToken::new(),
        }]
    }

    /// The file is open in the editor, or why not.
    fn edit_started(
        &mut self,
        tab_id: TabId,
        result: Result<Box<EditSession>, FilesError>,
    ) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab_id) else {
            return Vec::new();
        };
        match result {
            Ok(session) => {
                let name = session.name.clone();
                files.edits.push(*session);
                self.tell(Notice::FilesEditing(name));
            }
            Err(error) => files.remote.error = Some(error),
        }
        Vec::new()
    }

    /// The editor did not start again on a file being edited.
    fn editor_failed(&mut self, tab_id: TabId, error: FilesError) {
        if let Some(files) = self.files_mut(tab_id) {
            files.remote.error = Some(error);
        }
    }

    /// Time to look at the files being edited: in each connected tab with some, unless a
    /// look runs there already.
    fn edit_tick(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        for tab in &mut self.tabs {
            let Some(files) = tab.files.as_deref_mut() else {
                continue;
            };
            if files.edits.is_empty() || files.checking_edits {
                continue;
            }
            // Disconnected: the saves wait for the connection to come back.
            let Some(client) = files.client.clone() else {
                continue;
            };
            files.checking_edits = true;
            effects.push(Effect::CheckEdits {
                tab: tab.id,
                client,
                edits: files.edits.clone(),
            });
        }
        effects
    }

    /// A look ended: each file takes what it found; a save sent or refused is said.
    fn edits_checked(&mut self, tab_id: TabId, results: Vec<(PathBuf, EditCheck)>) -> Vec<Effect> {
        let mut said = None;
        let mut sent_any = false;
        let Some(files) = self.files_mut(tab_id) else {
            return Vec::new();
        };
        files.checking_edits = false;
        for (local, check) in results {
            let Some(session) = files.edits.iter_mut().find(|edit| edit.local == local) else {
                continue;
            };
            session.apply(&check);
            match check {
                EditCheck::Sent { .. } => {
                    sent_any = true;
                    said = Some(Notice::FilesAutoUploaded(session.name.clone()));
                }
                EditCheck::Refused { error, .. } => {
                    said = Some(Notice::FilesAutoUploadRefused {
                        name: session.name.clone(),
                        error: error.clone(),
                    });
                    files.remote.error = Some(error);
                }
                _ => {}
            }
        }
        if let Some(notice) = said {
            self.tell(notice);
        }
        if sent_any {
            self.list(tab_id, Side::Remote)
        } else {
            Vec::new()
        }
    }
}
