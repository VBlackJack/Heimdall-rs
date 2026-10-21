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

//! "Share folder" in the window: the start dialog with its cleartext warning and the choice
//! of the address to listen on, the dialog shown once it runs, and the words the status line
//! and the served pages use.

use std::time::Duration;

use heimdall_app::share::{
    BindChoice, ListingLabels, ShareFailure, ShareField, ShareForm, ShareInfo, ShareNotice,
    ShareProblem,
};
use heimdall_app::{Message as AppMessage, ShareMessage, server_text};
use heimdall_core::settings::{
    FILE_SHARE_MINUTES_MAX, FILE_SHARE_MINUTES_MIN, FILE_SHARE_PORT_MAX,
};
use iced::widget::{column, pick_list, row, text};
use iced::{Alignment, Element, Font, Length};

use crate::dialog_parts;
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::spacing;

/// Seconds in a minute and in an hour, for the time left.
const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 3600;

/// How often the bar draws the time left.
pub const TICK: Duration = Duration::from_secs(1);

/// The words of a served listing, in the language shown.
#[must_use]
pub fn listing_labels() -> ListingLabels {
    ListingLabels {
        index_of: fl!("ui-share-listing-index-of"),
        name: fl!("ui-share-listing-name"),
        size: fl!("ui-share-listing-size"),
    }
}

/// The time left as the bar shows it: minutes and seconds, with hours from one.
#[must_use]
pub fn remaining_text(remaining: Duration) -> String {
    let seconds = remaining.as_secs();
    if seconds >= SECONDS_PER_HOUR {
        format!(
            "{}:{:02}:{:02}",
            seconds / SECONDS_PER_HOUR,
            seconds % SECONDS_PER_HOUR / SECONDS_PER_MINUTE,
            seconds % SECONDS_PER_MINUTE
        )
    } else {
        format!(
            "{:02}:{:02}",
            seconds / SECONDS_PER_MINUTE,
            seconds % SECONDS_PER_MINUTE
        )
    }
}

/// The sentence for `problem`.
fn problem_text(problem: ShareProblem) -> String {
    match problem {
        ShareProblem::NoFolder => fl!("ui-share-problem-folder"),
        ShareProblem::Port => fl!("ui-share-problem-port", max = FILE_SHARE_PORT_MAX),
        ShareProblem::Minutes => fl!(
            "ui-share-problem-minutes",
            min = FILE_SHARE_MINUTES_MIN,
            max = FILE_SHARE_MINUTES_MAX
        ),
    }
}

/// What the status line says of a share.
#[must_use]
pub fn notice_text(notice: &ShareNotice) -> String {
    match notice {
        ShareNotice::Started(address) => {
            fl!("ui-share-notice-started", address = address.as_str())
        }
        ShareNotice::Stopped => fl!("ui-share-notice-stopped"),
        ShareNotice::Expired => fl!("ui-share-notice-expired"),
        ShareNotice::UrlCopied => fl!("ui-share-notice-url-copied"),
        ShareNotice::CommandCopied => fl!("ui-share-notice-command-copied"),
        ShareNotice::Failed(failure) => match failure {
            ShareFailure::Folder => fl!("ui-share-notice-failed-folder"),
            ShareFailure::NotAFolder => fl!("ui-share-notice-failed-not-folder"),
            ShareFailure::EveryInterface => fl!("ui-share-notice-failed-every-interface"),
            ShareFailure::Bind => fl!("ui-share-notice-failed-bind"),
            ShareFailure::Random => fl!("ui-share-notice-failed-random"),
        },
    }
}

/// An address in the list: the words it is shown with, and what it chooses.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BindOption {
    choice: BindChoice,
    label: String,
}

impl std::fmt::Display for BindOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

/// The addresses to choose among: this computer alone first, then the interfaces.
fn bind_options(form: &ShareForm) -> Vec<BindOption> {
    let mut options = vec![BindOption {
        choice: BindChoice::Loopback,
        label: fl!("ui-share-bind-loopback"),
    }];
    options.extend(form.addresses.iter().map(|found| BindOption {
        choice: BindChoice::Interface(found.address),
        label: fl!(
            "ui-share-bind-interface",
            name = server_text(&found.name),
            address = found.address.to_string()
        ),
    }));
    options
}

fn share(message: ShareMessage) -> Message {
    Message::App(AppMessage::Share(message))
}

fn field(label: String, value: &str, which: ShareField) -> Element<'_, Message> {
    column![
        dialog_parts::dialog_label(label),
        styles::field("", value)
            .style(styles::text_input)
            .on_input(move |value| share(ShareMessage::Field {
                field: which,
                value
            }))
            .on_submit(Message::App(AppMessage::ConfirmDialog)),
    ]
    .spacing(spacing::XS)
    .into()
}

