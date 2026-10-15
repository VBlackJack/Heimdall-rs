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

//! The ICMP Ping tool view: continuous latency monitor with live running
//! statistics, response log, and CSV export.

use heimdall_app::TabId;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::ping::{self, PingInputs, PingProbeResult, PingStatsSnapshot};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the ICMP Ping tool view.
#[derive(Debug, Clone)]
pub enum PingMessage {
    /// Target host input changed.
    HostChanged(String),
    /// Timeout input changed.
    TimeoutChanged(String),
    /// Count input changed.
    CountChanged(String),
    /// Start ping sequence.
    Start,
    /// Stop running ping sequence.
    Stop,
    /// Probes batch completed.
    Finished(Vec<PingProbeResult>),
    /// Copy summary text.
    CopyResults,
    /// Copy CSV export.
    CopyCsv,
}

/// The ICMP Ping tool pane state.
#[derive(Debug)]
pub struct PingPane {
    host: String,
    timeout: String,
    count: String,
    running: bool,
    error: Option<String>,
    results: Vec<PingProbeResult>,
    stats: PingStatsSnapshot,
}

impl Default for PingPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            timeout: ping::DEFAULT_PING_TIMEOUT_MS.to_string(),
            count: "4".to_string(),
            running: false,
            error: None,
            results: Vec::new(),
            stats: PingStatsSnapshot::default(),
        }
    }
}

