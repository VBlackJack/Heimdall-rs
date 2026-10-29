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

use heimdall_app::credential_provider::{
    LaunchReason, ProviderFailure, ProviderTest, milliseconds,
};
use heimdall_app::{App, Message as AppMessage, ProviderMessage};
use std::time::Duration;

use heimdall_core::credential_provider::{
    MAX_TIMEOUT, MIN_TIMEOUT, PRESETS, ProviderKind, TemplateProblem, preset_index_of,
};
use iced::widget::{Column, button, checkbox, column, container, pick_list, radio, row, text};
use iced::{Alignment, Element, Length};

use crate::browse::{BrowseTarget, browse_button};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Width of a label before its field.
const LABEL_WIDTH: f32 = 200.0;

fn provider(message: ProviderMessage) -> Message {
    Message::App(AppMessage::CredentialProvider(message))
}

/// The preset the selector shows for `command`: the one whose command it is, or none (the
/// "Custom" placeholder) when it is blank or was edited by hand, as in C#.
fn selected_preset(command: &str) -> Option<Preset> {
    preset_index_of(command).map(Preset)
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
            )
            .style(styles::pick_list)
            .menu_style(styles::menu),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center),
        text(fl!("ui-settings-provider-timeout-hint")).size(font_size::CAPTION),
    ]
    .spacing(spacing::XS)
    .into()
}

/// A quick setup preset, as the list shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Preset(usize);

impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The tools' own names, as the C# list shows them in every language.
        f.write_str(PRESETS.get(self.0).map_or("", |preset| preset.name))
    }
}

/// A path's `input`, the C# "Browse..." of `target` beside it (`MainWindow.xaml:4106`,
/// `4124`).
fn browsed<'a>(
    input: impl Into<Element<'a, Message>>,
    target: BrowseTarget,
) -> Element<'a, Message> {
    row![input.into(), browse_button(target)]
        .spacing(spacing::SM)
        .align_y(Alignment::Center)
        .into()
}

/// A label, its field, and a hint under them when there is one.
fn field<'a>(
    label: String,
    input: impl Into<Element<'a, Message>>,
    hint: Option<String>,
) -> Element<'a, Message> {
    let mut block = Column::new().spacing(spacing::XS).push(
        row![text(label).width(LABEL_WIDTH), input.into()]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
    );
    if let Some(hint) = hint {
        block = block.push(text(hint).size(font_size::CAPTION));
    }
    block.into()
}

/// The card, `unlock` being the unlock secret typed and not saved yet.
pub fn card<'a>(app: &'a App, unlock: &'a str) -> Element<'a, Message> {
    let settings = &app.settings().credential_provider;
    let mut card = column![
        text(fl!("ui-settings-provider-title")).size(font_size::SUBTITLE),
        checkbox(settings.enabled)
            .style(styles::checkbox)
            .label(fl!("ui-settings-provider-enabled"))
            .on_toggle(|on| provider(ProviderMessage::Enabled(on))),
    ]
    .spacing(spacing::SM);
    if !settings.enabled {
        card = card.push(text(fl!("ui-settings-provider-disabled-hint")).size(font_size::CAPTION));
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
        card = card.push(text(fl!("ui-settings-provider-credman-help")).size(font_size::CAPTION));
        return boxed(card);
    }
    let presets: Vec<Preset> = (0..PRESETS.len()).map(Preset).collect();
    card = card
        .push(field(
            fl!("ui-settings-provider-preset"),
            pick_list(presets, selected_preset(&settings.command), |preset| {
                provider(ProviderMessage::Preset(preset.0))
            })
            .style(styles::pick_list)
            .menu_style(styles::menu)
            .placeholder(fl!("ui-settings-provider-preset-custom"))
            .width(Length::Fill),
            None,
        ))
        .push(field(
            fl!("ui-settings-provider-command"),
            styles::field(
                &fl!("ui-settings-provider-command-placeholder"),
                &settings.command,
            )
            .style(styles::text_input)
            .on_input(|value| provider(ProviderMessage::Command(value))),
            Some(fl!("ui-settings-provider-placeholders")),
        ))
        .push(field(
            fl!("ui-settings-provider-username"),
            styles::field(
                &fl!("ui-settings-provider-username-placeholder"),
                &settings.username_command,
            )
            .style(styles::text_input)
            .on_input(|value| provider(ProviderMessage::UsernameCommand(value))),
            Some(fl!("ui-settings-provider-username-help")),
        ))
        .push(unlock_field(app, unlock))
        .push(field(
            fl!("ui-settings-provider-database"),
            browsed(
                styles::field("", &settings.database)
                    .style(styles::text_input)
                    .on_input(|value| provider(ProviderMessage::Database(value))),
                BrowseTarget::ProviderDatabase,
            ),
            None,
        ))
        .push(field(
            fl!("ui-settings-provider-key-file"),
            browsed(
                styles::field("", &settings.key_file)
                    .style(styles::text_input)
                    .on_input(|value| provider(ProviderMessage::KeyFile(value))),
                BrowseTarget::ProviderKeyFile,
            ),
            Some(fl!("ui-settings-provider-key-file-hint")),
        ));
    card = card.push(entry_field(app)).push(test_row(app));
    card = card
        .push(
            checkbox(settings.first_line_only)
                .style(styles::checkbox)
                .label(fl!("ui-settings-provider-first-line"))
                .on_toggle(|on| provider(ProviderMessage::FirstLineOnly(on))),
        )
        .push(text(fl!("ui-settings-provider-first-line-help")).size(font_size::CAPTION))
        .push(timeout_choice(settings.timeout))
        .push(text(fl!("ui-settings-provider-keepass2-hint")).size(font_size::CAPTION));
    boxed(card)
}

