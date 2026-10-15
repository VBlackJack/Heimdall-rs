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

//! The SSL/TLS Certificate Inspector tool view: remote certificate inspection,
//! expiration status badges, hostname match verification, SANs display, and CSV export.

use heimdall_app::TabId;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::cert_inspector::{self, CertExpirationStatus, CertInspectionSummary};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the certificate inspector view.
#[derive(Debug, Clone)]
pub enum CertInspectorMessage {
    /// Host input changed.
    HostChanged(String),
    /// Ports input changed.
    PortsChanged(String),
    /// Inspect target certificates.
    Inspect,
    /// Inspection completed.
    Finished(Result<Vec<CertInspectionSummary>, String>),
    /// Copy plain text details.
    CopyDetails,
    /// Copy CSV export.
    CopyCsv,
}

/// The SSL/TLS Certificate Inspector tool pane state.
#[derive(Debug)]
pub struct CertInspectorPane {
    host: String,
    ports: String,
    running: bool,
    error: Option<String>,
    results: Vec<CertInspectionSummary>,
}

impl Default for CertInspectorPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            ports: "443".to_string(),
            running: false,
            error: None,
            results: Vec::new(),
        }
    }
}

/// Outcome of updating the Certificate Inspector tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl CertInspectorPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: CertInspectorMessage) -> Outcome {
        match message {
            CertInspectorMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
                Outcome::None
            }
            CertInspectorMessage::PortsChanged(val) => {
                self.ports = val;
                self.error = None;
                Outcome::None
            }
            CertInspectorMessage::Inspect => {
                let host = self.host.trim().to_string();
                if host.is_empty() {
                    self.error = Some(fl!("ui-tool-cert-error-host-required"));
                    return Outcome::None;
                }

                let ports = cert_inspector::parse_ports(&self.ports);
                let ports = if ports.is_empty() { vec![443] } else { ports };

                self.running = true;
                self.error = None;
                self.results.clear();

                let task_host = host;
                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut list = Vec::new();
                            for p in ports {
                                match cert_inspector::probe_cert(&task_host, p, 3000) {
                                    Ok(sum) => list.push(sum),
                                    Err(e) => return Err(e),
                                }
                            }
                            Ok(list)
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::Cert(CertInspectorMessage::Finished(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            CertInspectorMessage::Finished(res) => {
                self.running = false;
                match res {
                    Ok(list) => {
                        self.results = list;
                        self.error = None;
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                }
                Outcome::None
            }
            CertInspectorMessage::CopyDetails => {
                if let Some(first) = self.results.first() {
                    let details = cert_inspector::build_details_text(first, &self.host);
                    Outcome::Copy(CopySlot::CertDetails, details)
                } else {
                    Outcome::None
                }
            }
            CertInspectorMessage::CopyCsv => {
                if !self.results.is_empty() {
                    let csv = cert_inspector::build_scan_csv(&self.results);
                    Outcome::Copy(CopySlot::CertCsv, csv)
                } else {
                    Outcome::None
                }
            }
        }
    }

    /// Renders the Certificate Inspector tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::Cert(msg));

        let host_input = text_input(&fl!("ui-tool-cert-host-watermark"), &self.host)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(CertInspectorMessage::HostChanged(v)))
            .on_submit(send(CertInspectorMessage::Inspect))
            .width(Length::FillPortion(3));

        let ports_input = text_input(&fl!("ui-tool-cert-ports-watermark"), &self.ports)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(CertInspectorMessage::PortsChanged(v)))
            .width(Length::FillPortion(1));

        let inspect_button = if self.running {
            super::action_button(fl!("ui-tool-cert-inspecting"), false, None)
        } else {
            super::action_button(
                fl!("ui-tool-cert-inspect"),
                true,
                Some(send(CertInspectorMessage::Inspect)),
            )
        };

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-cert-host")), host_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(3)),
            column![super::field_label(fl!("ui-tool-cert-ports")), ports_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(1)),
            container(inspect_button).align_y(Alignment::End),
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

        for r in &self.results {
            let status_badge = match r.expiration_status {
                CertExpirationStatus::Valid => container(
                    text(fl!("ui-tool-cert-status-valid"))
                        .size(font_size::CAPTION)
                        .style(text::success),
                )
                .padding([2.0, 6.0])
                .style(styles::card),
                CertExpirationStatus::Warning => container(
                    text(fl!("ui-tool-cert-status-warning"))
                        .size(font_size::CAPTION)
                        .style(text::warning),
                )
                .padding([2.0, 6.0])
                .style(styles::card),
                CertExpirationStatus::Expired => container(
                    text(fl!("ui-tool-cert-status-expired"))
                        .size(font_size::CAPTION)
                        .style(text::danger),
                )
                .padding([2.0, 6.0])
                .style(styles::card),
            };

            let match_badge = if r.hostname_matches {
                container(
                    text(fl!("ui-tool-cert-match-yes"))
                        .size(font_size::CAPTION)
                        .style(text::success),
                )
                .padding([2.0, 6.0])
                .style(styles::card)
            } else {
                container(
                    text(fl!("ui-tool-cert-match-no"))
                        .size(font_size::CAPTION)
                        .style(text::danger),
                )
                .padding([2.0, 6.0])
                .style(styles::card)
            };

            let copy_details_btn = super::copy_button(
                fl!("ui-tool-cert-copy-details"),
                state.copied(CopySlot::CertDetails),
                send(CertInspectorMessage::CopyDetails),
                super::COPY_PADDING,
            );

            let copy_csv_btn = super::copy_button(
                fl!("ui-tool-cert-copy-csv"),
                state.copied(CopySlot::CertCsv),
                send(CertInspectorMessage::CopyCsv),
                super::COPY_PADDING,
            );

            let header_row = row![
                text(format!(
                    "Port {}: {}",
                    r.port,
                    cert_inspector::extract_cn(&r.subject)
                ))
                .size(font_size::SUBTITLE)
                .width(Length::Fill),
                status_badge,
                match_badge,
                copy_details_btn,
                copy_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let detail_rows = column![
                row![
                    text(fl!("ui-tool-cert-subject"))
                        .size(font_size::CAPTION)
                        .style(text::secondary)
                        .width(Length::Fixed(120.0)),
                    text(&r.subject).size(font_size::BODY).font(super::BOX_FONT),
                ]
                .spacing(spacing::SM),
                row![
                    text(fl!("ui-tool-cert-issuer"))
                        .size(font_size::CAPTION)
                        .style(text::secondary)
                        .width(Length::Fixed(120.0)),
                    text(&r.issuer).size(font_size::BODY).font(super::BOX_FONT),
                ]
                .spacing(spacing::SM),
                row![
                    text(fl!("ui-tool-cert-valid-from"))
                        .size(font_size::CAPTION)
                        .style(text::secondary)
                        .width(Length::Fixed(120.0)),
                    text(&r.not_before)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT),
                ]
                .spacing(spacing::SM),
                row![
                    text(fl!("ui-tool-cert-valid-to"))
                        .size(font_size::CAPTION)
                        .style(text::secondary)
                        .width(Length::Fixed(120.0)),
                    text(format!(
                        "{} ({} {})",
                        r.not_after,
                        r.days_remaining,
                        fl!("ui-tool-cert-days-remaining")
                    ))
                    .size(font_size::BODY)
                    .font(super::BOX_FONT),
                ]
                .spacing(spacing::SM),
                row![
                    text(fl!("ui-tool-cert-tls-version"))
                        .size(font_size::CAPTION)
                        .style(text::secondary)
                        .width(Length::Fixed(120.0)),
                    text(&r.tls_version)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT),
                ]
                .spacing(spacing::SM),
                row![
                    text(fl!("ui-tool-cert-key-size"))
                        .size(font_size::CAPTION)
                        .style(text::secondary)
                        .width(Length::Fixed(120.0)),
                    text(if r.key_size_bits > 0 {
                        format!("{} bits", r.key_size_bits)
                    } else {
                        "-".to_string()
                    })
                    .size(font_size::BODY)
                    .font(super::BOX_FONT),
                ]
                .spacing(spacing::SM),
                row![
                    text(fl!("ui-tool-cert-sans"))
                        .size(font_size::CAPTION)
                        .style(text::secondary)
                        .width(Length::Fixed(120.0)),
                    text(r.sans.join(", "))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT),
                ]
                .spacing(spacing::SM),
            ]
            .spacing(spacing::XS);

            let card = container(column![header_row, detail_rows].spacing(spacing::MD))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        }

        super::content_column(body)
    }
}
