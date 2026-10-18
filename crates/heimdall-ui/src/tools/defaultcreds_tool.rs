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

//! The Default Credentials tool, as the C# `DefaultCredentialView` and
//! `DefaultCredentialViewModel` for the catalog, but never for the login attempts
//! they make: owner decision (2026-10-17, item 10) keeps this tool list-only. It
//! lists the known vendor default credentials for a product or service, searchable
//! by service name, with plain-text report copy and CSV export. It never opens a
//! connection to anything, so there is no target host, no scan, and no result
//! status: only the catalog.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::module_name_repetitions,
    clippy::redundant_closure_for_method_calls
)]

use heimdall_app::TabId;
use heimdall_core::tools::default_creds::{
    self, DefaultCredEntry, build_csv_export, build_report_text, password_display,
};
use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Theme};

use super::{CopySlot, ToolMessage, ToolPane, key_parts};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, radius, spacing};

/// Messages dispatched by the Default Credentials tool.
#[derive(Debug, Clone)]
pub enum DefaultCredsMessage {
    /// Service name filter edited.
    FilterChanged(String),
    /// Copy the plain-text tabular report to clipboard.
    CopyReport,
    /// Export CSV to clipboard.
    ExportCsv,
}

/// Action outcome from updating the tool pane.
#[derive(Debug)]
pub enum Outcome {
    /// No action.
    None,
    /// Copy text into a copy slot.
    Copy(CopySlot, String),
}

/// The Default Credentials tool view state.
pub struct DefaultCredsPane {
    filter: String,
    report_text: String,
    csv_text: String,
}

impl DefaultCredsPane {
    /// Creates a new Default Credentials pane, listing the full catalog.
    #[must_use]
    pub fn new() -> Self {
        let mut pane = Self {
            filter: String::new(),
            report_text: String::new(),
            csv_text: String::new(),
        };
        pane.recompute_exports();
        pane
    }

    /// The catalog entries matching the current filter.
    fn entries(&self) -> Vec<DefaultCredEntry> {
        default_creds::filter_entries(&self.filter)
    }
}

impl Default for DefaultCredsPane {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for DefaultCredsPane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultCredsPane")
            .field("filter", &self.filter)
            .finish_non_exhaustive()
    }
}

impl DefaultCredsPane {
    /// Handles Default Credentials tool messages.
    pub fn update(&mut self, msg: DefaultCredsMessage) -> Outcome {
        match msg {
            DefaultCredsMessage::FilterChanged(filter) => {
                self.filter = filter;
                self.recompute_exports();
                Outcome::None
            }
            DefaultCredsMessage::CopyReport => {
                if !self.report_text.is_empty() {
                    return Outcome::Copy(CopySlot::DefaultCredsReport, self.report_text.clone());
                }
                Outcome::None
            }
            DefaultCredsMessage::ExportCsv => {
                if !self.csv_text.is_empty() {
                    return Outcome::Copy(CopySlot::DefaultCredsCsv, self.csv_text.clone());
                }
                Outcome::None
            }
        }
    }

    fn recompute_exports(&mut self) {
        let loc = |k: &str| localize_key(k);
        let entries = self.entries();
        self.report_text = build_report_text(&entries, Some(&loc));
        self.csv_text = build_csv_export(&entries, Some(&loc));
    }

    /// Renders the Default Credentials tool page.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, ToolMessage::DefaultCreds(msg));

