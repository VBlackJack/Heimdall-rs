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

//! The HMAC generator, as the C# `HmacGeneratorView` and `HmacGeneratorViewModel`: an
//! algorithm, a secret key typed hidden or shown, a message; the HMAC of the message's UTF-8
//! under the key's, in hexadecimal or Base64, recomputed as either is typed; its length; an
//! expected HMAC pasted and checked.
//!
//! The key is never logged nor kept: it lives in the tab, wiped from memory when the tab
//! closes or the key is retyped.

use std::fmt;

use heimdall_app::TabId;
use heimdall_core::tools::hash_computer::HashAlgorithm;
use heimdall_core::tools::hmac_computer::{self, HMAC_ALGORITHMS, HmacOutputFormat};
use iced::widget::text_editor::{Action, Content};
use iced::widget::{column, container, pick_list, radio, row, text, text_input, tooltip};
use iced::{Element, Length};
use zeroize::Zeroizing;

use super::crypto_parts::{self, Tone};
use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::icons::{self, Icon};
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Height of the message's box, as the C# `Height="120"`.
const INPUT_HEIGHT: f32 = 120.0;

/// Height of the output's box: one line.
const OUTPUT_HEIGHT: f32 = 36.0;

/// Room between the sections, as the C# `MarginSectionSeparator`.
const SECTION_GAP: f32 = 16.0;

/// Room between the two format choices, as the C# `Margin="0,0,16,0"`.
const CHOICE_GAP: f32 = 16.0;

/// Bits in a byte, for the length said under the output.
const BITS_PER_BYTE: usize = 8;

/// What the HMAC generator is asked.
#[derive(Clone)]
pub enum HmacMessage {
    /// The algorithm chosen.
    Algorithm(HmacChoice),
    /// The key typed.
    Key(String),
    /// Show or hide the key, as the C# eye button.
    ToggleKey,
    /// Something done in the message's box.
    Input(Action),
    /// The output's format chosen.
    Format(HmacOutputFormat),
    /// Something done in the output's box, which does not change it.
    Output(Action),
    /// The expected HMAC typed.
    Verify(String),
    /// Copy the output.
    Copy,
}

impl fmt::Debug for HmacMessage {
    /// The key typed is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Algorithm(choice) => write!(f, "Algorithm({choice})"),
            Self::Key(_) => f.write_str("Key(..)"),
            Self::ToggleKey => f.write_str("ToggleKey"),
            Self::Input(_) => f.write_str("Input(..)"),
            Self::Format(format) => write!(f, "Format({format:?})"),
            Self::Output(_) => f.write_str("Output(..)"),
            Self::Verify(_) => f.write_str("Verify(..)"),
            Self::Copy => f.write_str("Copy"),
        }
    }
}

/// An algorithm in the list, by its C# name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HmacChoice(pub HashAlgorithm);

impl fmt::Display for HmacChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(hmac_computer::display_name(self.0).unwrap_or_default())
    }
}

/// The HMAC generator's state, as the C# view model's.
pub struct HmacPane {
    algorithm: HashAlgorithm,
    key: Zeroizing<String>,
    key_shown: bool,
    input: Content,
    format: HmacOutputFormat,
    /// The HMAC of the message under the key; `None` while either is empty.
    code: Option<Vec<u8>>,
    output: Content,
    verify: String,
}

