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

//! The Route Table tool view: IPv4 active routing table inspection,
//! destination subnet masks, gateway resolution, interface mapping, and CSV export.

use heimdall_app::TabId;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::route_table::{self, RouteEntry};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the Route Table view.
#[derive(Debug, Clone)]
pub enum RouteTableMessage {
    /// Refresh routing table now.
    Refresh,
    /// Refresh completed with routing table entries.
    Finished(Vec<RouteEntry>),
    /// Copy CSV export to clipboard.
    CopyCsv,
}

/// The Route Table tool pane state.
#[derive(Debug, Default)]
pub struct RouteTablePane {
    running: bool,
    error: Option<String>,
    routes: Vec<RouteEntry>,
    last_count: usize,
    last_time: Option<String>,
}

/// Outcome of updating the Route Table tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl RouteTablePane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: RouteTableMessage) -> Outcome {
        match message {
            RouteTableMessage::Refresh => {
                self.running = true;
                self.error = None;

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(route_table::run_route_command)
                            .await
                            .unwrap_or_default()
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::Routes(RouteTableMessage::Finished(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            RouteTableMessage::Finished(res) => {
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
                self.routes = res;
                Outcome::None
            }
            RouteTableMessage::CopyCsv => {
                if !self.routes.is_empty() {
                    let csv = route_table::build_route_csv(&self.routes);
                    Outcome::Copy(CopySlot::RoutesCsv, csv)
                } else {
                    Outcome::None
                }
            }
        }
    }

    /// Renders the Route Table tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::Routes(msg));

        let refresh_btn = if self.running {
            super::action_button(fl!("ui-tool-routes-scanning"), false, None)
        } else {
            super::action_button(
                fl!("ui-tool-routes-refresh"),
                true,
                Some(send(RouteTableMessage::Refresh)),
            )
        };

        let copy_csv_btn = super::copy_button(
            fl!("ui-tool-routes-copy-csv"),
            state.copied(CopySlot::RoutesCsv),
            send(RouteTableMessage::CopyCsv),
            super::COPY_PADDING,
        );

        let top_row = row![
            text(fl!("ui-tool-routes-name"))
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
                "ui-tool-routes-status",
                count = self.last_count,
                time = time.as_str()
            );
            body = body.push(
                text(status_msg)
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }

        if !self.routes.is_empty() {
            let table_headers = row![
                text(fl!("ui-tool-routes-col-dest"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-routes-col-mask"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-routes-col-gateway"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-routes-col-interface"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-routes-col-metric"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(1)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for rt in &self.routes {
                let r = row![
                    text(&rt.destination)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(&rt.mask)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(&rt.gateway)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(&rt.interface)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(&rt.metric)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(1)),
                ]
                .spacing(spacing::SM);

                entry_rows = entry_rows.push(r);
            }

            let card = container(scrollable(entry_rows))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        } else if !self.running {
            body = body.push(super::empty_state(fl!("ui-tool-routes-empty-state")));
        }

        super::content_column(body)
    }
}
