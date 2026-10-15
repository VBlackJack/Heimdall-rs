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

//! The DNS record lookup tool view, as the C# `DnsLookupView` and `DnsLookupViewModel`:
//! record type selection, public nameserver presets, a lookup bounded in time that Stop
//! cancels, as the C# `Cancel`, and the answer shown and copied.
//!
//! A lookup is one task, aborted by Stop or when the tab closes, which kills the `nslookup`
//! program under way; its answer carries its lookup's number, so an earlier one never
//! replaces a later one. Enter while a lookup goes on starts nothing, as the C#
//! `CanLookup`.

use std::fmt;
use std::fmt::Write as _;

use heimdall_app::TabId;
use heimdall_app::net_tools;
use iced::task::Handle;
use iced::widget::{column, container, pick_list, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::dns_lookup::{
    self, DnsLookupError, DnsLookupRequest, DnsLookupResult, DnsRecordType, DnsServerPreset,
};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the DNS lookup tool view.
#[derive(Debug, Clone)]
pub enum DnsMessage {
    /// Hostname input changed.
    HostChanged(String),
    /// Record type selection changed.
    RecordTypeSelected(DnsRecordType),
    /// Nameserver preset selection changed.
    ServerSelected(DnsServerPreset),
    /// Execute lookup query.
    Lookup,
    /// Cancel the lookup under way.
    Stop,
    /// Lookup `.0` completed.
    Finished(u64, Result<DnsLookupResult, DnsLookupError>),
    /// Copy output to clipboard.
    CopyOutput,
}

/// A nameserver preset as the list names it, in the user's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ServerChoice(DnsServerPreset);

/// The presets, in the list's order.
const SERVER_CHOICES: [ServerChoice; 5] = [
    ServerChoice(DnsServerPreset::DefaultSystem),
    ServerChoice(DnsServerPreset::Cloudflare),
    ServerChoice(DnsServerPreset::Google),
    ServerChoice(DnsServerPreset::Quad9),
    ServerChoice(DnsServerPreset::OpenDns),
];

impl fmt::Display for ServerChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match (self.0.provider_name(), self.0.server_ip()) {
            (Some(name), Some(address)) => {
                fl!("ui-tool-dns-server-preset", name = name, address = address)
            }
            _ => fl!("ui-tool-dns-server-system"),
        };
        f.write_str(&label)
    }
}

/// The DNS Lookup tool pane state.
#[derive(Debug)]
pub struct DnsPane {
    host: String,
    record_type: DnsRecordType,
    server: DnsServerPreset,
    running: bool,
    /// The number of the latest lookup; the answer of any other is ignored.
    run: u64,
    /// Aborts the lookup under way, also when the pane is dropped.
    stop: Option<Handle>,
    error: Option<String>,
    result: Option<DnsLookupResult>,
}

impl Default for DnsPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            record_type: DnsRecordType::A,
            server: DnsServerPreset::DefaultSystem,
            running: false,
            run: 0,
            stop: None,
            error: None,
            result: None,
        }
    }
}

/// Outcome of updating the DNS Lookup tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

/// Why a lookup gave no answer, in the user's words.
fn lookup_error(error: &DnsLookupError) -> String {
    match error {
        DnsLookupError::TimedOut => fl!("ui-tool-dns-error-timeout"),
        DnsLookupError::NoProgram => fl!("ui-tool-dns-error-no-program"),
        DnsLookupError::Failed(why) => fl!("ui-tool-dns-error-failed", reason = why.as_str()),
    }
}

/// The title of a result.
fn result_title(result: &DnsLookupResult) -> String {
    fl!(
        "ui-tool-dns-results-title",
        host = result.host.as_str(),
        record_type = result.record_type.wire_format(),
        time = result.elapsed_ms.to_string()
    )
}