impl fmt::Debug for HmacPane {
    /// The key is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HmacPane")
            .field("algorithm", &self.algorithm)
            .field("key_shown", &self.key_shown)
            .field("format", &self.format)
            .field("code", &self.code.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for HmacPane {
    /// A new tab's state, as the C# `Initialize`: the first algorithm, HMAC-SHA256,
    /// hexadecimal, the key hidden.
    fn default() -> Self {
        Self {
            algorithm: HMAC_ALGORITHMS[0],
            key: Zeroizing::new(String::new()),
            key_shown: false,
            input: Content::new(),
            format: HmacOutputFormat::Hex,
            code: None,
            output: Content::new(),
            verify: String::new(),
        }
    }
}

impl HmacPane {
    /// Applies `message`; what the copy button copies, when it is pressed.
    pub fn update(&mut self, message: HmacMessage) -> Option<String> {
        match message {
            HmacMessage::Algorithm(HmacChoice(algorithm)) => {
                self.algorithm = algorithm;
                self.compute();
            }
            HmacMessage::Key(typed) => {
                // The key replaced is wiped as it goes.
                self.key = Zeroizing::new(typed);
                self.compute();
            }
            HmacMessage::ToggleKey => self.key_shown = !self.key_shown,
            HmacMessage::Input(action) => {
                let edit = action.is_edit();
                self.input.perform(action);
                if edit {
                    self.compute();
                }
            }
            HmacMessage::Format(format) => {
                // As the C# `OnOutputFormatChanged`: the same code written again.
                self.format = format;
                self.write_output();
            }
            HmacMessage::Output(action) => super::read_only(&mut self.output, action),
            HmacMessage::Verify(typed) => self.verify = typed,
            HmacMessage::Copy => {
                let output = self.output_text();
                return (!output.is_empty()).then_some(output);
            }
        }
        None
    }

    /// The HMAC of the message under the key, as the C# `ComputeAsync`
    /// (`HmacGeneratorViewModel.cs:186-235`): nothing while either is empty.
    fn compute(&mut self) {
        let message = super::box_text(&self.input);
        self.code = if message.is_empty() || self.key.is_empty() {
            None
        } else {
            hmac_computer::compute(self.algorithm, self.key.as_bytes(), message.as_bytes()).ok()
        };
        self.write_output();
    }

    /// The output's box written again, as the C# `RefreshFormattedOutput`.
    fn write_output(&mut self) {
        self.output = Content::with_text(&self.output_text());
    }

    /// The code as the format says, or nothing.
    #[must_use]
    pub fn output_text(&self) -> String {
        self.code
            .as_deref()
            .map(|code| hmac_computer::format(code, self.format))
            .unwrap_or_default()
    }

    /// What the check of the expected HMAC says, as the C# `UpdateVerifyResult`
    /// (`HmacGeneratorViewModel.cs:247-266`): nothing while it or the code is missing.
    fn verdict(&self) -> Option<(String, Tone)> {
        let code = self.code.as_deref()?;
        if self.verify.trim().is_empty() {
            return None;
        }
        Some(match hmac_computer::verify(code, &self.verify) {
            Some(_) => (fl!("ui-tool-hmac-match"), Tone::Success),
            None => (fl!("ui-tool-hmac-no-match"), Tone::Error),
        })
    }

    /// The tool's page, as the C# `HmacGeneratorView.xaml`: the algorithm, the key with its
    /// eye, the message, the format, the output with its copy button and its length, the
    /// expected HMAC and what its check says.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Hmac(message));
        let algorithms = pick_list(
            HMAC_ALGORITHMS.map(HmacChoice),
            Some(HmacChoice(self.algorithm)),
            move |choice| send(HmacMessage::Algorithm(choice)),
        )
        .width(Length::Fill)
        .padding(super::INPUT_PADDING)
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .text_size(font_size::BODY_LARGE);
        let key = self.key_row(send);
        let input = super::text_box(&self.input, Some(fl!("ui-tool-hmac-input-placeholder")))
            .height(INPUT_HEIGHT)
            .on_action(move |action| send(HmacMessage::Input(action)));
        let formats = row![
            radio(
                fl!("ui-tool-hmac-format-hex"),
                HmacOutputFormat::Hex,
                Some(self.format),
                move |format| send(HmacMessage::Format(format)),
            )
            .text_size(font_size::BODY),
            radio(
                fl!("ui-tool-hmac-format-base64"),
                HmacOutputFormat::Base64,
                Some(self.format),
                move |format| send(HmacMessage::Format(format)),
            )
            .text_size(font_size::BODY),
        ]
        .spacing(CHOICE_GAP);
        let output = self.output_row(send, state);
        let length = self.code.as_deref().map(|code| {
            let bytes = code.len();
            let bits = bytes * BITS_PER_BYTE;
            crypto_parts::said(
                fl!("ui-tool-hmac-byte-length", bytes = bytes, bits = bits),
                Tone::Quiet,
                font_size::CAPTION,
                false,
            )
        });
        let verdict = self
            .verdict()
            .map(|(said, tone)| crypto_parts::said(said, tone, font_size::BODY, true));
        super::content_column(
            column![
                crypto_parts::field_label(fl!("ui-tool-hmac-algorithm")),
                algorithms,
                iced::widget::space().height(SECTION_GAP - spacing::SM),
                crypto_parts::field_label(fl!("ui-tool-hmac-key")),
                key,
                iced::widget::space().height(SECTION_GAP - spacing::SM),
                crypto_parts::field_label(fl!("ui-tool-hmac-input")),
                input,
                iced::widget::space().height(SECTION_GAP - spacing::SM),
                crypto_parts::field_label(fl!("ui-tool-hmac-format")),
                formats,
            ]
            .push(
                self.code
                    .is_none()
                    .then(|| crypto_parts::empty_state(fl!("ui-tool-hmac-empty"))),
            )
            .push(crypto_parts::field_label(fl!("ui-tool-hmac-output")))
            .push(output)
            .push(length)
            .push(iced::widget::space().height(SECTION_GAP - spacing::SM))
            .push(crypto_parts::field_label(fl!("ui-tool-hmac-verify")))
            .push(
                text_input(&fl!("ui-tool-hmac-verify-placeholder"), &self.verify)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY_LARGE)
                    .padding(super::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| send(HmacMessage::Verify(typed))),
            )
            .push(verdict)
            .spacing(spacing::SM),
        )
    }
    /// The key, hidden or shown, and its eye button, as the C# `PasswordBox`, `TextBox` and
    /// `BtnToggleKey`.
    fn key_row<'a>(
        &'a self,
        send: impl Fn(HmacMessage) -> Message + Copy + 'a,
    ) -> Element<'a, Message> {
        let eye = if self.key_shown {
            Icon::EyeHidden
        } else {
            Icon::Eye
        };
        row![
            text_input(&fl!("ui-tool-hmac-key-placeholder"), &self.key)
                .secure(!self.key_shown)
                .font(super::BOX_FONT)
                .size(font_size::BODY_LARGE)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(HmacMessage::Key(typed))),
            tooltip(
                icons::button(eye)
                    .style(styles::secondary)
                    .on_press(send(HmacMessage::ToggleKey)),
                text(fl!("ui-tool-hmac-toggle-key")).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
        ]
        .spacing(spacing::XS)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// The output in the accent and its copy button.
    fn output_row<'a>(
        &'a self,
        send: impl Fn(HmacMessage) -> Message + Copy + 'a,
        state: &ToolPane,
    ) -> Element<'a, Message> {
        row![
            crypto_parts::tinted_box(&self.output, crypto_parts::accent)
                .height(OUTPUT_HEIGHT)
                .on_action(move |action| send(HmacMessage::Output(action))),
            super::copy_button(
                fl!("ui-tool-hmac-copy"),
                state.copied(CopySlot::HmacOutput),
                send(HmacMessage::Copy),
                super::COPY_PADDING,
            ),
        ]
        .spacing(spacing::XS)
        .align_y(iced::Alignment::Center)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iced::widget::text_editor::Edit;

    use super::*;

    fn typed(pane: &mut HmacPane, message: &str) {
        pane.update(HmacMessage::Input(Action::Edit(Edit::Paste(Arc::new(
            message.to_owned(),
        )))));
    }

    #[test]
    fn the_hmac_follows_the_key_the_message_and_the_algorithm() {
        let mut pane = HmacPane::default();
        assert_eq!(pane.algorithm, HashAlgorithm::Sha256, "the C# first choice");
        typed(&mut pane, "what do ya want for nothing?");
        assert!(pane.code.is_none(), "no key yet");
        pane.update(HmacMessage::Key("Jefe".to_owned()));
        // RFC 4231, test case 2.
        assert_eq!(
            pane.output_text(),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        pane.update(HmacMessage::Algorithm(HmacChoice(HashAlgorithm::Md5)));
        assert_eq!(pane.output_text(), "750c783e6ab0b503eaa86e310a5db738");
        pane.update(HmacMessage::Format(HmacOutputFormat::Base64));
        assert_eq!(pane.output_text(), "dQx4PmqwtQPqqG4xCl23OA==");
        assert_eq!(
            pane.update(HmacMessage::Copy).as_deref(),
            Some("dQx4PmqwtQPqqG4xCl23OA==")
        );
        pane.update(HmacMessage::Key(String::new()));
        assert!(pane.code.is_none() && pane.output_text().is_empty());
        assert_eq!(pane.update(HmacMessage::Copy), None, "nothing to copy");
    }

    #[test]
    fn the_expected_hmac_is_checked_in_either_format() {
        let mut pane = HmacPane::default();
        typed(&mut pane, "message");
        pane.update(HmacMessage::Key("key".to_owned()));
        pane.update(HmacMessage::Verify("  ".to_owned()));
        assert_eq!(pane.verdict(), None);
        let hex = pane.output_text().to_uppercase();
        pane.update(HmacMessage::Verify(hex));
        assert_eq!(pane.verdict().map(|(_, tone)| tone), Some(Tone::Success));
        pane.update(HmacMessage::Verify("nope".to_owned()));
        assert_eq!(pane.verdict().map(|(_, tone)| tone), Some(Tone::Error));
    }

    #[test]
    fn the_key_is_hidden_until_shown_and_never_written_out() {
        let mut pane = HmacPane::default();
        pane.update(HmacMessage::Key("hunter2".to_owned()));
        assert!(!pane.key_shown);
        pane.update(HmacMessage::ToggleKey);
        assert!(pane.key_shown);
        assert!(!format!("{pane:?}").contains("hunter2"));
        assert!(!format!("{:?}", HmacMessage::Key("hunter2".to_owned())).contains("hunter2"));
    }
}