/// The start dialog, filled as `form`: the cleartext warning first, then the folder, the
/// address, the port and the time; "Start sharing" is offered once nothing is missing.
#[must_use]
pub fn dialog(form: &ShareForm) -> Element<'_, Message> {
    let folder = form.folder.as_ref().map_or_else(
        || fl!("ui-share-folder-none"),
        |folder| server_text(&folder.display().to_string()),
    );
    let options = bind_options(form);
    let selected = options
        .iter()
        .find(|option| option.choice == form.bind)
        .cloned();
    let problem = form.problem();
    let mut content = column![
        dialog_parts::note(fl!("ui-share-description")),
        text(fl!("ui-share-warning")).style(text::warning),
        column![
            dialog_parts::dialog_label(fl!("ui-share-folder-label")),
            row![
                text(folder).width(Length::Fill),
                dialog_parts::action(fl!("ui-share-folder-choose-button"), styles::secondary)
                    .on_press(share(ShareMessage::ChooseFolder)),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
        ]
        .spacing(spacing::XS),
        column![
            dialog_parts::dialog_label(fl!("ui-share-bind-label")),
            pick_list(options, selected, |chosen: BindOption| {
                share(ShareMessage::Bind(chosen.choice))
            })
            .style(styles::pick_list)
            .menu_style(styles::menu),
        ]
        .spacing(spacing::XS),
    ]
    .spacing(spacing::SM);
    if form.listing {
        content = content.push(dialog_parts::note(fl!("ui-share-bind-listing")));
    }
    content = content
        .push(field(
            fl!("ui-share-port-label"),
            &form.port,
            ShareField::Port,
        ))
        .push(field(
            fl!("ui-share-minutes-label"),
            &form.minutes,
            ShareField::Minutes,
        ));
    // A problem about the folder is said by the button staying off; the others by a line.
    if let Some(problem) = problem.filter(|problem| *problem != ShareProblem::NoFolder) {
        content = content.push(dialog_parts::error(problem_text(problem)));
    }
    dialog_parts::form(
        dialog_parts::header(
            dialog_parts::Severity::Warning,
            dialog_parts::title(fl!("ui-share-title")),
        ),
        content,
        dialog_parts::buttons([
            dialog_parts::cancel(),
            dialog_parts::confirm(
                fl!("ui-share-start-button"),
                problem
                    .is_none()
                    .then_some(Message::App(AppMessage::ConfirmDialog)),
            ),
        ]),
    )
}

/// The dialog shown once the share runs: where, the warning once more, and the copy of the
/// URL and of the command. Closing it leaves the share running; "Stop sharing" ends it.
#[must_use]
pub fn started(info: &ShareInfo) -> Element<'_, Message> {
    let minutes = info.lifetime.as_secs() / SECONDS_PER_MINUTE;
    let body = column![
        dialog_parts::body(fl!(
            "ui-share-started-body",
            folder = server_text(&info.folder.display().to_string()),
            address = info.address_text(),
            minutes = minutes
        )),
        text(info.address_text()).font(Font::MONOSPACE),
        text(fl!("ui-share-warning")).style(text::warning),
    ]
    .spacing(spacing::SM);
    dialog_parts::form(
        dialog_parts::header(
            dialog_parts::Severity::Warning,
            dialog_parts::title(fl!("ui-share-started-title")),
        ),
        body,
        dialog_parts::buttons([
            dialog_parts::action(fl!("ui-share-started-copy-url-button"), styles::secondary)
                .on_press(share(ShareMessage::CopyUrl)),
            dialog_parts::action(
                fl!("ui-share-started-copy-command-button"),
                styles::secondary,
            )
            .on_press(share(ShareMessage::CopyCommand)),
            dialog_parts::action(fl!("ui-share-started-stop-button"), styles::secondary)
                .on_press(share(ShareMessage::Toggle)),
            dialog_parts::confirm(
                fl!("ui-share-started-keep-button"),
                Some(Message::App(AppMessage::DismissDialog)),
            ),
        ]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_left_is_minutes_and_seconds_then_hours() {
        assert_eq!(remaining_text(Duration::from_secs(0)), "00:00");
        assert_eq!(remaining_text(Duration::from_secs(61)), "01:01");
        assert_eq!(remaining_text(Duration::from_mins(30)), "30:00");
        assert_eq!(remaining_text(Duration::from_hours(1)), "1:00:00");
        assert_eq!(remaining_text(Duration::from_hours(24)), "24:00:00");
    }

    #[test]
    fn the_dialog_offers_this_computer_first_and_each_listed_address() {
        use heimdall_app::share::ShareAddress;
        let mut form = ShareForm::new(&heimdall_core::settings::Settings::default());
        assert_eq!(bind_options(&form).len(), 1);
        assert_eq!(bind_options(&form)[0].choice, BindChoice::Loopback);
        form.addresses.push(ShareAddress {
            name: "eth0".to_owned(),
            address: "192.168.1.10".parse().unwrap(),
        });
        let options = bind_options(&form);
        assert_eq!(options.len(), 2);
        assert_eq!(
            options[1].choice,
            BindChoice::Interface("192.168.1.10".parse().unwrap())
        );
        assert!(options[1].label.contains("192.168.1.10"));
    }
}
