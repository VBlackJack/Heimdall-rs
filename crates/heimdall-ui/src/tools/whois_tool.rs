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

//! WHOIS Lookup tool pane for querying domain and IP registration data, as the C#
//! `WhoisLookupViewModel`.

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
use heimdall_app::network_tools::{self, WhoisFailure, WhoisReply};
use heimdall_core::tools::whois::{self, MAX_REPLY_BYTES};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Bytes in a KiB, for the size a reply is cut at.
const BYTES_PER_KIB: usize = 1024;

/// Messages emitted by the WHOIS Lookup tool.
#[derive(Debug, Clone)]
pub enum WhoisMessage {
    /// Domain or IP text input changed.
    DomainChanged(String),
    /// Start WHOIS lookup.
    Lookup,
    /// The lookup of `.0` ended.
    LookupDone(String, Result<WhoisReply, WhoisFailure>),
    /// Copy output text to clipboard.
    CopyOutput,
}

/// What the tool says went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Problem {
    /// Nothing typed, the C# `ToolWhoisErrorDomainRequired`.
    Required,
    /// Not a domain or an IP address, the C# `ToolWhoisErrorInvalidDomain`.
    Invalid,
    /// The lookup failed.
    Failed(WhoisFailure),
}

/// The WHOIS Lookup tool pane state.
#[derive(Debug, Default)]
pub struct WhoisPane {
    domain: String,
    running: bool,
    problem: Option<Problem>,
    /// The domain asked and its reply.
    result: Option<(String, WhoisReply)>,
}

/// Outcome of updating the WHOIS Lookup tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

/// What a message did, before its tab is known.
#[derive(Debug)]
enum Applied {
    None,
    Run(Task<WhoisMessage>),
    Copy(CopySlot, String),
}

impl WhoisPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: WhoisMessage) -> Outcome {
        match self.apply(message) {
            Applied::None => Outcome::None,
            Applied::Run(task) => Outcome::Task(
                task.map(move |message| Message::Tool(tab, ToolMessage::Whois(message))),
            ),
            Applied::Copy(slot, content) => Outcome::Copy(slot, content),
        }
    }

    /// Handles a message, as [`Self::update`] without the tab.
    fn apply(&mut self, message: WhoisMessage) -> Applied {
        match message {
            WhoisMessage::DomainChanged(val) => {
                self.domain = val;
                self.problem = None;
                Applied::None
            }
            WhoisMessage::Lookup => {
                // One lookup at a time, as the C# `CanLookup`.
                if self.running {
                    return Applied::None;
                }
                if self.domain.trim().is_empty() {
                    self.problem = Some(Problem::Required);
                    return Applied::None;
                }
                let Some(query) = whois::whois_query(&self.domain) else {
                    self.problem = Some(Problem::Invalid);
                    return Applied::None;
                };

                self.running = true;
                self.problem = None;
                self.result = None;

                let server = whois::resolve_whois_server(&query);
                Applied::Run(Task::perform(
                    async move {
                        let reply = network_tools::whois(server, &query).await;
                        (query, reply)
                    },
                    |(query, reply)| WhoisMessage::LookupDone(query, reply),
                ))
            }
            WhoisMessage::LookupDone(query, reply) => {
                self.running = false;
                match reply {
                    Ok(reply) => self.result = Some((query, reply)),
                    Err(failure) => self.problem = Some(Problem::Failed(failure)),
                }
                Applied::None
            }
            WhoisMessage::CopyOutput => {
                let output = self
                    .result
                    .as_ref()
                    .map(|(_, reply)| reply.text.clone())
                    .unwrap_or_default();
                Applied::Copy(CopySlot::WhoisOutput, output)
            }
        }
    }

    /// What the tool says went wrong, in the user's language, as the C# `ToolWhoisError*`.
    fn problem_text(problem: &Problem) -> String {
        match problem {
            Problem::Required => fl!("ui-tool-whois-error-required"),
            Problem::Invalid => fl!("ui-tool-whois-error-invalid"),
            Problem::Failed(WhoisFailure::TimedOut) => fl!("ui-tool-whois-error-timeout"),
            Problem::Failed(WhoisFailure::Failed(reason)) => fl!(
                "ui-tool-whois-error-failed",
                reason = super::down_reason_text(reason)
            ),
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
        .on_press_maybe(
            (!self.running).then_some(Message::Tool(tab, ToolMessage::Whois(WhoisMessage::Lookup))),
        );

        let input_row = row![domain_input, lookup_btn]
            .spacing(spacing::MD)
            .align_y(Alignment::Center);

        let mut body_col = column![input_row].spacing(spacing::MD);

        if let Some(problem) = &self.problem {
            body_col = body_col.push(
                text(Self::problem_text(problem))
                    .size(font_size::BODY)
                    .color(colors.red),
            );
        }

        if let Some((query, reply)) = &self.result {
            let elapsed = u64::try_from(reply.elapsed.as_millis()).unwrap_or(u64::MAX);
            let header_row = row![
                column![
                    text(fl!("ui-tool-whois-result-header", domain = query.as_str()))
                        .size(font_size::BODY)
                        .font(styles::SEMIBOLD),
                    text(fl!("ui-tool-whois-status-complete", elapsed = elapsed))
                        .size(font_size::CAPTION)
                        .color(colors.green),
                ]
                .width(Length::Fill),
                super::copy_button(
                    fl!("ui-tool-whois-btn-copy"),
                    state.copied(CopySlot::WhoisOutput),
                    Message::Tool(tab, ToolMessage::Whois(WhoisMessage::CopyOutput),),
                    super::COPY_PADDING,
                ),
            ]
            .align_y(Alignment::Center);
            body_col = body_col.push(header_row);

            if reply.truncated {
                let kib = MAX_REPLY_BYTES / BYTES_PER_KIB;
                body_col = body_col.push(
                    text(fl!("ui-tool-whois-truncated", kib = kib))
                        .size(font_size::CAPTION)
                        .color(colors.orange),
                );
            }

            let output_viewer = container(
                scrollable(
                    text(&reply.text)
                        .size(font_size::CAPTION)
                        .font(super::BOX_FONT),
                )
                .height(Length::Fill),
            )
            .padding([10.0, 12.0])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(styles::card);

            body_col = body_col.push(output_viewer);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_domain_is_checked_before_any_lookup() {
        let mut pane = WhoisPane::default();
        assert!(matches!(pane.apply(WhoisMessage::Lookup), Applied::None));
        assert_eq!(pane.problem, Some(Problem::Required));
        pane.apply(WhoisMessage::DomainChanged(
            "example.com\r\nevil".to_owned(),
        ));
        assert!(matches!(pane.apply(WhoisMessage::Lookup), Applied::None));
        assert_eq!(pane.problem, Some(Problem::Invalid));
        assert!(!pane.running);
    }

    #[test]
    fn a_second_lookup_waits_for_the_first() {
        let mut pane = WhoisPane::default();
        pane.apply(WhoisMessage::DomainChanged("example.com".to_owned()));
        assert!(matches!(pane.apply(WhoisMessage::Lookup), Applied::Run(_)));
        assert!(pane.running);
        assert!(matches!(pane.apply(WhoisMessage::Lookup), Applied::None));
        pane.apply(WhoisMessage::LookupDone(
            "example.com".to_owned(),
            Err(WhoisFailure::TimedOut),
        ));
        assert!(!pane.running);
        assert_eq!(pane.problem, Some(Problem::Failed(WhoisFailure::TimedOut)));
    }
}
