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

//! The Hosts File Editor tool view: system hosts file inspection,
//! entry enable/disable toggling, IP/hostname validation, and persistence.

use std::path::PathBuf;

use heimdall_app::TabId;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::hosts_editor::{self, HostEntry, HostsParser};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the Hosts File Editor view.
#[derive(Debug, Clone)]
pub enum HostsMessage {
    /// Reload hosts file from disk.
    Reload,
    /// File reload completed.
    Loaded(Result<(Vec<String>, Vec<HostEntry>), String>),
    /// Toggle whether an entry is active or commented out.
    ToggleEntry(usize),
    /// Delete an entry by index.
    DeleteEntry(usize),
    /// Input text for new entry IP address.
    NewIpChanged(String),
    /// Input text for new entry hostname.
    NewHostChanged(String),
    /// Input text for new entry comment.
    NewCommentChanged(String),
    /// Add the new entry.
    AddEntry,
    /// Save all changes to disk.
    Save,
    /// Save operation completed.
    Saved(Result<(), String>),
    /// Copy entire serialized hosts file to clipboard.
    CopyAll,
}

/// The Hosts File Editor tool pane state.
#[derive(Debug)]
pub struct HostsEditorPane {
    path: PathBuf,
    loaded: bool,
    preamble_lines: Vec<String>,
    entries: Vec<HostEntry>,
    new_ip: String,
    new_host: String,
    new_comment: String,
    error: Option<String>,
    status: Option<String>,
}

impl Default for HostsEditorPane {
    fn default() -> Self {
        Self {
            path: hosts_editor::default_path(),
            loaded: false,
            preamble_lines: Vec::new(),
            entries: Vec::new(),
            new_ip: String::new(),
            new_host: String::new(),
            new_comment: String::new(),
            error: None,
            status: None,
        }
    }
}

