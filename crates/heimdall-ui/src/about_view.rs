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

//! The About page of the window's navigation, laid out as the C# About page
//! (`MainWindow.xaml:4684-4848`): one centred column of cards as wide as each other, the
//! application's card first, its icon, name and version above its author and licence, then
//! the System, Data and Quick access cards. Heimdall-rs's own cards follow in the same
//! column: the settings file's export and import, and the diagnostics log switch. What it
//! says of the application comes from its package, never written twice.

use std::path::{Path, PathBuf};

use heimdall_app::update_check::{ReleaseTag, running_release};
use heimdall_app::{App, Message as AppMessage, SettingsMessage, SettingsTransferMessage};
use iced::widget::{Column, button, checkbox, column, container, row, space, text};
use iced::{Alignment, Element, Length};

use crate::dialog_parts;
use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, icon_size, spacing};

/// Width of the column of cards, as the C# page's `MaxWidth` (`MainWindow.xaml:4686`): the
/// column narrows with a narrower window, never widens.
pub const COLUMN_WIDTH: f32 = 560.0;

/// Width of a label before its value, as the C# grids' first column
/// (`MainWindow.xaml:4719`, `:4749`, `:4794`).
pub const LABEL_WIDTH: f32 = 120.0;

/// Space inside the application's card, as its C# `Padding` (`MainWindow.xaml:4690`).
const HEADER_PADDING: f32 = 28.0;

/// Space inside the other cards, as their C# `Padding="20"` (`MainWindow.xaml:4739`):
/// `SpacingLg`.
const CARD_PADDING: f32 = spacing::LG;

/// Space between two cards, as their C# top margin of 12 (`MainWindow.xaml:4739`):
/// `SpacingMd`.
const CARD_GAP: f32 = spacing::MD;

/// Space under the icon, as its C# bottom margin (`MainWindow.xaml:4702`).
const ICON_GAP: f32 = 10.0;

/// Space under the version, then under the line across the card, as their C# bottom
/// margins (`MainWindow.xaml:4696`, `:4714`).
const HEADER_GAP: f32 = 16.0;

/// Height of the line across the application's card (`MainWindow.xaml:4714`).
const RULE_HEIGHT: f32 = 1.0;

/// Space between the author's line and the licence's, as their C# bottom margin
/// (`MainWindow.xaml:4727`).
const HEADER_ROW_GAP: f32 = 6.0;

/// Space under a card's title, as its C# bottom margin (`MainWindow.xaml:4746`).
const TITLE_GAP: f32 = 10.0;

/// Space between a card's lines, as their C# bottom margin (`MainWindow.xaml:4760`).
const ROW_GAP: f32 = 5.0;

/// Padding of a card's button, above and below then on its sides, as the C# Quick access
/// buttons' `Padding="12,6"` (`MainWindow.xaml:4837`).
const BUTTON_PADDING: [f32; 2] = [6.0, 12.0];

/// Space between a card's buttons, across and down, as their C# margin of 8
/// (`MainWindow.xaml:4837`): `SpacingSm`.
const BUTTON_GAP: f32 = spacing::SM;

/// The application's version, author, licence and repository, from its package.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const LICENSE: &str = env!("CARGO_PKG_LICENSE");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// The id of the column of cards, as a test finds it.
#[must_use]
pub fn column_id() -> iced::widget::Id {
    iced::widget::Id::from("about-column")
}

/// The day this build was made, as the C# About page's "Build date"
/// (`MainWindow.xaml:4775-4778`, filled at `MainWindow.xaml.cs:614-617`): a release's, read
/// from its tag as the C# reads it from its version; else the day the build script found,
/// `SOURCE_DATE_EPOCH` or the commit's (see `build.rs`); else none, and the row is left
/// out where the C# writes its version instead.
#[must_use]
pub fn build_date() -> Option<String> {
    running_release()
        .and_then(ReleaseTag::date)
        // The variable `crate::build_date::BUILD_DATE_VARIABLE` names: `option_env!` takes a
        // literal only.
        .or_else(|| option_env!("HEIMDALL_BUILD_DATE").map(str::to_owned))
}

