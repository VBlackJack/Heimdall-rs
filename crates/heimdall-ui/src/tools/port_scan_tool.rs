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

//! The TCP Port Scanner tool view: port range parsing, concurrent TCP handshake probes,
//! service identification, status badges, summary metrics, and CSV export.

use heimdall_app::TabId;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::banner_grab;
use heimdall_core::tools::port_scanner::{
    self, DEFAULT_COMMON_PORTS, DEFAULT_PORT_TIMEOUT_MS, PortProbeResult, PortScanSummary,
    PortStatus,
};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the port scanner view.
#[derive(Debug, Clone)]
pub enum PortScanMessage {
    /// Host input changed.
    HostChanged(String),
    /// Ports range input changed.
    PortsChanged(String),
    /// Timeout input changed.
    TimeoutChanged(String),
    /// Start port scan.
    Start,
    /// Cancel in-progress scan.
    Stop,
    /// Scan completed.
    Finished(Vec<PortProbeResult>),
    /// Copy CSV export.
    CopyCsv,
}

/// The TCP Port Scanner tool pane state.
#[derive(Debug)]
pub struct PortScanPane {
    host: String,
    ports: String,
    timeout: String,
    running: bool,
    error: Option<String>,
    results: Vec<PortProbeResult>,
    summary: PortScanSummary,
}

impl Default for PortScanPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            ports: DEFAULT_COMMON_PORTS.to_string(),
            timeout: DEFAULT_PORT_TIMEOUT_MS.to_string(),
            running: false,
            error: None,
            results: Vec::new(),
            summary: PortScanSummary::default(),
        }
    }
}

/// Outcome of updating the Port Scanner tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl PortScanPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: PortScanMessage) -> Outcome {
        match message {
            PortScanMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
                Outcome::None
            }
            PortScanMessage::PortsChanged(val) => {
                self.ports = val;
                self.error = None;
                Outcome::None
            }
            PortScanMessage::TimeoutChanged(val) => {
                self.timeout = val;
                self.error = None;
                Outcome::None
            }
            PortScanMessage::Start => {
                let host = self.host.trim().to_string();
                if host.is_empty() {
                    self.error = Some(fl!("ui-tool-portscan-error-host-required"));
                    return Outcome::None;
                }

                let ports = banner_grab::parse_ports(&self.ports);
                if ports.is_empty() {
                    self.error = Some(fl!("ui-tool-portscan-error-ports-invalid"));
                    return Outcome::None;
                }

                let timeout_ms: u64 = match self.timeout.trim().parse() {
                    Ok(t) if (100..=10000).contains(&t) => t,
                    _ => DEFAULT_PORT_TIMEOUT_MS,
                };

                self.running = true;
                self.error = None;
                self.results.clear();
                self.summary = PortScanSummary::default();

                let task_host = host;
                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut list = Vec::with_capacity(ports.len());
                            for p in ports {
                                list.push(port_scanner::probe_single_port(
                                    &task_host, p, timeout_ms,
                                ));
                            }
                            list
                        })
                        .await
                        .unwrap_or_default()
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::PortScan(PortScanMessage::Finished(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            PortScanMessage::Stop => {
                self.running = false;
                Outcome::None
            }
            PortScanMessage::Finished(res) => {
                self.running = false;
                self.summary = port_scanner::compute_port_summary(&res);
                self.results = res;
                Outcome::None
            }
            PortScanMessage::CopyCsv => {
                if self.results.is_empty() {
                    return Outcome::None;
                }
                let csv = port_scanner::format_port_scan_csv(&self.results);
                Outcome::Copy(CopySlot::PortScanCsv, csv)
            }
        }
    }

    /// Renders the Port Scanner view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::PortScan(msg));

        let host_input = text_input(&fl!("ui-tool-portscan-host-watermark"), &self.host)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(PortScanMessage::HostChanged(v)))
            .on_submit(send(PortScanMessage::Start))
            .width(Length::FillPortion(3));

        let ports_input = text_input(&fl!("ui-tool-portscan-ports-watermark"), &self.ports)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(PortScanMessage::PortsChanged(v)))
            .width(Length::FillPortion(3));

        let timeout_input = text_input(&fl!("ui-tool-portscan-timeout-watermark"), &self.timeout)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(PortScanMessage::TimeoutChanged(v)))
            .width(Length::FillPortion(1));

        let scan_button = if self.running {
            super::action_button(
                fl!("ui-tool-portscan-stop"),
                false,
                Some(send(PortScanMessage::Stop)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-portscan-start"),
                true,
                Some(send(PortScanMessage::Start)),
            )
        };

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-portscan-host")), host_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(3)),
            column![
                super::field_label(fl!("ui-tool-portscan-ports")),
                ports_input
            ]
            .spacing(spacing::XS)
            .width(Length::FillPortion(3)),
            column![
                super::field_label(fl!("ui-tool-portscan-timeout")),
                timeout_input
            ]
            .spacing(spacing::XS)
            .width(Length::FillPortion(1)),
            container(scan_button).align_y(Alignment::End),
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
                    fl!("ui-tool-portscan-total"),
                    self.summary.total.to_string()
                ),
                metric_card(fl!("ui-tool-portscan-open"), self.summary.open.to_string()),
                metric_card(
                    fl!("ui-tool-portscan-closed"),
                    self.summary.closed.to_string()
                ),
                metric_card(
                    fl!("ui-tool-portscan-filtered"),
                    self.summary.filtered.to_string()
                ),
            ]
            .spacing(spacing::SM);

            let copy_csv_btn = super::copy_button(
                fl!("ui-tool-portscan-copy-csv"),
                state.copied(CopySlot::PortScanCsv),
                send(PortScanMessage::CopyCsv),
                super::COPY_PADDING,
            );

            let header_row = row![
                text(fl!("ui-tool-portscan-results-title"))
                    .size(font_size::SUBTITLE)
                    .width(Length::Fill),
                copy_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let mut list_col = column![].spacing(spacing::XS);
            for r in &self.results {
                let status_cell = match r.status {
                    PortStatus::Open => text(fl!("ui-tool-portscan-status-open"))
                        .size(font_size::BODY)
                        .style(text::success)
                        .width(Length::Fixed(90.0)),
                    PortStatus::Closed => text(fl!("ui-tool-portscan-status-closed"))
                        .size(font_size::BODY)
                        .style(text::secondary)
                        .width(Length::Fixed(90.0)),
                    PortStatus::Filtered => text(fl!("ui-tool-portscan-status-filtered"))
                        .size(font_size::BODY)
                        .style(text::warning)
                        .width(Length::Fixed(90.0)),
                };

                let lat_text = if r.latency_ms >= 0.0 {
                    format!("{:.1} ms", r.latency_ms)
                } else {
                    "-".to_string()
                };

                let item_row = row![
                    text(format!("Port {}", r.port))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                    text(r.service)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    status_cell,
                    text(lat_text)
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