/// Outcome of updating the Hosts File Editor tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl HostsEditorPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: HostsMessage) -> Outcome {
        match message {
            HostsMessage::Reload => {
                self.error = None;
                self.status = None;
                let path = self.path.clone();

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            hosts_editor::load_from_file(Some(&path))
                                .map(|(pre, entries, _, _)| (pre, entries))
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r)
                    },
                    move |res| {
                        Message::Tool(tab, super::ToolMessage::Hosts(HostsMessage::Loaded(res)))
                    },
                );

                Outcome::Task(task)
            }
            HostsMessage::Loaded(Ok((preamble, entries))) => {
                self.loaded = true;
                self.preamble_lines = preamble;
                self.entries = entries;
                self.status = Some(fl!(
                    "ui-tool-hosts-loaded",
                    count = self.entries.len(),
                    path = self.path.display().to_string()
                ));
                Outcome::None
            }
            HostsMessage::Loaded(Err(err)) => {
                self.loaded = true;
                self.error = Some(err);
                Outcome::None
            }
            HostsMessage::ToggleEntry(idx) => {
                if let Some(entry) = self.entries.get_mut(idx) {
                    entry.enabled = !entry.enabled;
                }
                Outcome::None
            }
            HostsMessage::DeleteEntry(idx) => {
                if idx < self.entries.len() {
                    self.entries.remove(idx);
                }
                Outcome::None
            }
            HostsMessage::NewIpChanged(ip) => {
                self.new_ip = ip;
                Outcome::None
            }
            HostsMessage::NewHostChanged(host) => {
                self.new_host = host;
                Outcome::None
            }
            HostsMessage::NewCommentChanged(comment) => {
                self.new_comment = comment;
                Outcome::None
            }
            HostsMessage::AddEntry => {
                let ip = self.new_ip.trim();
                let host = self.new_host.trim();
                let comment = self.new_comment.trim();

                if !HostsParser::is_valid_ip(ip) {
                    self.error = Some(fl!("ui-tool-hosts-invalid-ip"));
                    return Outcome::None;
                }
                if host.is_empty() {
                    self.error = Some(fl!("ui-tool-hosts-missing-host"));
                    return Outcome::None;
                }

                self.error = None;
                self.entries.push(HostEntry {
                    enabled: true,
                    ip: ip.to_owned(),
                    host: host.to_owned(),
                    comment: comment.to_owned(),
                });
                self.new_ip.clear();
                self.new_host.clear();
                self.new_comment.clear();
                Outcome::None
            }
            HostsMessage::Save => {
                self.error = None;
                self.status = None;
                let path = self.path.clone();
                let preamble = self.preamble_lines.clone();
                let entries = self.entries.clone();

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let content = hosts_editor::HostsParser::serialize(&preamble, &entries);
                            hosts_editor::save_to_file(Some(&path), &content)
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r)
                    },
                    move |res| {
                        Message::Tool(tab, super::ToolMessage::Hosts(HostsMessage::Saved(res)))
                    },
                );

                Outcome::Task(task)
            }
            HostsMessage::Saved(Ok(())) => {
                self.status = Some(fl!("ui-tool-hosts-save-success"));
                Outcome::None
            }
            HostsMessage::Saved(Err(err)) => {
                self.error = Some(fl!("ui-tool-hosts-save-failed", error = err));
                Outcome::None
            }
            HostsMessage::CopyAll => {
                let text = HostsParser::serialize(&self.preamble_lines, &self.entries);
                Outcome::Copy(CopySlot::HostsFileText, text)
            }
        }
    }

    /// Renders the Hosts File Editor tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::Hosts(msg));

        let reload_btn = super::action_button(
            fl!("ui-tool-hosts-reload"),
            false,
            Some(send(HostsMessage::Reload)),
        );

        let save_btn = super::action_button(
            fl!("ui-tool-hosts-save"),
            true,
            Some(send(HostsMessage::Save)),
        );

        let copy_btn = super::copy_button(
            fl!("ui-tool-hosts-copy-all"),
            state.copied(CopySlot::HostsFileText),
            send(HostsMessage::CopyAll),
            super::COPY_PADDING,
        );

        let header_row = row![
            text(fl!("ui-tool-hosts-title"))
                .size(font_size::SUBTITLE)
                .width(Length::Fill),
            reload_btn,
            save_btn,
            copy_btn,
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let header_card = container(header_row)
            .padding(spacing::MD)
            .style(styles::card);

        // Add new entry controls
        let ip_input = text_input(fl!("ui-tool-hosts-col-ip").as_str(), &self.new_ip)
            .on_input(move |val| send(HostsMessage::NewIpChanged(val)))
            .padding(super::INPUT_PADDING)
            .size(font_size::BODY)
            .width(Length::Fixed(150.0));

        let host_input = text_input(fl!("ui-tool-hosts-col-hostname").as_str(), &self.new_host)
            .on_input(move |val| send(HostsMessage::NewHostChanged(val)))
            .padding(super::INPUT_PADDING)
            .size(font_size::BODY)
            .width(Length::FillPortion(2));

        let comment_input =
            text_input(fl!("ui-tool-hosts-col-comment").as_str(), &self.new_comment)
                .on_input(move |val| send(HostsMessage::NewCommentChanged(val)))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
                .width(Length::FillPortion(1));

        let add_btn = button(text(fl!("ui-tool-hosts-btn-add")).size(font_size::BODY))
            .padding(super::PRIMARY_PADDING)
            .style(styles::secondary)
            .on_press(send(HostsMessage::AddEntry));

        let add_row = row![ip_input, host_input, comment_input, add_btn]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

        let add_card = container(add_row).padding(spacing::MD).style(styles::card);

        let mut body = column![header_card, add_card].spacing(spacing::MD);

        if let Some(err) = &self.error {
            body = body.push(
                container(text(err.as_str()).size(font_size::BODY).style(text::danger))
                    .padding(spacing::MD)
                    .style(styles::card),
            );
        }

        if let Some(status) = &self.status {
            body = body.push(
                text(status.as_str())
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }

        if !self.entries.is_empty() {
            let table_headers = row![
                text(fl!("ui-tool-hosts-col-enabled"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(60.0)),
                text(fl!("ui-tool-hosts-col-ip"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-hosts-col-hostname"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-hosts-col-comment"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(1)),
                text(fl!("ui-tool-hosts-col-action"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(70.0)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for (idx, e) in self.entries.iter().enumerate() {
                let chk =
                    checkbox(e.enabled).on_toggle(move |_| send(HostsMessage::ToggleEntry(idx)));

                let del_btn =
                    button(text(fl!("ui-tool-hosts-btn-delete")).size(font_size::CAPTION))
                        .style(styles::subtle)
                        .on_press(send(HostsMessage::DeleteEntry(idx)));

                let r = row![
                    container(chk).width(Length::Fixed(60.0)),
                    text(&e.ip)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(&e.host)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    text(&e.comment)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .style(text::secondary)
                        .width(Length::FillPortion(1)),
                    del_btn,
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center);

                entry_rows = entry_rows.push(r);
            }

            let card = container(scrollable(entry_rows))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        } else if self.loaded {
            body = body.push(super::empty_state(fl!("ui-tool-hosts-empty-state")));
        } else {
            // First time shown: trigger initial load
            body = body.push(super::empty_state(fl!("ui-tool-hosts-loading-hint")));
        }

        super::content_column(body)
    }
}
