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

//! Banner Grabber tool pane for probing network ports and inspecting service banners.

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
use heimdall_core::tools::banner_grab::{
    self, BannerProbeResult, DEFAULT_CONNECT_TIMEOUT_MS, DEFAULT_READ_TIMEOUT_MS,
};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Default common ports string.
const DEFAULT_PORTS: &str = "21,22,23,25,80,110,143,443,3306,3389,5432,6379,8080";
const WEB_PORTS: &str = "80,443,8000,8080,8443,8888,9000,9090";
const MAIL_PORTS: &str = "25,110,143,465,587,993,995";
const DB_PORTS: &str = "1433,1521,3306,5432,6379,27017";

/// Messages emitted by the Banner Grabber tool.
#[derive(Debug, Clone)]
pub enum BannerMessage {
    /// Target host changed.
    HostChanged(String),
    /// Ports specification changed.
    PortsChanged(String),
    /// Select common ports preset.
    SelectPresetCommon,
    /// Select web ports preset.
    SelectPresetWeb,
    /// Select mail ports preset.
    SelectPresetMail,
    /// Select database ports preset.
    SelectPresetDb,
    /// Start banner grabbing.
    Start,
    /// Stop current scan.
    Stop,
    /// All probes finished.
    Finished(Vec<BannerProbeResult>),
    /// Copy tabulated text to clipboard.
    CopyResults,
    /// Export CSV to clipboard.
    ExportCsv,
}

/// The Banner Grabber tool pane state.
#[derive(Debug)]
pub struct BannerPane {
    host: String,
    ports: String,
    running: bool,
    error: Option<String>,
    results: Vec<BannerProbeResult>,
    completed: usize,
    total: usize,
}

impl Default for BannerPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            ports: DEFAULT_PORTS.to_string(),
            running: false,
            error: None,
            results: Vec::new(),
            completed: 0,
            total: 0,
        }
    }
}

