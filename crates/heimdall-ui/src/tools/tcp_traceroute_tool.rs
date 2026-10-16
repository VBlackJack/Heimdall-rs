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

//! The TCP Traceroute tool view: network path discovery, hop latency,
//! and CSV export.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use heimdall_app::TabId;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::net_command::RunEnd;
use heimdall_core::tools::tcp_traceroute::{
    self, DEFAULT_MAX_HOPS, HopStatus, TraceHopResult, TraceInputError,
};

use super::network_run::{RunSlot, blocking_run};
use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the TCP traceroute view.
#[derive(Debug, Clone)]
pub enum TcpTracerouteMessage {
    /// Target host input changed.
    HostChanged(String),
    /// Max hops input changed.
    MaxHopsChanged(String),
    /// Start traceroute.
    Start,
    /// Stop trace.
    Stop,
    /// A hop of run `.0` was printed.
    Hop(u64, TraceHopResult),
    /// Run `.0` ended as `.1` says; `None` when its work failed.
    Ended(u64, Option<RunEnd>),
    /// Copy CSV export.
    CopyCsv,
}

/// The TCP Traceroute tool pane state.
#[derive(Debug)]
pub struct TcpTraceroutePane {
    host: String,
    max_hops: String,
    run: RunSlot,
    error: Option<String>,
    /// How the last run ended, when it says something: stopped, past its deadline.
    notice: Option<String>,
    hops: Vec<TraceHopResult>,
}

impl Default for TcpTraceroutePane {
    fn default() -> Self {
        Self {
            host: String::new(),
            max_hops: DEFAULT_MAX_HOPS.to_string(),
            run: RunSlot::default(),
            error: None,
            notice: None,
            hops: Vec::new(),
        }
    }
}

/// Outcome of updating the TCP Traceroute tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

/// What a message asks of the pane, before its tab is known.
#[derive(Debug)]
enum Step {
    None,
    /// Trace `.2` up to `.3` hops as run `.0`, watching stop `.1`.
    Trace(u64, Arc<AtomicBool>, String, usize),
    Copy(CopySlot, String),
}

