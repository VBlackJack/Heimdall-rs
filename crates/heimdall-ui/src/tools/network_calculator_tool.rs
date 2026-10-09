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

//! The network calculator, as the C# `NetworkCalculatorView`: three modes, the supernet of
//! networks given one a line (computed on Compute or Ctrl+Enter), the networks covering a
//! range of addresses, and the network a count of hosts needs (each on Compute or Enter);
//! IPv4 only, by computation alone; the result copied.

use heimdall_app::TabId;
use heimdall_core::tools::network_calculator::{self, NetCalcError, address_text};
use heimdall_core::tools::number_text;
use iced::keyboard::{Key, key::Named};
use iced::widget::text_editor::{Action, Binding, Content, KeyPress, Status};
use iced::widget::{column, container, radio, row, text, text_input};
use iced::{Element, Length};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Height of the networks' box, as the C#'s 100.
const SUPERNET_HEIGHT: f32 = 100.0;

/// Room between the start and end of a range, as the C#'s column of 16.
const RANGE_GAP: f32 = 16.0;

/// Widest the hosts' box is, as the C# `MaxWidth`.
const HOSTS_MAX_WIDTH: f32 = 150.0;

/// Widest the base network's box is, as the C# `MaxWidth`.
const BASE_MAX_WIDTH: f32 = 200.0;

/// Height of the result before it scrolls, as the C# `MaxHeight`.
const RESULT_MAX_HEIGHT: f32 = 300.0;

/// Width of a field's label in the VLAN planner's grid.
const LABEL_WIDTH: f32 = 140.0;

/// The indent of a network covering a range, as the C#'s two spaces.
const RANGE_INDENT: &str = "  ";

/// A mode of the calculator, as the C# radio buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetCalcMode {
    /// The supernet of networks.
    #[default]
    Supernet,
    /// The networks covering a range.
    RangeToCidr,
    /// The network a count of hosts needs.
    VlanPlanner,
}

/// A box of the calculator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetCalcField {
    /// The range's start.
    StartIp,
    /// The range's end.
    EndIp,
    /// The hosts needed.
    Hosts,
    /// The base network.
    BaseNetwork,
}

/// What the network calculator is asked.
#[derive(Debug, Clone)]
pub enum NetCalcMessage {
    /// A mode chosen.
    Mode(NetCalcMode),
    /// Something done in the networks' box.
    Networks(Action),
    /// A box typed.
    Edited(NetCalcField, String),
    /// Compute the mode shown, as its button, Enter or Ctrl+Enter.
    Compute,
    /// Something done in the result, which does not change it.
    Result(Action),
    /// Copy the result.
    Copy,
}

/// The network calculator's state, as the C# view's.
#[derive(Debug, Default)]
pub struct NetCalcPane {
    mode: NetCalcMode,
    networks: Content,
    start: String,
    end: String,
    hosts: String,
    base: String,
    error: Option<String>,
    result: Option<Content>,
}

impl NetCalcPane {
    /// Applies `message`; what the copy button copies, when it is pressed.
    pub fn update(&mut self, message: NetCalcMessage) -> Option<(CopySlot, String)> {
        match message {
            NetCalcMessage::Mode(mode) => {
                // As the C# `OnModeChanged`: the error and the result hidden.
                self.mode = mode;
                self.error = None;
                self.result = None;
            }
            NetCalcMessage::Networks(action) => self.networks.perform(action),
            NetCalcMessage::Edited(field, typed) => match field {
                NetCalcField::StartIp => self.start = typed,
                NetCalcField::EndIp => self.end = typed,
                NetCalcField::Hosts => self.hosts = typed,
                NetCalcField::BaseNetwork => self.base = typed,
            },
            NetCalcMessage::Compute => self.compute(),
            NetCalcMessage::Result(action) => {
                if let Some(result) = &mut self.result {
                    super::read_only(result, action);
                }
            }
            NetCalcMessage::Copy => {
                let result = self.result_text();
                if !result.trim().is_empty() {
                    return Some((CopySlot::NetCalcResult, result));
                }
            }
        }
        None
    }

