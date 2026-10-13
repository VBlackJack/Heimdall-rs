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

//! The "Browse..." buttons beside the paths of the Settings page and beside the gateway
//! editor's key file, as the C# ones (`MainWindow.xaml:2902-4410`, their handlers
//! `MainWindow.xaml.cs:2742-2767` and `2994-3082`; `GatewayDialog.xaml:81-89`,
//! `GatewayDialog.xaml.cs:234-248`): the system's file dialog, or its folder dialog for the
//! transcripts' folder, held by the main window and opened where the path already set is.
//! The path picked fills the field and is applied, as typing it and pressing Enter would.
//!
//! The system's dialogs filter by extension only: the C# `PuTTY` filter, which names
//! `putty.exe` and `puttycac.exe`, is offered as the executables' filter.

use std::path::{Path, PathBuf};

use heimdall_core::paths;
use iced::widget::{button, text};
use iced::{Element, Task, window};

use crate::file_dialog::{self, FileDialog, Pick};
use crate::i18n::fl;
use crate::settings_rows::ToolPath;
use crate::shell::Message;
use crate::styles;

/// The extensions of the C# executables' filter, offered where programs have one.
const EXECUTABLE_EXTENSIONS: &[&str] = &["exe"];

/// Every file, as the C# "All files (*.*)".
const ALL_FILES: &[&str] = &["*"];

/// The extensions of the C# credential provider's database filter.
const DATABASE_EXTENSIONS: &[&str] = &["kdbx", "db", "gpg"];

/// The extensions of the C# credential provider's key file filter.
const KEY_FILE_EXTENSIONS: &[&str] = &["keyx", "key"];

/// The extensions of the C# gateway key filters, `.ppk` then `.pem`.
const PPK_EXTENSIONS: &[&str] = &["ppk"];
const PEM_EXTENSIONS: &[&str] = &["pem"];

/// A path a "Browse..." button picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowseTarget {
    /// A program's path of the SSH tab: `PuTTY` or the X server.
    Tool(ToolPath),
    /// The external editor.
    ExternalEditor,
    /// The folder transcripts go to.
    SessionLogDirectory,
    /// The credential provider's password database.
    ProviderDatabase,
    /// The credential provider's key file.
    ProviderKeyFile,
    /// The SSH key of the gateway editor.
    GatewayKey,
}

impl BrowseTarget {
    /// Whether it is a folder, picked in the folder dialog, as the C# transcripts' folder.
    #[must_use]
    pub fn picks_folder(self) -> bool {
        self == Self::SessionLogDirectory
    }

    /// The dialog's title, the C#'s; `None` where the C# leaves the system's own.
    #[must_use]
    pub fn title(self) -> Option<String> {
        match self {
            Self::Tool(ToolPath::Putty) => Some(fl!("ui-settings-browse-putty-title")),
            Self::Tool(ToolPath::X11Server) => Some(fl!("ui-settings-browse-x11-title")),
            Self::ExternalEditor => Some(fl!("ui-settings-browse-editor-title")),
            Self::SessionLogDirectory => Some(fl!("ui-settings-browse-log-directory-title")),
            Self::GatewayKey => Some(fl!("ui-profile-browse-key-title")),
            Self::ProviderDatabase | Self::ProviderKeyFile => None,
        }
    }

    /// The dialog's filters, named, in the C#'s order. A program has an extension only on
    /// Windows: elsewhere every file is offered.
    #[must_use]
    pub fn filters(self) -> Vec<(String, &'static [&'static str])> {
        let all = || (fl!("ui-settings-browse-all-files"), ALL_FILES);
        match self {
            Self::Tool(_) | Self::ExternalEditor if cfg!(windows) => vec![
                (fl!("ui-settings-browse-executables"), EXECUTABLE_EXTENSIONS),
                all(),
            ],
            Self::Tool(_) | Self::ExternalEditor | Self::SessionLogDirectory => Vec::new(),
            Self::ProviderDatabase => vec![
                (
                    fl!("ui-settings-browse-database-filter"),
                    DATABASE_EXTENSIONS,
                ),
                all(),
            ],
            Self::ProviderKeyFile => vec![
                (
                    fl!("ui-settings-browse-key-file-filter"),
                    KEY_FILE_EXTENSIONS,
                ),
                all(),
            ],
            Self::GatewayKey => vec![
                (fl!("ui-profile-browse-key-ppk"), PPK_EXTENSIONS),
                (fl!("ui-profile-browse-key-pem"), PEM_EXTENSIONS),
                (fl!("ui-profile-browse-key-all"), ALL_FILES),
            ],
        }
    }

    /// Where the dialog opens, from `current`, the path set now: the folder itself for a
    /// folder, the folder of the file for a file, as the C# handlers start, when it exists;
    /// for a gateway key with none, `~/.ssh` when it exists, as the profile form's key.
    #[must_use]
    pub fn start(self, current: &Path) -> Option<PathBuf> {
        let folder = if self.picks_folder() {
            Some(current)
        } else {
            current.parent()
        };
        folder
            .filter(|folder| !folder.as_os_str().is_empty() && folder.is_dir())
            .map(Path::to_path_buf)
            .or_else(|| {
                (self == Self::GatewayKey)
                    .then(paths::home_dir)
                    .flatten()
                    .map(|home| home.join(paths::OPENSSH_FOLDER))
                    .filter(|folder| folder.is_dir())
            })
    }

    /// The dialog, opening at `start`, over `parent` when there is one.
    fn dialog(self, start: Option<&Path>, parent: Option<&dyn window::Window>) -> FileDialog {
        let mut dialog = FileDialog::new();
        if let Some(title) = self.title() {
            dialog = dialog.set_title(title);
        }
        for (name, extensions) in self.filters() {
            dialog = dialog.add_filter(name, extensions);
        }
        if let Some(start) = start {
            dialog = dialog.set_directory(start);
        }
        if let Some(parent) = parent {
            dialog = dialog.set_parent(parent);
        }
        dialog
    }

    /// The dialog, opened.
    fn open(self, start: Option<&Path>, parent: Option<&dyn window::Window>) -> Pick<PathBuf> {
        let dialog = self.dialog(start, parent);
        if self.picks_folder() {
            dialog.pick_folder()
        } else {
            dialog.pick_file()
        }
    }
}

/// The C# "Browse..." button of `target`.
pub fn browse_button<'a>(target: BrowseTarget) -> Element<'a, Message> {
    button(text(fl!("ui-profile-browse-button")))
        .style(styles::secondary)
        .on_press(Message::Browse(target))
        .into()
}

/// Opens the dialog of `target` over the main window, `main`, at `start`: the path picked
/// comes back as [`Message::Browsed`]; nothing when none is.
pub fn pick(
    target: BrowseTarget,
    start: Option<PathBuf>,
    main: Option<window::Id>,
) -> Task<Message> {
    crate::shell::main_window_task(main).then(move |id| {
        let start = start.clone();
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                target.open(start.as_deref(), Some(window))
            }),
            None => Task::done(target.open(start.as_deref(), None)),
        };
        pick.then(move |pick| {
            Task::future(pick).then(move |picked| match picked {
                Ok(Some(path)) => Task::done(Message::Browsed(target, path.display().to_string())),
                Ok(None) => Task::none(),
                Err(unavailable) => Task::done(file_dialog::failed(&unavailable)),
            })
        })
    })
}
