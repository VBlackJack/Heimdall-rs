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

//! The settings carried to another computer, as the C# "Export settings" and "Import
//! settings": the preferences written to a file of their own, nothing secret; read back
//! after showing what they change.

use heimdall_core::settings::{SettingsImport, TransferError};

use super::appearance::palette;
use super::{App, Dialog, Effect, Notice};

/// A step of carrying the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsTransferMessage {
    /// Write the settings to a file the user picks.
    Export,
    /// Read the settings of a file the user picks.
    Import,
    /// The file picked to import was read, or why not.
    Read(Result<String, String>),
    /// The settings were written, or why not.
    Written(Result<(), String>),
}

impl App {
    /// A step of carrying the settings.
    pub(super) fn settings_transfer(&mut self, message: SettingsTransferMessage) -> Vec<Effect> {
        match message {
            SettingsTransferMessage::Export => {
                let home = std::env::home_dir();
                let (document, held_back) = self.settings.export(home.as_deref(), false);
                // A path of this computer's user travels only when asked, as the C# asks.
                if held_back > 0 {
                    self.dialog = Some(Dialog::ConfirmSettingsExportPaths { count: held_back });
                    return Vec::new();
                }
                vec![Effect::SaveSettingsFile { document }]
            }
            SettingsTransferMessage::Import => vec![Effect::PickSettingsFile],
            SettingsTransferMessage::Read(Err(reason)) => {
                self.tell(Notice::SettingsImportFailed(reason));
                Vec::new()
            }
            SettingsTransferMessage::Read(Ok(text)) => {
                match self.settings.import(&text) {
                    Err(TransferError::NotSettings) => self.tell(Notice::SettingsImportInvalid),
                    Err(TransferError::Newer) => self.tell(Notice::SettingsImportNewer),
                    Ok(read) if read.changes.is_empty() => {
                        self.tell(Notice::SettingsImportNothing);
                    }
                    Ok(read) => self.dialog = Some(Dialog::ConfirmSettingsImport(Box::new(read))),
                }
                Vec::new()
            }
            SettingsTransferMessage::Written(result) => {
                self.tell(match result {
                    Ok(()) => Notice::SettingsExported,
                    Err(reason) => Notice::SettingsExportFailed(reason),
                });
                Vec::new()
            }
        }
    }

    /// The settings written, with the paths of this computer's user or without them, as
    /// the user answered.
    pub(super) fn export_settings(&self, with_home: bool) -> Vec<Effect> {
        let home = std::env::home_dir();
        let (document, _) = self.settings.export(home.as_deref(), with_home);
        vec![Effect::SaveSettingsFile { document }]
    }

    /// The question about the paths of this computer's user, answered no: the settings
    /// are written without them. `None` when it is not that question.
    pub(super) fn dismiss_settings_export(&mut self) -> Option<Vec<Effect>> {
        matches!(self.dialog, Some(Dialog::ConfirmSettingsExportPaths { .. })).then(|| {
            self.dialog = None;
            self.export_settings(false)
        })
    }

    /// The settings read, taken as the user agreed to: saved, then in use.
    pub(super) fn apply_imported_settings(&mut self, read: SettingsImport) -> Vec<Effect> {
        let count = read.changes.len();
        let before = std::mem::replace(&mut self.settings, read.settings);
        if let Err(error) = self.settings.save(&self.settings_file) {
            self.settings = before;
            self.dialog = Some(Dialog::save_failed(&error));
            return Vec::new();
        }
        let palette = palette(self.settings.color_scheme);
        for tab in &mut self.tabs {
            tab.terminal.set_palette(palette);
        }
        self.tell(Notice::SettingsImported(count));
        // Credential Guard required by the file: checked now, as when turned on by hand.
        if before.require_credential_guard {
            Vec::new()
        } else {
            self.warm_credential_guard()
        }
    }
}
