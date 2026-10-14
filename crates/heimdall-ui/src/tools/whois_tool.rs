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

//! WHOIS Lookup tool pane for querying domain and IP registration data.

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
use heimdall_core::tools::whois::{self, WhoisLookupRequest, WhoisLookupResult};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the WHOIS Lookup tool.
#[derive(Debug, Clone)]
pub enum WhoisMessage {
    /// Domain or IP text input changed.
    DomainChanged(String),
    /// Start WHOIS lookup.
    Lookup,
    /// WHOIS lookup query completed.
    LookupDone(WhoisLookupResult),
    /// Copy output text to clipboard.
    CopyOutput,
}

/// The WHOIS Lookup tool pane state.
#[derive(Debug, Default)]
pub struct WhoisPane {
    domain: String,
    running: bool,
    error: Option<String>,
    result: Option<WhoisLookupResult>,
}

/// Outcome of updating the WHOIS Lookup tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl WhoisPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: WhoisMessage) -> Outcome {
        match message {
            WhoisMessage::DomainChanged(val) => {
                self.domain = val;
                self.error = None;
                Outcome::None
            }
            WhoisMessage::Lookup => {
                let domain = self.domain.trim().to_string();
                if domain.is_empty() {
                    self.error = Some(fl!("ui-tool-whois-error-required"));
                    return Outcome::None;
                }

                self.running = true;
                self.error = None;
                self.result = None;

                let domain_fallback = domain.clone();
                let req = WhoisLookupRequest { domain };

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || whois::lookup_whois(&req))
                            .await
                            .unwrap_or_else(|_| WhoisLookupResult {
                                domain: domain_fallback,
                                output: String::new(),
                                elapsed_ms: 0,
                                success: false,
                                error: Some("Lookup task failed to execute".into()),
                            })
                    },
                    move |res| {
                        Message::Tool(tab, ToolMessage::Whois(WhoisMessage::LookupDone(res)))
                    },
                );

                Outcome::Task(task)
            }
            WhoisMessage::LookupDone(res) => {
                self.running = false;
                if res.success {
                    self.result = Some(res);
                } else {
                    self.error = res.error;
                }
                Outcome::None
            }
            WhoisMessage::CopyOutput => {
                let output = self
                    .result
                    .as_ref()
                    .map(|r| r.output.clone())
                    .unwrap_or_default();
                Outcome::Copy(CopySlot::WhoisOutput, output)
            }
        }
    }

    /// Renders the WHOIS Lookup tool pane.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &'a ToolPane,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let colors = crate::themes::colors_of(theme);

        let domain_input = text_input(&fl!("ui-tool-whois-domain-placeholder"), &self.domain)
            .on_input(move |v| {
                Message::Tool(tab, ToolMessage::Whois(WhoisMessage::DomainChanged(v)))
            })
            .on_submit(Message::Tool(tab, ToolMessage::Whois(WhoisMessage::Lookup)))
            .padding(super::INPUT_PADDING)
            .size(font_size::BODY)
            .width(Length::Fill);

        let lookup_btn = button(
            text(if self.running {
                fl!("ui-tool-whois-status-querying")
            } else {
                fl!("ui-tool-whois-btn-lookup")
            })
            .size(font_size::BODY)
            .font(styles::SEMIBOLD),
        )
        .padding(super::PRIMARY_PADDING)
        .style(styles::primary)
        .on_press(Message::Tool(tab, ToolMessage::Whois(WhoisMessage::Lookup)));

        let input_row = row![domain_input, lookup_btn]
            .spacing(spacing::MD)
            .align_y(Alignment::Center);

        let mut body_col = column![input_row].spacing(spacing::MD);

        if let Some(err) = &self.error {
            body_col = body_col.push(text(err).size(font_size::BODY).color(colors.red));
        }

        if let Some(res) = &self.result {
            let header_row = row![
                text(fl!(
                    "ui-tool-whois-status-complete",
                    elapsed = res.elapsed_ms
                ))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .color(colors.green)
                .width(Length::Fill),
                super::copy_button(
                    fl!("ui-tool-whois-btn-copy"),
                    state.copied(CopySlot::WhoisOutput),
                    Message::Tool(tab, ToolMessage::Whois(WhoisMessage::CopyOutput),),
                    super::COPY_PADDING,
                ),
            ]
            .align_y(Alignment::Center);

            let output_viewer = container(
                scrollable(
                    text(&res.output)
                        .size(font_size::CAPTION)
                        .font(super::BOX_FONT),
                )
                .height(Length::Fill),
            )
            .padding([10.0, 12.0])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(styles::card);

            body_col = body_col.push(header_row).push(output_viewer);
        } else if !self.running {
            body_col = body_col.push(
                container(
                    text(fl!("ui-tool-whois-empty-state"))
                        .size(font_size::BODY)
                        .style(text::secondary),
                )
                .padding(super::EMPTY_STATE_PADDING),
            );
        }

        let content = column![body_col]
            .spacing(spacing::LG)
            .padding(super::BODY_PADDING)
            .max_width(super::CONTENT_MAX_WIDTH);

        content.into()
    }
}