/// The components the System card names with the version this build holds, as the C#
/// names its `WebView2` and SSH.NET (`MainWindow.xaml:4767-4774`): the window's toolkit,
/// the SSH client and the RDP client, each its label and its version. The build script
/// reads the versions from `Cargo.lock` (see `crate::component_versions`); a component it
/// could not pin is left out.
#[must_use]
pub fn components() -> Vec<(String, &'static str)> {
    // The variables `crate::component_versions` names: `option_env!` takes a literal only.
    [
        (fl!("ui-about-iced"), option_env!("HEIMDALL_ICED_VERSION")),
        (fl!("ui-about-russh"), option_env!("HEIMDALL_RUSSH_VERSION")),
        (
            fl!("ui-about-ironrdp"),
            option_env!("HEIMDALL_IRONRDP_VERSION"),
        ),
    ]
    .into_iter()
    .filter_map(|(label, version)| version.map(|version| (label, version)))
    .collect()
}

/// A label and its value, on a line, as the C# grids: the label in the secondary text in
/// its column, the value beside it, a long path wrapped in the card.
fn line<'a>(label: String, value: String) -> Element<'a, Message> {
    row![
        text(label)
            .size(font_size::BODY)
            .width(Length::Fixed(LABEL_WIDTH))
            .style(text::secondary),
        text(value)
            .size(font_size::BODY)
            .width(Length::Fill)
            .wrapping(text::Wrapping::WordOrGlyph),
    ]
    .into()
}

/// A card of the column, as the C# section cards: its title semi-bold, `content` under it.
fn card<'a>(title: String, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(
        column![
            text(title)
                .size(font_size::BODY_LARGE)
                .font(styles::SEMIBOLD),
            content.into(),
        ]
        .spacing(TITLE_GAP),
    )
    .padding(CARD_PADDING)
    .width(Length::Fill)
    .style(styles::large_card)
    .into()
}

/// A card's lines.
fn lines<'a>(lines: impl IntoIterator<Item = Element<'a, Message>>) -> Column<'a, Message> {
    Column::with_children(lines).spacing(ROW_GAP)
}

/// A card's button, padded as the C# Quick access buttons.
fn card_button<'a>(label: String, action: Option<Message>) -> Element<'a, Message> {
    button(text(label).size(font_size::BODY))
        .padding(BUTTON_PADDING)
        .style(styles::secondary)
        .on_press_maybe(action)
        .into()
}

/// A card's buttons, wrapped onto a new line when the card is too narrow.
fn buttons<'a>(buttons: impl IntoIterator<Item = Element<'a, Message>>) -> Element<'a, Message> {
    iced::widget::Row::with_children(buttons)
        .spacing(BUTTON_GAP)
        .wrap()
        .vertical_spacing(BUTTON_GAP)
        .into()
}

/// A path as shown: the folder, or nothing known.
fn shown(path: Option<&Path>) -> String {
    path.map(|path| path.display().to_string())
        .unwrap_or_default()
}

/// The application's card, as the C# identity card (`MainWindow.xaml:4690-4736`): the
/// icon in the accent, the name, the version and what it is, a line, then the author and
/// the licence centred under them.
fn header<'a>() -> Element<'a, Message> {
    let identity = column![
        icons::icon(Icon::Monitor, Tint::Accent, icon_size::HERO),
        space().height(ICON_GAP),
        text(fl!("ui-window-title"))
            .size(font_size::HEADLINE)
            .font(crate::detail_view::BOLD),
        space().height(spacing::XS),
        text(fl!("ui-about-version", version = VERSION))
            .size(font_size::BODY_LARGE)
            .style(text::secondary),
        text(fl!("ui-about-tagline"))
            .size(font_size::BODY)
            .style(text::secondary),
    ]
    .align_x(Alignment::Center)
    .width(Length::Fill);
    let header_line = |label: String, value: String, bold: bool| {
        let value = text(value).size(font_size::BODY);
        row![
            text(label)
                .size(font_size::BODY)
                .width(Length::Fixed(LABEL_WIDTH))
                .style(text::secondary),
            if bold {
                value.font(styles::SEMIBOLD)
            } else {
                value
            },
        ]
    };
    container(column![
        identity,
        space().height(HEADER_GAP),
        container(space())
            .width(Length::Fill)
            .height(RULE_HEIGHT)
            .style(styles::rule),
        space().height(HEADER_GAP),
        container(
            column![
                header_line(fl!("ui-about-author"), AUTHORS.to_owned(), true),
                header_line(fl!("ui-about-license"), LICENSE.to_owned(), false),
            ]
            .spacing(HEADER_ROW_GAP)
        )
        .center_x(Length::Fill),
    ])
    .padding(HEADER_PADDING)
    .width(Length::Fill)
    .style(styles::large_card)
    .into()
}

