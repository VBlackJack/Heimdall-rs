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

//! The ICMP Ping tool view, as the C# `PingToolView` and `PingToolViewModel`: a latency
//! monitor probing once a second, for a count or until stopped, its probes shown as they
//! come with running statistics, and copied as text or CSV.
//!
//! A run is one stream of probes, aborted by Stop or when the tab closes, which kills the
//! `ping` program under way; each probe carries its run's number, so one of an earlier run
//! never lands in a later one. Enter while a run goes on starts nothing, as the C#
//! `StartAsync` returns when `IsRunning`.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::time::Duration;

use heimdall_app::TabId;
use heimdall_app::net_tools;
use iced::futures::SinkExt as _;
use iced::task::Handle;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::ping::{
    self, PingDetail, PingInputs, PingProbeResult, PingStatsSnapshot, PingStatus,
};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Probes of the latest ones shown in the history: the statistics and the exports cover
/// every probe kept.
const MAX_SHOWN_PROBES: usize = 200;

/// Probes buffered between the run and the view.
const PROBE_BUFFER: usize = 16;

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
    /// A probe of run `.0` ended.
    Probed(u64, PingProbeResult),
    /// Run `.0` sent its count.
    Ended(u64),
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
    /// The number of the latest run; the probes of any other are ignored.
    run: u64,
    /// Aborts the run under way, also when the pane is dropped.
    stop: Option<Handle>,
    error: Option<String>,
    results: VecDeque<PingProbeResult>,
    stats: PingStatsSnapshot,
}

impl Default for PingPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            timeout: ping::DEFAULT_PING_TIMEOUT_MS.to_string(),
            count: "4".to_string(),
            running: false,
            run: 0,
            stop: None,
            error: None,
            results: VecDeque::new(),
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

/// The text of a validation error key of [`ping::validate_inputs`].
fn input_error(key: &str) -> String {
    match key {
        "ui-tool-ping-error-host-required" => fl!("ui-tool-ping-error-host-required"),
        "ui-tool-ping-error-host-invalid" => fl!("ui-tool-ping-error-host-invalid"),
        "ui-tool-ping-error-timeout-invalid" => fl!("ui-tool-ping-error-timeout-invalid"),
        _ => fl!("ui-tool-ping-error-count-invalid"),
    }
}

/// What a probe came to, in the user's words.
fn detail_text(result: &PingProbeResult) -> String {
    match &result.detail {
        PingDetail::Ok => fl!(
            "ui-tool-ping-latency",
            ms = format!("{:.1}", result.latency_ms)
        ),
        PingDetail::TimedOut => fl!("ui-tool-ping-status-timeout"),
        PingDetail::Unreachable => fl!("ui-tool-ping-detail-unreachable"),
        PingDetail::Unresolved => fl!("ui-tool-ping-detail-unresolved"),
        PingDetail::NoProgram => fl!("ui-tool-ping-detail-no-program"),
        PingDetail::Failed(why) => fl!("ui-tool-ping-detail-failed", reason = why.as_str()),
    }
}

/// The run of `inputs` numbered `run`: a probe every [`ping::PING_INTERVAL_MS`], the first at
/// once, until the count is sent or the run is aborted.
fn ping_run(tab: TabId, run: u64, inputs: PingInputs) -> (Task<Message>, Handle) {
    let probes = iced::stream::channel(PROBE_BUFFER, async move |mut out| {
        let mut ticks = tokio::time::interval(Duration::from_millis(ping::PING_INTERVAL_MS));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut seq = 0usize;
        while inputs.count == 0 || seq < inputs.count {
            ticks.tick().await;
            seq += 1;
            let result = net_tools::ping::probe(&inputs.host, seq, inputs.timeout_ms).await;
            if out.send(PingMessage::Probed(run, result)).await.is_err() {
                return;
            }
        }
        let _ = out.send(PingMessage::Ended(run)).await;
    });
    Task::run(probes, move |message| {
        Message::Tool(tab, super::ToolMessage::Ping(message))
    })
    .abortable()
}

