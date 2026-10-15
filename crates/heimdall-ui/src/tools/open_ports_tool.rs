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

//! The Open Ports tool view: local TCP/UDP listening ports and active sockets
//! enumeration with owning process resolution, search filter, and CSV export.

use heimdall_app::TabId;
use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::open_ports::{self, PortEntry};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Maximum rows rendered in the table to maintain smooth scrolling performance.
const MAX_DISPLAY_ROWS: usize = 300;

/// Messages emitted by the Open Ports view.
#[derive(Debug, Clone)]
pub enum OpenPortsMessage {
    /// Refresh the socket list now.
    Refresh,
    /// Refresh finished with results.
    Finished(Vec<PortEntry>),
    /// Filter query text changed.
    FilterChanged(String),
    /// Copy CSV export to clipboard.
    CopyCsv,
}

/// The Open Ports tool pane state.
#[derive(Debug, Default)]
pub struct OpenPortsPane {
    running: bool,
    error: Option<String>,
    ports: Vec<PortEntry>,
    filter: String,
    last_count: usize,
    last_time: Option<String>,
}

/// Outcome of updating the Open Ports tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl OpenPortsPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: OpenPortsMessage) -> Outcome {
        match message {
            OpenPortsMessage::Refresh => {
                self.running = true;
                self.error = None;

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(open_ports::run_netstat_command)
                            .await
                            .unwrap_or_default()
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::OpenPorts(OpenPortsMessage::Finished(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            OpenPortsMessage::Finished(res) => {
                self.running = false;
                self.last_count = res.len();
                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let hour = (secs / 3600) % 24;
                let min = (secs / 60) % 60;
                let sec = secs % 60;
                self.last_time = Some(format!("{hour:02}:{min:02}:{sec:02}"));
                self.ports = res;
                Outcome::None
            }
            OpenPortsMessage::FilterChanged(filter) => {
                self.filter = filter;
                Outcome::None
            }
            OpenPortsMessage::CopyCsv => {
                let matching: Vec<PortEntry> = self
                    .ports
                    .iter()
                    .filter(|p| self.matches_filter(p))
                    .cloned()
                    .collect();
                if !matching.is_empty() {
                    let csv = open_ports::build_open_ports_csv(&matching);
                    Outcome::Copy(CopySlot::OpenPortsCsv, csv)
                } else {
                    Outcome::None
                }
            }
        }
    }

    /// Checks if a port entry matches the current filter query.
    fn matches_filter(&self, p: &PortEntry) -> bool {
        let q = self.filter.trim().to_lowercase();
        if q.is_empty() {
            true
        } else {
            p.protocol.to_lowercase().contains(&q)
                || p.local_address.to_lowercase().contains(&q)
                || p.local_port.to_string().contains(&q)
                || p.remote_address.to_lowercase().contains(&q)
                || p.remote_port.to_string().contains(&q)
                || p.state.to_lowercase().contains(&q)
                || p.pid.to_string().contains(&q)
                || p.process_name.to_lowercase().contains(&q)
        }
    }

    /// Renders the Open Ports tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::OpenPorts(msg));

        let refresh_btn = if self.running {
            super::action_button(fl!("ui-tool-openports-scanning"), false, None)
        } else {
            super::action_button(
                fl!("ui-tool-openports-refresh"),
                true,
                Some(send(OpenPortsMessage::Refresh)),
            )
        };

        let copy_csv_btn = super::copy_button(
            fl!("ui-tool-openports-copy-csv"),
            state.copied(CopySlot::OpenPortsCsv),
            send(OpenPortsMessage::CopyCsv),
            super::COPY_PADDING,
        );

        let filter_input = text_input(
            fl!("ui-tool-openports-filter-placeholder").as_str(),
            &self.filter,
        )
        .on_input(move |val| send(OpenPortsMessage::FilterChanged(val)))
        .padding(super::INPUT_PADDING)
        .size(font_size::BODY)
        .width(Length::FillPortion(3));

        let top_row = row![filter_input, refresh_btn, copy_csv_btn]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

        let header_card = container(top_row).padding(spacing::MD).style(styles::card);

        let mut body = column![header_card].spacing(spacing::MD);

        if let Some(err) = &self.error {
            body = body.push(
                container(text(err.as_str()).size(font_size::BODY).style(text::danger))
                    .padding(spacing::MD)
                    .style(styles::card),
            );
        }

        if let Some(ref time) = self.last_time {
            let status_msg = fl!(
                "ui-tool-openports-status",
                count = self.last_count,
                time = time.as_str()
            );
            body = body.push(
                text(status_msg)
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }

        let matching: Vec<&'a PortEntry> = self
            .ports
            .iter()
            .filter(|p| self.matches_filter(p))
            .collect();
        if !matching.is_empty() {
            let table_headers = row![
                text(fl!("ui-tool-openports-col-proto"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(60.0)),
                text(fl!("ui-tool-openports-col-local-addr"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-openports-col-local-port"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(80.0)),
                text(fl!("ui-tool-openports-col-remote-addr"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-openports-col-remote-port"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(80.0)),
                text(fl!("ui-tool-openports-col-state"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
                text(fl!("ui-tool-openports-col-pid"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(60.0)),
                text(fl!("ui-tool-openports-col-process"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for p in matching.iter().take(MAX_DISPLAY_ROWS) {
                let r = row![
                    text(&p.protocol)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(60.0)),
                    text(&p.local_address)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(p.local_port.to_string())
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(80.0)),
                    text(&p.remote_address)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(if p.remote_port == 0 {
                        "-".to_owned()
                    } else {
                        p.remote_port.to_string()
                    })
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .width(Length::Fixed(80.0)),
                    text(&p.state)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                    text(p.pid.to_string())
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(60.0)),
                    text(&p.process_name)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                ]
                .spacing(spacing::SM);

                entry_rows = entry_rows.push(r);
            }

            let card = container(scrollable(entry_rows))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        } else if !self.running && self.ports.is_empty() {
            body = body.push(super::empty_state(fl!("ui-tool-openports-empty-state")));
        }

        super::content_column(body)
    }
}
