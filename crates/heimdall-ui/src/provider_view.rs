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

//! The external credential provider's card on the Settings page, as the C# one: the command
//! and its quick setup presets, the unlock secret, the database, the key file and the Test
//! button.

use heimdall_app::credential_provider::{ProviderFailure, ProviderTest};
use heimdall_app::{App, Message as AppMessage, ProviderMessage};
use std::time::Duration;

use heimdall_core::credential_provider::{
    MAX_TIMEOUT, MIN_TIMEOUT, PRESETS, ProviderKind, TemplateProblem,
};
use iced::widget::{
    Column, button, checkbox, column, container, pick_list, radio, row, text, text_input,
};
use iced::{Alignment, Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

/// Space between elements.
const SPACING: f32 = 8.0;
/// Inner margin of a card.
const PADDING: f32 = 12.0;
/// Size of hints.
const SMALL_SIZE: f32 = 12.0;
/// Size of a card's title.
const TITLE_SIZE: f32 = 16.0;
/// Width of a label before its field.
const LABEL_WIDTH: f32 = 200.0;

fn provider(message: ProviderMessage) -> Message {
    Message::App(AppMessage::CredentialProvider(message))
}

/// A command's time limit in the list, in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seconds(u64);

impl std::fmt::Display for Seconds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&fl!(
            "ui-settings-provider-timeout-seconds",
            seconds = self.0
        ))
    }
}

/// How long a command is given, chosen by the second within the C# range, as the C#
/// "Command timeout" field, its hint under it.
fn timeout_choice<'a>(timeout: Duration) -> Element<'a, Message> {
    let choices: Vec<Seconds> = (MIN_TIMEOUT.as_secs()..=MAX_TIMEOUT.as_secs())
        .map(Seconds)
        .collect();
    column![
        row![
            text(fl!("ui-settings-provider-timeout")).width(LABEL_WIDTH),
            pick_list(
                choices,
                Some(Seconds(timeout.as_secs())),
                |Seconds(seconds)| {
                    provider(ProviderMessage::Timeout(Duration::from_secs(seconds)))
                }
            ),
        ]
        .spacing(SPACING)
        .align_y(iced::Alignment::Center),
        text(fl!("ui-settings-provider-timeout-hint")).size(SMALL_SIZE),
    ]
    .spacing(SPACING / 2.0)
    .into()
}

/// A quick setup preset, as the list shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Preset(usize);

impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The tools' own names, as the C# list shows them in every language.
        f.write_str(PRESETS.get(self.0).map_or("", |(name, _)| name))
    }
}

/// A label, its field, and a hint under them when there is one.
fn field<'a>(
    label: String,
    input: impl Into<Element<'a, Message>>,
    hint: Option<String>,
) -> Element<'a, Message> {
    let mut block = Column::new().spacing(SPACING / 2.0).push(
        row![text(label).width(LABEL_WIDTH), input.into()]
            .spacing(SPACING)
            .align_y(Alignment::Center),
    );
    if let Some(hint) = hint {
        block = block.push(text(hint).size(SMALL_SIZE));
    }
    block.into()
}

/// The card, `unlock` being the unlock secret typed and not saved yet.
pub fn card<'a>(app: &'a App, unlock: &'a str) -> Element<'a, Message> {
    let settings = &app.settings().credential_provider;
    let mut card = column![
        text(fl!("ui-settings-provider-title")).size(TITLE_SIZE),
        checkbox(settings.enabled)
            .label(fl!("ui-settings-provider-enabled"))
            .on_toggle(|on| provider(ProviderMessage::Enabled(on))),
    ]
    .spacing(SPACING);
    if !settings.enabled {
        card = card.push(text(fl!("ui-settings-provider-disabled-hint")).size(SMALL_SIZE));
        return boxed(card);
    }
    // Windows Credential Manager is offered on Windows, as the C# one; chosen elsewhere
    // (a settings file carried over), the choice still shows, to go back.
    let credential_manager = settings.kind == ProviderKind::WindowsCredentialManager;
    if cfg!(windows) || credential_manager {
        card = card.push(kind_choice(settings.kind));
    }
    if credential_manager {
        // As in C#: nothing to set, the entry's name comes from the profile.
        card = card.push(text(fl!("ui-settings-provider-credman-help")).size(SMALL_SIZE));
        return boxed(card);
    }
    let presets: Vec<Preset> = (0..PRESETS.len()).map(Preset).collect();
    card = card
        .push(field(
            fl!("ui-settings-provider-preset"),
            pick_list(presets, None::<Preset>, |preset| {
                provider(ProviderMessage::Preset(preset.0))
            })
            .placeholder(fl!("ui-settings-provider-preset-custom"))
            .width(Length::Fill),
            None,
        ))
        .push(field(
            fl!("ui-settings-provider-command"),
            text_input(
                &fl!("ui-settings-provider-command-placeholder"),
                &settings.command,
            )
            .on_input(|value| provider(ProviderMessage::Command(value))),
            Some(fl!("ui-settings-provider-placeholders")),
        ))
        .push(field(
            fl!("ui-settings-provider-username"),
            text_input(
                &fl!("ui-settings-provider-username-placeholder"),
                &settings.username_command,
            )
            .on_input(|value| provider(ProviderMessage::UsernameCommand(value))),
            Some(fl!("ui-settings-provider-username-help")),
        ))
        .push(unlock_field(app, unlock))
        .push(field(
            fl!("ui-settings-provider-database"),
            text_input("", &settings.database)
                .on_input(|value| provider(ProviderMessage::Database(value))),
            None,
        ))
        .push(field(
            fl!("ui-settings-provider-key-file"),
            text_input("", &settings.key_file)
                .on_input(|value| provider(ProviderMessage::KeyFile(value))),
            Some(fl!("ui-settings-provider-key-file-hint")),
        ));
    let running = app.provider_test() == Some(&ProviderTest::Running);
    let mut test = row![
        button(text(fl!("ui-settings-provider-test")))
            .style(button::secondary)
            .on_press_maybe((!running).then(|| provider(ProviderMessage::Test))),
    ]
    .spacing(SPACING)
    .align_y(Alignment::Center);
    if let Some(outcome) = app.provider_test() {
        let seconds = settings.timeout.as_secs();
        test = test.push(text(test_text(outcome, seconds)).size(SMALL_SIZE));
    }
    card = card
        .push(test)
        .push(
            checkbox(settings.first_line_only)
                .label(fl!("ui-settings-provider-first-line"))
                .on_toggle(|on| provider(ProviderMessage::FirstLineOnly(on))),
        )
        .push(text(fl!("ui-settings-provider-first-line-help")).size(SMALL_SIZE))
        .push(timeout_choice(settings.timeout))
        .push(text(fl!("ui-settings-provider-keepass2-hint")).size(SMALL_SIZE));
    boxed(card)
}

