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

//! The command snippet library: the catalog filtered by platform and by what is typed, each
//! snippet with its description, the values it asks for and its command with them put in,
//! copied by its own button.

use std::collections::HashMap;

use heimdall_app::TabId;
use heimdall_core::tools::command_library::{
    self, BUILTIN_SNIPPETS, CommandSnippet, ParamLabel, SnippetCategory, SnippetId, SnippetPlatform,
};
use iced::widget::{Column, Row, button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Width of a value's label, as the result grids' label column.
const PARAM_LABEL_WIDTH: f32 = 160.0;

/// What the command library is asked.
#[derive(Debug, Clone)]
pub enum CommandLibraryMessage {
    /// The platform shown; `None` for every one.
    Platform(Option<SnippetPlatform>),
    /// The search typed.
    Search(String),
    /// Value `.1` of snippet `.0` typed.
    Param(SnippetId, &'static str, String),
    /// Copy the command of this snippet.
    Copy(SnippetId),
}

/// The command library's state.
#[derive(Debug, Default)]
pub struct CommandLibraryPane {
    platform: Option<SnippetPlatform>,
    search: String,
    values: HashMap<(SnippetId, &'static str), String>,
}

impl CommandLibraryPane {
    /// Applies `message`; what to copy, and by which button.
    pub fn update(&mut self, message: CommandLibraryMessage) -> Option<(CopySlot, String)> {
        match message {
            CommandLibraryMessage::Platform(platform) => self.platform = platform,
            CommandLibraryMessage::Search(typed) => self.search = typed,
            CommandLibraryMessage::Param(id, key, value) => {
                self.values.insert((id, key), value);
            }
            CommandLibraryMessage::Copy(id) => {
                let snippet = command_library::snippet(id)?;
                return Some((CopySlot::CmdLibCommand(id), self.render(snippet)));
            }
        }
        None
    }

    /// The command of `snippet` with the values typed.
    fn render(&self, snippet: &CommandSnippet) -> String {
        snippet.render(|key| self.values.get(&(snippet.id, key)).map(String::as_str))
    }

    /// The snippets the platform and the search keep.
    fn shown(&self) -> Vec<&'static CommandSnippet> {
        BUILTIN_SNIPPETS
            .iter()
            .filter(|snippet| command_library::on_platform(snippet, self.platform))
            .filter(|snippet| {
                let (title, description) = snippet_texts(snippet.id);
                let category = category_name(snippet.category);
                command_library::matches_query(
                    snippet,
                    &[&title, &description, &category],
                    &self.search,
                )
            })
            .collect()
    }

    /// The tool's page: the platforms and the search, the count, then each snippet.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let platform = |label: String, choice: Option<SnippetPlatform>| {
            button(text(label).size(font_size::BODY))
                .padding(super::COPY_PADDING)
                .style(if self.platform == choice {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press(send(CommandLibraryMessage::Platform(choice)))
                .into()
        };
        let platforms = Row::with_children(
            std::iter::once(platform(fl!("ui-tool-cmdlib-platform-all"), None)).chain(
                SnippetPlatform::ALL
                    .into_iter()
                    .map(|choice| platform(platform_name(choice), Some(choice))),
            ),
        )
        .spacing(spacing::SM);
        let search = text_input(&fl!("ui-tool-cmdlib-search-placeholder"), &self.search)
            .size(font_size::BODY)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |typed| send(CommandLibraryMessage::Search(typed)));
        let shown = self.shown();
        let count = shown.len();
        let list: Element<'a, Message> = if shown.is_empty() {
            super::empty_state(fl!("ui-tool-cmdlib-empty"))
        } else {
            let cards = shown
                .into_iter()
                .map(|snippet| self.card(tab, state, snippet));
            scrollable(Column::with_children(cards).spacing(spacing::MD))
                .height(Length::Fill)
                .into()
        };
        super::tool_body(
            column![
                row![platforms, search]
                    .spacing(spacing::MD)
                    .align_y(iced::Alignment::Center),
                text(fl!("ui-tool-cmdlib-count", count = count))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
                list,
            ]
            .spacing(spacing::SM),
        )
    }

    /// One snippet: its title, platform and category, its description, its values and its
    /// command with its copy button.
    fn card<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        snippet: &'static CommandSnippet,
    ) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let (title, description) = snippet_texts(snippet.id);
        let header = row![
            text(title)
                .size(font_size::BODY_LARGE)
                .font(styles::SEMIBOLD),
            text(platform_name(snippet.platform))
                .size(font_size::CAPTION)
                .style(text::secondary),
            text(category_name(snippet.category))
                .size(font_size::CAPTION)
                .style(text::secondary),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let params = Column::with_children(snippet.parameters.iter().map(|parameter| {
            let id = snippet.id;
            let key = parameter.key;
            let value = self.values.get(&(id, key)).map_or("", String::as_str);
            row![
                text(param_label(parameter.label))
                    .size(font_size::BODY)
                    .width(PARAM_LABEL_WIDTH),
                text_input(parameter.default_value, value)
                    .size(font_size::BODY)
                    .padding(super::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| send(CommandLibraryMessage::Param(id, key, typed))),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center)
            .into()
        }))
        .spacing(spacing::XS);
        let command = container(
            row![
                text(self.render(snippet))
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .width(Length::Fill),
                super::copy_button(
                    fl!("ui-tool-cmdlib-copy"),
                    state.copied(CopySlot::CmdLibCommand(snippet.id)),
                    send(CommandLibraryMessage::Copy(snippet.id)),
                    super::COPY_PADDING,
                ),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center),
        )
        .padding(spacing::SM)
        .width(Length::Fill)
        .style(styles::field_box);
        container(
            column![
                header,
                text(description)
                    .size(font_size::BODY)
                    .style(text::secondary),
                params,
                command
            ]
            .spacing(spacing::SM),
        )
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(styles::card)
        .into()
    }
}

