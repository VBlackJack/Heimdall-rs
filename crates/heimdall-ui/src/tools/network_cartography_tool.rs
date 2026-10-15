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

//! The Network Cartography tool view: subnet host expansion, port recon sweep,
//! role classification heuristics, and CSV export.

use heimdall_app::TabId;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::network_cartography::{self, CartographySummary, DiscoveredNode};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the network cartography view.
#[derive(Debug, Clone)]
pub enum NetworkCartographyMessage {
    /// CIDR input changed.
    CidrChanged(String),
    /// Start network discovery.
    Start,
    /// Stop discovery.
    Stop,
    /// Discovery finished.
    Finished(Result<(Vec<DiscoveredNode>, CartographySummary), String>),
    /// Copy CSV export.
    CopyCsv,
}

/// The Network Cartography tool pane state.
#[derive(Debug)]
pub struct NetworkCartographyPane {
    cidr: String,
    running: bool,
    error: Option<String>,
    nodes: Vec<DiscoveredNode>,
    summary: CartographySummary,
}

impl Default for NetworkCartographyPane {
    fn default() -> Self {
        Self {
            cidr: "192.168.1.0/24".to_string(),
            running: false,
            error: None,
            nodes: Vec::new(),
            summary: CartographySummary {
                hosts_scanned: 0,
                hosts_up: 0,
                open_ports_count: 0,
            },
        }
    }
}

/// Outcome of updating the Network Cartography tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl NetworkCartographyPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: NetworkCartographyMessage) -> Outcome {
        match message {
            NetworkCartographyMessage::CidrChanged(val) => {
                self.cidr = val;
                self.error = None;
                Outcome::None
            }
            NetworkCartographyMessage::Start => {
                let cidr = self.cidr.trim().to_string();
                if cidr.is_empty() {
                    self.error = Some(fl!("ui-tool-netmap-error-cidr-required"));
                    return Outcome::None;
                }

                self.running = true;
                self.error = None;
                self.nodes.clear();

                let task_cidr = cidr;
                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            network_cartography::discover_subnet(&task_cidr, 200)
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::NetMap(NetworkCartographyMessage::Finished(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            NetworkCartographyMessage::Stop => {
                self.running = false;
                Outcome::None
            }
            NetworkCartographyMessage::Finished(res) => {
                self.running = false;
                match res {
                    Ok((nodes, summary)) => {
                        self.nodes = nodes;
                        self.summary = summary;
                        self.error = None;
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                }
                Outcome::None
            }
            NetworkCartographyMessage::CopyCsv => {
                if !self.nodes.is_empty() {
                    let csv = network_cartography::build_nodes_csv(&self.nodes);
                    Outcome::Copy(CopySlot::NetMapCsv, csv)
                } else {
                    Outcome::None
                }
            }
        }
    }

    /// Renders the Network Cartography tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::NetMap(msg));

        let cidr_input = text_input(&fl!("ui-tool-netmap-subnet-watermark"), &self.cidr)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(NetworkCartographyMessage::CidrChanged(v)))
            .on_submit(send(NetworkCartographyMessage::Start))
            .width(Length::FillPortion(3));

        let action_btn = if self.running {
            super::action_button(fl!("ui-tool-netmap-discovering"), false, None)
        } else {
            super::action_button(
                fl!("ui-tool-netmap-discover"),
                true,
                Some(send(NetworkCartographyMessage::Start)),
            )
        };

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-netmap-subnet")), cidr_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(3)),
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

        if !self.nodes.is_empty() {
            let summary_cards = row![
                container(
                    column![
                        text(fl!("ui-tool-netmap-hosts-scanned"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.hosts_scanned.to_string())
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
                        text(fl!("ui-tool-netmap-hosts-up"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.hosts_up.to_string())
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
                        text(fl!("ui-tool-netmap-ports-open"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.open_ports_count.to_string())
                            .size(font_size::TITLE)
                            .font(super::BOX_FONT)
                            .style(text::primary),
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
                fl!("ui-tool-netmap-copy-csv"),
                state.copied(CopySlot::NetMapCsv),
                send(NetworkCartographyMessage::CopyCsv),
                super::COPY_PADDING,
            );

            let header_row = row![
                text(fl!("ui-tool-netmap-name"))
                    .size(font_size::SUBTITLE)
                    .width(Length::Fill),
                copy_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let table_headers = row![
                text(fl!("ui-tool-netmap-col-ip"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-netmap-col-role"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-netmap-col-ports"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-netmap-col-latency"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
            ]
            .spacing(spacing::SM);

            let mut node_rows = column![table_headers].spacing(spacing::XS);

            for n in &self.nodes {
                let ports_str = n
                    .open_ports
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");

                let r = row![
                    text(&n.ip)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(&n.role)
                        .size(font_size::BODY)
                        .style(text::primary)
                        .width(Length::FillPortion(2)),
                    text(ports_str)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    text(format!("{} ms", n.response_time_ms))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                ]
                .spacing(spacing::SM);

                node_rows = node_rows.push(r);
            }

            let card = container(column![header_row, node_rows].spacing(spacing::MD))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        }

        super::content_column(body)
    }
}
