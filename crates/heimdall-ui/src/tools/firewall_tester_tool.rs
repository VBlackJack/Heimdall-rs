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

//! The Firewall Egress Tester tool view: outbound egress port connectivity testing,
//! latency measurement, matrix summary, and CSV export.

use heimdall_app::TabId;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::firewall_tester::{self, FwProbeResult, FwProbeStatus, FwProbeSummary};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the firewall tester view.
#[derive(Debug, Clone)]
pub enum FirewallTesterMessage {
    /// Target hosts input changed.
    HostsChanged(String),
    /// Ports input changed.
    PortsChanged(String),
    /// Start test.
    Start,
    /// Stop test.
    Stop,
    /// Test finished.
    Finished(Vec<FwProbeResult>),
    /// Copy matrix CSV.
    CopyCsv,
}

/// The Firewall Egress Tester tool pane state.
#[derive(Debug)]
pub struct FirewallTesterPane {
    hosts: String,
    ports: String,
    running: bool,
    error: Option<String>,
    results: Vec<FwProbeResult>,
    summary: FwProbeSummary,
}

impl Default for FirewallTesterPane {
    fn default() -> Self {
        Self {
            hosts: String::new(),
            ports: "80,443,8080".to_string(),
            running: false,
            error: None,
            results: Vec::new(),
            summary: FwProbeSummary {
                open: 0,
                closed: 0,
                timeout: 0,
                total: 0,
            },
        }
    }
}

