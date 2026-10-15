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

//! The DNS record lookup tool view: record type selection, public nameserver presets,
//! execution via native resolver and nslookup, and record display.

use heimdall_app::TabId;
use iced::widget::{column, container, pick_list, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::dns_lookup::{
    self, DnsLookupRequest, DnsLookupResult, DnsRecordType, DnsServerPreset,
};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the DNS lookup tool view.
#[derive(Debug, Clone)]
pub enum DnsMessage {
    /// Hostname input changed.
    HostChanged(String),
    /// Record type selection changed.
    RecordTypeSelected(DnsRecordType),
    /// Nameserver preset selection changed.
    ServerSelected(DnsServerPreset),
    /// Execute lookup query.
    Lookup,
    /// Query completed.
    Finished(DnsLookupResult),
    /// Copy output to clipboard.
    CopyOutput,
}

/// The DNS Lookup tool pane state.
#[derive(Debug)]
pub struct DnsPane {
    host: String,
    record_type: DnsRecordType,
    server: DnsServerPreset,
    running: bool,
    error: Option<String>,
    result: Option<DnsLookupResult>,
}

impl Default for DnsPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            record_type: DnsRecordType::A,
            server: DnsServerPreset::DefaultSystem,
            running: false,
            error: None,
            result: None,
        }
    }
}

/// Outcome of updating the DNS Lookup tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl DnsPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: DnsMessage) -> Outcome {
        match message {
            DnsMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
                Outcome::None
            }
            DnsMessage::RecordTypeSelected(rt) => {
                self.record_type = rt;
                Outcome::None
            }
            DnsMessage::ServerSelected(srv) => {
                self.server = srv;
                Outcome::None
            }
            DnsMessage::Lookup => {
                let host = self.host.trim().to_string();
                if host.is_empty() {
                    self.error = Some(fl!("ui-tool-dns-error-host-required"));
                    return Outcome::None;
                }

                self.running = true;
                self.error = None;

                let server_ip = self.server.server_ip().map(ToString::to_string);
                let request = DnsLookupRequest {
                    host,
                    record_type: self.record_type,
                    server: server_ip,
                };

                let req_clone = request.clone();
                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || dns_lookup::query_dns_sync(&request))
                            .await
                            .unwrap_or_else(|err| DnsLookupResult {
                                host: req_clone.host,
                                record_type: req_clone.record_type,
                                server: req_clone.server,
                                records: Vec::new(),
                                aliases: Vec::new(),
                                elapsed_ms: 0,
                                raw_output: String::new(),
                                error: Some(err.to_string()),
                            })
                    },
                    move |res| {
                        Message::Tool(tab, super::ToolMessage::Dns(DnsMessage::Finished(res)))
                    },
                );

                Outcome::Task(task)
            }
            DnsMessage::Finished(res) => {
                self.running = false;
                if let Some(err) = &res.error {
                    self.error = Some(err.clone());
                } else {
                    self.error = None;
                }
                self.result = Some(res);
                Outcome::None
            }
            DnsMessage::CopyOutput => {
                if let Some(res) = &self.result {
                    let mut text_buf = String::new();
                    text_buf.push_str(&format!(
                        ";; Query: {} (type: {}, time: {} ms)\n",
                        res.host,
                        res.record_type.wire_format(),
                        res.elapsed_ms
                    ));
                    if let Some(srv) = &res.server {
                        text_buf.push_str(&format!(";; Server: {srv}\n"));
                    }
                    if !res.aliases.is_empty() {
                        text_buf.push_str(&format!(";; Aliases: {}\n", res.aliases.join(", ")));
                    }
                    text_buf.push_str("\n;; Answers:\n");
                    for r in &res.records {
                        text_buf.push_str(&format!("{r}\n"));
                    }
                    if !res.raw_output.is_empty() && res.records.is_empty() {
                        text_buf.push_str(&res.raw_output);
                    }
                    Outcome::Copy(CopySlot::DnsOutput, text_buf)
                } else {
                    Outcome::None
                }
            }
        }
    }

    /// Renders the DNS Lookup tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::Dns(msg));

        let host_input = text_input(&fl!("ui-tool-dns-host-watermark"), &self.host)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(DnsMessage::HostChanged(v)))
            .on_submit(send(DnsMessage::Lookup))
            .width(Length::FillPortion(3));

        let type_picker = pick_list(&DnsRecordType::ALL[..], Some(self.record_type), move |t| {
            send(DnsMessage::RecordTypeSelected(t))
        })
        .style(styles::pick_list)
        .width(Length::FillPortion(1));

        let server_picker = pick_list(&DnsServerPreset::ALL[..], Some(self.server), move |s| {
            send(DnsMessage::ServerSelected(s))
        })
        .style(styles::pick_list)
        .width(Length::FillPortion(2));

        let lookup_button = if self.running {
            super::action_button(fl!("ui-tool-dns-searching"), false, None)
        } else {
            super::action_button(
                fl!("ui-tool-dns-lookup"),
                true,
                Some(send(DnsMessage::Lookup)),
            )
        };

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-dns-host")), host_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(3)),
            column![super::field_label(fl!("ui-tool-dns-type")), type_picker]
                .spacing(spacing::XS)
                .width(Length::FillPortion(1)),
            column![super::field_label(fl!("ui-tool-dns-server")), server_picker]
                .spacing(spacing::XS)
                .width(Length::FillPortion(2)),
            container(lookup_button).align_y(Alignment::End),
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

        if let Some(res) = &self.result {
            let copy_btn = super::copy_button(
                fl!("ui-tool-dns-copy"),
                state.copied(CopySlot::DnsOutput),
                send(DnsMessage::CopyOutput),
                super::COPY_PADDING,
            );

            let header_row = row![
                text(fl!(
                    "ui-tool-dns-results-title",
                    host = res.host.as_str(),
                    record_type = res.record_type.wire_format(),
                    time = res.elapsed_ms.to_string()
                ))
                .size(font_size::SUBTITLE)
                .width(Length::Fill),
                copy_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let mut details_col = column![header_row].spacing(spacing::SM);

            if !res.aliases.is_empty() {
                details_col = details_col.push(
                    row![
                        text(fl!("ui-tool-dns-aliases"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(res.aliases.join(", "))
                            .size(font_size::BODY)
                            .font(super::BOX_FONT),
                    ]
                    .spacing(spacing::SM),
                );
            }

            let mut records_col = column![].spacing(spacing::XS);
            if res.records.is_empty() {
                records_col = records_col.push(
                    text(if res.raw_output.is_empty() {
                        fl!("ui-tool-dns-no-records")
                    } else {
                        res.raw_output.clone()
                    })
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .style(text::secondary),
                );
            } else {
                for r in &res.records {
                    records_col = records_col.push(
                        container(text(r.as_str()).size(font_size::BODY).font(super::BOX_FONT))
                            .padding(spacing::XS)
                            .style(styles::card)
                            .width(Length::Fill),
                    );
                }
            }

            details_col = details_col.push(records_col);

            let results_card = container(details_col)
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(results_card);
        }

        super::content_column(body)
    }
}