/// The title and description of snippet `id`, in the window's language.
fn snippet_texts(id: SnippetId) -> (String, String) {
    match id {
        SnippetId::LinuxListeningPorts => (
            fl!("ui-tool-cmdlib-snippet-listening-ports"),
            fl!("ui-tool-cmdlib-snippet-listening-ports-description"),
        ),
        SnippetId::LinuxKillPort => (
            fl!("ui-tool-cmdlib-snippet-kill-port"),
            fl!("ui-tool-cmdlib-snippet-kill-port-description"),
        ),
        SnippetId::LinuxJournalFollow => (
            fl!("ui-tool-cmdlib-snippet-journal-follow"),
            fl!("ui-tool-cmdlib-snippet-journal-follow-description"),
        ),
        SnippetId::LinuxCertExpiry => (
            fl!("ui-tool-cmdlib-snippet-cert-expiry"),
            fl!("ui-tool-cmdlib-snippet-cert-expiry-description"),
        ),
        SnippetId::WindowsTestConnection => (
            fl!("ui-tool-cmdlib-snippet-test-connection"),
            fl!("ui-tool-cmdlib-snippet-test-connection-description"),
        ),
        SnippetId::WindowsPortOwner => (
            fl!("ui-tool-cmdlib-snippet-port-owner"),
            fl!("ui-tool-cmdlib-snippet-port-owner-description"),
        ),
        SnippetId::WindowsRestartService => (
            fl!("ui-tool-cmdlib-snippet-restart-service"),
            fl!("ui-tool-cmdlib-snippet-restart-service-description"),
        ),
        SnippetId::WindowsFlushDns => (
            fl!("ui-tool-cmdlib-snippet-flush-dns"),
            fl!("ui-tool-cmdlib-snippet-flush-dns-description"),
        ),
        SnippetId::MtrProbe => (
            fl!("ui-tool-cmdlib-snippet-mtr"),
            fl!("ui-tool-cmdlib-snippet-mtr-description"),
        ),
        SnippetId::NmapServiceScan => (
            fl!("ui-tool-cmdlib-snippet-nmap"),
            fl!("ui-tool-cmdlib-snippet-nmap-description"),
        ),
    }
}

/// The name of `platform`.
fn platform_name(platform: SnippetPlatform) -> String {
    match platform {
        SnippetPlatform::Linux => fl!("ui-tool-cmdlib-platform-linux"),
        SnippetPlatform::Windows => fl!("ui-tool-cmdlib-platform-windows"),
        SnippetPlatform::CrossPlatform => fl!("ui-tool-cmdlib-platform-cross"),
    }
}

/// The name of `category`.
fn category_name(category: SnippetCategory) -> String {
    match category {
        SnippetCategory::Network => fl!("ui-tool-cmdlib-category-network"),
        SnippetCategory::System => fl!("ui-tool-cmdlib-category-system"),
        SnippetCategory::Services => fl!("ui-tool-cmdlib-category-services"),
        SnippetCategory::Security => fl!("ui-tool-cmdlib-category-security"),
        SnippetCategory::Diagnostics => fl!("ui-tool-cmdlib-category-diagnostics"),
    }
}

/// The label of a value of kind `label`.
fn param_label(label: ParamLabel) -> String {
    match label {
        ParamLabel::Host => fl!("ui-tool-cmdlib-param-host"),
        ParamLabel::Port => fl!("ui-tool-cmdlib-param-port"),
        ParamLabel::LocalPort => fl!("ui-tool-cmdlib-param-local-port"),
        ParamLabel::PortRange => fl!("ui-tool-cmdlib-param-port-range"),
        ParamLabel::Service => fl!("ui-tool-cmdlib-param-service"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snippet_copies_its_command_with_the_values_typed_by_its_own_button() {
        let mut pane = CommandLibraryPane::default();
        pane.update(CommandLibraryMessage::Param(
            SnippetId::MtrProbe,
            "host",
            "10.0.0.1".to_owned(),
        ));
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(SnippetId::MtrProbe)),
            Some((
                CopySlot::CmdLibCommand(SnippetId::MtrProbe),
                "mtr -rw -c 50 10.0.0.1".to_owned()
            ))
        );
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(SnippetId::WindowsFlushDns)),
            Some((
                CopySlot::CmdLibCommand(SnippetId::WindowsFlushDns),
                "Clear-DnsClientCache".to_owned()
            ))
        );
    }

    #[test]
    fn the_platform_and_the_search_filter_the_catalog() {
        let mut pane = CommandLibraryPane::default();
        assert_eq!(pane.shown().len(), BUILTIN_SNIPPETS.len());
        pane.update(CommandLibraryMessage::Platform(Some(
            SnippetPlatform::Windows,
        )));
        assert!(
            pane.shown()
                .iter()
                .all(|snippet| { snippet.platform != SnippetPlatform::Linux })
        );
        pane.update(CommandLibraryMessage::Search("dnsclient".to_owned()));
        let shown: Vec<SnippetId> = pane.shown().iter().map(|snippet| snippet.id).collect();
        assert_eq!(shown, [SnippetId::WindowsFlushDns]);
    }
}
