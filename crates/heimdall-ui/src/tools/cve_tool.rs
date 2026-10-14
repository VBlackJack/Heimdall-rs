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

//! The Offline CVE Lookup tool, as the C# `CveLookupView` and `CveLookupViewModel`:
//! searches an embedded database of known high-impact CVEs for common infrastructure
//! software (OpenSSH, Apache, nginx, Tomcat, MySQL, PostgreSQL, Redis, MongoDB,
//! Elasticsearch, IIS, ProFTPD, vsftpd, Postfix, Exim, PHP, Node.js, Jenkins, Plink, PuTTY).
//!
//! Features automatic banner parsing, semantic version matching, CVSS scoring,
//! severity classification badges, and clipboard report generation. Completely offline.

use heimdall_app::TabId;
use heimdall_core::tools::cve_lookup::{
    self, CveMatch, CveSearchResult, CveSeverity, format_report, total_cve_count,
    total_product_count,
};
use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Theme};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, radius, spacing};

/// Messages emitted by the CVE Lookup tool.
#[derive(Debug, Clone)]
pub enum CveMessage {
    /// Search input query modified.
    InputChanged(String),
    /// Search submitted (via Enter key or Search button).
    Search,
    /// Copy the formatted CVE report to the clipboard.
    CopyReport,
}

/// The CVE Lookup tool pane state.
#[derive(Debug, Default)]
pub struct CvePane {
    query: String,
    last_result: Option<CveSearchResult>,
    searched: bool,
}

/// Outcome of updating the CVE Lookup tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Copy(CopySlot, String),
}

impl CvePane {
    /// Handles a tool message.
    pub fn update(&mut self, message: CveMessage) -> Outcome {
        match message {
            CveMessage::InputChanged(val) => {
                self.query = val;
                Outcome::None
            }
            CveMessage::Search => {
                let trimmed = self.query.trim();
                if trimmed.is_empty() {
                    self.last_result = None;
                    self.searched = false;
                } else {
                    self.last_result = Some(cve_lookup::search(trimmed));
                    self.searched = true;
                }
                Outcome::None
            }
            CveMessage::CopyReport => {
                if let Some(ref res) = self.last_result {
                    if !res.matches.is_empty() {
                        let text = format_report(res);
                        return Outcome::Copy(CopySlot::CveReport, text);
                    }
                }
                Outcome::None
            }
        }
    }

    /// Renders the CVE Lookup pane view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &'a ToolPane,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg: CveMessage| Message::Tool(tab, ToolMessage::Cve(msg));

        // 1. Search input toolbar
        let input_box = text_input(
            &fl!("ui-tool-cve-input-placeholder"),
            &self.query,
        )
        .on_input(move |v| send(CveMessage::InputChanged(v)))
        .on_submit(send(CveMessage::Search))
        .padding([spacing::SM, spacing::MD])
        .size(font_size::BODY)
        .width(Length::FillPortion(3));

        let search_btn = super::action_button(
            fl!("ui-tool-cve-btn-search"),
            true,
            Some(send(CveMessage::Search)),
        );

        let mut header_row = row![input_box, search_btn]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

        if let Some(ref res) = self.last_result {
            if !res.matches.is_empty() {
                let copy_btn = super::copy_button(
                    fl!("ui-tool-cve-btn-copy"),
                    state.copied(CopySlot::CveReport),
                    send(CveMessage::CopyReport),
                    super::COPY_PADDING,
                );
                header_row = header_row.push(copy_btn);
            }
        }

        let mut body_col = column![header_row].spacing(spacing::LG);

