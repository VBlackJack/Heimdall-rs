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

//! The import report of a shared source, as the C# `SharedSessionImportReportDialog`
//! (`SharedSessionImportReportBuilder`): over the window, what the last read of the source's
//! file took and what it did not, so that whoever keeps the file can fix it. The sessions
//! taken, the entries skipped and why, a file read only in part, the logins that name a
//! variable with no value, the names that are not their host's, the hosts filed twice.
//! Read-only: there is a way out and nothing else to press.

use heimdall_app::shared_report::{ImportReport, ReportCheck, ReportEntry, SkipReason};
use heimdall_app::shared_sessions::SourceState;
use iced::widget::{Column, button, column, container, row, scrollable, text};
use iced::{Color, Element, Length, Theme};

use crate::i18n::fl;
use crate::shared_view::{source_label, source_notes};
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Widest the report grows.
const REPORT_WIDTH: f32 = 640.0;

/// Tallest the list of findings grows before it scrolls.
const LIST_HEIGHT: f32 = 360.0;

/// Opacity of the veil under the report, as the other dialogs'.
const VEIL_ALPHA: f32 = 0.7;

/// The checks in the order a report lists them.
const CHECKS: [ReportCheck; 4] = [
    ReportCheck::SkippedSession,
    ReportCheck::UnresolvedVariable,
    ReportCheck::NameHostMismatch,
    ReportCheck::DuplicateHost,
];

/// The title of a section of findings, with how many it holds.
#[must_use]
pub fn check_title(check: ReportCheck, count: usize) -> String {
    match check {
        ReportCheck::SkippedSession => fl!("ui-shared-report-skipped", count = count),
        ReportCheck::UnresolvedVariable => fl!("ui-shared-report-unresolved", count = count),
        ReportCheck::NameHostMismatch => fl!("ui-shared-report-mismatch", count = count),
        ReportCheck::DuplicateHost => fl!("ui-shared-report-duplicate-host", count = count),
    }
}

/// What a finding says, after the session it is about.
#[must_use]
pub fn entry_text(entry: &ReportEntry) -> String {
    match entry.check {
        ReportCheck::SkippedSession => match entry.reason {
            Some(SkipReason::OtherProtocol) => {
                fl!(
                    "ui-shared-report-reason-other-protocol",
                    protocol = entry.detail.as_str()
                )
            }
            Some(SkipReason::InvalidHost) => {
                fl!(
                    "ui-shared-report-reason-invalid-host",
                    host = entry.detail.as_str()
                )
            }
            Some(SkipReason::DuplicateSession) => fl!("ui-shared-report-reason-duplicate-session"),
            Some(SkipReason::OverLimit) => fl!("ui-shared-report-reason-over-limit"),
            None => String::new(),
        },
        ReportCheck::UnresolvedVariable => {
            fl!(
                "ui-shared-report-detail-unresolved",
                name = entry.detail.as_str()
            )
        }
        ReportCheck::NameHostMismatch => {
            fl!(
                "ui-shared-report-detail-mismatch",
                host = entry.detail.as_str()
            )
        }
        ReportCheck::DuplicateHost => fl!(
            "ui-shared-report-detail-duplicate-host",
            host = entry.detail.as_str(),
            others = entry.context.as_str()
        ),
    }
}

/// The session a finding is about, with its folder.
fn entry_session(entry: &ReportEntry) -> String {
    if entry.folder.is_empty() {
        entry.session.clone()
    } else {
        format!("{} / {}", entry.folder, entry.session)
    }
}

/// The report of `state`'s source over the window, veiled as under a dialog: what is under it
/// is neither hovered nor clicked.
pub fn layer<'a>(state: &SourceState, report: &ImportReport) -> Element<'a, Message> {
    let mut summary = Column::new()
        .spacing(spacing::XS)
        .push(text(fl!("ui-shared-report-taken", count = state.sessions)).size(font_size::CAPTION));
    for (note, warning) in source_notes(state) {
        summary = summary.push(text(note).size(font_size::CAPTION).style(if warning {
            text::warning
        } else {
            text::secondary
        }));
    }
    let mut list = Column::new().spacing(spacing::SM);
    for check in CHECKS {
        let entries: Vec<&ReportEntry> = report
            .entries
            .iter()
            .filter(|entry| entry.check == check)
            .collect();
        if entries.is_empty() {
            continue;
        }
        list = list.push(text(check_title(check, entries.len())).size(font_size::BODY));
        for entry in entries {
            list = list.push(
                column![
                    text(entry_session(entry)).size(font_size::CAPTION),
                    text(entry_text(entry))
                        .size(font_size::CAPTION)
                        .style(text::secondary),
                ]
                .spacing(spacing::XS),
            );
        }
    }
    if report.is_empty() {
        list = list.push(text(fl!("ui-shared-report-empty")).style(text::secondary));
    }
    let card = container(
        column![
            text(fl!("ui-shared-report-title", name = source_label(state))).size(font_size::TITLE),
            summary,
            scrollable(list).height(Length::Shrink).width(Length::Fill),
            row![
                iced::widget::space::horizontal(),
                button(text(fl!("ui-shared-report-close")))
                    .style(styles::primary)
                    .on_press(Message::SharedReportClose),
            ],
        ]
        .spacing(spacing::MD),
    )
    .padding(spacing::LG)
    .max_width(REPORT_WIDTH)
    .max_height(LIST_HEIGHT + REPORT_HEADROOM)
    .style(styles::panel);
    iced::widget::opaque(
        container(card)
            .center(Length::Fill)
            .padding(spacing::LG)
            .style(|_: &Theme| container::Style {
                background: Some(
                    Color {
                        a: VEIL_ALPHA,
                        ..Color::BLACK
                    }
                    .into(),
                ),
                ..container::Style::default()
            }),
    )
}

/// Room for the title, the summary and the button above and below the list.
const REPORT_HEADROOM: f32 = 200.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(check: ReportCheck, reason: Option<SkipReason>, detail: &str) -> ReportEntry {
        ReportEntry {
            check,
            folder: "Prod".to_owned(),
            session: "web01".to_owned(),
            detail: detail.to_owned(),
            context: "db".to_owned(),
            reason,
        }
    }

    #[test]
    fn a_finding_names_its_session_with_its_folder() {
        assert_eq!(
            entry_session(&entry(ReportCheck::NameHostMismatch, None, "h")),
            "Prod / web01"
        );
        let mut at_root = entry(ReportCheck::NameHostMismatch, None, "h");
        at_root.folder.clear();
        assert_eq!(entry_session(&at_root), "web01");
    }

    #[test]
    fn every_finding_says_what_it_found() {
        let said = entry_text(&entry(
            ReportCheck::SkippedSession,
            Some(SkipReason::OtherProtocol),
            "FTP",
        ));
        assert!(said.contains("FTP"), "{said}");
        let said = entry_text(&entry(ReportCheck::UnresolvedVariable, None, "GHOST"));
        assert!(said.contains("GHOST"), "{said}");
        let said = entry_text(&entry(ReportCheck::DuplicateHost, None, "db01"));
        assert!(said.contains("db01") && said.contains("db"), "{said}");
        for reason in [
            SkipReason::InvalidHost,
            SkipReason::DuplicateSession,
            SkipReason::OverLimit,
        ] {
            assert!(!entry_text(&entry(ReportCheck::SkippedSession, Some(reason), "x")).is_empty());
        }
        for check in CHECKS {
            assert!(check_title(check, 2).contains('2'));
        }
    }
}
