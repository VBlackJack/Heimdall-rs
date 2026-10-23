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

//! The DNS Batch Resolver tool view, as the C# `DnsBatchResolverView` and
//! `DnsBatchResolverViewModel`: a list of hostnames resolved concurrently, at most
//! [`dns_batch::MAX_CONCURRENT_LOOKUPS`] at once, the rows shown in the list's order as they
//! come, with a summary and a CSV export.
//!
//! A batch is one stream, aborted by Stop, as the C# `Cancel`, or when the tab closes; its
//! rows carry its batch's number, so one of an earlier batch never lands in a later one. A
//! list longer than [`dns_batch::MAX_BATCH_HOSTS`] is refused, not cut.

use heimdall_app::TabId;
use heimdall_app::net_tools;
use iced::futures::{SinkExt as _, StreamExt as _};
use iced::task::Handle;
use iced::widget::{column, container, row, text, text_editor};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::dns_batch::{
    self, DnsBatchFailure, DnsBatchItemResult, DnsBatchItemStatus, DnsBatchSummary,
};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Rows gathered into one message, at most, when several are ready at once.
const ROW_BATCH: usize = 32;

/// Messages buffered between the batch and the view.
const EVENT_BUFFER: usize = 16;

/// Messages emitted by the DNS batch resolver view.
#[derive(Debug, Clone)]
pub enum DnsBatchMessage {
    /// Editor action on multiline domain list.
    EditorAction(text_editor::Action),
    /// Execute batch resolution.
    Resolve,
    /// Cancel the batch under way.
    Stop,
    /// Rows of batch `.0` resolved, in the list's order.
    Resolved(u64, Vec<DnsBatchItemResult>),
    /// Batch `.0` resolved every hostname.
    Ended(u64),
    /// Copy CSV export to clipboard.
    CopyCsv,
}

/// The DNS Batch Resolver tool pane state.
#[derive(Debug)]
pub struct DnsBatchPane {
    editor: text_editor::Content,
    running: bool,
    /// The number of the latest batch; the rows of any other are ignored.
    run: u64,
    /// Aborts the batch under way, also when the pane is dropped.
    stop: Option<Handle>,
    error: Option<String>,
    results: Vec<DnsBatchItemResult>,
    /// The hostnames of the batch.
    total: usize,
    summary: DnsBatchSummary,
}

