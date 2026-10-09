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

//! The SSH config generator, as the C# `SshConfigGeneratorView`: a form of a host's
//! options, Generate (or Enter in a box) writing its `~/.ssh/config` block, "Generate All"
//! saying how the form is meant to be used, and the block copied.

use heimdall_app::TabId;
use heimdall_core::tools::ssh_config::{self, HostBlock};
use iced::widget::text_editor::{Action, Content};
use iced::widget::{Column, checkbox, column, container, row, text, text_input};
use iced::{Element, Length};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Width of a field's label, as the C# grid's 150.
const LABEL_WIDTH: f32 = 150.0;

/// Widest a number's box is, as the C# `MaxWidth`.
const NUMBER_MAX_WIDTH: f32 = 100.0;

/// Height of the output before it scrolls, as the C# `MaxHeight`.
const OUTPUT_MAX_HEIGHT: f32 = 400.0;

/// A box of the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshField {
    /// `Host`.
    Alias,
    /// `HostName`.
    HostName,
    /// `User`.
    User,
    /// `Port`.
    Port,
    /// `IdentityFile`.
    IdentityFile,
    /// `ProxyJump`.
    ProxyJump,
    /// `ServerAliveInterval`.
    AliveInterval,
}

impl SshField {
    /// The boxes, in the form's order.
    const ALL: [Self; 7] = [
        Self::Alias,
        Self::HostName,
        Self::User,
        Self::Port,
        Self::IdentityFile,
        Self::ProxyJump,
        Self::AliveInterval,
    ];
}

/// What the SSH config generator is asked.
#[derive(Debug, Clone)]
pub enum SshConfigMessage {
    /// A box typed.
    Edited(SshField, String),
    /// `ForwardAgent` ticked or not.
    ForwardAgent(bool),
    /// Generate the block, as the button or Enter in a box.
    Generate,
    /// Generate All.
    GenerateAll,
    /// Something done in the output, which does not change it.
    Output(Action),
    /// Copy the output.
    Copy,
}

/// The SSH config generator's state, as the C# view's boxes.
#[derive(Debug)]
pub struct SshConfigPane {
    alias: String,
    host_name: String,
    user: String,
    port: String,
    identity_file: String,
    proxy_jump: String,
    alive_interval: String,
    forward_agent: bool,
    output: Option<Content>,
}

impl Default for SshConfigPane {
    /// A new tab's form, as the C# `Initialize`: port 22, a keep-alive of 60.
    fn default() -> Self {
        Self {
            alias: String::new(),
            host_name: String::new(),
            user: String::new(),
            port: ssh_config::DEFAULT_PORT.to_string(),
            identity_file: String::new(),
            proxy_jump: String::new(),
            alive_interval: ssh_config::DEFAULT_ALIVE_INTERVAL_INPUT.to_string(),
            forward_agent: false,
            output: None,
        }
    }
}

impl SshConfigPane {
    /// Applies `message`; what the copy button copies, when it is pressed.
    pub fn update(&mut self, message: SshConfigMessage) -> Option<(CopySlot, String)> {
        match message {
            SshConfigMessage::Edited(field, typed) => *self.field_mut(field) = typed,
            SshConfigMessage::ForwardAgent(on) => self.forward_agent = on,
            SshConfigMessage::Generate => {
                let block = self.generate();
                self.output = Some(Content::with_text(&block));
            }
            SshConfigMessage::GenerateAll => {
                self.output = Some(Content::with_text(&fl!(
                    "ui-tool-sshconfig-generate-all-hint"
                )));
            }
            SshConfigMessage::Output(action) => {
                if let Some(output) = &mut self.output {
                    super::read_only(output, action);
                }
            }
            SshConfigMessage::Copy => {
                let output = self.output_text();
                if !output.trim().is_empty() {
                    return Some((CopySlot::SshConfigOutput, output));
                }
            }
        }
        None
    }

