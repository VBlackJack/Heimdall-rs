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

//! TCP Ping tool pane for measuring connection handshake latency.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::module_name_repetitions,
    clippy::redundant_closure_for_method_calls,
    clippy::too_many_lines
)]

use heimdall_app::TabId;
use heimdall_core::tools::tcp_ping::{
    self, TcpPingProbeRequest, TcpPingProbeResult, TcpPingProbeStatus, TcpPingSummary,
};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the TCP Ping tool.
#[derive(Debug, Clone)]
pub enum TcpPingMessage {
    /// Target host changed.
    HostChanged(String),
    /// Target port changed.
    PortChanged(String),
    /// Probe count changed.
    CountChanged(String),
    /// Start ping sequence.
    Start,
    /// Cancel in-progress ping sequence.
    Stop,
    /// All probes finished.
    Finished(Vec<TcpPingProbeResult>),
    /// Copy summary and results log to clipboard.
    CopyResults,
}

/// The TCP Ping tool pane state.
#[derive(Debug)]
pub struct TcpPingPane {
    host: String,
    port: String,
    count: String,
    running: bool,
    error: Option<String>,
    probes: Vec<TcpPingProbeResult>,
    summary: Option<TcpPingSummary>,
}

impl Default for TcpPingPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: "443".to_string(),
            count: "10".to_string(),
            running: false,
            error: None,
            probes: Vec::new(),
            summary: None,
        }
    }
}

