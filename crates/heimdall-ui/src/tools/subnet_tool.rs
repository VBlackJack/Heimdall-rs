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

//! The subnet calculator, as the C# `SubnetCalculatorView`: an address and its prefix
//! broken down as it is typed, IPv4 or IPv6, by computation alone; each value copied.

use heimdall_app::TabId;
use heimdall_core::tools::number_text;
use heimdall_core::tools::subnet_calculator::{self, HostCount, Subnet};
use iced::Element;
use iced::widget::{column, text_input};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// A value of the breakdown, in the C# rows' order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubnetField {
    /// The network address.
    Network,
    /// The broadcast address; IPv4 only.
    Broadcast,
    /// The subnet mask; IPv4 only.
    Mask,
    /// The first host.
    FirstHost,
    /// The last host.
    LastHost,
    /// How many hosts.
    TotalHosts,
    /// The network and its prefix.
    Cidr,
    /// The wildcard mask; IPv4 only.
    Wildcard,
}

impl SubnetField {
    /// Every row, in order.
    const ALL: [Self; 8] = [
        Self::Network,
        Self::Broadcast,
        Self::Mask,
        Self::FirstHost,
        Self::LastHost,
        Self::TotalHosts,
        Self::Cidr,
        Self::Wildcard,
    ];
}

/// What the subnet calculator is asked.
#[derive(Debug, Clone)]
pub enum SubnetMessage {
    /// The input typed: calculated at once.
    InputEdited(String),
    /// Enter in it.
    Calculate,
    /// Copy a value.
    Copy(SubnetField),
}

/// The subnet calculator's state, as the C# view's.
#[derive(Debug, Default)]
pub struct SubnetPane {
    input: String,
    result: Option<Subnet>,
    error: bool,
}

impl SubnetPane {
    /// Applies `message`; what a copy button copies, when one is pressed.
    pub fn update(&mut self, message: SubnetMessage) -> Option<(CopySlot, String)> {
        match message {
            SubnetMessage::InputEdited(typed) => {
                self.input = typed;
                self.calculate();
            }
            SubnetMessage::Calculate => self.calculate(),
            SubnetMessage::Copy(field) => {
                let value = self.value(field)?;
                return (!value.is_empty()).then_some((CopySlot::Subnet(field), value));
            }
        }
        None
    }

    /// The input calculated, as the C# `Calculate` (`SubnetCalculatorView.xaml.cs:164-197`):
    /// nothing typed shows the empty state, something not read the error over it.
    fn calculate(&mut self) {
        let input = self.input.trim();
        self.result = None;
        self.error = false;
        if input.is_empty() {
            return;
        }
        self.result = subnet_calculator::calculate(input);
        self.error = self.result.is_none();
    }

    /// Whether the input was not read.
    #[cfg(test)]
    #[must_use]
    pub fn has_error(&self) -> bool {
        self.error
    }

    /// The text of `field`; `None` when nothing is shown or the row is hidden.
    #[must_use]
    pub fn value(&self, field: SubnetField) -> Option<String> {
        let subnet = self.result.as_ref()?;
        match field {
            SubnetField::Network => Some(subnet.network.clone()),
            SubnetField::Broadcast => subnet.broadcast.clone(),
            SubnetField::Mask => subnet.mask.clone(),
            SubnetField::FirstHost => Some(subnet.first_host.clone()),
            SubnetField::LastHost => Some(subnet.last_host.clone()),
            SubnetField::TotalHosts => Some(match subnet.total_hosts {
                HostCount::Count(count) => {
                    number_text::group_digits(count, &fl!("ui-tool-number-group-separator"))
                }
                HostCount::TooMany => fl!("ui-tool-subnet-too-many"),
            }),
            SubnetField::Cidr => Some(subnet.cidr.clone()),
            SubnetField::Wildcard => subnet.wildcard.clone(),
        }
    }