    /// The box of `field`.
    fn field_mut(&mut self, field: SshField) -> &mut String {
        match field {
            SshField::Alias => &mut self.alias,
            SshField::HostName => &mut self.host_name,
            SshField::User => &mut self.user,
            SshField::Port => &mut self.port,
            SshField::IdentityFile => &mut self.identity_file,
            SshField::ProxyJump => &mut self.proxy_jump,
            SshField::AliveInterval => &mut self.alive_interval,
        }
    }

    /// What `field` holds.
    fn field(&self, field: SshField) -> &str {
        match field {
            SshField::Alias => &self.alias,
            SshField::HostName => &self.host_name,
            SshField::User => &self.user,
            SshField::Port => &self.port,
            SshField::IdentityFile => &self.identity_file,
            SshField::ProxyJump => &self.proxy_jump,
            SshField::AliveInterval => &self.alive_interval,
        }
    }

    /// The block, as the C# `OnGenerateClick` (`SshConfigGeneratorView.xaml.cs:119-148`): a
    /// host name required, the alias the host name when blank, a port or interval not read
    /// taken as the default.
    fn generate(&self) -> String {
        let host_name = self.host_name.trim();
        if host_name.is_empty() {
            return fl!("ui-tool-sshconfig-error-host-required");
        }
        let alias = match self.alias.trim() {
            "" => host_name,
            alias => alias,
        };
        ssh_config::generate(
            &HostBlock {
                alias: alias.to_owned(),
                host_name: host_name.to_owned(),
                user: self.user.trim().to_owned(),
                port: ssh_config::parse_int(&self.port, ssh_config::DEFAULT_PORT),
                identity_file: self.identity_file.trim().to_owned(),
                proxy_jump: self.proxy_jump.trim().to_owned(),
                forward_agent: self.forward_agent,
                alive_interval: ssh_config::parse_int(
                    &self.alive_interval,
                    ssh_config::DEFAULT_ALIVE_INTERVAL,
                ),
            },
            super::NEW_LINE,
        )
    }

    /// The output; empty before anything is generated.
    #[must_use]
    pub fn output_text(&self) -> String {
        self.output
            .as_ref()
            .map(super::box_text)
            .unwrap_or_default()
    }

    /// The tool's page, as the C# `SshConfigGeneratorView.xaml`: the form, Generate and
    /// Generate All, then the empty state or the output with its copy button.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::SshConfig(message));
        let mut form = Column::new().spacing(spacing::SM);
        for field in SshField::ALL {
            let label = match field {
                SshField::Alias => fl!("ui-tool-sshconfig-host-alias"),
                SshField::HostName => fl!("ui-tool-sshconfig-host-name"),
                SshField::User => fl!("ui-tool-sshconfig-user"),
                SshField::Port => fl!("ui-tool-sshconfig-port"),
                SshField::IdentityFile => fl!("ui-tool-sshconfig-identity-file"),
                SshField::ProxyJump => fl!("ui-tool-sshconfig-proxy-jump"),
                SshField::AliveInterval => fl!("ui-tool-sshconfig-alive-interval"),
            };
            let monospaced = !matches!(
                field,
                SshField::User | SshField::Port | SshField::AliveInterval
            );
            let input = text_input("", self.field(field))
                .size(font_size::BODY)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .font(if monospaced {
                    super::BOX_FONT
                } else {
                    iced::Font::DEFAULT
                })
                .on_input(move |typed| send(SshConfigMessage::Edited(field, typed)))
                .on_submit(send(SshConfigMessage::Generate));
            let input: Element<'a, Message> =
                if matches!(field, SshField::Port | SshField::AliveInterval) {
                    container(input).width(NUMBER_MAX_WIDTH).into()
                } else {
                    input.into()
                };
            form = form.push(labelled(label, input));
            if field == SshField::ProxyJump {
                form = form.push(labelled(
                    fl!("ui-tool-sshconfig-forward-agent"),
                    checkbox(self.forward_agent)
                        .on_toggle(move |on| send(SshConfigMessage::ForwardAgent(on)))
                        .style(styles::checkbox)
                        .into(),
                ));
            }
        }
        let buttons = row![
            super::action_button(
                fl!("ui-tool-sshconfig-generate"),
                true,
                Some(send(SshConfigMessage::Generate))
            ),
            super::action_button(
                fl!("ui-tool-sshconfig-generate-all"),
                false,
                Some(send(SshConfigMessage::GenerateAll))
            ),
        ]
        .spacing(spacing::SM);
        let result: Element<'a, Message> = match &self.output {
            None => super::hint(fl!("ui-tool-sshconfig-empty")),
            Some(output) => container(
                column![
                    super::copy_button(
                        fl!("ui-tool-sshconfig-copy"),
                        state.copied(CopySlot::SshConfigOutput),
                        send(SshConfigMessage::Copy),
                        super::COPY_PADDING,
                    ),
                    super::text_box(output, None)
                        .size(font_size::BODY)
                        .max_height(OUTPUT_MAX_HEIGHT)
                        .on_action(move |action| send(SshConfigMessage::Output(action))),
                ]
                .spacing(spacing::SM),
            )
            .padding(spacing::MD)
            .width(Length::Fill)
            .style(styles::card)
            .into(),
        };
        super::content_column(
            column![
                container(form)
                    .padding(spacing::MD)
                    .width(Length::Fill)
                    .style(styles::card),
                buttons,
                result,
            ]
            .spacing(spacing::MD),
        )
    }
}

