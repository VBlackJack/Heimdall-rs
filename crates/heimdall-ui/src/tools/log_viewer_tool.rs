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

//! The Log Viewer tool view: real-time file tailing, regex filtering, auto-scroll,
//! and formatted log line stream inspection.

use std::path::PathBuf;

use heimdall_app::TabId;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::log_viewer::{self, LogFilter, LogLine, LogTailReader};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Maximum log lines displayed in UI to ensure smooth rendering performance.
const MAX_DISPLAY_LINES: usize = 1000;

/// Messages emitted by the Log Viewer view.
#[derive(Debug, Clone)]
pub enum LogViewerMessage {
    /// File path text changed.
    FilePathChanged(String),
    /// Load or reload log file.
    LoadFile,
    /// Clear currently loaded log buffer.
    Clear,
    /// Tail poll tick.
    TailTick,
    /// File read result.
    FileRead(Result<Vec<LogLine>, String>),
    /// Tail update result.
    TailUpdate(Result<Vec<LogLine>, String>),
    /// Toggle tail mode.
    ToggleTail(bool),
    /// Toggle auto-scroll.
    ToggleAutoScroll(bool),
    /// Filter query text changed.
    FilterChanged(String),
    /// Toggle case sensitivity.
    ToggleCaseSensitive(bool),
    /// Toggle regex filter mode.
    ToggleRegex(bool),
    /// Copy all displayed log lines.
    CopyLog,
}

/// The Log Viewer tool pane state.
#[derive(Debug, Default)]
pub struct LogViewerPane {
    file_path: String,
    filter_query: String,
    case_sensitive: bool,
    regex_mode: bool,
    tail_mode: bool,
    auto_scroll: bool,
    loading: bool,
    error: Option<String>,
    lines: Vec<LogLine>,
    byte_count: u64,
}