impl DnsPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: DnsMessage) -> Outcome {
        if !matches!(message, DnsMessage::Lookup) {
            return self.apply(message);
        }
        let Some(request) = self.start() else {
            return Outcome::None;
        };
        let run = self.run;
        let (task, handle) = Task::perform(net_tools::dns::lookup(request), move |answer| {
            Message::Tool(
                tab,
                super::ToolMessage::Dns(DnsMessage::Finished(run, answer)),
            )
        })
        .abortable();
        self.stop = Some(handle.abort_on_drop());
        Outcome::Task(task)
    }

    /// Handles every message but [`DnsMessage::Lookup`], which needs the tab for its task.
    fn apply(&mut self, message: DnsMessage) -> Outcome {
        match message {
            DnsMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
            }
            DnsMessage::RecordTypeSelected(rt) => self.record_type = rt,
            DnsMessage::ServerSelected(srv) => self.server = srv,
            DnsMessage::Lookup => {}
            DnsMessage::Stop => self.finish(),
            DnsMessage::Finished(run, answer) => {
                if run == self.run && self.running {
                    self.finish();
                    match answer {
                        Ok(result) => self.result = Some(result),
                        Err(error) => self.error = Some(lookup_error(&error)),
                    }
                }
            }
            DnsMessage::CopyOutput => {
                if let Some(res) = &self.result {
                    let mut text_buf = String::new();
                    let _ = writeln!(text_buf, "{}", result_title(res));
                    for r in &res.records {
                        let _ = writeln!(text_buf, "{r}");
                    }
                    return Outcome::Copy(CopySlot::DnsOutput, text_buf);
                }
            }
        }
        Outcome::None
    }

    /// A new lookup's request, the pane reset for it; `None` while a lookup goes on, or when
    /// the host is refused, said in the error card.
    fn start(&mut self) -> Option<DnsLookupRequest> {
        if self.running {
            return None;
        }
        match dns_lookup::validate_request(&self.host, self.record_type, self.server) {
            Ok(request) => {
                self.run = self.run.wrapping_add(1);
                self.running = true;
                self.error = None;
                self.result = None;
                Some(request)
            }
            Err(key) => {
                self.error = Some(match key {
                    "ui-tool-dns-error-host-required" => fl!("ui-tool-dns-error-host-required"),
                    _ => fl!("ui-tool-dns-error-host-invalid"),
                });
                None
            }
        }
    }

    /// The lookup over: stopped, its task aborted.
    fn finish(&mut self) {
        self.running = false;
        if let Some(stop) = self.stop.take() {
            stop.abort();
        }
    }

    /// Renders the DNS Lookup tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::Dns(msg));

        let mut host_input = text_input(&fl!("ui-tool-dns-host-watermark"), &self.host)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .width(Length::FillPortion(3));
        if !self.running {
            host_input = host_input
                .on_input(move |v| send(DnsMessage::HostChanged(v)))
                .on_submit(send(DnsMessage::Lookup));
        }

        let type_picker = pick_list(&DnsRecordType::ALL[..], Some(self.record_type), move |t| {
            send(DnsMessage::RecordTypeSelected(t))
        })
        .style(styles::pick_list)
        .width(Length::FillPortion(1));

        let server_picker = pick_list(
            &SERVER_CHOICES[..],
            Some(ServerChoice(self.server)),
            move |choice: ServerChoice| send(DnsMessage::ServerSelected(choice.0)),
        )
        .style(styles::pick_list)
        .width(Length::FillPortion(2));

        let lookup_button = if self.running {
            super::action_button(fl!("ui-tool-dns-stop"), false, Some(send(DnsMessage::Stop)))
        } else {
            super::action_button(
                fl!("ui-tool-dns-lookup"),
                true,
                Some(send(DnsMessage::Lookup)),
            )
        };

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-dns-host")), host_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(3)),
            column![super::field_label(fl!("ui-tool-dns-type")), type_picker]
                .spacing(spacing::XS)
                .width(Length::FillPortion(1)),
            column![super::field_label(fl!("ui-tool-dns-server")), server_picker]
                .spacing(spacing::XS)
                .width(Length::FillPortion(2)),
            container(lookup_button).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::End);

        let input_card = container(column![inputs_row])
            .padding(spacing::MD)
            .style(styles::card);
        let mut body = column![input_card].spacing(spacing::MD);

        if self.running {
            body = body.push(
                text(fl!("ui-tool-dns-searching"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }

        if let Some(err) = &self.error {
            body = body.push(
                container(text(err.as_str()).size(font_size::BODY).style(text::danger))
                    .padding(spacing::MD)
                    .style(styles::card),
            );
        }

        if let Some(res) = &self.result {
            let copy_btn = super::copy_button(
                fl!("ui-tool-dns-copy"),
                state.copied(CopySlot::DnsOutput),
                send(DnsMessage::CopyOutput),
                super::COPY_PADDING,
            );

            let header_row = row![
                text(result_title(res))
                    .size(font_size::SUBTITLE)
                    .width(Length::Fill),
                copy_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let mut records_col = column![].spacing(spacing::XS);
            if res.records.is_empty() {
                records_col = records_col.push(
                    text(fl!("ui-tool-dns-no-records"))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .style(text::secondary),
                );
            } else {
                for r in &res.records {
                    records_col = records_col.push(
                        container(text(r.as_str()).size(font_size::BODY).font(super::BOX_FONT))
                            .padding(spacing::XS)
                            .style(styles::card)
                            .width(Length::Fill),
                    );
                }
            }

            let results_card = container(column![header_row, records_col].spacing(spacing::SM))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(results_card);
        }

        super::content_column(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(host: &str) -> DnsLookupResult {
        DnsLookupResult {
            host: host.into(),
            record_type: DnsRecordType::A,
            server: None,
            records: vec!["192.0.2.1".into()],
            elapsed_ms: 3,
        }
    }

    fn started(pane: &mut DnsPane, host: &str) -> Handle {
        pane.apply(DnsMessage::HostChanged(host.into()));
        assert!(pane.start().is_some(), "{:?}", pane.error);
        let (_, handle) = Task::<()>::none().abortable();
        let watcher = handle.clone();
        pane.stop = Some(handle.abort_on_drop());
        watcher
    }

    #[test]
    fn stop_cancels_the_lookup_and_its_late_answer_is_dropped() {
        let mut pane = DnsPane::default();
        let handle = started(&mut pane, "example.com");
        assert_eq!(pane.start(), None, "one lookup at a time");
        pane.apply(DnsMessage::Stop);
        assert!(!pane.running && handle.is_aborted());
        pane.apply(DnsMessage::Finished(pane.run, Ok(answer("example.com"))));
        assert!(pane.result.is_none());
    }

    #[test]
    fn an_earlier_answer_never_replaces_a_later_one() {
        let mut pane = DnsPane::default();
        started(&mut pane, "first.example");
        let first = pane.run;
        pane.apply(DnsMessage::Stop);
        started(&mut pane, "second.example");
        pane.apply(DnsMessage::Finished(first, Ok(answer("first.example"))));
        assert!(pane.result.is_none() && pane.running);
        pane.apply(DnsMessage::Finished(pane.run, Ok(answer("second.example"))));
        assert_eq!(
            pane.result.map(|result| result.host),
            Some("second.example".into())
        );
    }

    #[test]
    fn a_host_that_reads_as_an_option_is_refused() {
        let mut pane = DnsPane::default();
        pane.apply(DnsMessage::HostChanged("-type=ANY".into()));
        assert_eq!(pane.start(), None);
        assert_eq!(
            pane.error.as_deref(),
            Some(fl!("ui-tool-dns-error-host-invalid").as_str())
        );
    }

    #[test]
    fn a_timeout_is_said_in_the_users_words() {
        let mut pane = DnsPane::default();
        started(&mut pane, "example.com");
        pane.apply(DnsMessage::Finished(
            pane.run,
            Err(DnsLookupError::TimedOut),
        ));
        assert_eq!(pane.error, Some(fl!("ui-tool-dns-error-timeout")));
        assert!(!pane.running);
    }

    #[test]
    fn the_presets_are_named_in_the_users_language() {
        assert_eq!(
            ServerChoice(DnsServerPreset::DefaultSystem).to_string(),
            fl!("ui-tool-dns-server-system")
        );
        assert!(
            ServerChoice(DnsServerPreset::Quad9)
                .to_string()
                .contains("9.9.9.9")
        );
    }
}
