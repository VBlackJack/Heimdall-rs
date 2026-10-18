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

use heimdall_app::{TabId, server_text};
use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::network_interfaces::{
    self, InterfaceKind, NicSnapshot, OperStatus, SpeedUnit,
};
use heimdall_core::tools::system_program::ProgramError;

use super::system_tables::{listing_error_text, read_time_now};
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
    /// Refresh completed with the interfaces, or why they could not be read.
    Finished(Result<Vec<NicSnapshot>, ProgramError>),
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
    /// Reads the list when the tab opens, as the C# `Initialize` runs `RefreshCommand`
    /// (`NetworkInterfacesView.xaml.cs:61`): the user does not have to press Refresh first.
    pub fn open(&mut self, tab: TabId) -> Task<Message> {
        match self.update(tab, NetIfMessage::Refresh) {
            Outcome::Task(task) => task,
            _ => Task::none(),
        }
    }

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
                            .unwrap_or_else(|error| {
                                Err(ProgramError::NotStarted(error.to_string()))
                            })
                    },
                    move |res| {
                        Message::Tool(tab, super::ToolMessage::NetIf(NetIfMessage::Finished(res)))
                    },
                );

                Outcome::Task(task)
            }
            NetIfMessage::Finished(Ok(interfaces)) => {
                self.running = false;
                self.last_count = interfaces.len();
                self.last_time = Some(read_time_now());
                self.interfaces = interfaces;
                Outcome::None
            }
            NetIfMessage::Finished(Err(error)) => {
                // A failure is said, never shown as an empty list.
                self.running = false;
                self.error = Some(listing_error_text(&error));
                self.interfaces.clear();
                self.last_time = None;
                Outcome::None
            }
            NetIfMessage::CopyCsv => {
                if !self.interfaces.is_empty() {
                    let csv = network_interfaces::build_interfaces_csv(
                        &self.interfaces,
                        &column_titles(),
                        |nic| {
                            [
                                kind_text(nic.kind),
                                status_text(nic.status),
                                speed_text(nic.speed_bps),
                                dhcp_text(nic.dhcp),
                            ]
                        },
                    );
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
            let [name, kind, status, speed, mac, ipv4, subnet, gateway, dhcp] = column_titles();
            let title = |label: String, width: f32| {
                text(label)
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(width))
            };
            let table_headers = row![
                title(name, 160.0),
                title(kind, 90.0),
                title(status, 80.0),
                title(speed, 80.0),
                title(mac, 140.0),
                title(ipv4, 120.0),
                title(subnet, 120.0),
                title(gateway, 120.0),
                title(dhcp, 60.0),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for nic in &self.interfaces {
                let r = row![
                    text(server_text(&nic.name))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(160.0)),
                    text(kind_text(nic.kind))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                    text(status_text(nic.status))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(80.0)),
                    text(speed_text(nic.speed_bps))
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
                    text(dhcp_text(nic.dhcp))
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
        } else if !self.running && self.error.is_none() {
            body = body.push(super::empty_state(fl!("ui-tool-netif-empty-state")));
        }

        super::content_column(body)
    }
}

/// The table's column titles, in the user's language: the view's and the CSV export's.
fn column_titles() -> [String; 9] {
    [
        fl!("ui-tool-netif-col-name"),
        fl!("ui-tool-netif-col-type"),
        fl!("ui-tool-netif-col-status"),
        fl!("ui-tool-netif-col-speed"),
        fl!("ui-tool-netif-col-mac"),
        fl!("ui-tool-netif-col-ipv4"),
        fl!("ui-tool-netif-col-subnet"),
        fl!("ui-tool-netif-col-gateway"),
        fl!("ui-tool-netif-col-dhcp"),
    ]
}

/// An interface's kind in the user's language.
fn kind_text(kind: InterfaceKind) -> String {
    match kind {
        InterfaceKind::Ethernet => fl!("ui-tool-netif-kind-ethernet"),
        InterfaceKind::Wireless => fl!("ui-tool-netif-kind-wireless"),
        InterfaceKind::Loopback => fl!("ui-tool-netif-kind-loopback"),
        InterfaceKind::Tunnel => fl!("ui-tool-netif-kind-tunnel"),
        InterfaceKind::Ppp => fl!("ui-tool-netif-kind-ppp"),
        InterfaceKind::MobileBroadband => fl!("ui-tool-netif-kind-mobile"),
        InterfaceKind::Unknown => fl!("ui-tool-netif-kind-unknown"),
        InterfaceKind::Other(code) => fl!("ui-tool-netif-kind-other", code = code),
    }
}

/// An interface's operational status in the user's language.
fn status_text(status: OperStatus) -> String {
    match status {
        OperStatus::Up => fl!("ui-tool-netif-status-up"),
        OperStatus::Down => fl!("ui-tool-netif-status-down"),
        OperStatus::Testing => fl!("ui-tool-netif-status-testing"),
        OperStatus::Unknown => fl!("ui-tool-netif-status-unknown"),
        OperStatus::Dormant => fl!("ui-tool-netif-status-dormant"),
        OperStatus::NotPresent => fl!("ui-tool-netif-status-not-present"),
        OperStatus::LowerLayerDown => fl!("ui-tool-netif-status-lower-layer-down"),
    }
}

