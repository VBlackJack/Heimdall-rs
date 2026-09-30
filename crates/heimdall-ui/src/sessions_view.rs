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

//! "Import OpenSSH config" in the window, as the C# one: the file picked in the system's
//! open dialog, from `~/.ssh`, then its preview, a row per server to tick, what was read
//! differently or left out below, then what was imported.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use heimdall_app::{
    Dialog, Message as AppMessage, SessionsMessage, SessionsPreview, SessionsRow, SessionsSource,
};
use heimdall_core::import::openssh::{Code, Diagnostic, GatewayStep, Level, Status};
use heimdall_core::import::putty;
use iced::widget::{Column, button, checkbox, column, container, row, scrollable, text};
use iced::{Element, Length, Theme};

use crate::i18n::fl;
use crate::shell::Message;

/// The file OpenSSH reads, offered first.
const CONFIG_FILE_NAME: &str = "config";

/// The folder OpenSSH keeps its files in, under the home folder.
pub const SSH_FOLDER: &str = ".ssh";

/// Room between the parts of the preview.
const SPACING: f32 = 8.0;

/// Size of the preview's text.
const TEXT_SIZE: f32 = 13.0;

/// Size of its title.
const TITLE_SIZE: f32 = 20.0;

/// Tallest the list of servers grows before it scrolls.
const ROWS_HEIGHT: f32 = 300.0;

/// Tallest the diagnostics grow before they scroll.
const DIAGNOSTICS_HEIGHT: f32 = 130.0;

/// Widths of the columns: tick, alias, host, port, user, key, gateway chain, status.
const COLUMNS: [f32; 8] = [50.0, 130.0, 160.0, 55.0, 100.0, 130.0, 250.0, 85.0];

/// The file the user picks in the open dialog, once it closes; `None` when cancelled.
pub type Pick = Pin<Box<dyn Future<Output = Option<rfd::FileHandle>> + Send>>;

/// Opens the system's open dialog titled `title`, in `~/.ssh` offering `config`, over
/// `parent` when there is one, as the C# `OpenFileDialog`.
#[must_use]
pub fn pick(title: String, parent: Option<&dyn iced::window::Window>) -> Pick {
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title(title)
        .set_file_name(CONFIG_FILE_NAME);
    if let Some(folder) = std::env::home_dir()
        .map(|home| home.join(SSH_FOLDER))
        .filter(|folder| folder.is_dir())
    {
        dialog = dialog.set_directory(folder);
    }
    if let Some(parent) = parent {
        dialog = dialog.set_parent(&parent);
    }
    Box::pin(dialog.pick_file())
}

/// The picked file's text, or why it could not be read; `None` when none was picked.
pub async fn read(pick: Pick) -> Option<Result<String, String>> {
    let file = pick.await?;
    Some(read_file(file.path()).await)
}

