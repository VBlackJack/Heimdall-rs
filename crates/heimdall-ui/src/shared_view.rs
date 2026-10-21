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

//! The tree's section of shared sessions, as the C# tree shows them: one header per source
//! with what became of its file, then its sessions, each marked shared. They are read-only:
//! a row only opens the session, there is no menu to edit, move or delete it.

use heimdall_app::shared_sessions::{SharedSession, SourceProblem, SourceState, SourceStatus};
use heimdall_app::{Message as AppMessage, ProfileKind};
use heimdall_core::shared_sessions::MAX_SOURCE_FILE_BYTES;
use iced::widget::{Column, button, container, row, text, tooltip};
use iced::{Element, Length, Theme};

use crate::ellipsis;
use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
use crate::shell::Message;
use crate::styles;
use crate::tokens::{BORDER_WIDTH, font_size, spacing};

/// Side of a session's protocol icon.
const ICON_SIDE: f32 = 14.0;

/// Padding of the "shared" badge.
const BADGE_PADDING: f32 = 4.0;

/// Radius of the "shared" badge.
const BADGE_RADIUS: f32 = 3.0;

/// Bytes in a megabyte, to say the limit.
const MEGABYTE: u64 = 1024 * 1024;

/// Indent of the sessions under their source's header.
const SESSION_INDENT: f32 = 12.0;

/// The localized reason a source holds nothing or kept its previous sessions.
#[must_use]
pub fn problem_text(problem: &SourceProblem, path: &str) -> String {
    match problem {
        SourceProblem::InvalidId => fl!("ui-shared-problem-invalid-id"),
        SourceProblem::DuplicateId => fl!("ui-shared-problem-duplicate-id"),
        SourceProblem::NoPath => fl!("ui-shared-problem-no-path"),
        SourceProblem::Missing => fl!("ui-shared-problem-missing", path = path),
        SourceProblem::TooLarge => {
            let limit: u64 = MAX_SOURCE_FILE_BYTES / MEGABYTE;
            fl!("ui-shared-problem-too-large", path = path, limit = limit)
        }
        SourceProblem::Unreadable(reason) => {
            fl!(
                "ui-shared-problem-unreadable",
                path = path,
                reason = reason.as_str()
            )
        }
        SourceProblem::Unparsable(reason) => {
            fl!(
                "ui-shared-problem-unparsable",
                path = path,
                reason = reason.as_str()
            )
        }
        SourceProblem::Empty => fl!("ui-shared-problem-empty", path = path),
    }
}

/// When an offline copy was taken, in the user's time zone.
fn copy_date(taken_at: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(taken_at)
        .format(COPY_DATE_FORMAT)
        .to_string()
}

/// How the date of an offline copy is written.
const COPY_DATE_FORMAT: &str = "%Y-%m-%d %H:%M";

/// What to call a source: the name the user gave it, else its file.
pub(crate) fn source_label(state: &SourceState) -> String {
    if state.name.trim().is_empty() {
        state.path.clone()
    } else {
        state.name.clone()
    }
}

/// The notes under a source's header: why it holds nothing or kept its previous sessions,
/// and what of its file was left out or never read.
#[must_use]
pub fn source_notes(state: &SourceState) -> Vec<(String, bool)> {
    let mut notes = Vec::new();
    match &state.status {
        SourceStatus::Disabled => notes.push((fl!("ui-shared-disabled"), false)),
        SourceStatus::Failed(problem) => notes.push((problem_text(problem, &state.path), true)),
        SourceStatus::Loaded => {}
    }
    if let Some(offline) = &state.offline {
        notes.push((
            fl!(
                "ui-shared-offline",
                date = copy_date(offline.taken_at),
                reason = problem_text(&offline.reason, &state.path)
            ),
            true,
        ));
    }
    if let Some(problem) = &state.kept_previous {
        notes.push((
            fl!(
                "ui-shared-kept-previous",
                reason = problem_text(problem, &state.path)
            ),
            true,
        ));
    }
    if state.refused > 0 {
        notes.push((fl!("ui-shared-refused", count = state.refused), false));
    }
    if state.skipped_entries > 0 {
        notes.push((
            fl!("ui-shared-skipped", count = state.skipped_entries),
            false,
        ));
    }
    if state.truncated {
        notes.push((fl!("ui-shared-truncated"), true));
    }
    if state.ignored_passwords > 0 {
        notes.push((
            fl!("ui-shared-passwords", count = state.ignored_passwords),
            false,
        ));
    }
    notes
}

/// The section, or `None` when no shared source is configured.
#[must_use]
pub fn section(app: &heimdall_app::App) -> Option<Element<'_, Message>> {
    let shared = app.shared_sessions();
    if shared.is_empty() {
        return None;
    }
    let mut list = Column::new().spacing(spacing::XS).push(
        text(fl!("ui-shared-title"))
            .size(font_size::CAPTION)
            .style(text::secondary),
    );
    for state in shared.sources() {
        list = list.push(
            text(fl!(
                "ui-shared-source-header",
                name = source_label(state),
                count = state.sessions
            ))
            .size(font_size::CAPTION),
        );
        for (note, warning) in source_notes(state) {
            list = list.push(text(note).size(font_size::SMALL_CAPTION).style(if warning {
                text::warning
            } else {
                text::secondary
            }));
        }
        for session in shared.sessions_of(&state.id) {
            list = list.push(session_row(session, &source_label(state)));
        }
    }
    Some(list.into())
}

/// A session: its protocol, name and the "shared" badge. Pressing it opens it.
fn session_row<'a>(session: &SharedSession, source: &str) -> Element<'a, Message> {
    let kind = match session.protocol() {
        "RDP" => ProfileKind::Rdp,
        _ => ProfileKind::Ssh,
    };
    let (host, port) = session.endpoint();
    let content = row![
        icons::icon(Icon::of(kind), Tint::Protocol(kind), ICON_SIDE),
        ellipsis::text(session.name().to_owned()).width(Length::Fill),
        badge(),
    ]
    .spacing(spacing::XS)
    .align_y(iced::Alignment::Center);
    let open = button(content)
        .padding(crate::tokens::BUTTON_PADDING)
        .width(Length::Fill)
        .style(styles::secondary)
        .on_press(Message::App(AppMessage::ConnectShared(session.id.clone())));
    let tip = fl!(
        "ui-shared-row-tooltip",
        source = source,
        address = format!("{host}:{port}")
    );
    row![
        iced::widget::space().width(SESSION_INDENT),
        styles::tip(
            open,
            text(tip).size(font_size::CAPTION),
            tooltip::Position::Right
        )
        .padding(styles::TOOLTIP_PADDING)
        .style(styles::tooltip),
    ]
    .into()
}

/// The mark of a shared session, in the tree's badge style.
fn badge<'a>() -> Element<'a, Message> {
    container(
        text(fl!("ui-shared-badge"))
            .size(font_size::SMALL_CAPTION)
            .style(text::secondary)
            .wrapping(text::Wrapping::None),
    )
    .padding([0.0, BADGE_PADDING])
    .style(|theme: &Theme| container::Style {
        background: Some(theme.extended_palette().background.weak.color.into()),
        border: iced::Border {
            color: theme.extended_palette().background.strong.color,
            width: BORDER_WIDTH,
            radius: BADGE_RADIUS.into(),
        },
        ..container::Style::default()
    })
    .into()
}
