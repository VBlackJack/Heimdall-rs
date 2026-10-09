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

//! The URL encoder, as the C# `UrlEncoderView` and `UrlEncoderViewModel`: two boxes kept in
//! step, the decoded text encoded as it is typed and the encoded text decoded as it is
//! typed; strict component encoding re-encodes what is decoded.

use heimdall_app::TabId;
use heimdall_core::tools::url_codec;
use iced::widget::text_editor::{Action, Content};
use iced::widget::{checkbox, column, row, text};
use iced::{Element, Length};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Height of each box, as the C# `MinHeight` and `MaxHeight` of its boxes.
const BOX_MIN_HEIGHT: f32 = 80.0;
const BOX_MAX_HEIGHT: f32 = 200.0;

/// Room under a label, as the C# `MarginLabelBottom`.
const LABEL_GAP: f32 = 4.0;

/// Room under a box, as the C# `MarginStackItem`.
const BOX_GAP: f32 = 8.0;

/// What the URL encoder is asked.
#[derive(Debug, Clone)]
pub enum UrlMessage {
    /// Something done in the decoded box.
    Decoded(Action),
    /// Something done in the encoded box.
    Encoded(Action),
    /// Strict component encoding on or off.
    Component(bool),
    /// Copy the decoded text.
    CopyDecoded,
    /// Copy the encoded text.
    CopyEncoded,
}

/// The URL encoder's state, as the C# view model's.
#[derive(Debug, Default)]
pub struct UrlPane {
    decoded: Content,
    encoded: Content,
    component: bool,
}

impl UrlPane {
    /// Applies `message`; what a copy button copies, when one is pressed.
    pub fn update(&mut self, message: UrlMessage) -> Option<(CopySlot, String)> {
        match message {
            UrlMessage::Decoded(action) => {
                let edit = action.is_edit();
                self.decoded.perform(action);
                // As the C# `OnDecodedTextChanged`: the encoded box follows.
                if edit {
                    self.encode();
                }
            }
            UrlMessage::Encoded(action) => {
                let edit = action.is_edit();
                self.encoded.perform(action);
                // As the C# `OnEncodedTextChanged`: the decoded box follows. .NET's
                // `UnescapeDataString` refuses nothing, so no decode error is shown.
                if edit {
                    self.decoded = Content::with_text(&url_codec::decode(&self.encoded.text()));
                }
            }
            UrlMessage::Component(on) => {
                self.component = on;
                // As the C# `OnComponentModeChanged`: what is decoded encoded again.
                if !self.decoded_text().is_empty() {
                    self.encode();
                }
            }
            UrlMessage::CopyDecoded => return Some((CopySlot::UrlDecoded, self.decoded_text())),
            UrlMessage::CopyEncoded => return Some((CopySlot::UrlEncoded, self.encoded_text())),
        }
        None
    }

    /// The decoded text, as the C# `DecodedText`: iced's box ends its text with a line
    /// break the C# box does not have.
    #[must_use]
    pub fn decoded_text(&self) -> String {
        super::box_text(&self.decoded)
    }

    /// The encoded text.
    #[must_use]
    pub fn encoded_text(&self) -> String {
        super::box_text(&self.encoded)
    }

    /// The encoded box set from the decoded one.
    fn encode(&mut self) {
        let decoded = self.decoded_text();
        self.encoded = Content::with_text(&url_codec::encode(&decoded, self.component));
    }

    /// The tool's page, as the C# `UrlEncoderView.xaml`: the decoded box, the encoded box,
    /// each with its copy button, and strict component encoding.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Url(message));
        let labelled = |label: String, slot: CopySlot, copy: UrlMessage| {
            row![
                text(label)
                    .size(font_size::BODY)
                    .style(text::secondary)
                    .width(Length::Fill),
                super::copy_button(
                    fl!("ui-tool-urlenc-copy"),
                    state.copied(slot),
                    send(copy),
                    super::COPY_PADDING,
                ),
            ]
            .align_y(iced::Alignment::End)
        };
        let decoded = super::text_box(
            &self.decoded,
            Some(fl!("ui-tool-urlenc-decoded-placeholder")),
        )
        .min_height(BOX_MIN_HEIGHT)
        .max_height(BOX_MAX_HEIGHT)
        .on_action(move |action| send(UrlMessage::Decoded(action)));
        let encoded = super::text_box(
            &self.encoded,
            Some(fl!("ui-tool-urlenc-encoded-placeholder")),
        )
        .min_height(BOX_MIN_HEIGHT)
        .max_height(BOX_MAX_HEIGHT)
        .on_action(move |action| send(UrlMessage::Encoded(action)));
        super::content_column(
            column![
                labelled(
                    fl!("ui-tool-urlenc-decoded"),
                    CopySlot::UrlDecoded,
                    UrlMessage::CopyDecoded
                ),
                column![decoded].padding(iced::Padding {
                    top: LABEL_GAP,
                    bottom: BOX_GAP,
                    ..iced::Padding::ZERO
                }),
                labelled(
                    fl!("ui-tool-urlenc-encoded"),
                    CopySlot::UrlEncoded,
                    UrlMessage::CopyEncoded
                ),
                column![encoded].padding(iced::Padding {
                    top: LABEL_GAP,
                    bottom: BOX_GAP,
                    ..iced::Padding::ZERO
                }),
                checkbox(self.component)
                    .label(fl!("ui-tool-urlenc-component"))
                    .on_toggle(move |on| send(UrlMessage::Component(on)))
                    .style(styles::quiet_checkbox)
                    .text_size(font_size::BODY),
            ]
            .spacing(spacing::XS),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iced::widget::text_editor::Edit;

    use super::*;

    fn paste(text: &str) -> Action {
        Action::Edit(Edit::Paste(Arc::new(text.to_owned())))
    }

    #[test]
    fn typing_in_either_box_fills_the_other() {
        let mut pane = UrlPane::default();
        pane.update(UrlMessage::Decoded(paste("https://example.com/a b?x=1")));
        assert_eq!(pane.encoded_text(), "https://example.com/a%20b?x=1");
        let mut pane = UrlPane::default();
        pane.update(UrlMessage::Encoded(paste("caf%C3%A9")));
        assert_eq!(pane.decoded_text(), "café");
    }

    #[test]
    fn strict_component_encoding_reencodes_what_is_decoded() {
        let mut pane = UrlPane::default();
        pane.update(UrlMessage::Decoded(paste("a/b?c=d")));
        assert_eq!(pane.encoded_text(), "a/b?c=d");
        pane.update(UrlMessage::Component(true));
        assert_eq!(pane.encoded_text(), "a%2Fb%3Fc%3Dd");
    }

    #[test]
    fn a_copy_button_copies_its_box() {
        let mut pane = UrlPane::default();
        pane.update(UrlMessage::Decoded(paste("a b")));
        assert_eq!(
            pane.update(UrlMessage::CopyEncoded),
            Some((CopySlot::UrlEncoded, "a%20b".to_owned()))
        );
        assert_eq!(
            pane.update(UrlMessage::CopyDecoded),
            Some((CopySlot::UrlDecoded, "a b".to_owned()))
        );
    }
}