/// The text of `path`, or its path and why it could not be read.
///
/// # Errors
///
/// The path and the system's reason when the file cannot be read as UTF-8 text.
pub async fn read_file(path: &Path) -> Result<String, String> {
    tokio::fs::read_to_string(path)
        .await
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn app(message: SessionsMessage) -> Message {
    Message::App(AppMessage::Sessions(message))
}

/// A cell of `width`, its text on one line and cut at its edge.
fn cell<'a>(value: impl Into<String>, width: f32) -> Element<'a, Message> {
    container(
        text(value.into())
            .size(TEXT_SIZE)
            .wrapping(text::Wrapping::None),
    )
    .width(width)
    .clip(true)
    .into()
}

/// The preview, as the C# `ImportSessionsPreviewDialog`.
#[must_use]
pub fn preview(preview: &SessionsPreview) -> Element<'_, Message> {
    let (total, new, duplicate, invalid) = preview.counts();
    let summary = if invalid > 0 {
        fl!(
            "ui-sessions-summary-invalid",
            total = total,
            new = new,
            duplicate = duplicate,
            invalid = invalid
        )
    } else {
        fl!(
            "ui-openssh-summary",
            total = total,
            new = new,
            duplicate = duplicate
        )
    };
    let header = row![
        cell("", COLUMNS[0]),
        cell(fl!("ui-openssh-column-alias"), COLUMNS[1]),
        cell(fl!("ui-openssh-column-host"), COLUMNS[2]),
        cell(fl!("ui-openssh-column-port"), COLUMNS[3]),
        cell(fl!("ui-openssh-column-user"), COLUMNS[4]),
        cell(fl!("ui-openssh-column-key"), COLUMNS[5]),
        cell(fl!("ui-openssh-column-chain"), COLUMNS[6]),
        cell(fl!("ui-openssh-column-status"), COLUMNS[7]),
    ];
    let rows = Column::with_children(
        preview
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| server_row(index, row)),
    )
    .spacing(2.0);
    let mut content = column![
        text(title(preview.source)).size(TITLE_SIZE),
        text(summary).size(TEXT_SIZE),
    ]
    .spacing(SPACING);
    // The C# hint is about ProxyJump, which only an OpenSSH file has.
    if preview.source == SessionsSource::OpenSsh {
        content = content.push(text(fl!("ui-openssh-hint")).size(TEXT_SIZE));
    }
    let mut content = content.push(
        column![
            checkbox(preview.all_chosen())
                .label(fl!("ui-openssh-choose-all"))
                .text_size(TEXT_SIZE)
                .on_toggle(|on| app(SessionsMessage::ChooseAll(on))),
            header,
            scrollable(rows).height(Length::Shrink).height(ROWS_HEIGHT),
        ]
        .spacing(SPACING),
    );
    let said: Vec<Element<'_, Message>> = preview
        .diagnostics
        .iter()
        .map(diagnostic_line)
        .chain(preview.putty_diagnostics.iter().map(putty_line))
        .collect();
    if !said.is_empty() {
        content = content
            .push(text(fl!("ui-openssh-diagnostics", count = said.len())).size(TEXT_SIZE))
            .push(scrollable(Column::with_children(said)).height(DIAGNOSTICS_HEIGHT));
    }
    content
        .push(
            row![
                button(text(fl!("ui-dialog-cancel-button")))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::DismissDialog)),
                button(text(fl!("ui-openssh-import-button"))).on_press_maybe(
                    preview
                        .can_import()
                        .then_some(Message::App(AppMessage::ConfirmDialog))
                ),
            ]
            .spacing(SPACING),
        )
        .into()
}

fn server_row(index: usize, row: &SessionsRow) -> Element<'_, Message> {
    let candidate = &row.assessment.candidate;
    let status = match row.assessment.status {
        Status::New => fl!("ui-openssh-status-new"),
        Status::Duplicate => fl!("ui-openssh-status-duplicate"),
        Status::Invalid => fl!("ui-sessions-status-invalid"),
    };
    // As the C# preview: an invalid server has no tick to give.
    let tick = checkbox(row.chosen).on_toggle_maybe(
        row.choosable()
            .then_some(move |_| app(SessionsMessage::Choose(index))),
    );
    let host = if candidate.host_name.is_empty() {
        fl!("ui-sessions-no-host")
    } else {
        candidate.host_name.clone()
    };
    row![
        container(tick).width(COLUMNS[0]),
        cell(candidate.alias.clone(), COLUMNS[1]),
        cell(host, COLUMNS[2]),
        cell(candidate.port.to_string(), COLUMNS[3]),
        cell(candidate.user.clone().unwrap_or_default(), COLUMNS[4]),
        cell(
            candidate.identity_file.clone().unwrap_or_default(),
            COLUMNS[5]
        ),
        cell(chain(&row.assessment.gateways), COLUMNS[6]),
        cell(status, COLUMNS[7]),
    ]
    .align_y(iced::Alignment::Center)
    .into()
}

/// The gateways on the way, `user@host:port -> ...`, a saved one named.
fn chain(steps: &[GatewayStep]) -> String {
    steps
        .iter()
        .map(|step| {
            let user = step
                .user
                .as_deref()
                .map(|user| format!("{user}@"))
                .unwrap_or_default();
            let hop = format!("{user}{}:{}", step.host, step.port);
            match &step.reused {
                Some(name) => format!(
                    "{hop} ({})",
                    fl!("ui-openssh-reusing", name = name.as_str())
                ),
                None => hop,
            }
        })
        .collect::<Vec<_>>()
        .join(" -> ")
}