impl TcpTraceroutePane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: TcpTracerouteMessage) -> Outcome {
        match self.apply(message) {
            Step::None => Outcome::None,
            Step::Copy(slot, content) => Outcome::Copy(slot, content),
            Step::Trace(run, stop, host, max_hops) => {
                let stream = blocking_run(
                    stop,
                    move |stop, hop| tcp_traceroute::run_traceroute(&host, max_hops, stop, hop),
                    move |hop: TraceHopResult| TcpTracerouteMessage::Hop(run, hop),
                    move |end| TcpTracerouteMessage::Ended(run, end),
                );
                let task = Task::run(stream, move |message| {
                    Message::Tool(tab, super::ToolMessage::TcpTrace(message))
                });
                Outcome::Task(self.run.attach(task))
            }
        }
    }

    /// What `message` changes, and what it asks.
    fn apply(&mut self, message: TcpTracerouteMessage) -> Step {
        match message {
            TcpTracerouteMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
                Step::None
            }
            TcpTracerouteMessage::MaxHopsChanged(val) => {
                self.max_hops = val;
                self.error = None;
                Step::None
            }
            TcpTracerouteMessage::Start => {
                if self.run.running() {
                    return Step::None;
                }
                let inputs = match tcp_traceroute::validate_inputs(&self.host, &self.max_hops) {
                    Ok(inputs) => inputs,
                    Err(TraceInputError::HostRequired) => {
                        self.error = Some(fl!("ui-tool-tcptrace-error-host-required"));
                        return Step::None;
                    }
                    Err(TraceInputError::InvalidHost) => {
                        self.error = Some(fl!("ui-tool-tcptrace-error-invalid-host"));
                        return Step::None;
                    }
                };
                // The hops kept, as the C# writes the value used back in its box.
                self.max_hops = inputs.max_hops.to_string();
                self.error = None;
                self.notice = None;
                self.hops.clear();
                let (run, stop) = self.run.begin();
                Step::Trace(run, stop, inputs.host, inputs.max_hops)
            }
            TcpTracerouteMessage::Stop => {
                if self.run.running() {
                    self.run.stop();
                    self.notice = Some(fl!("ui-tool-tcptrace-stopped"));
                }
                Step::None
            }
            TcpTracerouteMessage::Hop(run, hop) => {
                if self.run.is_current(run) {
                    self.hops.push(hop);
                }
                Step::None
            }
            TcpTracerouteMessage::Ended(run, end) => {
                if !self.run.finish(run) {
                    return Step::None;
                }
                match end {
                    Some(RunEnd::Exited) if self.hops.is_empty() => {
                        self.notice = Some(fl!("ui-tool-tcptrace-no-hops"));
                    }
                    Some(RunEnd::Exited | RunEnd::Stopped) => {}
                    Some(RunEnd::TimedOut) => {
                        self.notice = Some(fl!("ui-tool-tcptrace-timed-out"));
                    }
                    Some(RunEnd::NotStarted) => {
                        self.error = Some(fl!("ui-tool-tcptrace-error-not-found"));
                    }
                    None => self.error = Some(fl!("ui-tool-tcptrace-error-failed")),
                }
                Step::None
            }
            TcpTracerouteMessage::CopyCsv => {
                if self.hops.is_empty() {
                    return Step::None;
                }
                let header = [
                    fl!("ui-tool-tcptrace-col-hop"),
                    fl!("ui-tool-tcptrace-col-address"),
                    fl!("ui-tool-tcptrace-col-hostname"),
                    fl!("ui-tool-tcptrace-col-latency"),
                    fl!("ui-tool-tcptrace-col-status"),
                ];
                let csv = tcp_traceroute::build_export_csv(
                    &self.hops,
                    header.each_ref().map(String::as_str),
                    status_label,
                );
                Step::Copy(CopySlot::TcpTraceCsv, csv)
            }
        }
    }

    /// Renders the TCP Traceroute tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::TcpTrace(msg));

        let host_input = text_input(&fl!("ui-tool-tcptrace-host-watermark"), &self.host)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(TcpTracerouteMessage::HostChanged(v)))
            .on_submit(send(TcpTracerouteMessage::Start))
            .width(Length::FillPortion(3));

        let max_hops_input = text_input(&DEFAULT_MAX_HOPS.to_string(), &self.max_hops)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(TcpTracerouteMessage::MaxHopsChanged(v)))
            .width(Length::FillPortion(1));

        let action_btn = if self.run.running() {
            super::action_button(
                fl!("ui-tool-tcptrace-stop"),
                true,
                Some(send(TcpTracerouteMessage::Stop)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-tcptrace-start"),
                true,
                Some(send(TcpTracerouteMessage::Start)),
            )
        };

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-tcptrace-host")), host_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(3)),
            column![
                super::field_label(fl!("ui-tool-tcptrace-max-hops")),
                max_hops_input
            ]
            .spacing(spacing::XS)
            .width(Length::FillPortion(1)),
            container(action_btn).align_y(Alignment::End),
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
        if let Some(notice) = &self.notice {
            body = body.push(
                container(
                    text(notice.as_str())
                        .size(font_size::BODY)
                        .style(text::secondary),
                )
                .padding(spacing::MD)
                .style(styles::card),
            );
        }

        if !self.hops.is_empty() {
            let copy_csv_btn = super::copy_button(
                fl!("ui-tool-tcptrace-copy-csv"),
                state.copied(CopySlot::TcpTraceCsv),
                send(TcpTracerouteMessage::CopyCsv),
                super::COPY_PADDING,
            );

            let header_row = row![
                text(fl!("ui-tool-tcptrace-hop-count", count = self.hops.len()))
                    .size(font_size::SUBTITLE)
                    .width(Length::Fill),
                copy_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let table_headers = row![
                text(fl!("ui-tool-tcptrace-col-hop"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(50.0)),
                text(fl!("ui-tool-tcptrace-col-address"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-tcptrace-col-hostname"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-tcptrace-col-latency"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
                text(fl!("ui-tool-tcptrace-col-status"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
            ]
            .spacing(spacing::SM);

            let mut hop_rows = column![table_headers].spacing(spacing::XS);

            for h in &self.hops {
                let style = match h.status {
                    HopStatus::Reply => text::success,
                    HopStatus::Timeout => text::secondary,
                    HopStatus::Destination => text::primary,
                };
                let status_el = text(status_label(h.status))
                    .size(font_size::BODY)
                    .style(style)
                    .width(Length::Fixed(90.0));

                let r = row![
                    text(h.hop.to_string())
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(50.0)),
                    text(&h.address)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(if h.hostname.is_empty() {
                        "-"
                    } else {
                        &h.hostname
                    })
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .width(Length::FillPortion(2)),
                    text(&h.latency)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                    status_el,
                ]
                .spacing(spacing::SM);

                hop_rows = hop_rows.push(r);
            }

            let card = container(column![header_row, hop_rows].spacing(spacing::MD))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        }

        super::content_column(body)
    }
}

/// A hop's status, as the view and the CSV say it.
fn status_label(status: HopStatus) -> String {
    match status {
        HopStatus::Reply => fl!("ui-tool-tcptrace-status-reply"),
        HopStatus::Timeout => fl!("ui-tool-tcptrace-status-timeout"),
        HopStatus::Destination => fl!("ui-tool-tcptrace-status-destination"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hop(n: usize) -> TraceHopResult {
        TraceHopResult {
            hop: n,
            address: format!("10.0.0.{n}"),
            hostname: String::new(),
            latency: "1 ms".to_owned(),
            status: HopStatus::Reply,
        }
    }

    fn started(pane: &mut TcpTraceroutePane) -> (u64, Arc<AtomicBool>) {
        pane.apply(TcpTracerouteMessage::HostChanged("10.0.0.9".to_owned()));
        match pane.apply(TcpTracerouteMessage::Start) {
            Step::Trace(run, stop, host, hops) => {
                assert_eq!((host.as_str(), hops), ("10.0.0.9", DEFAULT_MAX_HOPS));
                (run, stop)
            }
            other => panic!("no trace: {other:?}"),
        }
    }

    #[test]
    fn hops_come_as_they_are_printed_and_the_csv_holds_them() {
        let mut pane = TcpTraceroutePane::default();
        let (run, _) = started(&mut pane);
        pane.apply(TcpTracerouteMessage::Hop(run, hop(1)));
        pane.apply(TcpTracerouteMessage::Hop(run, hop(2)));
        assert_eq!(pane.hops.len(), 2, "shown before the trace ends");
        assert!(
            matches!(pane.apply(TcpTracerouteMessage::Start), Step::None),
            "one run at a time"
        );
        pane.apply(TcpTracerouteMessage::Ended(run, Some(RunEnd::Exited)));
        assert!(!pane.run.running());
        let Step::Copy(CopySlot::TcpTraceCsv, csv) = pane.apply(TcpTracerouteMessage::CopyCsv)
        else {
            panic!("a CSV");
        };
        assert_eq!(csv.lines().count(), 3);
    }

    #[test]
    fn stop_really_stops_and_a_late_hop_of_the_stopped_run_is_let_go() {
        let mut pane = TcpTraceroutePane::default();
        let (run, stop) = started(&mut pane);
        pane.apply(TcpTracerouteMessage::Hop(run, hop(1)));
        pane.apply(TcpTracerouteMessage::Stop);
        assert!(
            stop.load(std::sync::atomic::Ordering::Relaxed),
            "the work told"
        );
        assert!(!pane.run.running());
        pane.apply(TcpTracerouteMessage::Hop(run, hop(2)));
        pane.apply(TcpTracerouteMessage::Ended(run, Some(RunEnd::Stopped)));
        assert_eq!(pane.hops.len(), 1);
        assert!(pane.notice.is_some());
        // A new run: the old one's messages never reach it.
        let (second, _) = started(&mut pane);
        assert_ne!(second, run);
        pane.apply(TcpTracerouteMessage::Hop(run, hop(9)));
        assert!(pane.hops.is_empty());
    }

    #[test]
    fn a_host_that_could_be_an_option_or_a_missing_program_is_said() {
        let mut pane = TcpTraceroutePane::default();
        pane.apply(TcpTracerouteMessage::HostChanged("-h 1".to_owned()));
        assert!(matches!(
            pane.apply(TcpTracerouteMessage::Start),
            Step::None
        ));
        assert!(pane.error.is_some());
        let (run, _) = started(&mut pane);
        pane.apply(TcpTracerouteMessage::Ended(run, Some(RunEnd::NotStarted)));
        assert!(pane.error.is_some());
        let (run, _) = started(&mut pane);
        pane.apply(TcpTracerouteMessage::Ended(run, Some(RunEnd::TimedOut)));
        assert!(pane.notice.is_some());
    }
}
