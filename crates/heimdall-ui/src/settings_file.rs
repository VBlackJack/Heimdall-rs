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

//! The settings file carried to another computer: where it is written and read, through
//! the system's dialogs held by the window, and the question showing what an import
//! changes, as the C# preview lists it.

use heimdall_app::{Message as AppMessage, SettingsTransferMessage};
use heimdall_core::settings::{
    SETTINGS_EXPORT_FILE_NAME, Sensitivity, SettingChange, SettingValue, SettingsImport,
};
use iced::widget::{Column, column, text};
use iced::{Element, Task, window};

use crate::dialog_parts::{self, Severity};
use crate::file_dialog::{FileDialog, Pick};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Extension of a settings file.
const EXTENSION: &str = "toml";

/// Extension of the C# Heimdall's "Export settings" file, which an import reads too.
const CSHARP_EXTENSION: &str = "json";

/// Most bytes a settings file read can have: far above any, far below a mistake.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Tallest the list of changes grows before it scrolls, in logical pixels.
const CHANGES_HEIGHT: f32 = 280.0;

/// Settings the file holds but does not import named before summing up the rest, as the
/// C# `SettingsImportPreview.MaxLines`.
const MAX_IGNORED_SHOWN: usize = 20;

/// Between the names of the settings not imported, as the C# preview's list separator.
const LIST_SEPARATOR: &str = ", ";

/// A dialog of settings files of `extensions` titled `title`, over `parent` when there is
/// one.
fn dialog(title: String, parent: Option<&dyn window::Window>, extensions: &[&str]) -> FileDialog {
    let mut dialog = FileDialog::new()
        .set_title(title)
        .add_filter(fl!("ui-settings-file-filter"), extensions);
    if let Some(parent) = parent {
        dialog = dialog.set_parent(parent);
    }
    dialog
}

/// `pick`, opened over the main window, `main`, when there is one.
fn over_window(
    main: Option<window::Id>,
    pick: fn(Option<&dyn window::Window>) -> Pick<std::path::PathBuf>,
) -> Task<Pick<std::path::PathBuf>> {
    crate::shell::main_window_task(main).then(move |id| match id {
        Some(id) => window::run(id, move |window| pick(Some(window))),
        None => Task::done(pick(None)),
    })
}

/// Asks where to save the settings file, over the main window `main`, then writes
/// `document` there; nothing when no file is picked.
pub fn save(document: String, main: Option<window::Id>) -> Task<Message> {
    over_window(main, |parent| {
        dialog(fl!("ui-dialog-settings-export-title"), parent, &[EXTENSION])
            .set_file_name(SETTINGS_EXPORT_FILE_NAME)
            .save_file()
    })
    .then(move |pick| {
        let document = document.clone();
        Task::future(pick).then(move |picked| match picked {
            Ok(Some(path)) => {
                let document = document.clone();
                Task::perform(
                    async move { tokio::fs::write(path, document).await },
                    |written| {
                        Message::App(AppMessage::SettingsTransfer(
                            SettingsTransferMessage::Written(
                                written.map_err(|error| error.to_string()),
                            ),
                        ))
                    },
                )
            }
            Ok(None) => Task::none(),
            Err(unavailable) => Task::done(Message::App(AppMessage::SettingsTransfer(
                SettingsTransferMessage::Written(Err(unavailable.message())),
            ))),
        })
    })
}

/// Asks which settings file to import, over the main window `main`, then reads it;
/// nothing when none is picked.
pub fn pick(main: Option<window::Id>) -> Task<Message> {
    over_window(main, |parent| {
        dialog(
            fl!("ui-dialog-settings-import-title"),
            parent,
            &[EXTENSION, CSHARP_EXTENSION],
        )
        .pick_file()
    })
    .then(|pick| {
        Task::future(pick).then(|picked| match picked {
            Ok(Some(path)) => Task::perform(read(path), |read| {
                Message::App(AppMessage::SettingsTransfer(SettingsTransferMessage::Read(
                    read,
                )))
            }),
            Ok(None) => Task::none(),
            Err(unavailable) => Task::done(Message::App(AppMessage::SettingsTransfer(
                SettingsTransferMessage::Read(Err(unavailable.message())),
            ))),
        })
    })
}

/// The text of the settings file at `path`; one too big is not read.
async fn read(path: std::path::PathBuf) -> Result<String, String> {
    let reason = |error: &dyn std::fmt::Display| format!("{}: {error}", path.display());
    let size = tokio::fs::metadata(&path)
        .await
        .map_err(|error| reason(&error))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(fl!("ui-import-file-too-large", size = size.to_string()));
    }
    tokio::fs::read_to_string(&path)
        .await
        .map_err(|error| reason(&error))
}

/// Between the tools of a list of external tools, as the C# `ListSeparator`.
const PROGRAMS_SEPARATOR: &str = ", ";