/// The vault entry the Test button looks up, as the C# "Vault entry to look up" field: a test
/// with a made-up entry reported a failure for every working setup.
fn entry_field(app: &App) -> Element<'_, Message> {
    let running = app.provider_test() == Some(&ProviderTest::Running);
    field(
        fl!("ui-settings-provider-test-entry"),
        styles::field(
            &fl!("ui-settings-provider-test-entry-placeholder"),
            app.provider_test_entry(),
        )
        .style(styles::text_input)
        .on_input(|value| provider(ProviderMessage::TestEntry(value)))
        .on_submit_maybe((!running).then(|| provider(ProviderMessage::Test))),
        None,
    )
}

/// The Test button, and what the last test found beside it.
fn test_row<'a>(app: &App) -> Element<'a, Message> {
    let running = app.provider_test() == Some(&ProviderTest::Running);
    let mut test = row![
        button(text(fl!("ui-settings-provider-test")))
            .padding(crate::tokens::BUTTON_PADDING)
            .style(styles::secondary)
            .on_press_maybe((!running).then(|| provider(ProviderMessage::Test))),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);
    if let Some(outcome) = app.provider_test() {
        test = test.push(text(test_text(outcome)).size(font_size::CAPTION));
    }
    test.into()
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
            )
            .style(styles::radio),
            radio(
                fl!("ui-settings-provider-type-credman"),
                ProviderKind::WindowsCredentialManager,
                Some(kind),
                |kind| provider(ProviderMessage::Kind(kind)),
            )
            .style(styles::radio),
        ]
        .spacing(spacing::SM * 2.0),
        None,
    )
}

fn boxed(card: Column<'_, Message>) -> Element<'_, Message> {
    container(card)
        .width(Length::Fill)
        .padding(spacing::MD)
        .style(styles::panel)
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
                .padding(crate::tokens::BUTTON_PADDING)
                .style(styles::secondary)
                .on_press(provider(ProviderMessage::ForgetUnlockSecret)),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center)
        .into()
    } else {
        let save = (!unlock.is_empty()).then_some(Message::SaveProviderUnlock);
        row![
            styles::field(&fl!("ui-settings-provider-unlock-placeholder"), unlock)
                .style(styles::text_input)
                .secure(true)
                .on_input(Message::ProviderUnlock)
                .on_submit_maybe(save.clone()),
            button(text(fl!("ui-settings-provider-unlock-save")))
                .padding(crate::tokens::BUTTON_PADDING)
                .style(styles::primary)
                .on_press_maybe(save),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center)
        .into()
    };
    field(
        fl!("ui-settings-provider-unlock"),
        input,
        Some(fl!("ui-settings-provider-unlock-help")),
    )
}