/// Outcome of updating the Log Viewer tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl LogViewerPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: LogViewerMessage) -> Outcome {
        match message {
            LogViewerMessage::FilePathChanged(path) => {
                self.file_path = path;
                Outcome::None
            }
            LogViewerMessage::LoadFile => {
                if self.file_path.trim().is_empty() {
                    return Outcome::None;
                }
                self.loading = true;
                self.error = None;
                let path = PathBuf::from(self.file_path.trim());

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut reader = LogTailReader::open(&path)?;
                            let lines = reader.read_initial(MAX_DISPLAY_LINES)?;
                            Ok(lines)
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r)
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::LogView(LogViewerMessage::FileRead(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            LogViewerMessage::Clear => {
                self.lines.clear();
                self.byte_count = 0;
                self.error = None;
                Outcome::None
            }
            LogViewerMessage::TailTick => {
                if !self.tail_mode || self.file_path.trim().is_empty() || self.loading {
                    return Outcome::None;
                }
                let path = PathBuf::from(self.file_path.trim());
                let offset = self.byte_count;

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut reader = LogTailReader::open(&path)?;
                            reader.seek_to(offset);
                            let (new_lines, new_pos) = reader.read_update()?;
                            Ok((new_lines, new_pos))
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r)
                    },
                    move |res: Result<(Vec<LogLine>, u64), String>| match res {
                        Ok((lines, _pos)) => Message::Tool(
                            tab,
                            super::ToolMessage::LogView(LogViewerMessage::TailUpdate(Ok(lines))),
                        ),
                        Err(err) => Message::Tool(
                            tab,
                            super::ToolMessage::LogView(LogViewerMessage::TailUpdate(Err(err))),
                        ),
                    },
                );

                Outcome::Task(task)
            }
            LogViewerMessage::FileRead(Ok(lines)) => {
                self.loading = false;
                self.byte_count = lines.iter().map(|l| l.text.len() as u64 + 1).sum();
                self.lines = lines;
                Outcome::None
            }
            LogViewerMessage::FileRead(Err(err)) => {
                self.loading = false;
                self.error = Some(err);
                Outcome::None
            }
            LogViewerMessage::TailUpdate(Ok(mut new_lines)) => {
                if !new_lines.is_empty() {
                    self.byte_count += new_lines
                        .iter()
                        .map(|l| l.text.len() as u64 + 1)
                        .sum::<u64>();
                    self.lines.append(&mut new_lines);
                    if self.lines.len() > MAX_DISPLAY_LINES {
                        let drain_count = self.lines.len() - MAX_DISPLAY_LINES;
                        self.lines.drain(0..drain_count);
                    }
                }
                Outcome::None
            }
            LogViewerMessage::TailUpdate(Err(err)) => {
                self.error = Some(err);
                Outcome::None
            }
            LogViewerMessage::ToggleTail(enabled) => {
                self.tail_mode = enabled;
                Outcome::None
            }
            LogViewerMessage::ToggleAutoScroll(enabled) => {
                self.auto_scroll = enabled;
                Outcome::None
            }
            LogViewerMessage::FilterChanged(filter) => {
                self.filter_query = filter;
                Outcome::None
            }
            LogViewerMessage::ToggleCaseSensitive(val) => {
                self.case_sensitive = val;
                Outcome::None
            }
            LogViewerMessage::ToggleRegex(val) => {
                self.regex_mode = val;
                Outcome::None
            }
            LogViewerMessage::CopyLog => {
                let filter = self.current_filter();
                let text = log_viewer::export_log_text(&self.lines, &filter);
                Outcome::Copy(CopySlot::LogViewText, text)
            }
        }
    }

    fn current_filter(&self) -> LogFilter {
        LogFilter {
            query: self.filter_query.clone(),
            case_sensitive: self.case_sensitive,
            use_regex: self.regex_mode,
        }
    }

    /// Renders the Log Viewer tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::LogView(msg));

        let path_input = text_input(&fl!("ui-tool-logview-path-placeholder"), &self.file_path)
            .on_input(move |s| send(LogViewerMessage::FilePathChanged(s)))
            .on_submit(send(LogViewerMessage::LoadFile))
            .width(Length::Fill);

        let btn_load = button(text(fl!("ui-tool-logview-btn-load")).size(font_size::CAPTION))
            .padding([spacing::XS, spacing::SM])
            .on_press_maybe(if self.loading || self.file_path.trim().is_empty() {
                None
            } else {
                Some(send(LogViewerMessage::LoadFile))
            });

        let btn_clear = button(text(fl!("ui-tool-logview-btn-clear")).size(font_size::CAPTION))
            .padding([spacing::XS, spacing::SM])
            .style(styles::secondary)
            .on_press(send(LogViewerMessage::Clear));

        let copy_btn = super::copy_button(
            fl!("ui-tool-logview-btn-copy"),
            state.copied(CopySlot::LogViewText),
            send(LogViewerMessage::CopyLog),
            super::COPY_PADDING,
        );

        let top_row = row![path_input, btn_load, btn_clear, copy_btn]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

        let filter_input = text_input(
            &fl!("ui-tool-logview-filter-placeholder"),
            &self.filter_query,
        )
        .on_input(move |s| send(LogViewerMessage::FilterChanged(s)))
        .width(Length::FillPortion(2));

        let chk_case = checkbox(self.case_sensitive)
            .label(fl!("ui-tool-logview-chk-casesensitive"))
            .size(font_size::CAPTION)
            .on_toggle(move |b| send(LogViewerMessage::ToggleCaseSensitive(b)));

        let chk_regex = checkbox(self.regex_mode)
            .label(fl!("ui-tool-logview-chk-regex"))
            .size(font_size::CAPTION)
            .on_toggle(move |b| send(LogViewerMessage::ToggleRegex(b)));

        let chk_tail = checkbox(self.tail_mode)
            .label(fl!("ui-tool-logview-chk-tail"))
            .size(font_size::CAPTION)
            .on_toggle(move |b| send(LogViewerMessage::ToggleTail(b)));

        let chk_autoscroll = checkbox(self.auto_scroll)
            .label(fl!("ui-tool-logview-chk-autoscroll"))
            .size(font_size::CAPTION)
            .on_toggle(move |b| send(LogViewerMessage::ToggleAutoScroll(b)));

        let filter_row = row![filter_input, chk_case, chk_regex, chk_tail, chk_autoscroll]
            .spacing(spacing::MD)
            .align_y(Alignment::Center);

        let filter = self.current_filter();
        let matching_lines = filter.filter_lines(&self.lines);

        let status_text = text(fl!(
            "ui-tool-logview-status",
            lines = matching_lines.len().to_string(),
            bytes = self.byte_count.to_string()
        ))
        .size(font_size::SMALL_CAPTION)
        .style(text::secondary);

        let content_body: Element<'a, Message> = if let Some(err) = &self.error {
            container(text(err).size(font_size::CAPTION).style(text::danger))
                .padding(spacing::MD)
                .into()
        } else if self.lines.is_empty() {
            container(
                text(fl!("ui-tool-logview-empty-state"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            )
            .padding(spacing::XL)
            .center_x(Length::Fill)
            .into()
        } else {
            let mut line_column = column![].spacing(spacing::XS);

            for line in matching_lines {
                let styled_text = if line.is_error {
                    text(&line.text)
                        .size(font_size::SMALL_CAPTION)
                        .style(text::danger)
                } else if line.is_warning {
                    text(&line.text)
                        .size(font_size::SMALL_CAPTION)
                        .style(text::secondary)
                } else {
                    text(&line.text).size(font_size::SMALL_CAPTION)
                };

                let line_row = row![
                    text(format!("{:4}", line.line_number))
                        .size(font_size::SMALL_CAPTION)
                        .style(text::secondary)
                        .width(Length::Fixed(40.0)),
                    styled_text,
                ]
                .spacing(spacing::SM);

                line_column = line_column.push(line_row);
            }

            scrollable(
                container(line_column)
                    .padding(spacing::SM)
                    .style(styles::card)
                    .width(Length::Fill),
            )
            .height(Length::Fill)
            .into()
        };

        column![top_row, filter_row, status_text, content_body]
            .spacing(spacing::MD)
            .padding(spacing::MD)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