/// A value as the list of changes shows it.
fn shown(value: Option<&SettingValue>) -> String {
    match value {
        None => fl!("ui-settings-value-empty"),
        Some(SettingValue::Flag(true)) => fl!("ui-settings-value-on"),
        Some(SettingValue::Flag(false)) => fl!("ui-settings-value-off"),
        Some(SettingValue::Text(text)) if text.trim().is_empty() => {
            fl!("ui-settings-value-empty")
        }
        Some(SettingValue::Text(text)) => heimdall_app::server_text(text),
        Some(SettingValue::List(count)) => fl!("ui-settings-value-items", count = (*count)),
        // Tool by tool, as the C# `DescribeFullValue`: what would run, never a count.
        Some(SettingValue::Programs(programs)) => programs
            .iter()
            .map(|shown| {
                let (name, program) = (
                    heimdall_app::server_text(&shown.name),
                    heimdall_app::server_text(&shown.program),
                );
                if shown.administrator {
                    fl!(
                        "ui-settings-value-tool-admin",
                        name = name,
                        program = program
                    )
                } else {
                    fl!("ui-settings-value-tool", name = name, program = program)
                }
            })
            .collect::<Vec<_>>()
            .join(PROGRAMS_SEPARATOR),
    }
}

/// One change: the setting, its value now and the file's.
fn change_line(change: &SettingChange) -> String {
    fl!(
        "ui-dialog-settings-import-line",
        key = change.key.as_str(),
        before = shown(change.before.as_ref()),
        after = shown(change.after.as_ref())
    )
}

/// The changes `changes`, one line each.
fn change_lines<'a, 'b>(changes: impl Iterator<Item = &'b SettingChange>) -> Column<'a, Message> {
    changes.fold(Column::new().spacing(2.0), |lines, change| {
        lines.push(text(change_line(change)).size(font_size::CAPTION))
    })
}

/// The changes flagged `why`, under `heading`; `None` when there is none.
fn flagged_group(
    read: &SettingsImport,
    why: Sensitivity,
    heading: String,
) -> Option<Column<'_, Message>> {
    read.flagged(why).next()?;
    Some(
        column![
            text(heading)
                .size(font_size::BODY)
                .style(styles::warning_text),
            change_lines(read.flagged(why)),
        ]
        .spacing(spacing::XS),
    )
}

/// The changes shown first, on a warning card: those deciding what runs on this computer,
/// under the C# warning, then those weakening a security check, under their own heading;
/// `None` when there is none.
fn sensitive_part(read: &SettingsImport) -> Option<Element<'_, Message>> {
    read.sensitive().next()?;
    Some(dialog_parts::warning_panel(
        column![]
            .push(flagged_group(
                read,
                Sensitivity::RunsOnThisComputer,
                fl!("ui-dialog-settings-import-sensitive"),
            ))
            .push(flagged_group(
                read,
                Sensitivity::WeakensSecurity,
                fl!("ui-dialog-settings-import-weakens"),
            ))
            .spacing(spacing::SM),
    ))
}

/// The settings the file holds but does not import, named, the first
/// [`MAX_IGNORED_SHOWN`] of them; `None` when there is none.
fn ignored_part(read: &SettingsImport) -> Option<Element<'_, Message>> {
    if read.ignored.is_empty() {
        return None;
    }
    let mut names = read
        .ignored
        .iter()
        .take(MAX_IGNORED_SHOWN)
        .map(|name| heimdall_app::server_text(name))
        .collect::<Vec<_>>()
        .join(LIST_SEPARATOR);
    let rest = read.ignored.len().saturating_sub(MAX_IGNORED_SHOWN);
    if rest > 0 {
        names.push_str(LIST_SEPARATOR);
        names.push_str(&fl!("ui-dialog-settings-import-more", count = rest));
    }
    Some(
        column![
            dialog_parts::note(fl!(
                "ui-dialog-settings-import-ignored",
                count = read.ignored.len()
            )),
            text(names).size(font_size::CAPTION),
        ]
        .spacing(spacing::XS)
        .into(),
    )
}

/// What importing `read` changes, as the C# preview: the changes deciding what runs on
/// this computer or weakening a security check first, under their warnings, then the others, each setting with its value
/// now and the file's; what the file holds that is not imported; then Cancel and Import.
pub fn import_question(read: &SettingsImport) -> Element<'_, Message> {
    let body = column![]
        .push(sensitive_part(read))
        .push(dialog_parts::body(fl!(
            "ui-dialog-settings-import-body",
            count = read.changes.len()
        )))
        .push(
            read.benign()
                .next()
                .is_some()
                .then(|| styles::scroll(change_lines(read.benign())).height(CHANGES_HEIGHT)),
        )
        .push(ignored_part(read))
        .spacing(spacing::SM);
    dialog_parts::choice(
        Severity::Warning,
        fl!("ui-dialog-settings-import-title"),
        body,
        fl!("ui-dialog-cancel-button"),
        fl!("ui-dialog-settings-import-confirm"),
    )
}

/// Whether the `count` settings naming folders of this computer's user go into the file,
/// as the C# asks: left out, or included.
pub fn export_question<'a>(count: usize) -> Element<'a, Message> {
    dialog_parts::choice(
        Severity::Info,
        fl!("ui-dialog-settings-export-title"),
        dialog_parts::body(fl!("ui-dialog-settings-export-paths", count = count)),
        fl!("ui-dialog-settings-export-without"),
        fl!("ui-dialog-settings-export-with"),
    )
}
