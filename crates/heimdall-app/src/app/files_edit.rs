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

use super::{App, Dialog, Effect, Notice};
use crate::external_edit::{EDIT_SIZE_LIMIT, EditCheck, EditSession, EditorRefused, editor};
use crate::files::{EntryKind, FilesError, Side};
use crate::ids::TabId;
use crate::sudo_edit::SudoPassword;

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
            FilesMessage::EditSendAnyway { tab, local } => self.edit_send_anyway(tab, &local),
            FilesMessage::EditSentAnyway { tab, local, check } => {
                self.edit_sent_anyway(tab, local, check)
            }
            FilesMessage::EditOpenFolder { tab, local } => local
                .parent()
                .map(|folder| {
                    vec![Effect::OpenFolder {
                        tab,
                        folder: folder.to_path_buf(),
                    }]
                })
                .unwrap_or_default(),
            FilesMessage::EditWithSudo { tab } => self.edit_with_sudo(tab),
            FilesMessage::EditSaveWithSudo { tab, local } => self.save_with_sudo(tab, &local),
            FilesMessage::SudoOpened {
                tab,
                remote,
                result,
            } => self.sudo_opened(tab, remote, result),
            FilesMessage::SudoSaved { tab, local, check } => self.sudo_saved(tab, local, check),
            FilesMessage::SudoPasswordGiven { tab, password } => {
                self.sudo_password_given(tab, password)
            }
            FilesMessage::EditStop { tab, local } => {
                if let Some(files) = self.files_mut(tab) {
                    files.edits.retain(|edit| edit.local != local);
                }
                Vec::new()
            }
            // What is left is about the integrated editor.
            message => self.editor_message(message),
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
        // The folders of every edit open in this run are kept by the sweep.
        let keep = self
            .tabs
            .iter()
            .filter_map(|tab| tab.files.as_deref())
            .flat_map(|files| &files.edits)
            .filter_map(|edit| edit.local.parent().map(std::path::Path::to_path_buf))
            .collect();
        vec![Effect::StartEdit {
            tab: tab_id,
            client,
            remote,
            editor,
            base,
            keep,
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

    /// "Send my version": a refused save sent over the server's file as it is now.
    fn edit_send_anyway(&mut self, tab_id: TabId, local: &std::path::Path) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab_id) else {
            return Vec::new();
        };
        let Some(client) = files.client.clone() else {
            return Vec::new();
        };
        let Some(edit) = files.edits.iter().find(|edit| edit.local == local) else {
            return Vec::new();
        };
        vec![Effect::SendEditAnyway {
            tab: tab_id,
            client,
            edit: Box::new(edit.clone()),
        }]
    }

    /// The save sent anyway, or why not: said as a look's would be.
    fn edit_sent_anyway(&mut self, tab_id: TabId, local: PathBuf, check: EditCheck) -> Vec<Effect> {
        let checking = self
            .tab(tab_id)
            .and_then(|tab| tab.files.as_deref())
            .is_some_and(|files| files.checking_edits);
        // A look running meanwhile ends on its own: its flag stays as it is.
        let effects = self.edits_checked(tab_id, vec![(local, check)]);
        if let Some(files) = self.files_mut(tab_id) {
            files.checking_edits = checking;
        }
        effects
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
                shell: files.shell.clone(),
                password: files.sudo_password.clone(),
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

    /// "Edit with sudo": the selected file of the server opened with sudo, with the password
    /// kept for the tab, if any; one already being edited is opened in the editor again.
    /// Only over SFTP, whose SSH connection runs sudo: an FTP tab has none, and never asks.
    fn edit_with_sudo(&mut self, tab_id: TabId) -> Vec<Effect> {
        let setting = self.settings.external_editor.clone();
        let base = self.edit_dir.clone();
        let keep = self.edit_folders();
        let Some(files) = self.files_mut(tab_id).filter(|files| files.shell.is_some()) else {
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
        let remote = files.remote.path.join(&entry.name);
        let editor = match editor(&setting) {
            Ok(editor) => editor,
            Err(refused) => {
                files.remote.error = Some(editor_error(refused));
                return Vec::new();
            }
        };
        if let Some(open) = files.edits.iter().find(|edit| edit.remote == remote) {
            return vec![Effect::LaunchEditor {
                tab: tab_id,
                editor,
                file: open.local.clone(),
            }];
        }
        let (Some(shell), Some(base)) = (files.shell.clone(), base) else {
            files.remote.error = Some(FilesError::WorkingFolderUnprotected);
            return Vec::new();
        };
        vec![Effect::SudoOpen {
            tab: tab_id,
            shell,
            remote,
            editor,
            folders: (base, keep),
            password: files.sudo_password.clone(),
        }]
    }

    /// The folders of every edit open in this run: kept by the sweep.
    fn edit_folders(&self) -> Vec<PathBuf> {
        self.tabs
            .iter()
            .filter_map(|tab| tab.files.as_deref())
            .flat_map(|files| &files.edits)
            .filter_map(|edit| edit.local.parent().map(std::path::Path::to_path_buf))
            .collect()
    }

    /// The file opened with sudo; a password asked when sudo wants one.
    fn sudo_opened(
        &mut self,
        tab_id: TabId,
        remote: heimdall_files::RemotePath,
        result: Result<Box<EditSession>, FilesError>,
    ) -> Vec<Effect> {
        let name = remote
            .file_name()
            .map(|name| crate::text::server_text(&heimdall_files::display_bytes(name)))
            .unwrap_or_default();
        match result {
            Ok(session) => self.edit_started(tab_id, Ok(session)),
            Err(error) => {
                self.sudo_refused(tab_id, name, SudoAction::Open(remote), error);
                Vec::new()
            }
        }
    }

    /// "Save with sudo": an edit's save sent with sudo, with the password kept for the tab.
    fn save_with_sudo(&mut self, tab_id: TabId, local: &std::path::Path) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab_id) else {
            return Vec::new();
        };
        let Some(shell) = files.shell.clone() else {
            return Vec::new();
        };
        let Some(edit) = files.edits.iter().find(|edit| edit.local == local) else {
            return Vec::new();
        };
        vec![Effect::SudoSave {
            tab: tab_id,
            shell,
            edit: Box::new(edit.clone()),
            password: files.sudo_password.clone(),
        }]
    }

    /// A save sent with sudo: from then on the edit is privileged; a password asked when
    /// sudo wants one.
    fn sudo_saved(&mut self, tab_id: TabId, local: PathBuf, check: EditCheck) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab_id) else {
            return Vec::new();
        };
        let Some(session) = files.edits.iter_mut().find(|edit| edit.local == local) else {
            return Vec::new();
        };
        let name = session.name.clone();
        match check {
            EditCheck::Sent { .. } => {
                session.privileged = true;
                session.apply(&check);
                self.tell(Notice::FilesSavedWithSudo(name));
                self.list(tab_id, Side::Remote)
            }
            EditCheck::Refused { ref error, .. } if is_password_error(error) => {
                let error = error.clone();
                self.sudo_refused(tab_id, name, SudoAction::Save(local), error);
                Vec::new()
            }
            check => {
                session.apply(&check);
                if let EditCheck::Refused { error, .. } | EditCheck::Failed(error) = check {
                    files.remote.error = Some(error);
                }
                Vec::new()
            }
        }
    }

    /// sudo did not do `action`: a password it wants is asked, one it refused forgotten
    /// and asked again; anything else said on the pane.
    pub(super) fn sudo_refused(
        &mut self,
        tab_id: TabId,
        name: String,
        action: SudoAction,
        error: FilesError,
    ) {
        let Some(files) = self.files_mut(tab_id) else {
            return;
        };
        if is_password_error(&error) {
            files.sudo_password = None;
            if error == FilesError::SudoPasswordRejected {
                files.remote.error = Some(error);
            }
            self.dialog = Some(Dialog::SudoPassword {
                tab: tab_id,
                name,
                action,
            });
        } else {
            files.remote.error = Some(error);
        }
    }

    /// The password typed for the question asked: kept for the tab, and what it was asked
    /// for done again with it.
    fn sudo_password_given(&mut self, tab_id: TabId, password: SudoPassword) -> Vec<Effect> {
        let Some(Dialog::SudoPassword { tab, action, .. }) = self.dialog.take() else {
            return Vec::new();
        };
        if tab != tab_id {
            return Vec::new();
        }
        if let Some(files) = self.files_mut(tab_id) {
            files.sudo_password = Some(password);
            files.remote.error = None;
        }
        match action {
            SudoAction::Save(local) => self.save_with_sudo(tab_id, &local),
            SudoAction::Open(remote) => self.reopen_with_sudo(tab_id, remote),
            SudoAction::List(path) => self.list_as_root(tab_id, path),
        }
    }

    /// Opens `remote` with sudo again, now a password is kept.
    fn reopen_with_sudo(
        &mut self,
        tab_id: TabId,
        remote: heimdall_files::RemotePath,
    ) -> Vec<Effect> {
        let setting = self.settings.external_editor.clone();
        let base = self.edit_dir.clone();
        let keep = self.edit_folders();
        let Some(files) = self.files_mut(tab_id) else {
            return Vec::new();
        };
        let (Ok(editor), Some(shell), Some(base)) = (editor(&setting), files.shell.clone(), base)
        else {
            return Vec::new();
        };
        vec![Effect::SudoOpen {
            tab: tab_id,
            shell,
            remote,
            editor,
            folders: (base, keep),
            password: files.sudo_password.clone(),
        }]
    }
}

/// What the password asked for sudo is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SudoAction {
    /// Opening the server's file.
    Open(heimdall_files::RemotePath),
    /// Saving the edit with this local copy.
    Save(PathBuf),
    /// Listing this folder of the server as root, the sudo mode being turned on.
    List(heimdall_files::RemotePath),
}

/// Whether sudo's refusal is about the password: one is wanted, or the one given refused.
pub(super) fn is_password_error(error: &FilesError) -> bool {
    matches!(
        error,
        FilesError::SudoPasswordNeeded | FilesError::SudoPasswordRejected
    )
}

/// What the pane says of an editor that is not run.
fn editor_error(refused: EditorRefused) -> FilesError {
    match refused {
        EditorRefused::Runs(_) => FilesError::EditorRunsFiles,
        EditorRefused::NotFound(path) | EditorRefused::NotAProgram(path) => {
            FilesError::EditorFailed { detail: path }
        }
    }
}
