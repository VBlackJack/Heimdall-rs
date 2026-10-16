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

use heimdall_app::{TabId, server_text};
use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::open_ports::{self, IDLE_PID, PortEntry};
use heimdall_core::tools::system_program::ProgramError;

use super::system_tables::{listing_error_text, read_time_now};
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
    /// Refresh finished with the sockets, or why they could not be read.
    Finished(Result<Vec<PortEntry>, ProgramError>),
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
                        tokio::task::spawn_blocking(open_ports::run_open_ports)
                            .await
                            .unwrap_or_else(|error| {
                                Err(ProgramError::NotStarted(error.to_string()))
                            })
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
            OpenPortsMessage::Finished(Ok(ports)) => {
                self.running = false;
                self.last_count = ports.len();
                self.last_time = Some(read_time_now());
                self.ports = ports;
                Outcome::None
            }
            OpenPortsMessage::Finished(Err(error)) => {
                // A failure is said, never shown as an empty list.
                self.running = false;
                self.error = Some(listing_error_text(&error));
                self.ports.clear();
                self.last_time = None;
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
                    let csv =
                        open_ports::build_open_ports_csv(&matching, &column_titles(), process_text);
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
            p.protocol.name().to_lowercase().contains(&q)
                || p.local_address.to_lowercase().contains(&q)
                || p.local_port.to_string().contains(&q)
                || p.remote_address.to_lowercase().contains(&q)
                || p.remote_port.to_string().contains(&q)
                || p.state.name().to_lowercase().contains(&q)
                || p.pid.is_some_and(|pid| pid.to_string().contains(&q))
                || process_text(p).to_lowercase().contains(&q)
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
            let [
                proto,
                local_addr,
                local_port,
                remote_addr,
                remote_port,
                state_title,
                pid,
                process,
            ] = column_titles();
            let title = |label: String, width: Length| {
                text(label)
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(width)
            };
            let table_headers = row![
                title(proto, Length::Fixed(60.0)),
                title(local_addr, Length::Fixed(140.0)),
                title(local_port, Length::Fixed(80.0)),
                title(remote_addr, Length::Fixed(140.0)),
                title(remote_port, Length::Fixed(80.0)),
                title(state_title, Length::Fixed(90.0)),
                title(pid, Length::Fixed(60.0)),
                title(process, Length::FillPortion(2)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for p in matching.iter().take(MAX_DISPLAY_ROWS) {
                let r = row![
                    text(p.protocol.name())
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
                    text(p.state.name())
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                    text(p.pid.map(|pid| pid.to_string()).unwrap_or_default())
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(60.0)),
                    text(process_text(p))
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
        } else if !self.running && self.ports.is_empty() && self.error.is_none() {
            body = body.push(super::empty_state(fl!("ui-tool-openports-empty-state")));
        }

        super::content_column(body)
    }
}

/// The table's column titles, in the user's language: the view's and the CSV export's.
fn column_titles() -> [String; 8] {
    [
        fl!("ui-tool-openports-col-proto"),
        fl!("ui-tool-openports-col-local-addr"),
        fl!("ui-tool-openports-col-local-port"),
        fl!("ui-tool-openports-col-remote-addr"),
        fl!("ui-tool-openports-col-remote-port"),
        fl!("ui-tool-openports-col-state"),
        fl!("ui-tool-openports-col-pid"),
        fl!("ui-tool-openports-col-process"),
    ]
}

/// The process owning a socket as shown: its name made safe to show; as the C#
/// `ResolveProcessName`, its identifier in brackets when it has no name any more (a process
/// gone since), the idle process named in the user's language; nothing when the system does
/// not say which process it is.
fn process_text(entry: &PortEntry) -> String {
    match (&entry.process_name, entry.pid) {
        (Some(name), _) => server_text(name),
        (None, Some(IDLE_PID)) => fl!("ui-tool-openports-process-idle"),
        (None, Some(pid)) => fl!("ui-tool-openports-process-pid", pid = pid),
        (None, None) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use heimdall_app::tools::ToolId;
    use heimdall_app::{App, AppConfig, Message as AppMessage, ToolsMessage};
    use heimdall_core::tools::open_ports::{PortState, Protocol};
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
            tool: ToolId::OpenPorts,
            title: String::new(),
        }));
        app.tabs[0].id
    }

    fn entry(pid: Option<u32>, process_name: Option<&str>) -> PortEntry {
        PortEntry {
            protocol: Protocol::Tcp,
            local_address: "0.0.0.0".to_owned(),
            local_port: 135,
            remote_address: "0.0.0.0".to_owned(),
            remote_port: 0,
            state: PortState::Listen,
            pid,
            process_name: process_name.map(str::to_owned),
        }
    }

    #[test]
    fn a_process_is_named_or_said_by_its_identifier_in_the_users_language() {
        assert_eq!(process_text(&entry(Some(4), Some("System"))), "System");
        assert_eq!(
            process_text(&entry(Some(0), None)),
            fl!("ui-tool-openports-process-idle")
        );
        assert_eq!(process_text(&entry(Some(4242), None)), "[4242]");
        assert_eq!(process_text(&entry(None, None)), "");
        assert_eq!(
            process_text(&entry(Some(1), Some("evil\u{202E}gpj.exe"))),
            "evilgpj.exe"
        );
    }

    #[test]
    fn a_failed_listing_is_said_not_shown_as_no_port() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = OpenPortsPane::default();
        let _ = pane.update(
            tab,
            OpenPortsMessage::Finished(Ok(vec![entry(Some(4), Some("System"))])),
        );
        assert_eq!(pane.ports.len(), 1);
        assert!(pane.last_time.is_some());
        let _ = pane.update(tab, OpenPortsMessage::Finished(Err(ProgramError::TimedOut)));
        assert_eq!(pane.error, Some(fl!("ui-tool-systable-error-timeout")));
        assert!(pane.ports.is_empty());
        assert_eq!(pane.last_time, None);
        assert!(!pane.running);
    }

    #[test]
    fn the_csv_export_has_the_users_column_titles_and_quoted_cells() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = OpenPortsPane::default();
        let _ = pane.update(
            tab,
            OpenPortsMessage::Finished(Ok(vec![entry(Some(0), None)])),
        );
        let Outcome::Copy(CopySlot::OpenPortsCsv, csv) =
            pane.update(tab, OpenPortsMessage::CopyCsv)
        else {
            panic!("a CSV export");
        };
        let mut lines = csv.lines();
        assert_eq!(
            lines.next(),
            Some(format!("\"{}\"", column_titles().join("\",\"")).as_str())
        );
        assert_eq!(
            lines.next(),
            Some(
                format!(
                    "\"TCP\",\"0.0.0.0\",\"135\",\"0.0.0.0\",\"0\",\"LISTEN\",\"0\",\"{}\"",
                    fl!("ui-tool-openports-process-idle")
                )
                .as_str()
            )
        );
    }
}