impl Default for DnsBatchPane {
    fn default() -> Self {
        Self {
            editor: text_editor::Content::with_text(
                "google.com\ncloudflare.com\ngithub.com\nwikipedia.org",
            ),
            running: false,
            run: 0,
            stop: None,
            error: None,
            results: Vec::new(),
            total: 0,
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

/// Why a row did not resolve, in the user's words.
fn failure_text(failure: &DnsBatchFailure) -> String {
    match failure {
        DnsBatchFailure::NoAddress => fl!("ui-tool-dnsbatch-no-address"),
        DnsBatchFailure::TimedOut => fl!("ui-tool-dnsbatch-timed-out"),
        DnsBatchFailure::Failed(why) => why.clone(),
    }
}

/// Batch `run` of `hostnames`, resolved [`dns_batch::MAX_CONCURRENT_LOOKUPS`] at a time.
fn batch_run(tab: TabId, run: u64, hostnames: Vec<String>) -> (Task<Message>, Handle) {
    let events = iced::stream::channel(EVENT_BUFFER, async move |mut out| {
        let mut rows = iced::futures::stream::iter(hostnames)
            .map(net_tools::dns::resolve_batch_item)
            .buffered(dns_batch::MAX_CONCURRENT_LOOKUPS)
            .ready_chunks(ROW_BATCH);
        while let Some(resolved) = rows.next().await {
            if out
                .send(DnsBatchMessage::Resolved(run, resolved))
                .await
                .is_err()
            {
                return;
            }
        }
        let _ = out.send(DnsBatchMessage::Ended(run)).await;
    });
    Task::run(events, move |message| {
        Message::Tool(tab, super::ToolMessage::DnsBatch(message))
    })
    .abortable()
}

impl DnsBatchPane {
    /// What its first box holds, for the tests of what a tool opens on.
    #[cfg(test)]
    pub(super) fn first_box(&self) -> String {
        super::box_text(&self.editor)
    }

    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: DnsBatchMessage) -> Outcome {
        if !matches!(message, DnsBatchMessage::Resolve) {
            return self.apply(message);
        }
        let Some(hostnames) = self.start() else {
            return Outcome::None;
        };
        let (task, handle) = batch_run(tab, self.run, hostnames);
        self.stop = Some(handle.abort_on_drop());
        Outcome::Task(task)
    }

    /// Handles every message but [`DnsBatchMessage::Resolve`], which needs the tab for its
    /// task.
    fn apply(&mut self, message: DnsBatchMessage) -> Outcome {
        match message {
            DnsBatchMessage::EditorAction(action) => {
                if !self.running || !action.is_edit() {
                    self.editor.perform(action);
                }
            }
            DnsBatchMessage::Resolve => {}
            DnsBatchMessage::Stop => self.finish(),
            DnsBatchMessage::Resolved(run, rows) => {
                if run == self.run && self.running {
                    self.results.extend(rows);
                    self.summary = dns_batch::compute_summary(&self.results);
                }
            }
            DnsBatchMessage::Ended(run) => {
                if run == self.run {
                    self.finish();
                }
            }
            DnsBatchMessage::CopyCsv => {
                if !self.results.is_empty() {
                    let csv = dns_batch::format_batch_csv(&self.results);
                    return Outcome::Copy(CopySlot::DnsBatchCsv, csv);
                }
            }
        }
        Outcome::None
    }

    /// A new batch's hostnames, the pane reset for it; `None` while a batch goes on, or when
    /// the list is empty or too long, said in the error card.
    fn start(&mut self) -> Option<Vec<String>> {
        if self.running {
            return None;
        }
        let hostnames = dns_batch::parse_hostnames(&self.editor.text());
        if hostnames.is_empty() {
            self.error = Some(fl!("ui-tool-dnsbatch-error-required"));
            return None;
        }
        if hostnames.len() > dns_batch::MAX_BATCH_HOSTS {
            self.error = Some(fl!(
                "ui-tool-dnsbatch-error-too-many",
                count = hostnames.len().to_string(),
                max = dns_batch::MAX_BATCH_HOSTS.to_string()
            ));
            return None;
        }
        self.run = self.run.wrapping_add(1);
        self.running = true;
        self.error = None;
        self.results.clear();
        self.total = hostnames.len();
        self.summary = DnsBatchSummary::default();
        Some(hostnames)
    }

    /// The batch over: stopped, its task aborted.
    fn finish(&mut self) {
        self.running = false;
        if let Some(stop) = self.stop.take() {
            stop.abort();
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
            super::action_button(
                fl!("ui-tool-dnsbatch-stop"),
                false,
                Some(send(DnsBatchMessage::Stop)),
            )
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
                container(
                    text(err.as_str())
                        .size(font_size::BODY)
                        .style(styles::error_text),
                )
                .padding(spacing::MD)
                .style(styles::card),
            );
        }

        if self.running {
            body = body.push(
                text(fl!(
                    "ui-tool-dnsbatch-progress",
                    done = self.results.len().to_string(),
                    total = self.total.to_string()
                ))
                .size(font_size::CAPTION)
                .style(text::secondary),
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
                        .style(styles::success_text)
                        .width(Length::Fixed(90.0)),
                    DnsBatchItemStatus::Failed => text(fl!("ui-tool-dnsbatch-status-failed"))
                        .size(font_size::BODY)
                        .style(styles::error_text)
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
                        .as_ref()
                        .map_or_else(|| fl!("ui-tool-dnsbatch-status-failed"), failure_text)
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
                    text(fl!("ui-tool-dnsbatch-time", ms = r.elapsed_ms.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use heimdall_core::tools::dns_batch::failed_item;

    fn listing(text: &str) -> DnsBatchPane {
        DnsBatchPane {
            editor: text_editor::Content::with_text(text),
            ..DnsBatchPane::default()
        }
    }

    fn started(pane: &mut DnsBatchPane) -> Handle {
        assert!(pane.start().is_some(), "{:?}", pane.error);
        let (_, handle) = Task::<()>::none().abortable();
        let watcher = handle.clone();
        pane.stop = Some(handle.abort_on_drop());
        watcher
    }

    #[test]
    fn stop_cancels_the_batch_and_its_late_rows_are_dropped() {
        let mut pane = listing("a.example\nb.example");
        let handle = started(&mut pane);
        assert_eq!(pane.start(), None, "one batch at a time");
        pane.apply(DnsBatchMessage::Stop);
        assert!(!pane.running && handle.is_aborted());
        pane.apply(DnsBatchMessage::Resolved(
            pane.run,
            vec![failed_item("a.example", DnsBatchFailure::TimedOut, 1)],
        ));
        assert!(pane.results.is_empty());
    }

    #[test]
    fn rows_of_an_earlier_batch_never_land_in_a_later_one() {
        let mut pane = listing("a.example");
        started(&mut pane);
        let first = pane.run;
        pane.apply(DnsBatchMessage::Stop);
        started(&mut pane);
        pane.apply(DnsBatchMessage::Resolved(
            first,
            vec![failed_item("a.example", DnsBatchFailure::TimedOut, 1)],
        ));
        pane.apply(DnsBatchMessage::Ended(first));
        assert!(pane.results.is_empty() && pane.running);
    }

    #[test]
    fn a_list_too_long_is_refused_not_cut() {
        let hosts: Vec<String> = (0..=dns_batch::MAX_BATCH_HOSTS)
            .map(|n| format!("host{n}.example"))
            .collect();
        let mut pane = listing(&hosts.join("\n"));
        assert_eq!(pane.start(), None);
        assert!(!pane.running);
        assert_eq!(
            pane.error,
            Some(fl!(
                "ui-tool-dnsbatch-error-too-many",
                count = (dns_batch::MAX_BATCH_HOSTS + 1).to_string(),
                max = dns_batch::MAX_BATCH_HOSTS.to_string()
            ))
        );
    }

    #[test]
    fn an_ipv6_address_is_resolved_whole() {
        let mut pane = listing("2001:db8::1");
        assert_eq!(pane.start(), Some(vec!["2001:db8::1".to_owned()]));
    }
}
