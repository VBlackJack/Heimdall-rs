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

use heimdall_app::{App, Message as AppMessage, SettingsMessage};
use iced::widget::{Column, button, checkbox, column, container, row, text};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

const SPACING: f32 = 8.0;
/// Room inside a card.
const PADDING: f32 = 12.0;
/// Widest a card grows, as the other Settings cards.
const CARD_WIDTH: f32 = 720.0;
/// Size of the application's name.
const NAME_SIZE: f32 = 22.0;
/// Size of a section's title.
const SECTION_SIZE: f32 = 15.0;
/// Size of a hint.
const SMALL_SIZE: f32 = 12.0;
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
    .spacing(SPACING)
    .into()
}

fn card(title: String, content: Column<'_, Message>) -> Element<'_, Message> {
    container(column![text(title).size(SECTION_SIZE), content.spacing(SPACING)].spacing(SPACING))
        .padding(PADDING)
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
        .style(button::secondary)
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
            text(fl!("ui-window-title")).size(NAME_SIZE),
            text(fl!("ui-about-version", version = VERSION)),
            text(fl!("ui-about-tagline")).style(text::secondary),
        ]
        .spacing(SPACING / 2.0),
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
                    open_button(fl!("ui-about-repository"), Some(PathBuf::from(REPOSITORY))),
                ]
                .spacing(SPACING)
                .wrap(),
            ],
        ),
        card(
            fl!("ui-about-section-diagnostics"),
            column![
                checkbox(settings.diagnostics_log)
                    .label(fl!("ui-about-diagnostics-log"))
                    .on_toggle(|on| {
                        Message::App(AppMessage::Settings(SettingsMessage::DiagnosticsLog(on)))
                    }),
                text(fl!("ui-about-diagnostics-log-hint")).size(SMALL_SIZE),
            ],
        ),
    ]
}
