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

//! Wake-on-LAN tool pane for sending magic packets across the network.

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
use heimdall_core::tools::wake_on_lan::{self, DEFAULT_BROADCAST_ADDRESS, DEFAULT_WOL_PORT};
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Background, Border, Element, Length, Theme};

use super::{ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, radius, spacing};

/// Messages emitted by the Wake-on-LAN tool.
#[derive(Debug, Clone)]
pub enum WolMessage {
    /// MAC address text changed.
    MacChanged(String),
    /// Broadcast address text changed.
    BroadcastChanged(String),
    /// UDP port text changed.
    PortChanged(String),
    /// Trigger sending the magic packet.
    SendPacket,
}

/// The Wake-on-LAN tool pane state.
#[derive(Debug)]
pub struct WolPane {
    mac: String,
    broadcast: String,
    port: String,
    feedback: Option<(bool, String)>,
}

impl Default for WolPane {
    fn default() -> Self {
        Self {
            mac: String::new(),
            broadcast: DEFAULT_BROADCAST_ADDRESS.to_string(),
            port: DEFAULT_WOL_PORT.to_string(),
            feedback: None,
        }
    }
}

/// Outcome of updating the Wake-on-LAN tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
}

impl WolPane {
    /// Handles tool messages.
    pub fn update(&mut self, message: WolMessage) -> Outcome {
        match message {
            WolMessage::MacChanged(val) => {
                self.mac = val;
                self.feedback = None;
                Outcome::None
            }
            WolMessage::BroadcastChanged(val) => {
                self.broadcast = val;
                self.feedback = None;
                Outcome::None
            }
            WolMessage::PortChanged(val) => {
                self.port = val;
                self.feedback = None;
                Outcome::None
            }
            WolMessage::SendPacket => {
                let Some(mac_bytes) = wake_on_lan::parse_mac_address(&self.mac) else {
                    self.feedback = Some((false, fl!("ui-tool-wol-error-invalid-mac")));
                    return Outcome::None;
                };

                let port: u16 = self.port.trim().parse().unwrap_or(DEFAULT_WOL_PORT);
                let broadcast = self.broadcast.trim();
                let b_addr = if broadcast.is_empty() {
                    DEFAULT_BROADCAST_ADDRESS
                } else {
                    broadcast
                };

                match wake_on_lan::send_magic_packet(&mac_bytes, b_addr, port) {
                    Ok(()) => {
                        let normalized_mac = wake_on_lan::format_mac_address(&mac_bytes);
                        let dest = format!("{b_addr}:{port}");
                        self.feedback = Some((
                            true,
                            fl!("ui-tool-wol-status-sent", mac = normalized_mac, dest = dest),
                        ));
                    }
                    Err(err) => {
                        self.feedback = Some((false, fl!("ui-tool-wol-status-error", error = err)));
                    }
                }

                Outcome::None
            }
        }
    }

    /// Renders the Wake-on-LAN tool pane.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        _state: &'a ToolPane,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let colors = crate::themes::colors_of(theme);

        let mac_input = column![
            text(fl!("ui-tool-wol-mac-label"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text_input(&fl!("ui-tool-wol-mac-placeholder"), &self.mac)
                .on_input(move |v| Message::Tool(tab, ToolMessage::Wol(WolMessage::MacChanged(v))))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::Fill)
        .spacing(spacing::XS);

        let broadcast_input = column![
            text(fl!("ui-tool-wol-broadcast-label"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text_input(&fl!("ui-tool-wol-broadcast-placeholder"), &self.broadcast)
                .on_input(move |v| Message::Tool(
                    tab,
                    ToolMessage::Wol(WolMessage::BroadcastChanged(v))
                ))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::FillPortion(3))
        .spacing(spacing::XS);

        let port_input = column![
            text(fl!("ui-tool-wol-port-label"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text_input("9", &self.port)
                .on_input(move |v| Message::Tool(tab, ToolMessage::Wol(WolMessage::PortChanged(v))))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::FillPortion(1))
        .spacing(spacing::XS);

        let send_btn = button(
            text(fl!("ui-tool-wol-btn-send"))
                .size(font_size::BODY)
                .font(styles::SEMIBOLD),
        )
        .padding(super::PRIMARY_PADDING)
        .style(styles::primary)
        .on_press(Message::Tool(tab, ToolMessage::Wol(WolMessage::SendPacket)));

        let mut body_col = column![
            mac_input,
            row![broadcast_input, port_input].spacing(spacing::MD),
            row![send_btn],
        ]
        .spacing(spacing::MD);

        if let Some((success, msg)) = &self.feedback {
            let (bg, border_color, text_color) = if *success {
                (colors.green.scale_alpha(0.15), colors.green, colors.green)
            } else {
                (colors.red.scale_alpha(0.15), colors.red, colors.red)
            };

            let feedback_card = container(
                text(msg.clone())
                    .size(font_size::BODY)
                    .color(text_color)
                    .font(styles::SEMIBOLD),
            )
            .padding([10.0, 14.0])
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    radius: radius::SM.into(),
                    width: 1.0,
                    color: border_color,
                },
                ..Default::default()
            });

            body_col = body_col.push(feedback_card);
        }

        let content = column![body_col]
            .spacing(spacing::LG)
            .padding(super::BODY_PADDING)
            .max_width(super::CONTENT_MAX_WIDTH);

        content.into()
    }
}