/// What the Test button found, in the C# words; the password is never shown, only the user
/// name and the shape of the value.
pub fn test_text(outcome: &ProviderTest) -> String {
    match outcome {
        ProviderTest::Running => fl!("ui-settings-provider-test-running"),
        ProviderTest::NoEntry => fl!("ui-settings-provider-test-no-entry"),
        ProviderTest::Success {
            entry,
            username,
            shape,
        } => {
            let said = match username {
                Some(username) => fl!(
                    "ui-settings-provider-test-success-user",
                    entry = entry.as_str(),
                    username = username.as_str()
                ),
                None => fl!("ui-settings-provider-test-success", entry = entry.as_str()),
            };
            // A value over several lines, or with a blank at an edge, looked like a plain
            // success and failed only at the next login (C# `DescribeTestValueShape`).
            if shape.lines > 1 {
                let lines = u64::try_from(shape.lines).unwrap_or(u64::MAX);
                let warning = fl!("ui-settings-provider-test-shape-multiline", lines = lines);
                format!("{said} {warning}")
            } else if shape.edge_space {
                format!(
                    "{said} {}",
                    fl!("ui-settings-provider-test-shape-edge-space")
                )
            } else {
                said
            }
        }
        ProviderTest::NoResult { entry } => {
            fl!(
                "ui-settings-provider-test-no-result",
                entry = entry.as_str()
            )
        }
        ProviderTest::TimedOut { program, timeout } => fl!(
            "ui-settings-provider-test-timeout",
            program = program.as_str(),
            milliseconds = milliseconds(*timeout)
        ),
        ProviderTest::Failed(failure) => failure_text(failure),
    }
}

/// Why a program could not be started, in the user's language: its file name and the reason,
/// never the OS error text.
#[must_use]
pub fn launch_reason_text(program: &str, reason: LaunchReason) -> String {
    match reason {
        LaunchReason::NotFound => fl!("ui-provider-launch-not-found", program = program),
        LaunchReason::PermissionDenied => {
            fl!("ui-provider-launch-permission-denied", program = program)
        }
        LaunchReason::Other => fl!("ui-provider-launch-other", program = program),
    }
}

/// What a command that could not run is said to have done.
fn failure_text(failure: &ProviderFailure) -> String {
    match failure {
        ProviderFailure::Template(TemplateProblem::Empty) => {
            fl!("ui-settings-provider-test-no-command")
        }
        ProviderFailure::Template(TemplateProblem::NoKeyFile) => {
            fl!("ui-settings-provider-test-no-key-file")
        }
        ProviderFailure::Template(TemplateProblem::UnclosedQuote) => {
            fl!("ui-settings-provider-test-unclosed-quote")
        }
        ProviderFailure::Template(TemplateProblem::OptionLikeValue(placeholder)) => {
            let placeholder: &str = placeholder;
            fl!(
                "ui-settings-provider-test-option-like-value",
                placeholder = placeholder
            )
        }
        ProviderFailure::Template(TemplateProblem::UnquotableValue(placeholder)) => {
            let placeholder: &str = placeholder;
            fl!(
                "ui-settings-provider-test-unquotable-value",
                placeholder = placeholder
            )
        }
        ProviderFailure::OutputTooLong => fl!("ui-settings-provider-test-output-too-long"),
        ProviderFailure::Launch { program, reason } => fl!(
            "ui-settings-provider-test-error",
            detail = launch_reason_text(program, *reason)
        ),
        // The test sorts these into "no result" with the entry looked up; one that comes here
        // has no entry to name, which is the C# no-entry case.
        ProviderFailure::Exit(_) | ProviderFailure::Empty => {
            fl!("ui-settings-provider-test-no-entry")
        }
        ProviderFailure::TimedOut { program, timeout } => fl!(
            "ui-settings-provider-test-timeout",
            program = program.as_str(),
            milliseconds = milliseconds(*timeout)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_selector_shows_the_preset_picked_and_custom_once_the_command_is_edited() {
        // Blank: Custom.
        assert_eq!(selected_preset(""), None);
        for (index, preset) in PRESETS.iter().enumerate() {
            let shown = selected_preset(preset.command).expect("a preset");
            assert_eq!(shown, Preset(index));
            assert_eq!(shown.to_string(), preset.name);
            // Edited by hand: back to Custom.
            assert_eq!(selected_preset(&format!("{} --more", preset.command)), None);
        }
    }
}