/// The System card, as the C#'s (`MainWindow.xaml:4739-4781`): the platform, the versions
/// of the components this build holds, the build's day. The C#'s runtime has no
/// counterpart: a Rust program carries none.
fn system<'a>() -> Element<'a, Message> {
    let mut rows = vec![line(
        fl!("ui-about-platform"),
        format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    )];
    rows.extend(
        components()
            .into_iter()
            .map(|(label, version)| line(label, version.to_owned())),
    );
    if let Some(date) = build_date() {
        rows.push(line(fl!("ui-about-build-date"), date));
    }
    card(fl!("ui-about-section-system"), lines(rows))
}

/// The About page: the column of cards, centred, scrolled when the window is shorter.
pub fn view(app: &App) -> Element<'_, Message> {
    let config = app.profiles_file().parent().map(Path::to_path_buf);
    let logs = heimdall_core::paths::log_dir();
    let settings = app.settings();
    let data = card(
        fl!("ui-about-section-data"),
        lines([
            line(
                fl!("ui-about-sessions"),
                app.profile_summaries().len().to_string(),
            ),
            line(fl!("ui-about-gateways"), app.gateways().len().to_string()),
            line(fl!("ui-about-config-path"), shown(config.as_deref())),
            line(fl!("ui-about-log-path"), shown(logs.as_deref())),
        ]),
    );
    let quick_access = card(
        fl!("ui-about-section-links"),
        buttons([
            card_button(
                fl!("ui-about-open-config"),
                config.map(Message::OpenWithSystem),
            ),
            card_button(fl!("ui-about-open-logs"), logs.map(Message::OpenWithSystem)),
            card_button(
                fl!("ui-about-open-notes"),
                Some(Message::OpenWithSystem(app.notes_dir())),
            ),
            card_button(
                fl!("ui-about-repository"),
                Some(Message::OpenWithSystem(PathBuf::from(REPOSITORY))),
            ),
        ]),
    );
    let settings_file = card(
        fl!("ui-about-section-settings-file"),
        column![
            buttons([
                card_button(
                    fl!("ui-about-export-settings"),
                    Some(Message::App(AppMessage::SettingsTransfer(
                        SettingsTransferMessage::Export
                    ))),
                ),
                card_button(
                    fl!("ui-about-import-settings"),
                    Some(Message::App(AppMessage::SettingsTransfer(
                        SettingsTransferMessage::Import
                    ))),
                ),
            ]),
            dialog_parts::hint(fl!("ui-about-settings-file-hint")),
        ]
        .spacing(spacing::SM),
    );
    let diagnostics = card(
        fl!("ui-about-section-diagnostics"),
        column![
            checkbox(settings.diagnostics_log)
                .style(styles::checkbox)
                .label(fl!("ui-about-diagnostics-log"))
                .text_size(font_size::BODY)
                .width(Length::Fill)
                .on_toggle(|on| {
                    Message::App(AppMessage::Settings(SettingsMessage::DiagnosticsLog(on)))
                }),
            dialog_parts::hint(fl!("ui-about-diagnostics-log-hint")),
        ]
        .spacing(spacing::SM),
    );
    let cards = container(
        column![
            header(),
            system(),
            data,
            quick_access,
            settings_file,
            diagnostics
        ]
        .spacing(CARD_GAP),
    )
    .id(column_id())
    .width(Length::Fill)
    .max_width(COLUMN_WIDTH);
    styles::scroll(container(cards).padding(spacing::XL).center_x(Length::Fill))
        .height(Length::Fill)
        .into()
}