/// Outcome of updating the Ping tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl PingPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: PingMessage) -> Outcome {
        match message {
            PingMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
                Outcome::None
            }
            PingMessage::TimeoutChanged(val) => {
                self.timeout = val;
                self.error = None;
                Outcome::None
            }
            PingMessage::CountChanged(val) => {
                self.count = val;
                self.error = None;
                Outcome::None
            }
            PingMessage::Start => {
                let host = self.host.trim();
                if host.is_empty() {
                    self.error = Some(fl!("ui-tool-ping-error-host-required"));
                    return Outcome::None;
                }
                let timeout_ms = match self.timeout.trim().parse::<u64>() {
                    Ok(t) if (100..=30000).contains(&t) => t,
                    _ => {
                        self.error = Some(fl!("ui-tool-ping-error-timeout-invalid"));
                        return Outcome::None;
                    }
                };
                let count = match self.count.trim().parse::<usize>() {
                    Ok(c) if c <= 100000 => c,
                    _ => {
                        self.error = Some(fl!("ui-tool-ping-error-count-invalid"));
                        return Outcome::None;
                    }
                };
                let inputs = PingInputs {
                    host: host.to_owned(),
                    timeout_ms,
                    count,
                };

                self.running = true;
                self.error = None;
                self.results.clear();
                self.stats = PingStatsSnapshot::default();

                let task_host = inputs.host.clone();
                let task_timeout = inputs.timeout_ms;
                let task_count = if inputs.count == 0 {
                    4
                } else {
                    inputs.count.min(100)
                };

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut list = Vec::with_capacity(task_count);
                            for seq in 1..=task_count {
                                let res = ping::probe_ping(&task_host, seq, task_timeout);
                                list.push(res);
                            }
                            list
                        })
                        .await
                        .unwrap_or_default()
                    },
                    move |results| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::Ping(PingMessage::Finished(results)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            PingMessage::Stop => {
                self.running = false;
                Outcome::None
            }
            PingMessage::Finished(res) => {
                self.running = false;
                self.stats = ping::compute_stats(&res);
                self.results = res;
                Outcome::None
            }
            PingMessage::CopyResults => {
                if self.results.is_empty() {
                    return Outcome::None;
                }
                let mut buf = String::new();
                buf.push_str(&format!(
                    "--- Ping Statistics for {} ---\nSent = {}, Received = {}, Lost = {} ({:.1}% loss)\nMin = {:.1}ms, Max = {:.1}ms, Avg = {:.1}ms, Jitter = {:.1}ms\n\n",
                    self.host,
                    self.stats.sent,
                    self.stats.received,
                    self.stats.lost,
                    self.stats.loss_percent,
                    self.stats.min_ms,
                    self.stats.max_ms,
                    self.stats.avg_ms,
                    self.stats.jitter_ms
                ));
                for r in &self.results {
                    buf.push_str(&format!(
                        "[{}] Reply from {}: status={}, time={:.1}ms, TTL={}\n",
                        r.timestamp,
                        r.ip_or_host,
                        r.status.label(),
                        r.latency_ms,
                        r.ttl
                    ));
                }
                Outcome::Copy(CopySlot::PingResults, buf)
            }
            PingMessage::CopyCsv => {
                if self.results.is_empty() {
                    return Outcome::None;
                }
                let csv = ping::build_csv_export(&self.results);
                Outcome::Copy(CopySlot::PingCsv, csv)
            }
        }
    }

    /// Renders the Ping tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::Ping(msg));

        let host_input = text_input(&fl!("ui-tool-ping-host-watermark"), &self.host)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(PingMessage::HostChanged(v)))
            .on_submit(send(PingMessage::Start))
            .width(Length::FillPortion(3));

        let timeout_input = text_input(&fl!("ui-tool-ping-timeout-watermark"), &self.timeout)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(PingMessage::TimeoutChanged(v)))
            .width(Length::FillPortion(1));

        let count_input = text_input(&fl!("ui-tool-ping-count-watermark"), &self.count)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(PingMessage::CountChanged(v)))
            .width(Length::FillPortion(1));

        let start_button = if self.running {
            super::action_button(
                fl!("ui-tool-ping-stop"),
                false,
                Some(send(PingMessage::Stop)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-ping-start"),
                true,
                Some(send(PingMessage::Start)),
            )
        };

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-ping-host")), host_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(3)),
            column![
                super::field_label(fl!("ui-tool-ping-timeout")),
                timeout_input
            ]
            .spacing(spacing::XS)
            .width(Length::FillPortion(1)),
            column![super::field_label(fl!("ui-tool-ping-count")), count_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(1)),
            container(start_button).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::End);

        let input_card = container(column![inputs_row])
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

        // Summary cards
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
                    fl!("ui-tool-ping-min"),
                    format!("{:.1} ms", self.stats.min_ms)
                ),
                metric_card(
                    fl!("ui-tool-ping-avg"),
                    format!("{:.1} ms", self.stats.avg_ms)
                ),
                metric_card(
                    fl!("ui-tool-ping-max"),
                    format!("{:.1} ms", self.stats.max_ms)
                ),
                metric_card(
                    fl!("ui-tool-ping-jitter"),
                    format!("{:.1} ms", self.stats.jitter_ms)
                ),
                metric_card(
                    fl!("ui-tool-ping-loss"),
                    format!("{:.1} %", self.stats.loss_percent)
                ),
                metric_card(
                    fl!("ui-tool-ping-sent"),
                    format!("{}/{}", self.stats.received, self.stats.sent)
                ),
            ]
            .spacing(spacing::SM);

            let copy_results_btn = super::copy_button(
                fl!("ui-tool-ping-copy-results"),
                state.copied(CopySlot::PingResults),
                send(PingMessage::CopyResults),
                super::COPY_PADDING,
            );

            let copy_csv_btn = super::copy_button(
                fl!("ui-tool-ping-copy-csv"),
                state.copied(CopySlot::PingCsv),
                send(PingMessage::CopyCsv),
                super::COPY_PADDING,
            );

            let header_row = row![
                text(fl!("ui-tool-ping-results-title"))
                    .size(font_size::SUBTITLE)
                    .width(Length::Fill),
                copy_results_btn,
                copy_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let mut list_col = column![].spacing(spacing::XS);
            for r in &self.results {
                let status_cell = match r.status {
                    ping::PingStatus::Success => text(format!("{:.1} ms", r.latency_ms))
                        .size(font_size::BODY)
                        .style(text::success)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(1)),
                    ping::PingStatus::Timeout => text(fl!("ui-tool-ping-status-timeout"))
                        .size(font_size::BODY)
                        .style(text::danger)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(1)),
                    ping::PingStatus::Error => text(fl!("ui-tool-ping-status-error"))
                        .size(font_size::BODY)
                        .style(text::warning)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(1)),
                };

                let item_row = row![
                    text(format!("#{}", r.seq))
                        .size(font_size::BODY)
                        .style(text::secondary)
                        .width(Length::Fixed(40.0)),
                    text(&r.timestamp)
                        .size(font_size::BODY)
                        .style(text::secondary)
                        .width(Length::Fixed(80.0)),
                    text(&r.ip_or_host)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    status_cell,
                    text(format!("TTL: {}", r.ttl))
                        .size(font_size::CAPTION)
                        .style(text::secondary)
                        .width(Length::Fixed(80.0)),
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
