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

//! The dialog "Copy output, secrets masked", as the C# `MaskedCopyDialog.xaml`: what the
//! session said with its secrets masked, how many were masked by each rule, the option to
//! pseudonymize host names and addresses, and a Copy that is the only way the text reaches
//! the clipboard.
//!
//! The wording says what the masking is: best effort. It never says the text is safe to
//! share, and it asks for the text to be read first, on this screen and again when no rule
//! found anything.

use heimdall_app::{MaskedCopy, MaskedCopyMessage, MaskedView, Message as AppMessage};
use heimdall_core::redaction::{self, Redacted, rule_id};
use iced::widget::{checkbox, column, container, scrollable, text};
use iced::{Element, Length, Theme};

use crate::dialog_parts::{self, Severity};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::spacing;

/// Tallest the masked text grows before it scrolls, in logical pixels.
const TEXT_HEIGHT: f32 = 340.0;

/// The name of a rule in the user's language, as the C# `MaskedCopyRule*` strings; the id
/// itself for a rule this window has no words for.
#[must_use]
pub fn rule_label(rule: &str) -> String {
    match rule {
        rule_id::PRIVATE_KEY_BLOCK => fl!("ui-masked-copy-rule-private-key-block"),
        rule_id::AUTHORIZATION_HEADER => fl!("ui-masked-copy-rule-authorization-header"),
        rule_id::COOKIE_HEADER => fl!("ui-masked-copy-rule-cookie-header"),
        rule_id::URL_CREDENTIALS => fl!("ui-masked-copy-rule-url-credentials"),
        rule_id::CLI_PASSWORD => fl!("ui-masked-copy-rule-cli-password"),
        rule_id::KEY_VALUE_SECRET => fl!("ui-masked-copy-rule-key-value-secret"),
        rule_id::AWS_ACCESS_KEY => fl!("ui-masked-copy-rule-aws-access-key"),
        rule_id::GITHUB_TOKEN => fl!("ui-masked-copy-rule-github-token"),
        rule_id::SLACK_TOKEN => fl!("ui-masked-copy-rule-slack-token"),
        rule_id::JWT => fl!("ui-masked-copy-rule-jwt"),
        rule_id::PASSWORD_PROMPT => fl!("ui-masked-copy-rule-password-prompt"),
        rule_id::IPV4_ADDRESS => fl!("ui-masked-copy-rule-ipv4-address"),
        rule_id::HOST_NAME => fl!("ui-masked-copy-rule-host-name"),
        other => other.to_owned(),
    }
}

/// One line per rule that masked something, in the catalogue's order so the lines do not
/// shuffle between runs: its name and how many occurrences.
#[must_use]
pub fn count_lines(redacted: &Redacted) -> Vec<String> {
    redaction::rule_ids()
        .into_iter()
        .map(|rule| (rule, redacted.count(rule)))
        .filter(|(_, count)| *count > 0)
        .map(|(rule, count)| {
            fl!(
                "ui-masked-copy-count-line",
                rule = rule_label(rule),
                count = count
            )
        })
        .collect()
}

/// The dialog over `state`.
#[must_use]
pub fn dialog(state: &MaskedCopy) -> Element<'_, Message> {
    let mut content = column![
        text(fl!("ui-masked-copy-intro")),
        dialog_parts::warning_card(fl!("ui-masked-copy-best-effort")),
    ]
    .spacing(spacing::SM);
    let mut copy = dialog_parts::action(fl!("ui-masked-copy-copy"), styles::primary);
    match &state.view {
        MaskedView::Working => {
            content = content.push(dialog_parts::hint(fl!("ui-masked-copy-working")));
        }
        MaskedView::Failed => {
            content = content.push(dialog_parts::error(fl!("ui-masked-copy-failed")));
        }
        MaskedView::Refused { rule } => {
            content = content.push(dialog_parts::error(fl!(
                "ui-masked-copy-refused",
                rule = rule_label(rule)
            )));
        }
        MaskedView::Ready(redacted) => {
            content = content.push(masked_text(&redacted.text));
            if redacted.truncated {
                content = content.push(dialog_parts::hint(fl!("ui-masked-copy-truncated")));
            }
            let lines = count_lines(redacted);
            if lines.is_empty() {
                content = content.push(dialog_parts::hint(fl!("ui-masked-copy-nothing-masked")));
            } else {
                content = content.push(column(lines.into_iter().map(|line| text(line).into())));
            }
            copy = copy.on_press(Message::App(AppMessage::ConfirmDialog));
        }
    }
    content = content.push(
        checkbox(state.pseudonymize_hosts)
            .style(styles::checkbox)
            .label(fl!("ui-masked-copy-hosts-option"))
            .on_toggle(|on| {
                Message::App(AppMessage::MaskedCopy(MaskedCopyMessage::HostsChanged(on)))
            }),
    );
    dialog_parts::form(
        dialog_parts::header(
            Severity::Warning,
            dialog_parts::title(fl!("ui-masked-copy-title")),
        ),
        content,
        dialog_parts::buttons([dialog_parts::cancel(), copy]),
    )
}

/// The masked text in a box that scrolls both ways, its lines never wrapped, as the C#
/// text box.
fn masked_text(masked: &str) -> Element<'_, Message> {
    let shown = styles::scroll(
        text(masked)
            .font(iced::Font::MONOSPACE)
            .wrapping(text::Wrapping::None),
    )
    .direction(scrollable::Direction::Both {
        vertical: styles::scrollbar(),
        horizontal: styles::scrollbar(),
    })
    .width(Length::Fill)
    .height(Length::Shrink);
    container(shown)
        .max_height(TEXT_HEIGHT)
        .padding(spacing::MD)
        .style(|theme: &Theme| container::rounded_box(theme))
        .into()
}