/// Outcome of updating the TCP Ping tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl TcpPingPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: TcpPingMessage) -> Outcome {
        match message {
            TcpPingMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
                Outcome::None
            }
            TcpPingMessage::PortChanged(val) => {
                self.port = val;
                self.error = None;
                Outcome::None
            }
            TcpPingMessage::CountChanged(val) => {
                self.count = val;
                self.error = None;
                Outcome::None
            }
            TcpPingMessage::Start => {
                let trimmed_host = self.host.trim();
                if trimmed_host.is_empty() {
                    self.error = Some(fl!("ui-tool-tcpping-error-host-required"));
                    return Outcome::None;
                }

                let port: u16 = match self.port.trim().parse() {
                    Ok(p) if p >= 1 => p,
                    _ => 443,
                };

                let count: usize = match self.count.trim().parse() {
                    Ok(c) if (1..=10000).contains(&c) => c,
                    _ => {
                        self.error = Some(fl!("ui-tool-tcpping-error-count"));
                        return Outcome::None;
                    }
                };

                self.running = true;
                self.error = None;
                self.probes.clear();
                self.summary = None;

                let host = trimmed_host.to_string();
                let task = Task::perform(
                    async move {
                        let mut results = Vec::with_capacity(count);
                        for seq in 1..=count {
                            let req = TcpPingProbeRequest {
                                host: host.clone(),
                                port,
                                seq,
                                timeout_ms: 5000,
                            };
                            let res =
                                tokio::task::spawn_blocking(move || tcp_ping::probe_tcp(&req))
                                    .await
                                    .unwrap_or_else(|_| {
                                        TcpPingProbeResult::failed(
                                            seq,
                                            host.clone(),
                                            port,
                                            "Task execution failed",
                                        )
                                    });
                            results.push(res);
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        }
                        results
                    },
                    move |results| {
                        Message::Tool(tab, ToolMessage::TcpPing(TcpPingMessage::Finished(results)))
                    },
                );

                Outcome::Task(task)
            }
            TcpPingMessage::Stop => {
                self.running = false;
                Outcome::None
            }
            TcpPingMessage::Finished(results) => {
                self.running = false;
                self.probes = results;
                self.summary = TcpPingSummary::from_probes(&self.probes);
                Outcome::None
            }
            TcpPingMessage::CopyResults => {
                let mut out = String::new();
                if let Some(sum) = &self.summary {
                    out.push_str(&format!(
                        "Min: {:.1}ms, Avg: {:.1}ms, Max: {:.1}ms, Lost: {}/{} ({:.1}%)\n\n",
                        sum.min_ms,
                        sum.avg_ms,
                        sum.max_ms,
                        sum.lost,
                        sum.total,
                        sum.loss_percentage()
                    ));
                }
                for p in &self.probes {
                    if p.status == TcpPingProbeStatus::Success {
                        out.push_str(&format!(
                            "[#{}] {}:{} - {:.2} ms\n",
                            p.seq, p.host, p.port, p.latency_ms
                        ));
                    } else {
                        let err = p.error_message.as_deref().unwrap_or("Failed");
                        out.push_str(&format!("[#{}] {}:{} - {}\n", p.seq, p.host, p.port, err));
                    }
                }
                Outcome::Copy(CopySlot::TcpPingResults, out)
            }
        }
    }

    /// Renders the TCP Ping tool pane.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &'a ToolPane,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let colors = crate::themes::colors_of(theme);

        let host_input = column![
            text(fl!("ui-tool-tcpping-host-label"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text_input(&fl!("ui-tool-tcpping-host-placeholder"), &self.host)
                .on_input(move |v| Message::Tool(
                    tab,
                    ToolMessage::TcpPing(TcpPingMessage::HostChanged(v))
                ))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::FillPortion(5))
        .spacing(spacing::XS);

        let port_input = column![
            text(fl!("ui-tool-tcpping-port-label"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text_input("443", &self.port)
                .on_input(move |v| Message::Tool(
                    tab,
                    ToolMessage::TcpPing(TcpPingMessage::PortChanged(v))
                ))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::FillPortion(2))
        .spacing(spacing::XS);

        let count_input = column![
            text(fl!("ui-tool-tcpping-count-label"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text_input("10", &self.count)
                .on_input(move |v| Message::Tool(
                    tab,
                    ToolMessage::TcpPing(TcpPingMessage::CountChanged(v))
                ))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::FillPortion(2))
        .spacing(spacing::XS);

        let start_btn = if self.running {
            button(
                text(fl!("ui-tool-tcpping-btn-stop"))
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD),
            )
            .padding(super::PRIMARY_PADDING)
            .style(styles::danger)
            .on_press(Message::Tool(
                tab,
                ToolMessage::TcpPing(TcpPingMessage::Stop),
            ))
        } else {
            button(
                text(fl!("ui-tool-tcpping-btn-start"))
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD),
            )
            .padding(super::PRIMARY_PADDING)
            .style(styles::primary)
            .on_press(Message::Tool(
                tab,
                ToolMessage::TcpPing(TcpPingMessage::Start),
            ))
        };

        let controls_row = row![
            host_input,
            port_input,
            count_input,
            container(start_btn).align_y(Alignment::End),
        ]
        .spacing(spacing::MD)
        .align_y(Alignment::End);

        let mut body_col = column!().spacing(spacing::MD);

        if let Some(err) = &self.error {
            body_col = body_col.push(text(err).size(font_size::BODY).color(colors.red));
        }

        if let Some(sum) = &self.summary {
            let summary_card = container(
                row![
                    text(fl!(
                        "ui-tool-tcpping-summary",
                        min = format!("{:.1}", sum.min_ms),
                        avg = format!("{:.1}", sum.avg_ms),
                        max = format!("{:.1}", sum.max_ms),
                        lost = sum.lost,
                        total = sum.total,
                        pct = format!("{:.0}", sum.loss_percentage())
                    ))
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD)
                    .width(Length::Fill),
                    super::copy_button(
                        fl!("ui-tool-tcpping-btn-copy"),
                        state.copied(CopySlot::TcpPingResults),
                        Message::Tool(tab, ToolMessage::TcpPing(TcpPingMessage::CopyResults),),
                        super::COPY_PADDING,
                    ),
                ]
                .align_y(Alignment::Center),
            )
            .padding([8.0, 12.0])
            .width(Length::Fill)
            .style(styles::card);

            body_col = body_col.push(summary_card);
        }

        if self.probes.is_empty() && !self.running {
            body_col = body_col.push(
                container(
                    text(fl!("ui-tool-tcpping-empty-state"))
                        .size(font_size::BODY)
                        .style(text::secondary),
                )
                .padding(super::EMPTY_STATE_PADDING),
            );
        } else {
            let mut list = column!().spacing(spacing::XS);
            for p in &self.probes {
                let (line_color, status_text) = if p.status == TcpPingProbeStatus::Success {
                    (colors.green, format!("{:.1} ms", p.latency_ms))
                } else {
                    (
                        colors.red,
                        p.error_message.clone().unwrap_or_else(|| "Failed".into()),
                    )
                };

                list = list.push(
                    row![
                        text(format!("[#{:02}]", p.seq))
                            .size(font_size::CAPTION)
                            .font(super::BOX_FONT)
                            .style(text::secondary)
                            .width(Length::Fixed(40.0)),
                        text(format!("{}:{}", p.host, p.port))
                            .size(font_size::CAPTION)
                            .font(super::BOX_FONT)
                            .width(Length::FillPortion(3)),
                        text(status_text)
                            .size(font_size::CAPTION)
                            .font(super::BOX_FONT)
                            .color(line_color)
                            .width(Length::FillPortion(2)),
                    ]
                    .spacing(spacing::MD)
                    .align_y(Alignment::Center),
                );
            }

            body_col = body_col.push(scrollable(list).height(Length::Fill));
        }

        let content = column![controls_row, body_col]
            .spacing(spacing::LG)
            .padding(super::BODY_PADDING)
            .max_width(super::CONTENT_MAX_WIDTH);

        content.into()
    }
}
