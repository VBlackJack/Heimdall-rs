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

//! The SNMP MIB Walker tool view: SNMPv2c MIB subtree traversal,
//! OID resolution, type decoding, and CSV export.

use heimdall_app::TabId;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::snmp_walker::{self, SnmpEntry};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the SNMP walker view.
#[derive(Debug, Clone)]
pub enum SnmpWalkerMessage {
    /// Host input changed.
    HostChanged(String),
    /// Community input changed.
    CommunityChanged(String),
    /// Root OID input changed.
    OidChanged(String),
    /// Start SNMP walk.
    Start,
    /// Stop walk.
    Stop,
    /// Walk completed.
    Finished(Result<Vec<SnmpEntry>, String>),
    /// Copy CSV export.
    CopyCsv,
}

/// The SNMP MIB Walker tool pane state.
#[derive(Debug)]
pub struct SnmpWalkerPane {
    host: String,
    community: String,
    oid: String,
    running: bool,
    error: Option<String>,
    entries: Vec<SnmpEntry>,
}

impl Default for SnmpWalkerPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            community: "public".to_string(),
            oid: "1.3.6.1.2.1.1".to_string(),
            running: false,
            error: None,
            entries: Vec::new(),
        }
    }
}

/// Outcome of updating the SNMP MIB Walker tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl SnmpWalkerPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: SnmpWalkerMessage) -> Outcome {
        match message {
            SnmpWalkerMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
                Outcome::None
            }
            SnmpWalkerMessage::CommunityChanged(val) => {
                self.community = val;
                self.error = None;
                Outcome::None
            }
            SnmpWalkerMessage::OidChanged(val) => {
                self.oid = val;
                self.error = None;
                Outcome::None
            }
            SnmpWalkerMessage::Start => {
                let host = self.host.trim().to_string();
                if host.is_empty() {
                    self.error = Some(fl!("ui-tool-snmpwalk-error-host-required"));
                    return Outcome::None;
                }

                self.running = true;
                self.error = None;
                self.entries.clear();

                let task_host = host;
                let task_comm = self.community.trim().to_string();
                let task_oid = self.oid.trim().to_string();

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            snmp_walker::walk_oid(&task_host, 161, &task_comm, &task_oid, 50, 2000)
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::SnmpWalk(SnmpWalkerMessage::Finished(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            SnmpWalkerMessage::Stop => {
                self.running = false;
                Outcome::None
            }
            SnmpWalkerMessage::Finished(res) => {
                self.running = false;
                match res {
                    Ok(entries) => {
                        self.entries = entries;
                        self.error = None;
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                }
                Outcome::None
            }
            SnmpWalkerMessage::CopyCsv => {
                if !self.entries.is_empty() {
                    let csv = snmp_walker::build_snmp_csv(&self.entries);
                    Outcome::Copy(CopySlot::SnmpWalkCsv, csv)
                } else {
                    Outcome::None
                }
            }
        }
    }

    /// Renders the SNMP MIB Walker tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::SnmpWalk(msg));

        let host_input = text_input(&fl!("ui-tool-snmpwalk-host-watermark"), &self.host)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(SnmpWalkerMessage::HostChanged(v)))
            .on_submit(send(SnmpWalkerMessage::Start))
            .width(Length::FillPortion(2));

        let comm_input = text_input("public", &self.community)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(SnmpWalkerMessage::CommunityChanged(v)))
            .width(Length::FillPortion(1));

        let oid_input = text_input("1.3.6.1.2.1.1", &self.oid)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(SnmpWalkerMessage::OidChanged(v)))
            .width(Length::FillPortion(2));

        let action_btn = if self.running {
            super::action_button(
                fl!("ui-tool-snmpwalk-stop"),
                true,
                Some(send(SnmpWalkerMessage::Stop)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-snmpwalk-start"),
                true,
                Some(send(SnmpWalkerMessage::Start)),
            )
        };

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-snmpwalk-host")), host_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(2)),
            column![
                super::field_label(fl!("ui-tool-snmpwalk-community")),
                comm_input
            ]
            .spacing(spacing::XS)
            .width(Length::FillPortion(1)),
            column![super::field_label(fl!("ui-tool-snmpwalk-oid")), oid_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(2)),
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

        if !self.entries.is_empty() {
            let copy_csv_btn = super::copy_button(
                fl!("ui-tool-snmpwalk-copy-csv"),
                state.copied(CopySlot::SnmpWalkCsv),
                send(SnmpWalkerMessage::CopyCsv),
                super::COPY_PADDING,
            );

            let header_row = row![
                text(format!(
                    "{} ({} entries)",
                    fl!("ui-tool-snmpwalk-name"),
                    self.entries.len()
                ))
                .size(font_size::SUBTITLE)
                .width(Length::Fill),
                copy_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let table_headers = row![
                text(fl!("ui-tool-snmpwalk-col-oid"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-snmpwalk-col-name"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-snmpwalk-col-type"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(100.0)),
                text(fl!("ui-tool-snmpwalk-col-value"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(3)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for e in &self.entries {
                let r = row![
                    text(&e.oid)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    text(&e.name)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    text(&e.val_type)
                        .size(font_size::BODY)
                        .style(text::secondary)
                        .width(Length::Fixed(100.0)),
                    text(&e.value)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(3)),
                ]
                .spacing(spacing::SM);

                entry_rows = entry_rows.push(r);
            }

            let card = container(column![header_row, entry_rows].spacing(spacing::MD))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        }

        super::content_column(body)
    }
}