impl PingPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: PingMessage) -> Outcome {
        if let PingMessage::Start = message {
            return match self.start() {
                Some(inputs) => {
                    let (task, handle) = ping_run(tab, self.run, inputs);
                    self.stop = Some(handle.abort_on_drop());
                    Outcome::Task(task)
                }
                None => Outcome::None,
            };
        }
        self.apply(message)
    }

    /// Handles every message but [`PingMessage::Start`], which needs the tab for its task.
    fn apply(&mut self, message: PingMessage) -> Outcome {
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
            PingMessage::Start => Outcome::None,
            PingMessage::Stop => {
                self.finish();
                Outcome::None
            }
            PingMessage::Probed(run, result) => {
                if run == self.run && self.running {
                    self.record(result);
                }
                Outcome::None
            }
            PingMessage::Ended(run) => {
                if run == self.run {
                    self.finish();
                }
                Outcome::None
            }
            PingMessage::CopyResults => {
                if self.results.is_empty() {
                    return Outcome::None;
                }
                Outcome::Copy(CopySlot::PingResults, self.summary_text())
            }
            PingMessage::CopyCsv => {
                if self.results.is_empty() {
                    return Outcome::None;
                }
                let csv = ping::build_csv_export(self.results.make_contiguous());
                Outcome::Copy(CopySlot::PingCsv, csv)
            }
        }
    }

    /// A new run's inputs, the pane reset for it; `None` while a run goes on, or when an
    /// input is refused, said in the error card.
    fn start(&mut self) -> Option<PingInputs> {
        if self.running {
            return None;
        }
        let inputs = match ping::validate_inputs(&self.host, &self.timeout, &self.count) {
            Ok(inputs) => inputs,
            Err(key) => {
                self.error = Some(input_error(key));
                return None;
            }
        };
        self.run = self.run.wrapping_add(1);
        self.running = true;
        self.error = None;
        self.results.clear();
        self.stats = PingStatsSnapshot::default();
        Some(inputs)
    }

    /// The run over: stopped, its task aborted.
    fn finish(&mut self) {
        self.running = false;
        if let Some(stop) = self.stop.take() {
            stop.abort();
        }
    }

    /// A probe of the current run kept, the oldest dropped past [`ping::MAX_HISTORY`].
    fn record(&mut self, result: PingProbeResult) {
        self.results.push_back(result);
        while self.results.len() > ping::MAX_HISTORY {
            self.results.pop_front();
        }
        self.stats = ping::compute_stats(self.results.make_contiguous());
    }

    /// The statistics and every probe, as text.
    fn summary_text(&self) -> String {
        let stats = &self.stats;
        let mut buf = String::new();
        let _ = writeln!(
            buf,
            "{}",
            fl!("ui-tool-ping-copy-header", host = self.host.trim())
        );
        let _ = writeln!(
            buf,
            "{}",
            fl!(
                "ui-tool-ping-copy-counts",
                sent = stats.sent.to_string(),
                received = stats.received.to_string(),
                lost = stats.lost.to_string(),
                loss = format!("{:.1}", stats.loss_percent)
            )
        );
        let _ = writeln!(
            buf,
            "{}",
            fl!(
                "ui-tool-ping-copy-times",
                min = format!("{:.1}", stats.min_ms),
                max = format!("{:.1}", stats.max_ms),
                avg = format!("{:.1}", stats.avg_ms),
                jitter = format!("{:.1}", stats.jitter_ms)
            )
        );
        let _ = writeln!(buf);
        for r in &self.results {
            let _ = writeln!(
                buf,
                "{}",
                fl!(
                    "ui-tool-ping-copy-line",
                    time = r.timestamp.as_str(),
                    host = r.ip_or_host.as_str(),
                    status = detail_text(r),
                    ttl = r.ttl.to_string()
                )
            );
        }
        buf
    }

    /// Renders the Ping tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::Ping(msg));

        let mut host_input = text_input(&fl!("ui-tool-ping-host-watermark"), &self.host)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .width(Length::FillPortion(3));
        let mut timeout_input = text_input(&fl!("ui-tool-ping-timeout-watermark"), &self.timeout)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .width(Length::FillPortion(1));
        let mut count_input = text_input(&fl!("ui-tool-ping-count-watermark"), &self.count)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .width(Length::FillPortion(1));
        // Read-only while a run goes on, as the C# view sets `IsReadOnly`.
        if !self.running {
            host_input = host_input
                .on_input(move |v| send(PingMessage::HostChanged(v)))
                .on_submit(send(PingMessage::Start));
            timeout_input = timeout_input.on_input(move |v| send(PingMessage::TimeoutChanged(v)));
            count_input = count_input.on_input(move |v| send(PingMessage::CountChanged(v)));
        }

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
            let latency = |ms: f64| fl!("ui-tool-ping-latency", ms = format!("{ms:.1}"));

            let metrics_row = row![
                metric_card(fl!("ui-tool-ping-min"), latency(self.stats.min_ms)),
                metric_card(fl!("ui-tool-ping-avg"), latency(self.stats.avg_ms)),
                metric_card(fl!("ui-tool-ping-max"), latency(self.stats.max_ms)),
                metric_card(fl!("ui-tool-ping-jitter"), latency(self.stats.jitter_ms)),
                metric_card(
                    fl!("ui-tool-ping-loss"),
                    fl!(
                        "ui-tool-ping-loss-value",
                        loss = format!("{:.1}", self.stats.loss_percent)
                    )
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
            let skipped = self.results.len().saturating_sub(MAX_SHOWN_PROBES);
            if skipped > 0 {
                list_col = list_col.push(
                    text(fl!(
                        "ui-tool-ping-shown-last",
                        shown = MAX_SHOWN_PROBES.to_string(),
                        total = self.results.len().to_string()
                    ))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
                );
            }
            for r in self.results.iter().skip(skipped) {
                let style = match r.status {
                    PingStatus::Success => text::success,
                    PingStatus::Timeout => text::danger,
                    PingStatus::Error => text::warning,
                };
                let status_cell = text(detail_text(r))
                    .size(font_size::BODY)
                    .style(style)
                    .font(super::BOX_FONT)
                    .width(Length::FillPortion(2));

                let item_row = row![
                    text(fl!("ui-tool-ping-seq", seq = r.seq.to_string()))
                        .size(font_size::BODY)
                        .style(text::secondary)
                        .width(Length::Fixed(60.0)),
                    text(&r.timestamp)
                        .size(font_size::BODY)
                        .style(text::secondary)
                        .width(Length::Fixed(80.0)),
                    text(&r.ip_or_host)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    status_cell,
                    text(fl!("ui-tool-ping-ttl", ttl = r.ttl.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(seq: usize) -> PingProbeResult {
        PingProbeResult::without_reply(seq, "12:00:00".into(), "host".into(), PingDetail::TimedOut)
    }

    fn filled(host: &str, count: &str) -> PingPane {
        let mut pane = PingPane::default();
        pane.apply(PingMessage::HostChanged(host.into()));
        pane.apply(PingMessage::CountChanged(count.into()));
        pane
    }

    /// The run started, its abort handle kept as `update` keeps it; returned, a handle
    /// that only watches it.
    fn started(pane: &mut PingPane) -> Handle {
        assert!(pane.start().is_some(), "{:?}", pane.error);
        let (_, handle) = Task::<()>::none().abortable();
        let watcher = handle.clone();
        pane.stop = Some(handle.abort_on_drop());
        watcher
    }

    #[test]
    fn enter_during_a_run_starts_no_second_one() {
        let mut pane = filled("192.0.2.1", "0");
        started(&mut pane);
        let run = pane.run;
        assert!(pane.running);
        assert_eq!(pane.start(), None);
        assert_eq!(pane.run, run, "the same run goes on");
    }

    #[test]
    fn stop_aborts_the_run_and_its_late_probes_are_dropped() {
        let mut pane = filled("192.0.2.1", "0");
        let handle = started(&mut pane);
        pane.apply(PingMessage::Probed(pane.run, probe(1)));
        assert_eq!(pane.results.len(), 1);
        pane.apply(PingMessage::Stop);
        assert!(!pane.running);
        assert!(handle.is_aborted(), "the probes stream is dropped");
        pane.apply(PingMessage::Probed(pane.run, probe(2)));
        assert_eq!(pane.results.len(), 1, "nothing lands after Stop");
    }

    #[test]
    fn closing_the_tool_aborts_its_run() {
        let mut pane = filled("192.0.2.1", "0");
        let handle = started(&mut pane);
        drop(pane);
        assert!(handle.is_aborted());
    }

    #[test]
    fn a_probe_of_an_earlier_run_never_lands_in_a_later_one() {
        let mut pane = filled("192.0.2.1", "3");
        started(&mut pane);
        let first = pane.run;
        pane.apply(PingMessage::Stop);
        started(&mut pane);
        pane.apply(PingMessage::Probed(first, probe(1)));
        pane.apply(PingMessage::Ended(first));
        assert!(pane.results.is_empty());
        assert!(pane.running, "the earlier run's end does not end this one");
        pane.apply(PingMessage::Probed(pane.run, probe(1)));
        pane.apply(PingMessage::Ended(pane.run));
        assert_eq!(pane.results.len(), 1);
        assert!(!pane.running);
    }

    #[test]
    fn an_invalid_input_is_said_and_starts_nothing() {
        let mut pane = filled("-t", "4");
        assert_eq!(pane.start(), None);
        assert!(!pane.running);
        assert_eq!(
            pane.error.as_deref(),
            Some(fl!("ui-tool-ping-error-host-invalid").as_str())
        );
        let mut pane = filled("192.0.2.1", "100001");
        assert_eq!(pane.start(), None);
        assert_eq!(
            pane.error.as_deref(),
            Some(fl!("ui-tool-ping-error-count-invalid").as_str()),
            "refused, not cut to 100"
        );
    }

    #[test]
    fn a_count_of_zero_runs_until_stopped() {
        let mut pane = filled("192.0.2.1", "0");
        assert_eq!(pane.start().map(|inputs| inputs.count), Some(0));
    }

    #[test]
    fn the_history_keeps_its_latest_probes() {
        let mut pane = filled("192.0.2.1", "0");
        started(&mut pane);
        pane.results = (1..=ping::MAX_HISTORY).map(probe).collect();
        pane.apply(PingMessage::Probed(pane.run, probe(ping::MAX_HISTORY + 1)));
        pane.apply(PingMessage::Probed(pane.run, probe(ping::MAX_HISTORY + 2)));
        assert_eq!(pane.results.len(), ping::MAX_HISTORY);
        assert_eq!(pane.results.front().map(|r| r.seq), Some(3));
        assert_eq!(pane.stats.sent, ping::MAX_HISTORY);
    }

    #[test]
    fn what_a_probe_came_to_is_said_in_the_users_words() {
        let failed = PingProbeResult::without_reply(
            1,
            "12:00:00".into(),
            "host".into(),
            PingDetail::Unreachable,
        );
        assert_eq!(detail_text(&failed), fl!("ui-tool-ping-detail-unreachable"));
    }
}
