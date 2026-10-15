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

//! The Network Interfaces tool view: local network adapter enumeration,
//! link speed, MAC address, IP configuration, and CSV export.

use heimdall_app::TabId;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::network_interfaces::{self, NicSnapshot};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the Network Interfaces view.
#[derive(Debug, Clone)]
pub enum NetIfMessage {
    /// Refresh interface list now.
    Refresh,
    /// Refresh completed with interface snapshots.
    Finished(Vec<NicSnapshot>),
    /// Copy CSV export to clipboard.
    CopyCsv,
}

/// The Network Interfaces tool pane state.
#[derive(Debug, Default)]
pub struct NetIfPane {
    running: bool,
    error: Option<String>,
    interfaces: Vec<NicSnapshot>,
    last_count: usize,
    last_time: Option<String>,
}

/// Outcome of updating the Network Interfaces tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl NetIfPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: NetIfMessage) -> Outcome {
        match message {
            NetIfMessage::Refresh => {
                self.running = true;
                self.error = None;

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(network_interfaces::run_interfaces_command)
                            .await
                            .unwrap_or_default()
                    },
                    move |res| {
                        Message::Tool(tab, super::ToolMessage::NetIf(NetIfMessage::Finished(res)))
                    },
                );

                Outcome::Task(task)
            }
            NetIfMessage::Finished(res) => {
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
                self.interfaces = res;
                Outcome::None
            }
            NetIfMessage::CopyCsv => {
                if !self.interfaces.is_empty() {
                    let csv = network_interfaces::build_interfaces_csv(&self.interfaces);
                    Outcome::Copy(CopySlot::NetIfCsv, csv)
                } else {
                    Outcome::None
                }
            }
        }
    }

    /// Renders the Network Interfaces tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::NetIf(msg));

        let refresh_btn = if self.running {
            super::action_button(fl!("ui-tool-netif-scanning"), false, None)
        } else {
            super::action_button(
                fl!("ui-tool-netif-refresh"),
                true,
                Some(send(NetIfMessage::Refresh)),
            )
        };

        let copy_csv_btn = super::copy_button(
            fl!("ui-tool-netif-copy-csv"),
            state.copied(CopySlot::NetIfCsv),
            send(NetIfMessage::CopyCsv),
            super::COPY_PADDING,
        );

        let top_row = row![
            text(fl!("ui-tool-netif-name"))
                .size(font_size::SUBTITLE)
                .width(Length::Fill),
            refresh_btn,
            copy_csv_btn,
        ]
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
                "ui-tool-netif-status",
                count = self.last_count,
                time = time.as_str()
            );
            body = body.push(
                text(status_msg)
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }

        if !self.interfaces.is_empty() {
            let table_headers = row![
                text(fl!("ui-tool-netif-col-name"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(160.0)),
                text(fl!("ui-tool-netif-col-type"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
                text(fl!("ui-tool-netif-col-status"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(80.0)),
                text(fl!("ui-tool-netif-col-speed"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(80.0)),
                text(fl!("ui-tool-netif-col-mac"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-netif-col-ipv4"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(120.0)),
                text(fl!("ui-tool-netif-col-subnet"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(120.0)),
                text(fl!("ui-tool-netif-col-gateway"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(120.0)),
                text(fl!("ui-tool-netif-col-dhcp"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(60.0)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for nic in &self.interfaces {
                let r = row![
                    text(&nic.name)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(160.0)),
                    text(&nic.interface_type)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                    text(&nic.status)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(80.0)),
                    text(&nic.speed)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(80.0)),
                    text(&nic.mac)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(&nic.ipv4)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(120.0)),
                    text(&nic.subnet)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(120.0)),
                    text(&nic.gateway)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(120.0)),
                    text(&nic.dhcp)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(60.0)),
                ]
                .spacing(spacing::SM);

                entry_rows = entry_rows.push(r);
            }

            let card = container(scrollable(entry_rows))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        } else if !self.running {
            body = body.push(super::empty_state(fl!("ui-tool-netif-empty-state")));
        }

        super::content_column(body)
    }
}