    /// The mode shown computed, its lines written in the language shown, as the C#
    /// `ShowResult` keeps them.
    fn compute(&mut self) {
        self.error = None;
        self.result = None;
        let group = fl!("ui-tool-number-group-separator");
        let lines = match self.mode {
            NetCalcMode::Supernet => {
                network_calculator::supernet(&self.networks.text()).map(|supernet| {
                    vec![
                        fl!(
                            "ui-tool-netcalc-supernet-result",
                            network = address_text(supernet.network),
                            prefix = supernet.prefix
                        ),
                        fl!(
                            "ui-tool-netcalc-supernet-range",
                            first = address_text(supernet.network),
                            last = address_text(supernet.broadcast)
                        ),
                        fl!(
                            "ui-tool-netcalc-supernet-hosts",
                            hosts = number_text::group_digits(u128::from(supernet.hosts), &group)
                        ),
                    ]
                })
            }
            NetCalcMode::RangeToCidr => network_calculator::range_to_cidrs(&self.start, &self.end)
                .map(|blocks| {
                    std::iter::once(fl!("ui-tool-netcalc-range-result"))
                        .chain(
                            blocks
                                .into_iter()
                                .map(|block| format!("{RANGE_INDENT}{block}")),
                        )
                        .collect()
                }),
            NetCalcMode::VlanPlanner => {
                network_calculator::vlan(&self.hosts, &self.base).map(|plan| {
                    vec![
                        fl!(
                            "ui-tool-netcalc-vlan-network",
                            network = address_text(plan.network),
                            prefix = plan.prefix
                        ),
                        fl!("ui-tool-netcalc-vlan-mask", mask = address_text(plan.mask)),
                        fl!(
                            "ui-tool-netcalc-vlan-broadcast",
                            broadcast = address_text(plan.broadcast)
                        ),
                        fl!(
                            "ui-tool-netcalc-vlan-usable-range",
                            first = address_text(plan.first_usable),
                            last = address_text(plan.last_usable)
                        ),
                        fl!(
                            "ui-tool-netcalc-vlan-usable-hosts",
                            hosts =
                                number_text::group_digits(u128::from(plan.usable_hosts), &group)
                        ),
                        fl!(
                            "ui-tool-netcalc-vlan-requested",
                            hosts = number_text::group_digits(u128::from(plan.requested), &group)
                        ),
                        fl!(
                            "ui-tool-netcalc-vlan-utilization",
                            percent = number_text::one_decimal(
                                plan.utilization,
                                &fl!("ui-tool-number-decimal-separator")
                            )
                        ),
                    ]
                })
            }
        };
        match lines {
            Ok(lines) => {
                self.result = Some(Content::with_text(&lines.join(super::NEW_LINE)));
            }
            Err(error) => self.error = Some(error_text(&error)),
        }
    }

    /// The mode shown.
    #[cfg(test)]
    #[must_use]
    pub fn mode(&self) -> NetCalcMode {
        self.mode
    }

    /// The error shown.
    #[cfg(test)]
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The result shown; empty when none is.
    #[must_use]
    pub fn result_text(&self) -> String {
        self.result
            .as_ref()
            .map(super::box_text)
            .unwrap_or_default()
    }

