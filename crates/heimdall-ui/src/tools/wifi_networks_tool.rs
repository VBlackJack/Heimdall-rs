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

//! The Wi-Fi Networks tool view: 802.11 wireless network scanner,
//! SSID, BSSID, signal strength, channel, cipher, and CSV export.

use heimdall_app::{TabId, server_text};
use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::system_program::ProgramError;
use heimdall_core::tools::wifi_scanner::{self, WifiEntry};

use super::system_tables::{listing_error_text, read_time_now};
use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the Wi-Fi Networks view.
#[derive(Debug, Clone)]
pub enum WifiMessage {
    /// Scan Wi-Fi networks in range now.
    Scan,
    /// Scan completed with the networks in range, or why they could not be listed.
    Finished(Result<Vec<WifiEntry>, ProgramError>),
    /// Copy CSV export to clipboard.
    CopyCsv,
}

/// The Wi-Fi Networks tool pane state.
#[derive(Debug, Default)]
pub struct WifiNetworksPane {
    running: bool,
    error: Option<String>,
    networks: Vec<WifiEntry>,
    last_count: usize,
    last_time: Option<String>,
}

/// Outcome of updating the Wi-Fi Networks tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl WifiNetworksPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: WifiMessage) -> Outcome {
        match message {
            WifiMessage::Scan => {
                self.running = true;
                self.error = None;

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(wifi_scanner::run_wifi_command)
                            .await
                            .unwrap_or_else(|error| {
                                Err(ProgramError::NotStarted(error.to_string()))
                            })
                    },
                    move |res| {
                        Message::Tool(tab, super::ToolMessage::Wifi(WifiMessage::Finished(res)))
                    },
                );

                Outcome::Task(task)
            }
            WifiMessage::Finished(Ok(networks)) => {
                self.running = false;
                self.last_count = networks.len();
                self.last_time = Some(read_time_now());
                self.networks = networks;
                Outcome::None
            }
            WifiMessage::Finished(Err(error)) => {
                // The WLAN service stopped or the location permission refused is said,
                // never shown as "0 networks", as the C# `ErrorText`.
                self.running = false;
                self.error = Some(listing_error_text(&error));
                self.networks.clear();
                self.last_time = None;
                Outcome::None
            }
            WifiMessage::CopyCsv => {
                if !self.networks.is_empty() {
                    let csv = wifi_scanner::build_wifi_csv(&self.networks, &column_titles());
                    Outcome::Copy(CopySlot::WifiCsv, csv)
                } else {
                    Outcome::None
                }
            }
        }
    }

    /// Renders the Wi-Fi Networks tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::Wifi(msg));

        let scan_btn = if self.running {
            super::action_button(fl!("ui-tool-wifi-scanning"), false, None)
        } else {
            super::action_button(
                fl!("ui-tool-wifi-scan"),
                true,
                Some(send(WifiMessage::Scan)),
            )
        };

        let copy_csv_btn = super::copy_button(
            fl!("ui-tool-wifi-copy-csv"),
            state.copied(CopySlot::WifiCsv),
            send(WifiMessage::CopyCsv),
            super::COPY_PADDING,
        );

        let top_row = row![
            text(fl!("ui-tool-wifi-name"))
                .size(font_size::SUBTITLE)
                .width(Length::Fill),
            scan_btn,
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
                "ui-tool-wifi-status",
                count = self.last_count,
                time = time.as_str()
            );
            body = body.push(
                text(status_msg)
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }

        if !self.networks.is_empty() {
            let [ssid, bssid, signal, channel, auth, encryption, radio] = column_titles();
            let title = |label: String, width: Length| {
                text(label)
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(width)
            };
            let table_headers = row![
                title(ssid, Length::Fixed(160.0)),
                title(bssid, Length::Fixed(140.0)),
                title(signal, Length::Fixed(70.0)),
                title(channel, Length::Fixed(70.0)),
                title(auth, Length::Fixed(130.0)),
                title(encryption, Length::Fixed(90.0)),
                title(radio, Length::FillPortion(1)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for w in &self.networks {
                let r = row![
                    text(server_text(&w.ssid))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(160.0)),
                    text(&w.bssid)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(&w.signal)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(70.0)),
                    text(&w.channel)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(70.0)),
                    text(server_text(&w.auth))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(130.0)),
                    text(server_text(&w.encryption))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                    text(&w.radio_type)
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
            body = body.push(super::empty_state(fl!("ui-tool-wifi-empty-state")));
        }

        super::content_column(body)
    }
}

/// The table's column titles, in the user's language: the view's and the CSV export's, as
/// the C# `BuildClipboardText` heads its copy with them.
fn column_titles() -> [String; 7] {
    [
        fl!("ui-tool-wifi-col-ssid"),
        fl!("ui-tool-wifi-col-bssid"),
        fl!("ui-tool-wifi-col-signal"),
        fl!("ui-tool-wifi-col-channel"),
        fl!("ui-tool-wifi-col-auth"),
        fl!("ui-tool-wifi-col-encryption"),
        fl!("ui-tool-wifi-col-radio"),
    ]
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
            tool: ToolId::Wifi,
            title: String::new(),
            context: heimdall_app::tools::ToolContext::default(),
        }));
        app.tabs[0].id
    }

    #[test]
    fn a_stopped_wlan_service_is_said_not_shown_as_no_network() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = WifiNetworksPane::default();
        let message = "The Wireless AutoConfig Service (wlansvc) is not running.";
        let _ = pane.update(
            tab,
            WifiMessage::Finished(Err(ProgramError::Failed {
                code: Some(1),
                detail: message.to_owned(),
            })),
        );
        let error = pane.error.clone().expect("an error shown");
        assert!(error.contains(message), "{error}");
        assert!(pane.networks.is_empty());
        assert_eq!(pane.last_time, None);
        assert!(matches!(
            pane.update(tab, WifiMessage::CopyCsv),
            Outcome::None
        ));
    }

    #[test]
    fn the_export_quotes_an_ssid_and_defuses_a_formula() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = WifiNetworksPane::default();
        let network = WifiEntry {
            ssid: "=HYPERLINK(\"http://x\",\"y\")".to_owned(),
            bssid: "10:20:30:40:50:60".to_owned(),
            signal: "82%".to_owned(),
            signal_value: 82,
            channel: "36".to_owned(),
            auth: "WPA2-Personal".to_owned(),
            encryption: "CCMP".to_owned(),
            radio_type: "802.11ax".to_owned(),
        };
        let _ = pane.update(tab, WifiMessage::Finished(Ok(vec![network])));
        let Outcome::Copy(CopySlot::WifiCsv, csv) = pane.update(tab, WifiMessage::CopyCsv) else {
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
                "\"'=HYPERLINK(\"\"http://x\"\",\"\"y\"\")\",\"10:20:30:40:50:60\",\"82%\",\"36\",\
                 \"WPA2-Personal\",\"CCMP\",\"802.11ax\""
            )
        );
    }
}
