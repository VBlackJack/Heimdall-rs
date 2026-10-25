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

//! Wake-on-LAN tool pane for sending magic packets across the network, as the C#
//! `WakeOnLanViewModel`: the packet is sent off the interface thread, through the same
//! sender as a profile's Wake-on-LAN (`heimdall_app::wake_on_lan`).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::module_name_repetitions,
    clippy::redundant_closure_for_method_calls,
    clippy::too_many_lines
)]

use std::net::SocketAddr;

use heimdall_app::{TabId, server_text, wake_on_lan};
use heimdall_core::metadata::MacAddress;
use heimdall_core::tools::wake_on_lan::{
    DEFAULT_BROADCAST_ADDRESS, DEFAULT_WOL_PORT, WolInputError, destination,
};
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Background, Border, Element, Length, Task, Theme};

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
    /// The packet for `.0` to `.1` was sent, or why not, in the system's words.
    Sent(MacAddress, SocketAddr, Result<(), String>),
}

/// What the tool last said.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Feedback {
    /// The fields cannot be sent.
    Invalid(WolInputError),
    /// The packet is on its way.
    Sending,
    /// The packet was sent.
    Sent(MacAddress, SocketAddr),
    /// The packet could not be sent, the system's words made safe.
    Failed(String),
}

/// The Wake-on-LAN tool pane state.
#[derive(Debug)]
pub struct WolPane {
    mac: String,
    broadcast: String,
    port: String,
    feedback: Option<Feedback>,
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
    Task(Task<Message>),
}