/// Outcome of updating the Banner Grabber tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl BannerPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: BannerMessage) -> Outcome {
        match message {
            BannerMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
                Outcome::None
            }
            BannerMessage::PortsChanged(val) => {
                self.ports = val;
                self.error = None;
                Outcome::None
            }
            BannerMessage::SelectPresetCommon => {
                self.ports = DEFAULT_PORTS.to_string();
                Outcome::None
            }
            BannerMessage::SelectPresetWeb => {
                self.ports = WEB_PORTS.to_string();
                Outcome::None
            }
            BannerMessage::SelectPresetMail => {
                self.ports = MAIL_PORTS.to_string();
                Outcome::None
            }
            BannerMessage::SelectPresetDb => {
                self.ports = DB_PORTS.to_string();
                Outcome::None
            }
            BannerMessage::Start => {
                let host = self.host.trim().to_string();
                if host.is_empty() {
                    self.error = Some(fl!("ui-tool-banner-error-host-required"));
                    return Outcome::None;
                }

                let ports = banner_grab::parse_ports(&self.ports);
                if ports.is_empty() {
                    self.error = Some(fl!("ui-tool-banner-error-host-required"));
                    return Outcome::None;
                }

                self.running = true;
                self.error = None;
                self.results.clear();
                self.completed = 0;
                self.total = ports.len();

                let task = Task::perform(
                    async move {
                        let mut results = Vec::with_capacity(ports.len());
                        for port in ports {
                            let h = host.clone();
                            let res = tokio::task::spawn_blocking(move || {
                                banner_grab::probe_port_banner(
                                    &h,
                                    port,
                                    DEFAULT_CONNECT_TIMEOUT_MS,
                                    DEFAULT_READ_TIMEOUT_MS,
                                )
                            })
                            .await
                            .unwrap_or_else(|_| BannerProbeResult {
                                port,
                                service: banner_grab::identify_service(port, None),
                                response_time: "-".into(),
                                banner: None,
                            });
                            results.push(res);
                        }
                        results
                    },
                    move |results| {
                        Message::Tool(tab, ToolMessage::Banner(BannerMessage::Finished(results)))
                    },
                );

                Outcome::Task(task)
            }
            BannerMessage::Stop => {
                self.running = false;
                Outcome::None
            }
            BannerMessage::Finished(results) => {
                self.running = false;
                self.completed = results.len();
                self.results = results;
                Outcome::None
            }
            BannerMessage::CopyResults => {
                let text = banner_grab::build_clipboard_text(&self.results);
                Outcome::Copy(CopySlot::BannerGrabResults, text)
            }
            BannerMessage::ExportCsv => {
                let csv = banner_grab::build_csv_export(&self.results);
                Outcome::Copy(CopySlot::BannerGrabCsv, csv)
            }
        }
    }

    /// Renders the Banner Grabber tool pane.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &'a ToolPane,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let colors = crate::themes::colors_of(theme);

        let host_input = column![
            text(fl!("ui-tool-banner-host-label"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text_input(&fl!("ui-tool-banner-host-placeholder"), &self.host)
                .on_input(move |v| Message::Tool(
                    tab,
                    ToolMessage::Banner(BannerMessage::HostChanged(v))
                ))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::FillPortion(3))
        .spacing(spacing::XS);

        let ports_input = column![
            text(fl!("ui-tool-banner-ports-label"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text_input(&fl!("ui-tool-banner-ports-placeholder"), &self.ports)
                .on_input(move |v| Message::Tool(
                    tab,
                    ToolMessage::Banner(BannerMessage::PortsChanged(v))
                ))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::FillPortion(4))
        .spacing(spacing::XS);

        let grab_btn = if self.running {
            button(
                text(fl!("ui-tool-banner-btn-stop"))
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD),
            )
            .padding(super::PRIMARY_PADDING)
            .style(styles::danger)
            .on_press(Message::Tool(tab, ToolMessage::Banner(BannerMessage::Stop)))
        } else {
            button(
                text(fl!("ui-tool-banner-btn-grab"))
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD),
            )
            .padding(super::PRIMARY_PADDING)
            .style(styles::primary)
            .on_press(Message::Tool(
                tab,
                ToolMessage::Banner(BannerMessage::Start),
            ))
        };

        let presets_row = row![
            button(text(fl!("ui-tool-banner-preset-common")).size(font_size::CAPTION))
                .padding([2.0, 6.0])
                .style(styles::subtle)
                .on_press(Message::Tool(
                    tab,
                    ToolMessage::Banner(BannerMessage::SelectPresetCommon)
                )),
            button(text(fl!("ui-tool-banner-preset-web")).size(font_size::CAPTION))
                .padding([2.0, 6.0])
                .style(styles::subtle)
                .on_press(Message::Tool(
                    tab,
                    ToolMessage::Banner(BannerMessage::SelectPresetWeb)
                )),
            button(text(fl!("ui-tool-banner-preset-mail")).size(font_size::CAPTION))
                .padding([2.0, 6.0])
                .style(styles::subtle)
                .on_press(Message::Tool(
                    tab,
                    ToolMessage::Banner(BannerMessage::SelectPresetMail)
                )),
            button(text(fl!("ui-tool-banner-preset-database")).size(font_size::CAPTION))
                .padding([2.0, 6.0])
                .style(styles::subtle)
                .on_press(Message::Tool(
                    tab,
                    ToolMessage::Banner(BannerMessage::SelectPresetDb)
                )),
        ]
        .spacing(spacing::XS);

        let controls_row = row![
            host_input,
            ports_input,
            container(grab_btn).align_y(Alignment::End),
        ]
        .spacing(spacing::MD)
        .align_y(Alignment::End);

        let mut body_col = column![controls_row, presets_row].spacing(spacing::SM);

        if let Some(err) = &self.error {
            body_col = body_col.push(text(err).size(font_size::BODY).color(colors.red));
        }

        if !self.results.is_empty() || self.running {
            let actions_row = row![
                text(fl!(
                    "ui-tool-banner-progress",
                    completed = self.completed,
                    total = self.total
                ))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .color(colors.blue)
                .width(Length::Fill),
                super::copy_button(
                    fl!("ui-tool-banner-btn-copy"),
                    state.copied(CopySlot::BannerGrabResults),
                    Message::Tool(tab, ToolMessage::Banner(BannerMessage::CopyResults),),
                    super::COPY_PADDING,
                ),
                super::copy_button(
                    fl!("ui-tool-banner-btn-export"),
                    state.copied(CopySlot::BannerGrabCsv),
                    Message::Tool(tab, ToolMessage::Banner(BannerMessage::ExportCsv),),
                    super::COPY_PADDING,
                ),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let mut table_rows = column!().spacing(spacing::XS);
            // Header
            table_rows = table_rows.push(
                row![
                    text(fl!("ui-tool-banner-col-port"))
                        .size(font_size::CAPTION)
                        .font(styles::SEMIBOLD)
                        .style(text::secondary)
                        .width(Length::Fixed(60.0)),
                    text(fl!("ui-tool-banner-col-service"))
                        .size(font_size::CAPTION)
                        .font(styles::SEMIBOLD)
                        .style(text::secondary)
                        .width(Length::Fixed(120.0)),
                    text(fl!("ui-tool-banner-col-time"))
                        .size(font_size::CAPTION)
                        .font(styles::SEMIBOLD)
                        .style(text::secondary)
                        .width(Length::Fixed(80.0)),
                    text(fl!("ui-tool-banner-col-banner"))
                        .size(font_size::CAPTION)
                        .font(styles::SEMIBOLD)
                        .style(text::secondary)
                        .width(Length::Fill),
                ]
                .spacing(spacing::MD),
            );

            for r in &self.results {
                let banner_str = r.banner.as_deref().unwrap_or("-");
                let banner_color = if r.banner.is_some() {
                    colors.green
                } else {
                    colors.comment
                };

                table_rows = table_rows.push(
                    row![
                        text(r.port.to_string())
                            .size(font_size::CAPTION)
                            .font(super::BOX_FONT)
                            .width(Length::Fixed(60.0)),
                        text(&r.service)
                            .size(font_size::CAPTION)
                            .font(super::BOX_FONT)
                            .width(Length::Fixed(120.0)),
                        text(&r.response_time)
                            .size(font_size::CAPTION)
                            .font(super::BOX_FONT)
                            .width(Length::Fixed(80.0)),
                        text(banner_str)
                            .size(font_size::CAPTION)
                            .font(super::BOX_FONT)
                            .color(banner_color)
                            .width(Length::Fill),
                    ]
                    .spacing(spacing::MD)
                    .align_y(Alignment::Center),
                );
            }

            body_col = body_col
                .push(actions_row)
                .push(scrollable(table_rows).height(Length::Fill));
        } else {
            body_col = body_col.push(
                container(
                    text(fl!("ui-tool-banner-empty-state"))
                        .size(font_size::BODY)
                        .style(text::secondary),
                )
                .padding(super::EMPTY_STATE_PADDING),
            );
        }

        let content = column![body_col]
            .spacing(spacing::LG)
            .padding(super::BODY_PADDING)
            .max_width(super::CONTENT_MAX_WIDTH);

        content.into()
    }
}
