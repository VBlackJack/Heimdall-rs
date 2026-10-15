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

//! The Command Snippet Library tool view: curated catalog of sysadmin, network,
//! and security commands with interactive parameter substitution and copying.

use std::collections::HashMap;

use heimdall_app::TabId;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::command_library::{BUILTIN_SNIPPETS, SnippetPlatform, filter_snippets};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the Command Snippet Library view.
#[derive(Debug, Clone)]
pub enum CommandLibraryMessage {
    /// Platform filter changed.
    PlatformFilterChanged(Option<SnippetPlatform>),
    /// Search query changed.
    SearchQueryChanged(String),
    /// Parameter value changed for a snippet.
    ParamValueChanged {
        snippet_id: String,
        param_name: String,
        value: String,
    },
    /// Copy rendered command to clipboard.
    CopyCommand(String),
}

/// The Command Snippet Library tool pane state.
#[derive(Debug, Default)]
pub struct CommandLibraryPane {
    platform_filter: Option<SnippetPlatform>,
    search_query: String,
    param_values: HashMap<(String, String), String>,
}

/// Outcome of updating the Command Snippet Library tool pane.
#[derive(Debug)]
#[allow(dead_code)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl CommandLibraryPane {
    /// Handles tool messages.
    pub fn update(&mut self, _tab: TabId, message: CommandLibraryMessage) -> Outcome {
        match message {
            CommandLibraryMessage::PlatformFilterChanged(platform) => {
                self.platform_filter = platform;
                Outcome::None
            }
            CommandLibraryMessage::SearchQueryChanged(query) => {
                self.search_query = query;
                Outcome::None
            }
            CommandLibraryMessage::ParamValueChanged {
                snippet_id,
                param_name,
                value,
            } => {
                self.param_values.insert((snippet_id, param_name), value);
                Outcome::None
            }
            CommandLibraryMessage::CopyCommand(cmd) => Outcome::Copy(CopySlot::CmdLibCommand, cmd),
        }
    }

    fn get_param_value<'a>(
        &'a self,
        snippet_id: &str,
        param_name: &str,
        default: &'a str,
    ) -> &'a str {
        if let Some(val) = self
            .param_values
            .get(&(snippet_id.to_string(), param_name.to_string()))
        {
            if !val.is_empty() {
                return val.as_str();
            }
        }
        default
    }

    /// Renders the Command Snippet Library tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::CmdLib(msg));

        let btn_all = button(text(fl!("ui-tool-cmdlib-platform-all")).size(font_size::CAPTION))
            .padding([spacing::XS, spacing::SM])
            .style(if self.platform_filter.is_none() {
                styles::primary
            } else {
                styles::secondary
            })
            .on_press(send(CommandLibraryMessage::PlatformFilterChanged(None)));

        let btn_linux = button(text(fl!("ui-tool-cmdlib-platform-linux")).size(font_size::CAPTION))
            .padding([spacing::XS, spacing::SM])
            .style(if self.platform_filter == Some(SnippetPlatform::Linux) {
                styles::primary
            } else {
                styles::secondary
            })
            .on_press(send(CommandLibraryMessage::PlatformFilterChanged(Some(
                SnippetPlatform::Linux,
            ))));

        let btn_windows =
            button(text(fl!("ui-tool-cmdlib-platform-windows")).size(font_size::CAPTION))
                .padding([spacing::XS, spacing::SM])
                .style(if self.platform_filter == Some(SnippetPlatform::Windows) {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press(send(CommandLibraryMessage::PlatformFilterChanged(Some(
                    SnippetPlatform::Windows,
                ))));

        let btn_cross = button(text(fl!("ui-tool-cmdlib-platform-cross")).size(font_size::CAPTION))
            .padding([spacing::XS, spacing::SM])
            .style(
                if self.platform_filter == Some(SnippetPlatform::CrossPlatform) {
                    styles::primary
                } else {
                    styles::secondary
                },
            )
            .on_press(send(CommandLibraryMessage::PlatformFilterChanged(Some(
                SnippetPlatform::CrossPlatform,
            ))));

        let search_input = text_input(
            &fl!("ui-tool-cmdlib-search-placeholder"),
            &self.search_query,
        )
        .on_input(move |s| send(CommandLibraryMessage::SearchQueryChanged(s)))
        .width(Length::Fill);

        let platform_row = row![btn_all, btn_linux, btn_windows, btn_cross, search_input]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

        let matching_snippets = filter_snippets(
            &BUILTIN_SNIPPETS,
            None,
            self.platform_filter,
            &self.search_query,
        );

        let count_text = text(fl!(
            "ui-tool-cmdlib-count",
            count = matching_snippets.len().to_string()
        ))
        .size(font_size::SMALL_CAPTION)
        .style(text::secondary);

        let list_content: Element<'a, Message> = if matching_snippets.is_empty() {
            container(
                text(fl!("ui-tool-cmdlib-empty"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            )
            .padding(spacing::XL)
            .center_x(Length::Fill)
            .into()
        } else {
            let mut cards_col = column![].spacing(spacing::MD);

            for snippet in matching_snippets {
                let snippet_id = snippet.id.to_string();

                let mut current_params = HashMap::new();
                for p in snippet.parameters {
                    let val = self.get_param_value(&snippet_id, p.key, p.default_value);
                    current_params.insert(p.key.to_string(), val.to_string());
                }

                let rendered_cmd = snippet.render(&current_params);
                let copy_cmd = rendered_cmd.clone();

                let header_row = row![
                    text(snippet.title).size(font_size::BODY_LARGE),
                    text(format!("[{}]", snippet.platform.label()))
                        .size(font_size::SMALL_CAPTION)
                        .style(text::secondary),
                    text(format!("#{}", snippet.category))
                        .size(font_size::SMALL_CAPTION)
                        .style(text::secondary),
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center);

                let desc_text = text(snippet.description)
                    .size(font_size::CAPTION)
                    .style(text::secondary);

                let mut params_col = column![].spacing(spacing::XS);
                for p in snippet.parameters {
                    let p_name = p.key.to_string();
                    let s_id = snippet.id.to_string();
                    let current_val = self.get_param_value(&s_id, p.key, p.default_value);

                    let p_row = row![
                        text(format!("{}:", p.label))
                            .size(font_size::SMALL_CAPTION)
                            .width(Length::Fixed(160.0)),
                        text_input(p.default_value, current_val)
                            .on_input(move |val| {
                                Message::Tool(
                                    tab,
                                    super::ToolMessage::CmdLib(
                                        CommandLibraryMessage::ParamValueChanged {
                                            snippet_id: s_id.clone(),
                                            param_name: p_name.clone(),
                                            value: val,
                                        },
                                    ),
                                )
                            })
                            .size(font_size::SMALL_CAPTION)
                            .width(Length::FillPortion(2)),
                    ]
                    .spacing(spacing::SM)
                    .align_y(Alignment::Center);

                    params_col = params_col.push(p_row);
                }

                let copy_cmd_btn = super::copy_button(
                    fl!("ui-tool-cmdlib-btn-copy"),
                    state.copied(CopySlot::CmdLibCommand),
                    send(CommandLibraryMessage::CopyCommand(copy_cmd)),
                    super::COPY_PADDING,
                );

                let cmd_preview = container(
                    row![
                        text(rendered_cmd)
                            .size(font_size::SMALL_CAPTION)
                            .width(Length::Fill),
                        copy_cmd_btn,
                    ]
                    .spacing(spacing::SM)
                    .align_y(Alignment::Center),
                )
                .padding(spacing::SM)
                .style(styles::card)
                .width(Length::Fill);

                let card = container(
                    column![header_row, desc_text, params_col, cmd_preview].spacing(spacing::SM),
                )
                .padding(spacing::MD)
                .style(styles::card)
                .width(Length::Fill);

                cards_col = cards_col.push(card);
            }

            scrollable(cards_col).height(Length::Fill).into()
        };

        column![platform_row, count_text, list_content]
            .spacing(spacing::MD)
            .padding(spacing::MD)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