impl WolPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: WolMessage) -> Outcome {
        match self.apply(message) {
            Some(task) => Outcome::Task(
                task.map(move |message| Message::Tool(tab, ToolMessage::Wol(message))),
            ),
            None => Outcome::None,
        }
    }

    /// Handles a message, as [`Self::update`] without the tab: the send to run, if any.
    fn apply(&mut self, message: WolMessage) -> Option<Task<WolMessage>> {
        match message {
            WolMessage::MacChanged(val) => {
                self.mac = val;
                self.feedback = None;
                None
            }
            WolMessage::BroadcastChanged(val) => {
                self.broadcast = val;
                self.feedback = None;
                None
            }
            WolMessage::PortChanged(val) => {
                self.port = val;
                self.feedback = None;
                None
            }
            WolMessage::SendPacket => {
                // One send at a time, as the C# `CanSend`.
                if self.feedback == Some(Feedback::Sending) {
                    return None;
                }
                let (mac, to) = match destination(&self.mac, &self.broadcast, &self.port) {
                    Ok(checked) => checked,
                    Err(problem) => {
                        self.feedback = Some(Feedback::Invalid(problem));
                        return None;
                    }
                };
                self.feedback = Some(Feedback::Sending);
                Some(Task::perform(
                    async move {
                        wake_on_lan::send_to(mac, to)
                            .await
                            .map_err(|error| server_text(&error.to_string()))
                    },
                    move |sent| WolMessage::Sent(mac, to, sent),
                ))
            }
            WolMessage::Sent(mac, to, sent) => {
                self.feedback = Some(match sent {
                    Ok(()) => Feedback::Sent(mac, to),
                    Err(error) => Feedback::Failed(error),
                });
                None
            }
        }
    }

    /// What the tool says, in the user's language, and whether it went well.
    fn feedback_text(feedback: &Feedback) -> (bool, String) {
        match feedback {
            Feedback::Invalid(WolInputError::InvalidMac) => {
                (false, fl!("ui-tool-wol-error-invalid-mac"))
            }
            Feedback::Invalid(WolInputError::InvalidBroadcast) => {
                (false, fl!("ui-tool-wol-error-invalid-broadcast"))
            }
            Feedback::Invalid(WolInputError::InvalidPort) => (false, fl!("ui-tool-net-error-port")),
            Feedback::Sending => (true, fl!("ui-tool-wol-status-sending")),
            Feedback::Sent(mac, to) => (
                true,
                fl!(
                    "ui-tool-wol-status-sent",
                    mac = mac.to_string(),
                    dest = to.to_string()
                ),
            ),
            Feedback::Failed(error) => (
                false,
                fl!("ui-tool-wol-status-error", error = error.as_str()),
            ),
        }
    }

    /// Renders the Wake-on-LAN tool pane.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        _state: &'a ToolPane,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let default_port = DEFAULT_WOL_PORT.to_string();

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
            text_input(DEFAULT_BROADCAST_ADDRESS, &self.broadcast)
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
            text_input(&default_port, &self.port)
                .on_input(move |v| Message::Tool(tab, ToolMessage::Wol(WolMessage::PortChanged(v))))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::FillPortion(1))
        .spacing(spacing::XS);

        let sending = self.feedback == Some(Feedback::Sending);
        let send_btn = button(
            text(fl!("ui-tool-wol-btn-send"))
                .size(font_size::BODY)
                .font(styles::SEMIBOLD),
        )
        .padding(super::PRIMARY_PADDING)
        .style(styles::primary)
        .on_press_maybe(
            (!sending).then_some(Message::Tool(tab, ToolMessage::Wol(WolMessage::SendPacket))),
        );

        let mut body_col = column![
            mac_input,
            row![broadcast_input, port_input].spacing(spacing::MD),
            row![send_btn],
        ]
        .spacing(spacing::MD);

        if let Some(feedback) = &self.feedback {
            let (success, msg) = Self::feedback_text(feedback);
            let (bg, border_color, text_color) = feedback_colors(theme, success);

            let feedback_card = container(
                text(msg)
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

/// How strongly the notice's colour fills behind it.
const FEEDBACK_TINT_ALPHA: f32 = 0.15;

/// The fill, the border and the text of the notice that says whether the packet was sent: the
/// text lifted to read on its own tint.
pub(crate) fn feedback_colors(
    theme: &Theme,
    success: bool,
) -> (iced::Color, iced::Color, iced::Color) {
    let colors = crate::themes::colors_of(theme);
    let (hue, text) = if success {
        (colors.green, styles::success_text_color(theme))
    } else {
        (colors.red, styles::error_text_color(theme))
    };
    (
        hue.scale_alpha(FEEDBACK_TINT_ALPHA),
        hue,
        styles::text_on_tint(theme, text, hue, FEEDBACK_TINT_ALPHA),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_fields_are_said_and_nothing_is_sent() {
        let mut pane = WolPane::default();
        pane.apply(WolMessage::MacChanged("0\u{00E9}000000000".to_owned()));
        assert!(pane.apply(WolMessage::SendPacket).is_none());
        assert_eq!(
            pane.feedback,
            Some(Feedback::Invalid(WolInputError::InvalidMac))
        );
        pane.apply(WolMessage::MacChanged("00:11:22:33:44:55".to_owned()));
        pane.apply(WolMessage::BroadcastChanged("lan.example".to_owned()));
        assert!(pane.apply(WolMessage::SendPacket).is_none());
        assert_eq!(
            pane.feedback,
            Some(Feedback::Invalid(WolInputError::InvalidBroadcast))
        );
        pane.apply(WolMessage::BroadcastChanged(
            DEFAULT_BROADCAST_ADDRESS.to_owned(),
        ));
        pane.apply(WolMessage::PortChanged("70000".to_owned()));
        assert!(pane.apply(WolMessage::SendPacket).is_none());
        assert_eq!(
            pane.feedback,
            Some(Feedback::Invalid(WolInputError::InvalidPort))
        );
    }

    #[test]
    fn a_send_runs_off_the_interface_thread_one_at_a_time() {
        let mut pane = WolPane::default();
        pane.apply(WolMessage::MacChanged("00:11:22:33:44:55".to_owned()));
        assert!(pane.apply(WolMessage::SendPacket).is_some());
        assert_eq!(pane.feedback, Some(Feedback::Sending));
        assert!(pane.apply(WolMessage::SendPacket).is_none());
    }
}
