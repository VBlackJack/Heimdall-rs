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

//! The DNS Batch Resolver tool view: bulk domain resolution, concurrent lookups,
//! tabular address display, and CSV export.

use heimdall_app::TabId;
use iced::widget::{column, container, row, text, text_editor};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::dns_batch::{
    self, DnsBatchItemResult, DnsBatchItemStatus, DnsBatchSummary,
};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the DNS batch resolver view.
#[derive(Debug, Clone)]
pub enum DnsBatchMessage {
    /// Editor action on multiline domain list.
    EditorAction(text_editor::Action),
    /// Execute batch resolution.
    Resolve,
    /// Batch resolution completed.
    Finished(Vec<DnsBatchItemResult>),
    /// Copy CSV export to clipboard.
    CopyCsv,
}

/// The DNS Batch Resolver tool pane state.
#[derive(Debug)]
pub struct DnsBatchPane {
    editor: text_editor::Content,
    running: bool,
    error: Option<String>,
    results: Vec<DnsBatchItemResult>,
    summary: DnsBatchSummary,
}

impl Default for DnsBatchPane {
    fn default() -> Self {
        Self {
            editor: text_editor::Content::with_text(
                "google.com\ncloudflare.com\ngithub.com\nwikipedia.org",
            ),
            running: false,
            error: None,
            results: Vec::new(),
            summary: DnsBatchSummary::default(),
        }
    }
}

/// Outcome of updating the DNS Batch Resolver tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl DnsBatchPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: DnsBatchMessage) -> Outcome {
        match message {
            DnsBatchMessage::EditorAction(action) => {
                self.editor.perform(action);
                Outcome::None
            }
            DnsBatchMessage::Resolve => {
                let text = self.editor.text();
                let hostnames = dns_batch::parse_hostnames(&text);
                if hostnames.is_empty() {
                    self.error = Some(fl!("ui-tool-dnsbatch-error-required"));
                    return Outcome::None;
                }

                self.running = true;
                self.error = None;
                self.results.clear();
                self.summary = DnsBatchSummary::default();

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut list = Vec::with_capacity(hostnames.len());
                            for h in hostnames {
                                list.push(dns_batch::resolve_hostname_sync(&h));
                            }
                            list
                        })
                        .await
                        .unwrap_or_default()
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::DnsBatch(DnsBatchMessage::Finished(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            DnsBatchMessage::Finished(res) => {
                self.running = false;
                self.summary = dns_batch::compute_summary(&res);
                self.results = res;
                Outcome::None
            }
            DnsBatchMessage::CopyCsv => {
                if self.results.is_empty() {
                    return Outcome::None;
                }
                let csv = dns_batch::format_batch_csv(&self.results);
                Outcome::Copy(CopySlot::DnsBatchCsv, csv)
            }
        }
    }

    /// Renders the DNS Batch Resolver view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::DnsBatch(msg));

        let input_editor = text_editor(&self.editor)
            .font(super::BOX_FONT)
            .size(font_size::BODY)
            .padding(super::INPUT_PADDING)
            .style(styles::text_box)
            .on_action(move |a| send(DnsBatchMessage::EditorAction(a)))
            .height(Length::Fixed(120.0));

        let resolve_button = if self.running {
            super::action_button(fl!("ui-tool-dnsbatch-resolving"), false, None)
        } else {
            super::action_button(
                fl!("ui-tool-dnsbatch-resolve"),
                true,
                Some(send(DnsBatchMessage::Resolve)),
            )
        };

        let controls_row = row![
            column![
                super::field_label(fl!("ui-tool-dnsbatch-hostnames-label")),
                input_editor
            ]
            .spacing(spacing::XS)
            .width(Length::Fill),
            container(resolve_button).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::End);

        let input_card = container(column![controls_row])
            .padding(spacing::MD)
            .style(styles::card);
        let mut body = column![input_card].spacing(spacing::MD);

        if let Some(err) = &self.error {
            body = body.push(
                container(text(err.as_str()).size(font_size::BODY).style(text::danger))
                    .padding(spacing::MD)
                    .style(styles::card),
            );
        }

        if !self.results.is_empty() {
            let metric_card = |label: String, val: String| {
                container(
                    column![
                        text(label).size(font_size::CAPTION).style(text::secondary),
                        text(val).size(font_size::SUBTITLE).font(super::BOX_FONT),
                    ]
                    .spacing(spacing::XS),
                )
                .padding(spacing::SM)
                .style(styles::card)
                .width(Length::FillPortion(1))
            };

            let metrics_row = row![
                metric_card(
                    fl!("ui-tool-dnsbatch-total"),
                    self.summary.total.to_string()
                ),
                metric_card(
                    fl!("ui-tool-dnsbatch-resolved"),
                    self.summary.resolved.to_string()
                ),
                metric_card(
                    fl!("ui-tool-dnsbatch-failed"),
                    self.summary.failed.to_string()
                ),
            ]
            .spacing(spacing::SM);

            let copy_csv_btn = super::copy_button(
                fl!("ui-tool-dnsbatch-copy-csv"),
                state.copied(CopySlot::DnsBatchCsv),
                send(DnsBatchMessage::CopyCsv),
                super::COPY_PADDING,
            );

            let header_row = row![
                text(fl!("ui-tool-dnsbatch-results-title"))
                    .size(font_size::SUBTITLE)
                    .width(Length::Fill),
                copy_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let mut list_col = column![].spacing(spacing::XS);
            for r in &self.results {
                let status_cell = match r.status {
                    DnsBatchItemStatus::Resolved => text(fl!("ui-tool-dnsbatch-status-resolved"))
                        .size(font_size::BODY)
                        .style(text::success)
                        .width(Length::Fixed(90.0)),
                    DnsBatchItemStatus::Failed => text(fl!("ui-tool-dnsbatch-status-failed"))
                        .size(font_size::BODY)
                        .style(text::danger)
                        .width(Length::Fixed(90.0)),
                    DnsBatchItemStatus::Pending => text(fl!("ui-tool-dnsbatch-status-pending"))
                        .size(font_size::BODY)
                        .style(text::secondary)
                        .width(Length::Fixed(90.0)),
                };

                let addr_text = if !r.ipv4.is_empty() {
                    r.ipv4.join(", ")
                } else if !r.ipv6.is_empty() {
                    r.ipv6.join(", ")
                } else {
                    r.error
                        .clone()
                        .unwrap_or_else(|| fl!("ui-tool-dnsbatch-status-failed"))
                };

                let item_row = row![
                    text(&r.hostname)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    status_cell,
                    text(addr_text)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .style(text::secondary)
                        .width(Length::FillPortion(3)),
                    text(format!("{} ms", r.elapsed_ms))
                        .size(font_size::CAPTION)
                        .style(text::secondary)
                        .width(Length::Fixed(70.0)),
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center);

                list_col = list_col.push(
                    container(item_row)
                        .padding(spacing::XS)
                        .style(styles::card)
                        .width(Length::Fill),
                );
            }

            let results_card =
                container(column![header_row, metrics_row, list_col].spacing(spacing::MD))
                    .padding(spacing::MD)
                    .style(styles::card);

            body = body.push(results_card);
        }

        super::content_column(body)
    }
}
