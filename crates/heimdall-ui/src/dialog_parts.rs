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
//! title, the message in the secondary text, the buttons at the bottom right. A dialog that
//! asks for something to be typed or chosen follows the C# input, vault, PIN and import
//! dialogs: a semi-bold title, what it is about in the secondary text, its fields labelled
//! above them, why what was typed is refused in the error colour, the buttons at the bottom
//! right.

use heimdall_app::Message as AppMessage;
use iced::widget::{Button, Column, Row, Text, button, column, container, row, space, text};
use iced::{Alignment, Element, Length};

use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, radius, spacing};

/// Padding of a dialog's button, as the C# `MessageDialog` sets it: 6 above and below, 16
/// on the sides.
pub const BUTTON_PADDING: [f32; 2] = [6.0, 16.0];

/// Side of a message's icon, as the C# glyph at `FontSizeHeadline`.
const ICON_SIDE: f32 = font_size::HEADLINE;

/// Space inside a warning's card, above and below then on its sides, as the C# `Padding`.
const WARNING_CARD_PADDING: [f32; 2] = [8.0, 12.0];

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

/// A field's label, above it, as the C# `DialogLabelStyle` of the bulk edit dialogs: in the
/// secondary text.
pub fn dialog_label<'a>(label: String) -> Text<'a> {
    text(label).size(font_size::BODY).style(text::secondary)
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
    form(
        header(
            severity,
            text(title).size(font_size::SUBTITLE).font(styles::SEMIBOLD),
        ),
        body,
        buttons,
    )
}

/// The icon of `severity` beside `title`, as the C# `MessageDialog` and the dialogs that
/// warn, `PasteConfirmDialog` and `HostKeyPromptDialog`, head themselves.
pub fn header(severity: Severity, title: Text<'_>) -> Row<'_, Message> {
    let (icon, tint) = severity.icon();
    row![icons::icon(icon, tint, ICON_SIDE), title]
        .spacing(spacing::MD)
        .align_y(Alignment::Center)
}

/// A dialog: its `heading`, its `body` under it, then its `buttons`.
pub fn form<'a>(
    heading: impl Into<Element<'a, Message>>,
    body: impl Into<Element<'a, Message>>,
    buttons: Row<'a, Message>,
) -> Element<'a, Message> {
    column![heading.into(), body.into(), buttons]
        .spacing(spacing::MD)
        .into()
}

/// A dialog's title, as the C# input, vault and PIN dialogs': `FontSizeTitle`, semi-bold.
pub fn title<'a>(title: String) -> Text<'a> {
    text(title).size(font_size::TITLE).font(styles::SEMIBOLD)
}

/// The title of a dialog over a list to choose from, as the C# import previews, file
/// conflict and restore dialogs set theirs: `FontSizeBodyLarge`, semi-bold.
pub fn list_title<'a>(title: String) -> Text<'a> {
    text(title)
        .size(font_size::BODY_LARGE)
        .font(styles::SEMIBOLD)
}

/// [`title`] centred, as the C# vault unlock and PIN dialogs set theirs.
pub fn centred_title<'a>(title: String) -> Text<'a> {
    self::title(title).width(Length::Fill).center()
}

/// What a dialog asks to be typed, above its field, as the C# `InputDialog` and
/// `PasswordInputDialog` prompt: `FontSizeBodyLarge`, in the text's colour.
pub fn prompt<'a>(prompt: String) -> Text<'a> {
    text(prompt).size(font_size::BODY_LARGE)
}

/// What a dialog is about, under its title, as the C# summaries and explanations: in the
/// secondary text.
pub fn note<'a>(note: String) -> Text<'a> {
    text(note).size(font_size::BODY).style(text::secondary)
}

/// Why what was typed is refused, as the C# dialogs' `ErrorMessage`: in the error colour.
pub fn error<'a>(error: String) -> Text<'a> {
    text(error).size(font_size::BODY).style(text::danger)
}

/// What holds a dialog back, as the C# vault and PIN dialogs show a lockout or what
/// disabling the master password does: in the warning colour, on a card.
pub fn warning_card<'a>(warning: String) -> Element<'a, Message> {
    container(text(warning).size(font_size::BODY).style(text::warning))
        .padding(WARNING_CARD_PADDING)
        .width(Length::Fill)
        .style(|theme: &iced::Theme| container::Style {
            background: Some(theme.extended_palette().background.weak.color.into()),
            border: iced::Border {
                radius: radius::SM.into(),
                ..iced::Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// A field under its label, as the C# vault, PIN and password dialogs lay them out: the
/// label small, in the secondary text, above it.
pub fn field<'a>(label: String, input: impl Into<Element<'a, Message>>) -> Column<'a, Message> {
    column![hint(label), input.into()].spacing(spacing::XS)
}

/// The main button of a dialog, in the accent: `label`, sending `message`, or faded while
/// there is none.
pub fn confirm<'a>(label: String, message: Option<Message>) -> Button<'a, Message> {
    action(label, styles::primary).on_press_maybe(message)
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