fn diagnostic_line(diagnostic: &Diagnostic) -> Element<'_, Message> {
    let value = diagnostic.context.as_deref().unwrap_or_default();
    let said = match diagnostic.code {
        Code::MatchBlockIgnored => fl!("ui-openssh-diag-match"),
        Code::IncludeIgnored => fl!("ui-openssh-diag-include", value = value),
        Code::WildcardAliasIgnored => fl!("ui-openssh-diag-wildcard", value = value),
        Code::UnknownDirectiveIgnored => fl!("ui-openssh-diag-unknown", value = value),
        Code::InvalidPort => fl!("ui-openssh-diag-port", value = value),
        Code::DuplicateAlias => fl!("ui-openssh-diag-duplicate", value = value),
        Code::ProxyCommandUnsupported => fl!("ui-openssh-diag-proxycommand", value = value),
        Code::ProxyJumpWithProxyCommand => fl!("ui-openssh-diag-mixed", value = value),
        Code::ProxyJumpToken => fl!("ui-openssh-diag-jump-token", value = value),
        Code::ProxyJumpCycle => fl!("ui-openssh-diag-cycle", value = value),
        Code::ProxyJumpSyntax => fl!("ui-openssh-diag-syntax", value = value),
        Code::IdentityFileTildeExpanded => fl!("ui-openssh-diag-tilde", value = value),
        Code::HostNameFallbackToAlias => fl!("ui-openssh-diag-fallback", value = value),
        Code::HostNameToken => fl!("ui-openssh-diag-host-token", value = value),
    };
    let warning = diagnostic.level == Level::Warning;
    text(fl!(
        "ui-openssh-diag-line",
        line = diagnostic.line,
        said = said
    ))
    .size(TEXT_SIZE)
    .style(move |theme: &Theme| text::Style {
        color: warning.then(|| theme.extended_palette().danger.base.color),
    })
    .into()
}

/// What a `PuTTY` session said, as the C# preview names it.
fn putty_line(diagnostic: &putty::Diagnostic) -> Element<'_, Message> {
    let session = diagnostic.session.as_str();
    let value = diagnostic.context.as_deref().unwrap_or_default();
    let said = match diagnostic.code {
        putty::Code::DefaultSettingsSkipped => fl!("ui-putty-diag-default", session = session),
        putty::Code::NotSsh => fl!("ui-putty-diag-not-ssh", session = session, value = value),
        putty::Code::MissingHost => fl!("ui-putty-diag-missing-host", session = session),
        putty::Code::InvalidPort => fl!("ui-putty-diag-port", session = session, value = value),
        putty::Code::PpkKey => fl!("ui-putty-diag-ppk", session = session, value = value),
        putty::Code::ProxyNotMapped => fl!("ui-putty-diag-proxy", session = session, value = value),
        putty::Code::ForwardingsNotMapped => fl!(
            "ui-putty-diag-forwards",
            session = session,
            count = value.parse::<usize>().unwrap_or_default()
        ),
        putty::Code::RemoteCommandNotMapped => {
            fl!("ui-putty-diag-command", session = session, value = value)
        }
    };
    let warning = diagnostic.level == putty::Level::Warning;
    text(said)
        .size(TEXT_SIZE)
        .style(move |theme: &Theme| text::Style {
            color: warning.then(|| theme.extended_palette().danger.base.color),
        })
        .into()
}

/// The title of an import's dialogs.
#[must_use]
pub fn title(source: SessionsSource) -> String {
    match source {
        SessionsSource::OpenSsh => fl!("ui-openssh-title"),
        SessionsSource::Putty => fl!("ui-putty-title"),
    }
}

/// What the import said when it ended, or why it did not start: the title and the lines
/// under it.
#[must_use]
pub fn report_lines(dialog: &Dialog) -> Option<(String, Vec<String>)> {
    let lines = match dialog {
        Dialog::SessionsDone { source, counts } => {
            let mut lines = vec![match source {
                SessionsSource::OpenSsh => fl!(
                    "ui-openssh-done",
                    imported = counts.imported,
                    duplicates = counts.duplicates,
                    warnings = counts.warnings
                ),
                SessionsSource::Putty => fl!(
                    "ui-putty-done",
                    imported = counts.imported,
                    duplicates = counts.duplicates,
                    invalid = counts.invalid,
                    warnings = counts.warnings
                ),
            }];
            if counts.gateways > 0 {
                lines.push(fl!("ui-openssh-done-gateways", count = counts.gateways));
            }
            (*source, lines)
        }
        Dialog::SessionsUnreadable { source, detail } => (
            *source,
            vec![match source {
                SessionsSource::OpenSsh => fl!("ui-openssh-unreadable", detail = detail.as_str()),
                SessionsSource::Putty => fl!("ui-putty-unreadable", detail = detail.as_str()),
            }],
        ),
        Dialog::SessionsEmpty { source } => (
            *source,
            vec![match source {
                SessionsSource::OpenSsh => fl!("ui-openssh-empty"),
                SessionsSource::Putty => fl!("ui-putty-empty"),
            }],
        ),
        _ => return None,
    };
    Some((title(lines.0), lines.1))
}
