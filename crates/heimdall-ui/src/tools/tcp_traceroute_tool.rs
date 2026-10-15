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

use heimdall_app::TabId;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::tcp_traceroute::{self, DEFAULT_MAX_HOPS, HopStatus, TraceHopResult};

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
    /// Trace finished.
    Finished(Vec<TraceHopResult>),
    /// Copy CSV export.
    CopyCsv,
}

/// The TCP Traceroute tool pane state.
#[derive(Debug)]
pub struct TcpTraceroutePane {
    host: String,
    max_hops: String,
    running: bool,
    error: Option<String>,
    hops: Vec<TraceHopResult>,
}

impl Default for TcpTraceroutePane {
    fn default() -> Self {
        Self {
            host: String::new(),
            max_hops: DEFAULT_MAX_HOPS.to_string(),
            running: false,
            error: None,
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

impl TcpTraceroutePane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: TcpTracerouteMessage) -> Outcome {
        match message {
            TcpTracerouteMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
                Outcome::None
            }
            TcpTracerouteMessage::MaxHopsChanged(val) => {
                self.max_hops = val;
                self.error = None;
                Outcome::None
            }
            TcpTracerouteMessage::Start => {
                let inputs = match tcp_traceroute::validate_inputs(&self.host, &self.max_hops) {
                    Ok(inp) => inp,
                    Err(_) => {
                        self.error = Some(fl!("ui-tool-tcptrace-error-host-required"));
                        return Outcome::None;
                    }
                };

                self.running = true;
                self.error = None;
                self.hops.clear();

                let task_host = inputs.host;
                let task_max_hops = inputs.max_hops;

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            tcp_traceroute::run_traceroute_command(&task_host, task_max_hops)
                        })
                        .await
                        .unwrap_or_default()
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::TcpTrace(TcpTracerouteMessage::Finished(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            TcpTracerouteMessage::Stop => {
                self.running = false;
                Outcome::None
            }
            TcpTracerouteMessage::Finished(hops) => {
                self.running = false;
                self.hops = hops;
                Outcome::None
            }
            TcpTracerouteMessage::CopyCsv => {
                if !self.hops.is_empty() {
                    let csv = tcp_traceroute::build_export_csv(&self.hops);
                    Outcome::Copy(CopySlot::TcpTraceCsv, csv)
                } else {
                    Outcome::None
                }
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

        let max_hops_input = text_input("30", &self.max_hops)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(TcpTracerouteMessage::MaxHopsChanged(v)))
            .width(Length::FillPortion(1));

        let action_btn = if self.running {
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

        if !self.hops.is_empty() {
            let copy_csv_btn = super::copy_button(
                fl!("ui-tool-tcptrace-copy-csv"),
                state.copied(CopySlot::TcpTraceCsv),
                send(TcpTracerouteMessage::CopyCsv),
                super::COPY_PADDING,
            );

            let header_row = row![
                text(format!(
                    "{} ({} hops)",
                    fl!("ui-tool-tcptrace-name"),
                    self.hops.len()
                ))
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
                let status_el = match h.status {
                    HopStatus::Reply => text(fl!("ui-tool-tcptrace-status-reply"))
                        .size(font_size::BODY)
                        .style(text::success)
                        .width(Length::Fixed(90.0)),
                    HopStatus::Timeout => text(fl!("ui-tool-tcptrace-status-timeout"))
                        .size(font_size::BODY)
                        .style(text::secondary)
                        .width(Length::Fixed(90.0)),
                    HopStatus::Destination => text(fl!("ui-tool-tcptrace-status-destination"))
                        .size(font_size::BODY)
                        .style(text::primary)
                        .width(Length::Fixed(90.0)),
                };

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