/// Where the password comes from, as the C# radio buttons choose it.
fn kind_choice<'a>(kind: ProviderKind) -> Element<'a, Message> {
    field(
        fl!("ui-settings-provider-type"),
        row![
            radio(
                fl!("ui-settings-provider-type-command"),
                ProviderKind::Command,
                Some(kind),
                |kind| provider(ProviderMessage::Kind(kind)),
            ),
            radio(
                fl!("ui-settings-provider-type-credman"),
                ProviderKind::WindowsCredentialManager,
                Some(kind),
                |kind| provider(ProviderMessage::Kind(kind)),
            ),
        ]
        .spacing(SPACING * 2.0),
        None,
    )
}

fn boxed(card: Column<'_, Message>) -> Element<'_, Message> {
    container(card)
        .width(Length::Fill)
        .padding(PADDING)
        .style(container::bordered_box)
        .into()
}

/// The unlock secret: typed and saved with a button, or said to be saved and forgotten with
/// another; it is never shown back.
fn unlock_field<'a>(app: &App, unlock: &'a str) -> Element<'a, Message> {
    let input: Element<'a, Message> = if app.provider_unlock_saved() {
        row![
            text(fl!("ui-settings-provider-unlock-saved")),
            iced::widget::space::horizontal(),
            button(text(fl!("ui-settings-provider-unlock-forget")))
                .style(button::secondary)
                .on_press(provider(ProviderMessage::ForgetUnlockSecret)),
        ]
        .spacing(SPACING)
        .align_y(Alignment::Center)
        .into()
    } else {
        let save = (!unlock.is_empty()).then_some(Message::SaveProviderUnlock);
        row![
            text_input(&fl!("ui-settings-provider-unlock-placeholder"), unlock)
                .secure(true)
                .on_input(Message::ProviderUnlock)
                .on_submit_maybe(save.clone()),
            button(text(fl!("ui-settings-provider-unlock-save"))).on_press_maybe(save),
        ]
        .spacing(SPACING)
        .align_y(Alignment::Center)
        .into()
    };
    field(
        fl!("ui-settings-provider-unlock"),
        input,
        Some(fl!("ui-settings-provider-unlock-help")),
    )
}

/// What the Test button found, in the C# words; the password is never shown.
fn test_text(outcome: &ProviderTest, seconds: u64) -> String {
    match outcome {
        ProviderTest::Running => fl!("ui-settings-provider-test-running"),
        ProviderTest::Success => fl!("ui-settings-provider-test-success"),
        ProviderTest::TimedOut => fl!("ui-settings-provider-test-timeout", seconds = seconds),
        ProviderTest::Failed(ProviderFailure::Template(TemplateProblem::Empty)) => {
            fl!("ui-settings-provider-test-no-command")
        }
        ProviderTest::Failed(ProviderFailure::Template(TemplateProblem::NoKeyFile)) => {
            fl!("ui-settings-provider-test-no-key-file")
        }
        ProviderTest::Failed(ProviderFailure::Template(TemplateProblem::UnclosedQuote)) => {
            fl!("ui-settings-provider-test-unclosed-quote")
        }
        ProviderTest::Failed(ProviderFailure::Launch(detail)) => {
            fl!("ui-settings-provider-test-error", detail = detail.as_str())
        }
        // A failure of the command is no result, as the test itself sorts it.
        ProviderTest::NoResult
        | ProviderTest::Failed(
            ProviderFailure::Exit(_) | ProviderFailure::Empty | ProviderFailure::TimedOut,
        ) => fl!("ui-settings-provider-test-no-result"),
    }
}
