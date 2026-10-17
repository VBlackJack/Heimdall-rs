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

use heimdall_app::{TabId, server_text};
use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::route_table::{self, Gateway, RouteEntry};
use heimdall_core::tools::system_program::ProgramError;

use super::system_tables::{listing_error_text, read_time_now};
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
    /// Refresh completed with the routes, or why they could not be read.
    Finished(Result<Vec<RouteEntry>, ProgramError>),
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
                            .unwrap_or_else(|error| {
                                Err(ProgramError::NotStarted(error.to_string()))
                            })
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
            RouteTableMessage::Finished(Ok(routes)) => {
                self.running = false;
                self.last_count = routes.len();
                self.last_time = Some(read_time_now());
                self.routes = routes;
                Outcome::None
            }
            RouteTableMessage::Finished(Err(error)) => {
                // A failure is said, never shown as an empty list.
                self.running = false;
                self.error = Some(listing_error_text(&error));
                self.routes.clear();
                self.last_time = None;
                Outcome::None
            }
            RouteTableMessage::CopyCsv => {
                if !self.routes.is_empty() {
                    let csv = route_table::build_route_csv(
                        &self.routes,
                        &column_titles(),
                        &fl!("ui-tool-routes-on-link"),
                    );
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
            let [destination, mask, gateway, interface, metric] = column_titles();
            let title = |label: String, width: Length| {
                text(label)
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(width)
            };
            let table_headers = row![
                title(destination, Length::Fixed(140.0)),
                title(mask, Length::Fixed(140.0)),
                title(gateway, Length::Fixed(140.0)),
                title(interface, Length::Fixed(140.0)),
                title(metric, Length::FillPortion(1)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for rt in &self.routes {
                let r = row![
                    text(rt.destination.to_string())
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(rt.mask.to_string())
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(gateway_text(&rt.gateway))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(server_text(&rt.interface))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(rt.metric.to_string())
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
        } else if !self.running && self.error.is_none() {
            body = body.push(super::empty_state(fl!("ui-tool-routes-empty-state")));
        }

        super::content_column(body)
    }
}

/// The table's column titles, in the user's language: the view's and the CSV export's.
fn column_titles() -> [String; 5] {
    [
        fl!("ui-tool-routes-col-dest"),
        fl!("ui-tool-routes-col-mask"),
        fl!("ui-tool-routes-col-gateway"),
        fl!("ui-tool-routes-col-interface"),
        fl!("ui-tool-routes-col-metric"),
    ]
}

/// A route's next hop as shown: an on-link route in the user's language.
fn gateway_text(gateway: &Gateway) -> String {
    match gateway {
        Gateway::OnLink => fl!("ui-tool-routes-on-link"),
        Gateway::Address(address) => server_text(address),
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use heimdall_app::tools::ToolId;
    use heimdall_app::{App, AppConfig, Message as AppMessage, ToolsMessage};
    use heimdall_ssh::AgentSource;
    use heimdall_term::GridSize;

    use super::*;

    /// A tool tab's identifier, from an application of its own in `dir`.
    fn tab(dir: &std::path::Path) -> TabId {
        let mut app = App::new(AppConfig {
            profiles_file: dir.join("profiles.toml"),
            known_hosts: dir.join("known_hosts"),
            legacy_dir: None,
            agent: AgentSource::Disabled,
            initial_grid: GridSize { cols: 80, rows: 24 },
            files_start: dir.to_owned(),
            system_credentials: heimdall_app::SystemCredentials::memory(),
        });
        let _ = app.update(AppMessage::Tools(ToolsMessage::Open {
            tool: ToolId::Routes,
            title: String::new(),
            context: heimdall_app::tools::ToolContext::default(),
        }));
        app.tabs[0].id
    }

    #[test]
    fn an_on_link_route_is_said_in_the_users_language_view_and_export_alike() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = RouteTablePane::default();
        let route = RouteEntry {
            destination: Ipv4Addr::new(192, 168, 1, 0),
            mask: Ipv4Addr::new(255, 255, 255, 0),
            gateway: Gateway::OnLink,
            interface: "192.168.1.66".to_owned(),
            metric: 281,
        };
        assert_eq!(gateway_text(&route.gateway), fl!("ui-tool-routes-on-link"));
        assert_eq!(
            gateway_text(&Gateway::Address("192.168.1.254".to_owned())),
            "192.168.1.254"
        );
        let _ = pane.update(tab, RouteTableMessage::Finished(Ok(vec![route])));
        let Outcome::Copy(CopySlot::RoutesCsv, csv) = pane.update(tab, RouteTableMessage::CopyCsv)
        else {
            panic!("a CSV export");
        };
        assert_eq!(
            csv.lines().nth(1),
            Some(
                format!(
                    "\"192.168.1.0\",\"255.255.255.0\",\"{}\",\"192.168.1.66\",\"281\"",
                    fl!("ui-tool-routes-on-link")
                )
                .as_str()
            )
        );
    }

    #[test]
    fn a_failed_listing_is_said_not_shown_as_no_route() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = RouteTablePane::default();
        let _ = pane.update(
            tab,
            RouteTableMessage::Finished(Err(ProgramError::Failed {
                code: Some(1),
                detail: "Access denied".to_owned(),
            })),
        );
        assert_eq!(
            pane.error,
            Some(fl!(
                "ui-tool-systable-error-failed-detail",
                error = "Access denied"
            ))
        );
        assert!(pane.routes.is_empty());
        assert!(!pane.running);
    }
}