/// Outcome of updating the Firewall Tester tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl FirewallTesterPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: FirewallTesterMessage) -> Outcome {
        match message {
            FirewallTesterMessage::HostsChanged(val) => {
                self.hosts = val;
                self.error = None;
                Outcome::None
            }
            FirewallTesterMessage::PortsChanged(val) => {
                self.ports = val;
                self.error = None;
                Outcome::None
            }
            FirewallTesterMessage::Start => {
                let hosts = firewall_tester::parse_hosts(&self.hosts);
                if hosts.is_empty() {
                    self.error = Some(fl!("ui-tool-fwtest-error-hosts-required"));
                    return Outcome::None;
                }

                let ports = firewall_tester::parse_ports(&self.ports);
                let ports = if ports.is_empty() {
                    vec![80, 443]
                } else {
                    ports
                };

                self.running = true;
                self.error = None;
                self.results.clear();

                let task_hosts = hosts;
                let task_ports = ports;

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut list = Vec::new();
                            for h in &task_hosts {
                                for &p in &task_ports {
                                    list.push(firewall_tester::probe_host_port(h, p, 1500));
                                }
                            }
                            list
                        })
                        .await
                        .unwrap_or_default()
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::FwTest(FirewallTesterMessage::Finished(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            FirewallTesterMessage::Stop => {
                self.running = false;
                Outcome::None
            }
            FirewallTesterMessage::Finished(res) => {
                self.running = false;
                self.summary = firewall_tester::compute_summary(&res);
                self.results = res;
                Outcome::None
            }
            FirewallTesterMessage::CopyCsv => {
                let hosts = firewall_tester::parse_hosts(&self.hosts);
                let ports = firewall_tester::parse_ports(&self.ports);
                if !self.results.is_empty() {
                    let csv = firewall_tester::build_matrix_csv(&self.results, &hosts, &ports);
                    Outcome::Copy(CopySlot::FwTestCsv, csv)
                } else {
                    Outcome::None
                }
            }
        }
    }

    /// Renders the Firewall Egress Tester tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::FwTest(msg));

        let hosts_input = text_input(&fl!("ui-tool-fwtest-hosts-watermark"), &self.hosts)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(FirewallTesterMessage::HostsChanged(v)))
            .width(Length::FillPortion(3));

        let ports_input = text_input(&fl!("ui-tool-fwtest-ports-watermark"), &self.ports)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(FirewallTesterMessage::PortsChanged(v)))
            .width(Length::FillPortion(1));

        let action_btn = if self.running {
            super::action_button(
                fl!("ui-tool-fwtest-stop"),
                true,
                Some(send(FirewallTesterMessage::Stop)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-fwtest-start"),
                true,
                Some(send(FirewallTesterMessage::Start)),
            )
        };

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-fwtest-hosts")), hosts_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(3)),
            column![super::field_label(fl!("ui-tool-fwtest-ports")), ports_input]
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

        if !self.results.is_empty() {
            let summary_cards = row![
                container(
                    column![
                        text(fl!("ui-tool-fwtest-total"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.total.to_string())
                            .size(font_size::TITLE)
                            .font(super::BOX_FONT),
                    ]
                    .spacing(spacing::XS)
                )
                .padding(spacing::SM)
                .style(styles::card)
                .width(Length::FillPortion(1)),
                container(
                    column![
                        text(fl!("ui-tool-fwtest-open"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.open.to_string())
                            .size(font_size::TITLE)
                            .font(super::BOX_FONT)
                            .style(text::success),
                    ]
                    .spacing(spacing::XS)
                )
                .padding(spacing::SM)
                .style(styles::card)
                .width(Length::FillPortion(1)),
                container(
                    column![
                        text(fl!("ui-tool-fwtest-closed"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.closed.to_string())
                            .size(font_size::TITLE)
                            .font(super::BOX_FONT)
                            .style(text::secondary),
                    ]
                    .spacing(spacing::XS)
                )
                .padding(spacing::SM)
                .style(styles::card)
                .width(Length::FillPortion(1)),
                container(
                    column![
                        text(fl!("ui-tool-fwtest-timeout"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.timeout.to_string())
                            .size(font_size::TITLE)
                            .font(super::BOX_FONT)
                            .style(text::danger),
                    ]
                    .spacing(spacing::XS)
                )
                .padding(spacing::SM)
                .style(styles::card)
                .width(Length::FillPortion(1)),
            ]
            .spacing(spacing::SM);

            body = body.push(summary_cards);

            let copy_csv_btn = super::copy_button(
                fl!("ui-tool-fwtest-copy-csv"),
                state.copied(CopySlot::FwTestCsv),
                send(FirewallTesterMessage::CopyCsv),
                super::COPY_PADDING,
            );

            let header_row = row![
                text(fl!("ui-tool-fwtest-name"))
                    .size(font_size::SUBTITLE)
                    .width(Length::Fill),
                copy_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let table_headers = row![
                text(fl!("ui-tool-fwtest-hosts"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-fwtest-ports"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(80.0)),
                text("Status")
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(110.0)),
                text("Latency")
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
            ]
            .spacing(spacing::SM);

            let mut probe_rows = column![table_headers].spacing(spacing::XS);

            for r in &self.results {
                let status_el = match r.status {
                    FwProbeStatus::Open => text(fl!("ui-tool-fwtest-open"))
                        .size(font_size::BODY)
                        .style(text::success)
                        .width(Length::Fixed(110.0)),
                    FwProbeStatus::Closed => text(fl!("ui-tool-fwtest-closed"))
                        .size(font_size::BODY)
                        .style(text::secondary)
                        .width(Length::Fixed(110.0)),
                    FwProbeStatus::Timeout => text(fl!("ui-tool-fwtest-timeout"))
                        .size(font_size::BODY)
                        .style(text::danger)
                        .width(Length::Fixed(110.0)),
                };

                let lat_str = match r.latency_ms {
                    Some(ms) => format!("{ms} ms"),
                    None => "-".to_string(),
                };

                let row_el = row![
                    text(&r.host)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    text(r.port.to_string())
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(80.0)),
                    status_el,
                    text(lat_str)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                ]
                .spacing(spacing::SM);

                probe_rows = probe_rows.push(row_el);
            }

            let card = container(column![header_row, probe_rows].spacing(spacing::MD))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        }

        super::content_column(body)
    }
}
