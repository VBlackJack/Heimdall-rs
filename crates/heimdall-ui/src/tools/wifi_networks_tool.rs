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

use heimdall_app::TabId;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::wifi_scanner::{self, WifiEntry};

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
    /// Scan completed with detected wireless networks.
    Finished(Vec<WifiEntry>),
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
                            .unwrap_or_default()
                    },
                    move |res| {
                        Message::Tool(tab, super::ToolMessage::Wifi(WifiMessage::Finished(res)))
                    },
                );

                Outcome::Task(task)
            }
            WifiMessage::Finished(res) => {
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
                self.networks = res;
                Outcome::None
            }
            WifiMessage::CopyCsv => {
                if !self.networks.is_empty() {
                    let csv = wifi_scanner::build_wifi_csv(&self.networks);
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
            let table_headers = row![
                text(fl!("ui-tool-wifi-col-ssid"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(160.0)),
                text(fl!("ui-tool-wifi-col-bssid"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-wifi-col-signal"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(70.0)),
                text(fl!("ui-tool-wifi-col-channel"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(70.0)),
                text(fl!("ui-tool-wifi-col-auth"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(130.0)),
                text(fl!("ui-tool-wifi-col-encryption"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
                text(fl!("ui-tool-wifi-col-radio"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(1)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for w in &self.networks {
                let r = row![
                    text(&w.ssid)
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
                    text(&w.auth)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(130.0)),
                    text(&w.encryption)
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
        } else if !self.running {
            body = body.push(super::empty_state(fl!("ui-tool-wifi-empty-state")));
        }

        super::content_column(body)
    }
}