/// A link speed in its unit, in the user's language; unknown when the system does not
/// know it, never a guess.
fn speed_text(bits_per_second: Option<u64>) -> String {
    let Some(bits_per_second) = bits_per_second else {
        return fl!("ui-tool-netif-speed-unknown");
    };
    match network_interfaces::speed_parts(bits_per_second) {
        (value, SpeedUnit::Kilobits) => fl!("ui-tool-netif-speed-kbps", value = value),
        (value, SpeedUnit::Megabits) => fl!("ui-tool-netif-speed-mbps", value = value),
        (value, SpeedUnit::Gigabits) => fl!("ui-tool-netif-speed-gbps", value = value),
    }
}

/// Whether an address came from DHCP, in the user's language, as the C#
/// `NicFormatter.FormatDhcp`.
fn dhcp_text(dhcp: Option<bool>) -> String {
    match dhcp {
        Some(true) => fl!("ui-tool-netif-dhcp-yes"),
        Some(false) => fl!("ui-tool-netif-dhcp-no"),
        None => fl!("ui-tool-netif-dhcp-unknown"),
    }
}

#[cfg(test)]
mod tests {
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
            tool: ToolId::NetIf,
            title: String::new(),
            context: heimdall_app::tools::ToolContext::default(),
        }));
        app.tabs[0].id
    }

    #[test]
    fn opening_the_interfaces_tab_reads_them_without_a_refresh_press() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = NetIfPane::default();
        assert!(!pane.running, "nothing runs before the tab opens");
        let _task = pane.open(tab);
        assert!(pane.running, "the C# Initialize runs the refresh at once");
    }

    fn nic(speed_bps: Option<u64>) -> NicSnapshot {
        NicSnapshot {
            name: "Ethernet".to_owned(),
            kind: InterfaceKind::Ethernet,
            status: OperStatus::Down,
            speed_bps,
            mac: "00:11:22:33:44:55".to_owned(),
            ipv4: String::new(),
            subnet: String::new(),
            gateway: String::new(),
            dhcp: Some(false),
        }
    }

    #[test]
    fn kinds_statuses_speeds_and_dhcp_are_said_in_the_users_language() {
        let kinds = [
            InterfaceKind::Ethernet,
            InterfaceKind::Wireless,
            InterfaceKind::Loopback,
            InterfaceKind::Tunnel,
            InterfaceKind::Ppp,
            InterfaceKind::MobileBroadband,
            InterfaceKind::Unknown,
            InterfaceKind::Other(53),
        ];
        let texts: std::collections::HashSet<String> = kinds.into_iter().map(kind_text).collect();
        assert_eq!(texts.len(), kinds.len());
        assert!(texts.iter().any(|text| text.contains("53")));

        let statuses = [
            OperStatus::Up,
            OperStatus::Down,
            OperStatus::Testing,
            OperStatus::Unknown,
            OperStatus::Dormant,
            OperStatus::NotPresent,
            OperStatus::LowerLayerDown,
        ];
        let texts: std::collections::HashSet<String> =
            statuses.into_iter().map(status_text).collect();
        assert_eq!(texts.len(), statuses.len());

        assert_eq!(speed_text(None), fl!("ui-tool-netif-speed-unknown"));
        assert_eq!(
            speed_text(Some(5_764_800_000)),
            fl!("ui-tool-netif-speed-gbps", value = 5)
        );
        assert_eq!(
            speed_text(Some(100_000_000)),
            fl!("ui-tool-netif-speed-mbps", value = 100)
        );
        assert_ne!(dhcp_text(Some(true)), dhcp_text(Some(false)));
        assert_ne!(dhcp_text(None), dhcp_text(Some(false)));
        let all = [
            kind_text(InterfaceKind::Wireless),
            status_text(OperStatus::Up),
        ];
        assert!(all.iter().all(|text| !text.contains("ui-tool-")));
    }

    #[test]
    fn a_failed_listing_is_said_and_a_good_one_exports_in_the_users_words() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = NetIfPane::default();
        let _ = pane.update(tab, NetIfMessage::Finished(Ok(vec![nic(None)])));
        let Outcome::Copy(CopySlot::NetIfCsv, csv) = pane.update(tab, NetIfMessage::CopyCsv) else {
            panic!("a CSV export");
        };
        assert_eq!(
            csv.lines().nth(1),
            Some(
                format!(
                    "\"Ethernet\",\"{}\",\"{}\",\"{}\",\"00:11:22:33:44:55\",\"\",\"\",\"\",\"{}\"",
                    kind_text(InterfaceKind::Ethernet),
                    status_text(OperStatus::Down),
                    fl!("ui-tool-netif-speed-unknown"),
                    dhcp_text(Some(false))
                )
                .as_str()
            )
        );

        let _ = pane.update(tab, NetIfMessage::Finished(Err(ProgramError::NotAvailable)));
        assert_eq!(
            pane.error,
            Some(fl!("ui-tool-systable-error-not-available"))
        );
        assert!(pane.interfaces.is_empty());
        assert_eq!(pane.last_time, None);
    }
}
