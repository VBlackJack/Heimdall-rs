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

//! The IP address converter, as the C# `IpConverterView` and `IpConverterViewModel`: an
//! IPv4 address converted as it is typed, from any of its forms into all of them; each
//! copied.

use heimdall_app::TabId;
use heimdall_core::tools::ip_codec::{self, IpConversion};
use iced::Element;
use iced::widget::{column, text_input};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// A form of the address, in the C# rows' order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpField {
    /// Dotted decimal.
    Dotted,
    /// An integer.
    Decimal,
    /// Hexadecimal.
    Hex,
    /// Dotted binary.
    Binary,
    /// IPv4-mapped IPv6.
    MappedIpv6,
}

impl IpField {
    /// Every row, in order.
    const ALL: [Self; 5] = [
        Self::Dotted,
        Self::Decimal,
        Self::Hex,
        Self::Binary,
        Self::MappedIpv6,
    ];
}

/// What the IP converter is asked.
#[derive(Debug, Clone)]
pub enum IpConverterMessage {
    /// The input typed: converted at once.
    InputEdited(String),
    /// Copy a form.
    Copy(IpField),
}

/// The IP converter's state, as the C# view model's.
#[derive(Debug, Default)]
pub struct IpConverterPane {
    input: String,
    result: Option<IpConversion>,
    error: bool,
}

impl IpConverterPane {
    /// Applies `message`; what a copy button copies, when one is pressed.
    pub fn update(&mut self, message: IpConverterMessage) -> Option<(CopySlot, String)> {
        match message {
            IpConverterMessage::InputEdited(typed) => {
                // As the C# `Convert` (`IpConverterViewModel.cs:110-137`): nothing typed is
                // the empty state, else every form or the error.
                self.input = typed;
                let trimmed = self.input.trim();
                self.result = ip_codec::convert(trimmed);
                self.error = self.result.is_none() && !trimmed.is_empty();
            }
            IpConverterMessage::Copy(field) => {
                let value = self.value(field);
                return (!value.is_empty()).then_some((CopySlot::IpConverter(field), value));
            }
        }
        None
    }

    /// Whether the input was not read.
    #[cfg(test)]
    #[must_use]
    pub fn has_error(&self) -> bool {
        self.error
    }

    /// Whether neither a result nor the error shows, as the C# `IsEmptyState`.
    #[must_use]
    pub fn is_empty_state(&self) -> bool {
        !self.error && self.result.is_none()
    }

    /// The text of `field`; empty when nothing is converted.
    #[must_use]
    pub fn value(&self, field: IpField) -> String {
        let Some(result) = &self.result else {
            return String::new();
        };
        match field {
            IpField::Dotted => result.dotted.clone(),
            IpField::Decimal => result.decimal.clone(),
            IpField::Hex => result.hex.clone(),
            IpField::Binary => result.binary.clone(),
            IpField::MappedIpv6 => result.mapped_ipv6.clone(),
        }
    }

    /// The tool's page, as the C# `IpConverterView.xaml`: the input, then the error, the
    /// empty state or each form with its copy button.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::IpConverter(message));
        let mut page = column![
            super::field_label(fl!("ui-tool-ipconv-input")),
            text_input(&fl!("ui-tool-ipconv-placeholder"), &self.input)
                .font(super::BOX_FONT)
                .size(font_size::BODY_LARGE)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(IpConverterMessage::InputEdited(typed))),
        ]
        .spacing(spacing::SM);
        if self.error {
            page = page.push(super::error_line(fl!("ui-tool-ipconv-error-invalid")));
        }
        if self.is_empty_state() {
            // A short page: its empty state fills the room left, as the C#'s.
            page = page.push(super::empty_state(fl!("ui-tool-ipconv-empty")));
        }
        if self.result.is_some() {
            let rows = IpField::ALL.into_iter().map(|field| {
                let label = match field {
                    IpField::Dotted => fl!("ui-tool-ipconv-dotted"),
                    IpField::Decimal => fl!("ui-tool-ipconv-integer"),
                    IpField::Hex => fl!("ui-tool-ipconv-hex"),
                    IpField::Binary => fl!("ui-tool-ipconv-binary"),
                    IpField::MappedIpv6 => fl!("ui-tool-ipconv-mapped"),
                };
                let copy = super::copy_button(
                    fl!("ui-tool-copy-value"),
                    state.copied(CopySlot::IpConverter(field)),
                    send(IpConverterMessage::Copy(field)),
                    super::COPY_PADDING,
                );
                super::result_row(label, self.value(field), copy)
            });
            page = page.push(super::results_card(rows));
        }
        super::tool_body(page.max_width(super::CONTENT_MAX_WIDTH))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(pane: &mut IpConverterPane, input: &str) {
        let _ = pane.update(IpConverterMessage::InputEdited(input.to_owned()));
    }

    #[test]
    fn nothing_typed_is_the_empty_state() {
        let mut pane = IpConverterPane::default();
        assert!(pane.is_empty_state());
        typed(&mut pane, "   ");
        assert!(pane.is_empty_state());
        assert!(!pane.has_error());
    }

    #[test]
    fn an_address_fills_every_form() {
        let mut pane = IpConverterPane::default();
        typed(&mut pane, " 192.168.1.1 ");
        assert!(!pane.is_empty_state());
        assert_eq!(pane.value(IpField::Dotted), "192.168.1.1");
        assert_eq!(pane.value(IpField::Decimal), "3232235777");
        assert_eq!(pane.value(IpField::Hex), "0xC0A80101");
        assert_eq!(
            pane.value(IpField::Binary),
            "11000000.10101000.00000001.00000001"
        );
        assert_eq!(pane.value(IpField::MappedIpv6), "::ffff:c0a8:0101");
    }

    #[test]
    fn something_not_read_says_so_and_clears_the_forms() {
        let mut pane = IpConverterPane::default();
        typed(&mut pane, "192.168.1.1");
        typed(&mut pane, "invalid");
        assert!(pane.has_error());
        assert!(!pane.is_empty_state());
        assert_eq!(pane.value(IpField::Dotted), "");
    }

    #[test]
    fn a_copy_button_copies_its_form_and_nothing_when_empty() {
        let mut pane = IpConverterPane::default();
        assert_eq!(pane.update(IpConverterMessage::Copy(IpField::Hex)), None);
        typed(&mut pane, "10.0.0.1");
        assert_eq!(
            pane.update(IpConverterMessage::Copy(IpField::Dotted)),
            Some((CopySlot::IpConverter(IpField::Dotted), "2.0.0.1".to_owned()))
        );
    }
}