    /// The tool's page, as the C# `NetworkCalculatorView.xaml`: the modes, the mode's
    /// boxes and Compute, the error, then the empty state or the result with its copy
    /// button.
    #[expect(clippy::too_many_lines, reason = "one page, in the C# view's order")]
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::NetCalc(message));
        let mode = |label: String, mode: NetCalcMode| {
            radio(label, mode, Some(self.mode), move |mode| {
                send(NetCalcMessage::Mode(mode))
            })
            .size(font_size::BODY)
            .text_size(font_size::BODY)
        };
        let modes = row![
            mode(fl!("ui-tool-netcalc-mode-supernet"), NetCalcMode::Supernet),
            mode(fl!("ui-tool-netcalc-mode-range"), NetCalcMode::RangeToCidr),
            mode(fl!("ui-tool-netcalc-mode-vlan"), NetCalcMode::VlanPlanner),
        ]
        .spacing(spacing::LG);
        let input = |field: NetCalcField, value: &'a str, placeholder: String| {
            text_input(&placeholder, value)
                .font(super::BOX_FONT)
                .size(font_size::BODY)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(NetCalcMessage::Edited(field, typed)))
                .on_submit(send(NetCalcMessage::Compute))
        };
        let compute = super::action_button(
            fl!("ui-tool-netcalc-compute"),
            true,
            Some(send(NetCalcMessage::Compute)),
        );
        let panel: Element<'a, Message> = match self.mode {
            NetCalcMode::Supernet => column![
                super::field_label(fl!("ui-tool-netcalc-supernet-input")),
                super::text_box(&self.networks, None)
                    .size(font_size::BODY)
                    .height(SUPERNET_HEIGHT)
                    .on_action(move |action| send(NetCalcMessage::Networks(action)))
                    .key_binding(move |press| ctrl_enter(press, send)),
                compute,
            ]
            .spacing(spacing::SM)
            .into(),
            NetCalcMode::RangeToCidr => column![
                row![
                    text(fl!("ui-tool-netcalc-start-ip")).size(font_size::BODY),
                    input(
                        NetCalcField::StartIp,
                        &self.start,
                        fl!("ui-tool-netcalc-start-placeholder")
                    ),
                    iced::widget::space().width(RANGE_GAP),
                    text(fl!("ui-tool-netcalc-end-ip")).size(font_size::BODY),
                    input(
                        NetCalcField::EndIp,
                        &self.end,
                        fl!("ui-tool-netcalc-end-placeholder")
                    ),
                ]
                .spacing(spacing::SM)
                .align_y(iced::Alignment::Center),
                compute,
            ]
            .spacing(spacing::SM)
            .into(),
            NetCalcMode::VlanPlanner => column![
                row![
                    text(fl!("ui-tool-netcalc-hosts-needed"))
                        .size(font_size::BODY)
                        .width(LABEL_WIDTH),
                    container(input(
                        NetCalcField::Hosts,
                        &self.hosts,
                        fl!("ui-tool-netcalc-hosts-placeholder")
                    ))
                    .width(HOSTS_MAX_WIDTH),
                ]
                .align_y(iced::Alignment::Center),
                row![
                    text(fl!("ui-tool-netcalc-base-network"))
                        .size(font_size::BODY)
                        .width(LABEL_WIDTH),
                    container(input(
                        NetCalcField::BaseNetwork,
                        &self.base,
                        fl!("ui-tool-netcalc-base-placeholder")
                    ))
                    .width(BASE_MAX_WIDTH),
                ]
                .align_y(iced::Alignment::Center),
                compute,
            ]
            .spacing(spacing::SM)
            .into(),
        };
        let mut page = column![
            super::field_label(fl!("ui-tool-netcalc-mode")),
            modes,
            container(panel)
                .padding(spacing::MD)
                .width(Length::Fill)
                .style(styles::card),
        ]
        .spacing(spacing::SM + spacing::XS);
        if let Some(error) = &self.error {
            page = page.push(super::error_line(error.clone()));
        }
        page = match &self.result {
            None => page.push(super::hint(fl!("ui-tool-netcalc-empty"))),
            Some(result) => page.push(
                container(
                    column![
                        super::copy_button(
                            fl!("ui-tool-copy-value"),
                            state.copied(CopySlot::NetCalcResult),
                            send(NetCalcMessage::Copy),
                            super::COPY_PADDING,
                        ),
                        super::text_box(result, None)
                            .size(font_size::BODY)
                            .max_height(RESULT_MAX_HEIGHT)
                            .on_action(move |action| send(NetCalcMessage::Result(action))),
                    ]
                    .spacing(spacing::SM),
                )
                .padding(spacing::MD)
                .width(Length::Fill)
                .style(styles::card),
            ),
        };
        super::content_column(page)
    }
}

/// What `error` says, as the C# `ShowError` calls.
fn error_text(error: &NetCalcError) -> String {
    match error {
        NetCalcError::NoCidrs => fl!("ui-tool-netcalc-error-no-cidrs"),
        NetCalcError::InvalidCidr(line) => {
            fl!("ui-tool-netcalc-error-invalid-cidr", line = line.as_str())
        }
        NetCalcError::InvalidIpRange => fl!("ui-tool-netcalc-error-invalid-range"),
        NetCalcError::StartAfterEnd => fl!("ui-tool-netcalc-error-start-after-end"),
        NetCalcError::InvalidHostCount => fl!("ui-tool-netcalc-error-host-count"),
        NetCalcError::InvalidBaseNetwork => fl!("ui-tool-netcalc-error-base-network"),
    }
}

