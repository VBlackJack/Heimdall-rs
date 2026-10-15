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

//! The ARP Table Monitor tool view: local ARP cache inspection,
//! hardware vendor identification, anomaly detection, and CSV export.

use heimdall_app::TabId;
use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::arp_monitor::{self, ArpEntry, ArpStatus};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the ARP monitor view.
#[derive(Debug, Clone)]
pub enum ArpMonitorMessage {
    /// Scan ARP table now.
    Scan,
    /// Scan finished.
    Finished(Vec<ArpEntry>),
    /// Copy CSV export.
    CopyCsv,
}

/// The ARP Table Monitor tool pane state.
#[derive(Debug, Default)]
pub struct ArpMonitorPane {
    running: bool,
    error: Option<String>,
    entries: Vec<ArpEntry>,
}

/// Outcome of updating the ARP Monitor tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl ArpMonitorPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: ArpMonitorMessage) -> Outcome {
        match message {
            ArpMonitorMessage::Scan => {
                self.running = true;
                self.error = None;

                let prev_entries = self.entries.clone();
                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let table = arp_monitor::read_system_arp_table();
                            let secs = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let hour = (secs / 3600) % 24;
                            let min = (secs / 60) % 60;
                            let sec = secs % 60;
                            let ts = format!("{hour:02}:{min:02}:{sec:02}");
                            arp_monitor::detect_changes(&prev_entries, &table, &ts)
                        })
                        .await
                        .unwrap_or_default()
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::ArpMon(ArpMonitorMessage::Finished(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            ArpMonitorMessage::Finished(res) => {
                self.running = false;
                self.entries = res;
                Outcome::None
            }
            ArpMonitorMessage::CopyCsv => {
                if !self.entries.is_empty() {
                    let csv = arp_monitor::build_arp_csv(&self.entries);
                    Outcome::Copy(CopySlot::ArpMonCsv, csv)
                } else {
                    Outcome::None
                }
            }
        }
    }

    /// Renders the ARP Table Monitor tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::ArpMon(msg));

        let scan_btn = if self.running {
            super::action_button(fl!("ui-tool-arpmon-scanning"), false, None)
        } else {
            super::action_button(
                fl!("ui-tool-arpmon-scan"),
                true,
                Some(send(ArpMonitorMessage::Scan)),
            )
        };

        let copy_csv_btn = super::copy_button(
            fl!("ui-tool-arpmon-copy-csv"),
            state.copied(CopySlot::ArpMonCsv),
            send(ArpMonitorMessage::CopyCsv),
            super::COPY_PADDING,
        );

        let top_row = row![
            text(fl!("ui-tool-arpmon-name"))
                .size(font_size::SUBTITLE)
                .width(Length::Fill),
            scan_btn,
            copy_csv_btn,
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let input_card = container(column![top_row])
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

        if !self.entries.is_empty() {
            let table_headers = row![
                text(fl!("ui-tool-arpmon-col-ip"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-arpmon-col-mac"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(160.0)),
                text(fl!("ui-tool-arpmon-col-vendor"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-arpmon-col-status"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
                text(fl!("ui-tool-arpmon-col-first-seen"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
                text(fl!("ui-tool-arpmon-col-last-seen"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for e in &self.entries {
                let status_el = match e.status {
                    ArpStatus::New => text(fl!("ui-tool-arpmon-status-new"))
                        .size(font_size::BODY)
                        .style(text::primary)
                        .width(Length::Fixed(90.0)),
                    ArpStatus::Known => text(fl!("ui-tool-arpmon-status-known"))
                        .size(font_size::BODY)
                        .style(text::secondary)
                        .width(Length::Fixed(90.0)),
                    ArpStatus::Changed => text(fl!("ui-tool-arpmon-status-changed"))
                        .size(font_size::BODY)
                        .style(text::warning)
                        .width(Length::Fixed(90.0)),
                    ArpStatus::Conflict => text(fl!("ui-tool-arpmon-status-conflict"))
                        .size(font_size::BODY)
                        .style(text::danger)
                        .width(Length::Fixed(90.0)),
                };

                let r = row![
                    text(&e.ip)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(&e.mac)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(160.0)),
                    text(&e.vendor)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    status_el,
                    text(&e.first_seen)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                    text(&e.last_seen)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                ]
                .spacing(spacing::SM);

                entry_rows = entry_rows.push(r);
            }

            let card = container(entry_rows)
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        }

        super::content_column(body)
    }
}
