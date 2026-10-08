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

//! The Settings page's About tab, as the C# About page: the version, the system, the data
//! and where it is kept, quick access to its folders, and the diagnostics log switch. What
//! it says of the application comes from its package, never written twice.

use std::path::{Path, PathBuf};

use heimdall_app::{App, Message as AppMessage, SettingsMessage, SettingsTransferMessage};
use iced::widget::{Column, button, checkbox, column, container, row, text};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Widest a card grows, as the other Settings cards.
const CARD_WIDTH: f32 = 720.0;
/// Width of a label before its value.
const LABEL_WIDTH: f32 = 140.0;

/// The application's version, author, licence and repository, from its package.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const LICENSE: &str = env!("CARGO_PKG_LICENSE");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// A label and its value, on a line.
fn line<'a>(label: String, value: String) -> Element<'a, Message> {
    row![
        text(label).width(Length::Fixed(LABEL_WIDTH)),
        text(value).style(text::secondary),
    ]
    .spacing(spacing::SM)
    .into()
}

fn card(title: String, content: Column<'_, Message>) -> Element<'_, Message> {
    container(
        column![
            text(title).size(font_size::SUBTITLE),
            content.spacing(spacing::SM)
        ]
        .spacing(spacing::SM),
    )
    .padding(spacing::MD)
    .max_width(CARD_WIDTH)
    .style(container::bordered_box)
    .into()
}

/// A path as shown: the folder, or nothing known.
fn shown(path: Option<&Path>) -> String {
    path.map(|path| path.display().to_string())
        .unwrap_or_default()
}

/// A button opening `target` with the system, offered only when it is known.
fn open_button<'a>(label: String, target: Option<PathBuf>) -> Element<'a, Message> {
    button(text(label))
        .style(styles::secondary)
        .on_press_maybe(target.map(Message::OpenWithSystem))
        .into()
}

/// The About tab.
pub fn view(app: &App) -> Column<'_, Message> {
    let config = app.profiles_file().parent().map(Path::to_path_buf);
    let logs = heimdall_core::paths::log_dir();
    let settings = app.settings();
    column![
        column![
            text(fl!("ui-window-title")).size(font_size::DISPLAY),
            text(fl!("ui-about-version", version = VERSION)),
            text(fl!("ui-about-tagline")).style(text::secondary),
        ]
        .spacing(spacing::XS),
        card(
            fl!("ui-about-section-system"),
            column![
                line(
                    fl!("ui-about-platform"),
                    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
                ),
                line(fl!("ui-about-author"), AUTHORS.to_owned()),
                line(fl!("ui-about-license"), LICENSE.to_owned()),
            ],
        ),
        card(
            fl!("ui-about-section-data"),
            column![
                line(
                    fl!("ui-about-sessions"),
                    app.profile_summaries().len().to_string()
                ),
                line(fl!("ui-about-gateways"), app.gateways().len().to_string()),
                line(fl!("ui-about-config-path"), shown(config.as_deref())),
                line(fl!("ui-about-log-path"), shown(logs.as_deref())),
            ],
        ),
        card(
            fl!("ui-about-section-links"),
            column![
                row![
                    open_button(fl!("ui-about-open-config"), config),
                    open_button(fl!("ui-about-open-logs"), logs),
                    open_button(fl!("ui-about-open-notes"), Some(app.notes_dir())),
                    open_button(fl!("ui-about-repository"), Some(PathBuf::from(REPOSITORY))),
                ]
                .spacing(spacing::SM)
                .wrap(),
            ],
        ),
        card(
            fl!("ui-about-section-settings-file"),
            column![
                row![
                    button(text(fl!("ui-about-export-settings")))
                        .style(styles::secondary)
                        .on_press(Message::App(AppMessage::SettingsTransfer(
                            SettingsTransferMessage::Export
                        ))),
                    button(text(fl!("ui-about-import-settings")))
                        .style(styles::secondary)
                        .on_press(Message::App(AppMessage::SettingsTransfer(
                            SettingsTransferMessage::Import
                        ))),
                ]
                .spacing(spacing::SM)
                .wrap(),
                text(fl!("ui-about-settings-file-hint")).size(font_size::CAPTION),
            ]
            .spacing(spacing::SM),
        ),
        card(
            fl!("ui-about-section-diagnostics"),
            column![
                checkbox(settings.diagnostics_log)
                    .style(styles::checkbox)
                    .label(fl!("ui-about-diagnostics-log"))
                    .on_toggle(|on| {
                        Message::App(AppMessage::Settings(SettingsMessage::DiagnosticsLog(on)))
                    }),
                text(fl!("ui-about-diagnostics-log-hint")).size(font_size::CAPTION),
            ],
        ),
    ]
}