        // 1. Note banner (always visible): list-only, and why.
        let note_banner = container(
            row![
                text("\u{2139}")
                    .font(styles::SEMIBOLD)
                    .size(font_size::BODY_LARGE),
                text(fl!("ui-tool-defaultcreds-note"))
                    .size(font_size::BODY)
                    .width(Length::Fill),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
        )
        .padding(spacing::SM)
        .width(Length::Fill)
        .style(|t: &Theme| {
            let palette = t.extended_palette();
            container::Style {
                background: Some(Background::Color(palette.primary.weak.color)),
                border: Border {
                    radius: radius::SM.into(),
                    width: 1.0,
                    color: palette.primary.base.color,
                },
                ..container::Style::default()
            }
        });

        // 2. Filter input
        let filter_input = text_input(
            &fl!("ui-tool-defaultcreds-filter-placeholder"),
            &self.filter,
        )
        .font(super::BOX_FONT)
        .size(font_size::BODY_LARGE)
        .padding(super::INPUT_PADDING)
        .style(styles::text_input)
        .on_input(move |f| send(DefaultCredsMessage::FilterChanged(f)));

        let input_card = key_parts::card(
            column![
                super::field_label(fl!("ui-tool-defaultcreds-filter-label")),
                filter_input
            ]
            .spacing(spacing::XS),
        );

        let mut body_col = column![note_banner, input_card].spacing(spacing::MD);

        // 3. Results table or empty state
        let entries = self.entries();
        if entries.is_empty() {
            body_col = body_col.push(super::empty_state(fl!("ui-tool-defaultcreds-empty-state")));
        } else {
            body_col = body_col.push(self.render_results_card(&entries));

            let count_label = fl!("ui-tool-defaultcreds-count", count = entries.len());

            let copy_report_btn = super::copy_button(
                fl!("ui-tool-defaultcreds-btn-copy"),
                state.copied(CopySlot::DefaultCredsReport),
                send(DefaultCredsMessage::CopyReport),
                super::COPY_PADDING,
            );

            let export_csv_btn = super::copy_button(
                fl!("ui-tool-defaultcreds-btn-export"),
                state.copied(CopySlot::DefaultCredsCsv),
                send(DefaultCredsMessage::ExportCsv),
                super::COPY_PADDING,
            );

            let footer_row = row![
                text(count_label)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .style(text::secondary)
                    .width(Length::Fill),
                copy_report_btn,
                export_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            body_col = body_col.push(key_parts::card(column![footer_row]));
        }

        super::content_column(body_col)
    }

    fn render_results_card<'a>(&'a self, entries: &[DefaultCredEntry]) -> Element<'a, Message> {
        // Header row
        let header_row = row![
            text(fl!("ui-tool-defaultcreds-col-service"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::secondary)
                .width(Length::Fixed(120.0)),
            text(fl!("ui-tool-defaultcreds-col-port"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::secondary)
                .width(Length::Fixed(70.0)),
            text(fl!("ui-tool-defaultcreds-col-user"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::secondary)
                .width(Length::Fixed(140.0)),
            text(fl!("ui-tool-defaultcreds-col-pass"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::secondary)
                .width(Length::Fill),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let mut rows_col = column![header_row].spacing(spacing::XS);

        for e in entries {
            let loc = |k: &str| localize_key(k);
            let pass_display = password_display(e.password, Some(&loc));
            let user_display = if e.username.is_empty() {
                fl!("ui-tool-defaultcreds-empty")
            } else {
                e.username.to_string()
            };

            let item_row = row![
                text(e.service)
                    .font(styles::SEMIBOLD)
                    .size(font_size::BODY)
                    .width(Length::Fixed(120.0)),
                text(e.port.to_string())
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .width(Length::Fixed(70.0)),
                text(user_display)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(pass_display)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .style(text::secondary)
                    .width(Length::Fill),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            rows_col = rows_col.push(item_row);
        }

        let scroll = scrollable(rows_col)
            .width(Length::Fill)
            .height(Length::Shrink);
        key_parts::card(column![scroll])
    }
}

/// Helper mapping the engine's report and CSV formatter keys to localized strings.
fn localize_key(key: &str) -> String {
    match key {
        "ToolDefCredColService" => fl!("ui-tool-defaultcreds-col-service"),
        "ToolDefCredColPort" => fl!("ui-tool-defaultcreds-col-port"),
        "ToolDefCredColUser" => fl!("ui-tool-defaultcreds-col-user"),
        "ToolDefCredColPass" => fl!("ui-tool-defaultcreds-col-pass"),
        "ToolDefCredEmpty" => fl!("ui-tool-defaultcreds-empty"),
        _ => key.to_string(),
    }
}