/// Ctrl+Enter in the networks' box computes, as the C# `OnSupernetInputPreviewKeyDown`:
/// Control alone held; any other key does what it does.
fn ctrl_enter(
    press: KeyPress,
    send: impl Fn(NetCalcMessage) -> Message,
) -> Option<Binding<Message>> {
    if matches!(press.status, Status::Focused { .. })
        && press.key == Key::Named(Named::Enter)
        && press.modifiers.control()
        && !press.modifiers.shift()
        && !press.modifiers.alt()
    {
        return Some(Binding::Custom(send(NetCalcMessage::Compute)));
    }
    Binding::from_key_press(press)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iced::widget::text_editor::Edit;

    use super::*;

    fn paste(pane: &mut NetCalcPane, text: &str) {
        let _ = pane.update(NetCalcMessage::Networks(Action::Edit(Edit::Paste(
            Arc::new(text.to_owned()),
        ))));
    }

    #[test]
    fn the_supernet_is_written_line_by_line() {
        let mut pane = NetCalcPane::default();
        paste(&mut pane, "10.0.0.0/24\n10.0.1.0/24");
        let _ = pane.update(NetCalcMessage::Compute);
        let nl = super::super::NEW_LINE;
        assert_eq!(
            pane.result_text(),
            format!(
                "Supernet: 10.0.0.0/23{nl}Range: 10.0.0.0 - 10.0.1.255{nl}Total usable hosts: 510"
            )
        );
        assert_eq!(pane.error(), None);
    }

    #[test]
    fn a_wrong_line_is_said_and_no_result_shows() {
        let mut pane = NetCalcPane::default();
        let _ = pane.update(NetCalcMessage::Compute);
        assert_eq!(pane.error(), Some("Enter at least one CIDR range."));
        paste(&mut pane, "10.0.0.0/24\nbad");
        let _ = pane.update(NetCalcMessage::Compute);
        assert_eq!(pane.error(), Some("Invalid CIDR notation: bad"));
        assert_eq!(pane.result_text(), "");
    }

    #[test]
    fn a_range_lists_its_networks_indented() {
        let mut pane = NetCalcPane::default();
        let _ = pane.update(NetCalcMessage::Mode(NetCalcMode::RangeToCidr));
        let _ = pane.update(NetCalcMessage::Edited(
            NetCalcField::StartIp,
            "10.0.0.0".to_owned(),
        ));
        let _ = pane.update(NetCalcMessage::Edited(
            NetCalcField::EndIp,
            "10.0.0.2".to_owned(),
        ));
        let _ = pane.update(NetCalcMessage::Compute);
        let nl = super::super::NEW_LINE;
        assert_eq!(
            pane.result_text(),
            format!("CIDR blocks covering the range:{nl}  10.0.0.0/31{nl}  10.0.0.2/32")
        );
        let _ = pane.update(NetCalcMessage::Edited(
            NetCalcField::EndIp,
            "9.0.0.0".to_owned(),
        ));
        let _ = pane.update(NetCalcMessage::Compute);
        assert_eq!(
            pane.error(),
            Some("Start IP must be less than or equal to End IP.")
        );
    }

    #[test]
    fn a_vlan_plan_is_written_with_its_utilization() {
        let mut pane = NetCalcPane::default();
        let _ = pane.update(NetCalcMessage::Mode(NetCalcMode::VlanPlanner));
        let _ = pane.update(NetCalcMessage::Edited(
            NetCalcField::Hosts,
            "1000".to_owned(),
        ));
        let _ = pane.update(NetCalcMessage::Edited(
            NetCalcField::BaseNetwork,
            "10.1.2.3".to_owned(),
        ));
        let _ = pane.update(NetCalcMessage::Compute);
        let text = pane.result_text();
        assert!(text.contains("Network: 10.1.0.0/22"), "{text}");
        assert!(text.contains("Usable hosts: 1,022"), "{text}");
        assert!(text.contains("Requested: 1,000"), "{text}");
        assert!(text.contains("Utilization: 97.8%"), "{text}");
    }

    #[test]
    fn a_mode_change_hides_the_result_and_the_copy_copies_it() {
        let mut pane = NetCalcPane::default();
        assert_eq!(pane.update(NetCalcMessage::Copy), None);
        paste(&mut pane, "10.0.0.0/8");
        let _ = pane.update(NetCalcMessage::Compute);
        let result = pane.result_text();
        assert_eq!(
            pane.update(NetCalcMessage::Copy),
            Some((CopySlot::NetCalcResult, result))
        );
        let _ = pane.update(NetCalcMessage::Mode(NetCalcMode::VlanPlanner));
        assert_eq!(pane.mode(), NetCalcMode::VlanPlanner);
        assert_eq!(pane.result_text(), "");
    }
}