/// A row of the form: its label, then its box.
fn labelled(label: String, input: Element<'_, Message>) -> Element<'_, Message> {
    row![text(label).size(font_size::BODY).width(LABEL_WIDTH), input]
        .align_y(iced::Alignment::Center)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(pane: &mut SshConfigPane, field: SshField, value: &str) {
        let _ = pane.update(SshConfigMessage::Edited(field, value.to_owned()));
    }

    #[test]
    fn a_new_tab_offers_port_22_and_a_keep_alive_of_60() {
        let pane = SshConfigPane::default();
        assert_eq!(pane.field(SshField::Port), "22");
        assert_eq!(pane.field(SshField::AliveInterval), "60");
        assert_eq!(pane.output_text(), "");
    }

    #[test]
    fn generate_writes_the_block_with_the_alias_the_host_name_when_blank() {
        let mut pane = SshConfigPane::default();
        typed(&mut pane, SshField::HostName, " web.example.com ");
        let _ = pane.update(SshConfigMessage::Generate);
        let nl = super::super::NEW_LINE;
        assert_eq!(
            pane.output_text(),
            format!(
                "Host web.example.com{nl}    HostName web.example.com{nl}    ServerAliveInterval 60"
            )
        );
        typed(&mut pane, SshField::Alias, "web");
        typed(&mut pane, SshField::Port, "abc");
        typed(&mut pane, SshField::AliveInterval, "");
        let _ = pane.update(SshConfigMessage::ForwardAgent(true));
        let _ = pane.update(SshConfigMessage::Generate);
        assert_eq!(
            pane.output_text(),
            format!("Host web{nl}    HostName web.example.com{nl}    ForwardAgent yes")
        );
    }

    #[test]
    fn a_missing_host_name_is_said_in_the_output() {
        let mut pane = SshConfigPane::default();
        let _ = pane.update(SshConfigMessage::Generate);
        assert_eq!(pane.output_text(), "HostName is required.");
    }

    #[test]
    fn generate_all_says_how_the_form_is_used() {
        let mut pane = SshConfigPane::default();
        let _ = pane.update(SshConfigMessage::GenerateAll);
        assert!(
            pane.output_text()
                .starts_with("Open this tool from a session context")
        );
    }

    #[test]
    fn the_copy_button_copies_the_output_and_nothing_before_it() {
        let mut pane = SshConfigPane::default();
        assert_eq!(pane.update(SshConfigMessage::Copy), None);
        typed(&mut pane, SshField::HostName, "db");
        let _ = pane.update(SshConfigMessage::Generate);
        let output = pane.output_text();
        assert_eq!(
            pane.update(SshConfigMessage::Copy),
            Some((CopySlot::SshConfigOutput, output))
        );
    }
}
