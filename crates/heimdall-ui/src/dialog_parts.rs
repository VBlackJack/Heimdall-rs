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

//! The parts the window's dialogs are built of, as the C# draws them. A form's section
//! heading, label and hint follow `Themes/DialogCommonStyles.xaml`; a question or a report
//! follows `Views/Dialogs/MessageDialog.xaml`: an icon of its severity beside a semi-bold
//! title, the message in the secondary text, the buttons at the bottom right.

use heimdall_app::Message as AppMessage;
use iced::widget::{Button, Row, Text, button, column, row, space, text};
use iced::{Alignment, Element};

use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Padding of a dialog's button, as the C# `MessageDialog` sets it: 6 above and below, 16
/// on the sides.
pub const BUTTON_PADDING: [f32; 2] = [6.0, 16.0];

/// Side of a message's icon, as the C# glyph at `FontSizeHeadline`.
const ICON_SIDE: f32 = font_size::HEADLINE;

/// Gap between a section's title and its description, as the C# title's bottom margin and
/// the description's top one.
const SECTION_TITLE_GAP: f32 = 6.0;

/// How serious a message is, as the C# `MessageDialog` severities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Something to know: the `Info` glyph, in the info colour.
    Info,
    /// Something to weigh before going on: the `Warning` glyph, in the warning colour.
    Warning,
    /// Something that destroys: the C# draws it as a warning; its action is red here.
    Danger,
    /// Something that failed: the `ErrorBadge` glyph, in the error colour.
    Error,
}

impl Severity {
    /// The glyph and colour of the severity, as the C# `ApplySeverityStyle` picks them.
    const fn icon(self) -> (Icon, Tint) {
        match self {
            Self::Info => (Icon::Info, Tint::Info),
            Self::Warning | Self::Danger => (Icon::Warning, Tint::Warning),
            Self::Error => (Icon::ErrorBadge, Tint::Danger),
        }
    }
}

/// A section's heading, as the C# `DialogSectionTitleStyle`: its title semi-bold, then its
/// description, when it has one, in the secondary text.
pub fn section<'a>(title: String, description: Option<String>) -> Element<'a, Message> {
    let mut heading = column![text(title).size(font_size::SUBTITLE).font(styles::SEMIBOLD)]
        .spacing(SECTION_TITLE_GAP);
    if let Some(description) = description {
        heading = heading.push(
            text(description)
                .size(font_size::BODY)
                .style(text::secondary),
        );
    }
    heading.into()
}

/// A field's label, above it, as the C# server dialog's `LabelStyle`: semi-bold, in the
/// secondary text.
pub fn label<'a>(label: String) -> Text<'a> {
    text(label)
        .size(font_size::BODY)
        .font(styles::SEMIBOLD)
        .style(text::secondary)
}

/// A hint under a field, as the C# `DialogHintTextStyle`: small, in the secondary text.
pub fn hint<'a>(hint: String) -> Text<'a> {
    text(hint).size(font_size::CAPTION).style(text::secondary)
}

/// A dialog's button: `label`, padded as the C#'s, drawn in `style`.
pub fn action<'a>(
    label: String,
    style: fn(&iced::Theme, button::Status) -> button::Style,
) -> Button<'a, Message> {
    button(text(label).size(font_size::BODY))
        .padding(BUTTON_PADDING)
        .style(style)
}

/// The Cancel of a dialog, which closes it and changes nothing.
pub fn cancel<'a>() -> Button<'a, Message> {
    action(fl!("ui-dialog-cancel-button"), styles::secondary)
        .on_press(Message::App(AppMessage::DismissDialog))
}

/// A dialog's buttons, at its bottom right, in the order given: the C# puts the action that
/// declines first and the main one last.
pub fn buttons<'a>(buttons: impl IntoIterator<Item = Button<'a, Message>>) -> Row<'a, Message> {
    buttons
        .into_iter()
        .fold(row![space::horizontal()], Row::push)
        .spacing(spacing::SM)
        .align_y(Alignment::Center)
}

/// A message, as the C# `MessageDialog`: the icon of `severity` beside `title`, `body`
/// under them, then `buttons`.
pub fn message<'a>(
    severity: Severity,
    title: String,
    body: impl Into<Element<'a, Message>>,
    buttons: Row<'a, Message>,
) -> Element<'a, Message> {
    let (icon, tint) = severity.icon();
    column![
        row![
            icons::icon(icon, tint, ICON_SIDE),
            text(title).size(font_size::SUBTITLE).font(styles::SEMIBOLD),
        ]
        .spacing(spacing::MD)
        .align_y(Alignment::Center),
        body.into(),
        buttons,
    ]
    .spacing(spacing::MD)
    .into()
}

/// A message's text, as the C# `MessageText`: a little larger than the window's, in the
/// secondary text.
pub fn body<'a>(body: String) -> Text<'a> {
    text(body)
        .size(font_size::BODY_LARGE)
        .style(text::secondary)
}

/// A question, as the C# `ShowConfirm`: `title`, `body`, then Cancel and `confirm`.
pub fn question<'a>(
    severity: Severity,
    title: String,
    body: String,
    confirm: String,
) -> Element<'a, Message> {
    choice(
        severity,
        title,
        self::body(body),
        fl!("ui-dialog-cancel-button"),
        confirm,
    )
}

/// A question with its own answers: `decline`, which closes it, then `confirm`, which sends
/// `ConfirmDialog`, drawn red when `severity` is [`Severity::Danger`], in the accent
/// otherwise.
pub fn choice<'a>(
    severity: Severity,
    title: String,
    body: impl Into<Element<'a, Message>>,
    decline: String,
    confirm: String,
) -> Element<'a, Message> {
    let style = if severity == Severity::Danger {
        styles::danger
    } else {
        styles::primary
    };
    message(
        severity,
        title,
        body,
        buttons([
            action(decline, styles::secondary).on_press(Message::App(AppMessage::DismissDialog)),
            action(confirm, style).on_press(Message::App(AppMessage::ConfirmDialog)),
        ]),
    )
}

/// The OK that closes a dialog which only informs.
pub fn ok<'a>() -> Button<'a, Message> {
    action(fl!("ui-dialog-ok-button"), styles::primary)
        .on_press(Message::App(AppMessage::DismissDialog))
}