    /// The tool's page, as the C# `SubnetCalculatorView.xaml`: the input, the error, then
    /// the empty state or each value with its copy button.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Subnet(message));
        let mut page = column![
            super::field_label(fl!("ui-tool-subnet-input")),
            text_input(&fl!("ui-tool-subnet-placeholder"), &self.input)
                .font(super::BOX_FONT)
                .size(font_size::BODY_LARGE)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(SubnetMessage::InputEdited(typed)))
                .on_submit(send(SubnetMessage::Calculate)),
        ]
        .spacing(spacing::SM);
        if self.error {
            page = page.push(super::error_line(fl!("ui-tool-subnet-error-invalid")));
        }
        if self.result.is_none() {
            return super::content_column(page.push(super::hint(fl!("ui-tool-subnet-empty"))));
        }
        let rows = SubnetField::ALL.into_iter().filter_map(|field| {
            let value = self.value(field)?;
            let label = match field {
                SubnetField::Network => fl!("ui-tool-subnet-network"),
                SubnetField::Broadcast => fl!("ui-tool-subnet-broadcast"),
                SubnetField::Mask => fl!("ui-tool-subnet-mask"),
                SubnetField::FirstHost => fl!("ui-tool-subnet-first-host"),
                SubnetField::LastHost => fl!("ui-tool-subnet-last-host"),
                SubnetField::TotalHosts => fl!("ui-tool-subnet-total-hosts"),
                SubnetField::Cidr => fl!("ui-tool-subnet-cidr"),
                SubnetField::Wildcard => fl!("ui-tool-subnet-wildcard"),
            };
            let copy = super::copy_button(
                fl!("ui-tool-copy-value"),
                state.copied(CopySlot::Subnet(field)),
                send(SubnetMessage::Copy(field)),
                super::COPY_PADDING,
            );
            Some(super::result_row(label, value, copy))
        });
        super::content_column(page.push(super::results_card(rows)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(input: &str) -> SubnetPane {
        let mut pane = SubnetPane::default();
        let _ = pane.update(SubnetMessage::InputEdited(input.to_owned()));
        pane
    }

    #[test]
    fn a_network_is_broken_down_as_it_is_typed() {
        let pane = typed("10.0.0.0/8");
        assert!(!pane.has_error());
        assert_eq!(
            pane.value(SubnetField::Network).as_deref(),
            Some("10.0.0.0")
        );
        assert_eq!(
            pane.value(SubnetField::TotalHosts).as_deref(),
            Some("16,777,214")
        );
        assert_eq!(
            pane.value(SubnetField::Wildcard).as_deref(),
            Some("0.255.255.255")
        );
    }

    #[test]
    fn an_ipv6_network_hides_the_ipv4_rows() {
        let pane = typed("2001:db8::/32");
        assert_eq!(pane.value(SubnetField::Broadcast), None);
        assert_eq!(pane.value(SubnetField::Mask), None);
        assert_eq!(pane.value(SubnetField::Wildcard), None);
        assert_eq!(
            pane.value(SubnetField::TotalHosts).as_deref(),
            Some("Too many to list")
        );
    }

    #[test]
    fn something_not_read_says_so_and_nothing_typed_says_nothing() {
        let mut pane = typed("10.0.0.0/40");
        assert!(pane.has_error());
        assert_eq!(pane.value(SubnetField::Network), None);
        let _ = pane.update(SubnetMessage::InputEdited("  ".to_owned()));
        assert!(!pane.has_error());
    }

    #[test]
    fn a_copy_button_copies_its_value() {
        let mut pane = typed("192.168.1.10/24");
        assert_eq!(
            pane.update(SubnetMessage::Copy(SubnetField::Cidr)),
            Some((
                CopySlot::Subnet(SubnetField::Cidr),
                "192.168.1.0/24".to_owned()
            ))
        );
        assert_eq!(
            typed("").update(SubnetMessage::Copy(SubnetField::Cidr)),
            None
        );
    }
}