        // 2. Results or Empty state
        if !self.searched {
            // Initial empty state
            let empty_text = text(fl!("ui-tool-cve-empty-state"))
                .size(font_size::BODY)
                .style(text::secondary);

            let db_info = text(fl!(
                "ui-tool-cve-db-info",
                cves = total_cve_count(),
                products = total_product_count()
            ))
            .size(font_size::CAPTION)
            .style(text::secondary);

            let empty_card = container(
                column![empty_text, db_info]
                    .spacing(spacing::MD)
                    .align_x(Alignment::Center),
            )
            .padding(spacing::XL)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .style(styles::card);

            body_col = body_col.push(empty_card);
        } else if let Some(ref res) = self.last_result {
            if res.matches.is_empty() {
                // No results found
                let no_res_text = text(fl!("ui-tool-cve-no-results"))
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD)
                    .style(move |t: &Theme| {
                        let palette = t.extended_palette();
                        text::Style {
                            color: Some(palette.success.strong.text),
                        }
                    });

                let query_echo = text(format!("Query: {}", res.resolved_query))
                    .size(font_size::CAPTION)
                    .style(text::secondary);

                let card = container(
                    column![no_res_text, query_echo]
                        .spacing(spacing::SM)
                        .align_x(Alignment::Center),
                )
                .padding(spacing::XL)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .style(styles::card);

                body_col = body_col.push(card);
            } else {
                // Summary header
                let summary_label = fl!(
                    "ui-tool-cve-summary",
                    count = res.matches.len(),
                    query = res.resolved_query.as_str()
                );

                let summary_badge = container(
                    text(summary_label)
                        .font(styles::SEMIBOLD)
                        .size(font_size::BODY_LARGE),
                )
                .padding([spacing::SM, spacing::MD])
                .style(styles::card);

                body_col = body_col.push(summary_badge);

                // Matches list
                let mut matches_col = column!().spacing(spacing::SM);
                for m in &res.matches {
                    matches_col = matches_col.push(self.render_cve_card(m, theme));
                }

                let matches_scroll = scrollable(matches_col)
                    .width(Length::Fill)
                    .height(Length::Shrink);

                body_col = body_col.push(matches_scroll);
            }
        }

        // 3. Footer database info
        let footer_info = text(fl!(
            "ui-tool-cve-db-info",
            cves = total_cve_count(),
            products = total_product_count()
        ))
        .size(font_size::CAPTION)
        .style(text::secondary);

        body_col = body_col.push(container(footer_info).align_x(Alignment::Start));

        super::content_column(body_col)
    }

    fn render_cve_card<'a>(&'a self, m: &'a CveMatch, theme: &Theme) -> Element<'a, Message> {
        let palette = theme.extended_palette();

        // Severity label and color mapping
        let (sev_label, sev_bg, sev_fg, sev_border) = match m.severity {
            CveSeverity::Critical => (
                fl!("ui-tool-cve-severity-critical"),
                palette.danger.weak.color,
                palette.danger.strong.text,
                palette.danger.base.color,
            ),
            CveSeverity::High => (
                fl!("ui-tool-cve-severity-high"),
                palette.danger.weak.color,
                palette.danger.strong.text,
                palette.danger.base.color,
            ),
            CveSeverity::Medium => (
                fl!("ui-tool-cve-severity-medium"),
                palette.warning.weak.color,
                palette.warning.strong.text,
                palette.warning.base.color,
            ),
            CveSeverity::Low => (
                fl!("ui-tool-cve-severity-low"),
                palette.primary.weak.color,
                palette.primary.strong.text,
                palette.primary.base.color,
            ),
            CveSeverity::None => (
                fl!("ui-tool-cve-severity-low"),
                palette.background.weak.color,
                palette.background.strong.text,
                palette.background.base.color,
            ),
        };

        // Header: ID + Severity Badge on left, CVSS Pill on right
        let id_text = text(&m.id)
            .font(styles::SEMIBOLD)
            .size(font_size::BODY_LARGE);

        let sev_pill = container(
            text(sev_label)
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(move |_: &Theme| text::Style {
                    color: Some(sev_fg),
                }),
        )
        .padding([2, 8])
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Color(sev_bg)),
            border: Border {
                color: sev_border,
                width: 1.0,
                radius: radius::XS.into(),
            },
            ..container::Style::default()
        });

        let cvss_pill = container(
            text(format!("CVSS {:.1}", m.cvss_score))
                .size(font_size::BODY)
                .font(styles::SEMIBOLD)
                .style(move |_: &Theme| text::Style {
                    color: Some(sev_fg),
                }),
        )
        .padding([4, 10])
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Color(sev_bg)),
            border: Border {
                color: sev_border,
                width: 1.0,
                radius: radius::XS.into(),
            },
            ..container::Style::default()
        });

        let top_row = row![
            row![id_text, sev_pill]
                .spacing(spacing::SM)
                .align_y(Alignment::Center),
            container(cvss_pill).width(Length::Fill).align_x(Alignment::End),
        ]
        .width(Length::Fill)
        .align_y(Alignment::Center);

        // Summary text
        let summary_text = text(&m.summary)
            .size(font_size::BODY)
            .style(text::secondary);

        // Affected versions line
        let affected_label = fl!("ui-tool-cve-col-affected");
        let affected_text = text(format!("{affected_label}: {}", m.affected_versions))
            .size(font_size::CAPTION)
            .style(text::secondary);

        let card_content = column![top_row, summary_text, affected_text]
            .spacing(spacing::XS)
            .width(Length::Fill);

        container(card_content)
            .padding(spacing::MD)
            .width(Length::Fill)
            .style(styles::card)
            .into()
    }
}
