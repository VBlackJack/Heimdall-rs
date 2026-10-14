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

//! HTTP status codes reference tool pane with filtering and category grouping.

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
use heimdall_core::tools::http_status::{self, HttpStatusCategory};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Theme};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the HTTP Status Codes tool.
#[derive(Debug, Clone)]
pub enum HttpStatusMessage {
    /// Filter query text modified.
    FilterChanged(String),
    /// Copy status code and name to clipboard.
    CopyCode(u16, &'static str),
}

/// The HTTP Status Codes tool pane state.
#[derive(Debug, Default)]
pub struct HttpStatusPane {
    filter: String,
}

/// Outcome of updating the HTTP Status Codes tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Copy(CopySlot, String),
}

impl HttpStatusPane {
    /// Handles tool messages.
    pub fn update(&mut self, message: HttpStatusMessage) -> Outcome {
        match message {
            HttpStatusMessage::FilterChanged(val) => {
                self.filter = val;
                Outcome::None
            }
            HttpStatusMessage::CopyCode(code, name) => {
                let text = format!("{code} {name}");
                Outcome::Copy(CopySlot::HttpStatus(code), text)
            }
        }
    }

    /// Renders the HTTP status tool content.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &'a ToolPane,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let filtered = http_status::filter_entries(&self.filter);

        let search_input = text_input(&fl!("ui-tool-http-filter-placeholder"), &self.filter)
            .on_input(move |val| {
                Message::Tool(
                    tab,
                    ToolMessage::HttpStatus(HttpStatusMessage::FilterChanged(val)),
                )
            })
            .padding(super::INPUT_PADDING)
            .size(font_size::BODY)
            .width(Length::Fill);

        let body: Element<'a, Message> = if filtered.is_empty() {
            container(
                text(fl!("ui-tool-http-no-results"))
                    .size(font_size::BODY)
                    .style(text::secondary),
            )
            .padding(super::EMPTY_STATE_PADDING)
            .into()
        } else {
            let mut list_col = column!().spacing(spacing::SM);

            let mut current_cat: Option<HttpStatusCategory> = None;

            for entry in filtered {
                if current_cat != Some(entry.category) {
                    current_cat = Some(entry.category);
                    let cat_title = match entry.category {
                        HttpStatusCategory::Informational => fl!("ui-tool-http-cat-1xx"),
                        HttpStatusCategory::Success => fl!("ui-tool-http-cat-2xx"),
                        HttpStatusCategory::Redirection => fl!("ui-tool-http-cat-3xx"),
                        HttpStatusCategory::ClientError => fl!("ui-tool-http-cat-4xx"),
                        HttpStatusCategory::ServerError => fl!("ui-tool-http-cat-5xx"),
                    };

                    list_col = list_col.push(
                        text(cat_title)
                            .size(font_size::CAPTION)
                            .font(styles::SEMIBOLD)
                            .style(text::secondary),
                    );
                }

                let badge_color = category_color(entry.category, theme);
                let is_copied = state.copied(CopySlot::HttpStatus(entry.code));
                let copy_label = if is_copied {
                    super::CHECK_MARK.to_string()
                } else {
                    entry.code.to_string()
                };

                let card = container(
                    row![
                        button(
                            text(copy_label)
                                .size(font_size::CAPTION)
                                .font(styles::SEMIBOLD)
                                .color(badge_color),
                        )
                        .padding([2.0, 8.0])
                        .style(styles::secondary)
                        .on_press(Message::Tool(
                            tab,
                            ToolMessage::HttpStatus(HttpStatusMessage::CopyCode(
                                entry.code, entry.name
                            )),
                        )),
                        text(entry.name)
                            .size(font_size::BODY)
                            .font(styles::SEMIBOLD)
                            .width(Length::Fill),
                    ]
                    .spacing(spacing::MD)
                    .align_y(Alignment::Center),
                )
                .padding([6.0, 10.0])
                .width(Length::Fill)
                .style(styles::card);

                list_col = list_col.push(card);
            }

            scrollable(list_col.padding([0.0, 4.0]))
                .height(Length::Fill)
                .into()
        };

        let content = column![
            row![search_input].spacing(spacing::SM),
            text(fl!("ui-tool-http-copy-hint"))
                .size(font_size::CAPTION)
                .style(text::secondary),
            body
        ]
        .spacing(spacing::MD)
        .padding(super::BODY_PADDING)
        .max_width(super::CONTENT_MAX_WIDTH);

        content.into()
    }
}

fn category_color(category: HttpStatusCategory, theme: &Theme) -> iced::Color {
    let colors = crate::themes::colors_of(theme);
    match category {
        HttpStatusCategory::Informational => colors.cyan,
        HttpStatusCategory::Success => colors.green,
        HttpStatusCategory::Redirection => colors.blue,
        HttpStatusCategory::ClientError => colors.orange,
        HttpStatusCategory::ServerError => colors.red,
    }
}
