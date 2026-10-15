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

//! The HTTP Header Analyzer tool view: URL normalization, header fetching,
//! security posture analysis, information disclosure detection, letter grading, and report export.

use std::process::Command;

use heimdall_app::TabId;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::http_headers::{
    self, HeaderCheckStatus, HeaderEvaluation, HttpGrade, HttpResponseInfo,
};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the HTTP Header Analyzer view.
#[derive(Debug, Clone)]
pub enum HttpHeadersMessage {
    /// Target URL input changed.
    UrlChanged(String),
    /// Start header analysis.
    Analyze,
    /// Header fetch and evaluation completed.
    Finished(Result<(HttpResponseInfo, HeaderEvaluation), String>),
    /// Copy report text to clipboard.
    CopyReport,
}

/// The HTTP Header Analyzer tool pane state.
#[derive(Debug)]
pub struct HttpHeadersPane {
    url: String,
    analyzing: bool,
    error: Option<String>,
    response_info: Option<HttpResponseInfo>,
    evaluation: Option<HeaderEvaluation>,
    report_text: String,
}

impl Default for HttpHeadersPane {
    fn default() -> Self {
        Self {
            url: String::new(),
            analyzing: false,
            error: None,
            response_info: None,
            evaluation: None,
            report_text: String::new(),
        }
    }
}

/// Outcome of updating the HTTP Header Analyzer tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

/// Fetches HTTP response headers via standard system curl with fallback.
fn fetch_headers_sync(url: &str) -> Result<String, String> {
    let mut cmd = Command::new("curl");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.arg("-I");
    cmd.arg("-s");
    cmd.arg("-S");
    cmd.arg("-k");
    cmd.arg("--max-time");
    cmd.arg("10");
    cmd.arg(url);

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stdout.trim().is_empty() {
                Ok(stdout.to_string())
            } else if !stderr.trim().is_empty() {
                Err(stderr.trim().to_string())
            } else {
                Err("Empty response received".to_string())
            }
        }
        Err(err) => Err(format!("Failed to execute header request: {err}")),
    }
}

impl HttpHeadersPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: HttpHeadersMessage) -> Outcome {
        match message {
            HttpHeadersMessage::UrlChanged(val) => {
                self.url = val;
                self.error = None;
                Outcome::None
            }
            HttpHeadersMessage::Analyze => {
                let normalized = match http_headers::try_normalize_url(&self.url) {
                    Ok(norm) => norm,
                    Err("ui-tool-headers-error-url-required") => {
                        self.error = Some(fl!("ui-tool-headers-error-url-required"));
                        return Outcome::None;
                    }
                    Err(_) => {
                        self.error = Some(fl!("ui-tool-headers-error-invalid-url"));
                        return Outcome::None;
                    }
                };

                self.analyzing = true;
                self.error = None;
                self.response_info = None;
                self.evaluation = None;
                self.report_text.clear();

                let task_url = normalized;
                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let raw_res = fetch_headers_sync(&task_url)?;
                            let info = http_headers::parse_http_response(&raw_res);
                            let eval = http_headers::evaluate_headers(&info.headers);
                            Ok((info, eval))
                        })
                        .await
                        .unwrap_or_else(|err| Err(err.to_string()))
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::HttpHeaders(HttpHeadersMessage::Finished(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            HttpHeadersMessage::Finished(res) => {
                self.analyzing = false;
                match res {
                    Ok((info, eval)) => {
                        self.report_text =
                            http_headers::build_text_report(&self.url, &eval, &info.header_section);
                        self.response_info = Some(info);
                        self.evaluation = Some(eval);
                        self.error = None;
                    }
                    Err(err) => {
                        self.error = Some(err);
                        self.response_info = None;
                        self.evaluation = None;
                        self.report_text.clear();
                    }
                }
                Outcome::None
            }
            HttpHeadersMessage::CopyReport => {
                if !self.report_text.is_empty() {
                    Outcome::Copy(CopySlot::HttpHeadersReport, self.report_text.clone())
                } else {
                    Outcome::None
                }
            }
        }
    }

    /// Renders the HTTP Header Analyzer view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::HttpHeaders(msg));

        let url_input = text_input(&fl!("ui-tool-headers-url-watermark"), &self.url)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(HttpHeadersMessage::UrlChanged(v)))
            .on_submit(send(HttpHeadersMessage::Analyze))
            .width(Length::Fill);

        let analyze_button = if self.analyzing {
            super::action_button(fl!("ui-tool-headers-analyzing"), false, None)
        } else {
            super::action_button(
                fl!("ui-tool-headers-analyze"),
                true,
                Some(send(HttpHeadersMessage::Analyze)),
            )
        };

        let controls_row = row![
            column![super::field_label(fl!("ui-tool-headers-url")), url_input]
                .spacing(spacing::XS)
                .width(Length::Fill),
            container(analyze_button).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::End);

        let input_card = container(column![controls_row])
            .padding(spacing::MD)
            .style(styles::card);
        let mut body = column![input_card].spacing(spacing::MD);

        if let Some(err) = &self.error {
            body = body.push(
                container(text(err.as_str()).size(font_size::BODY).style(text::danger))
                    .padding(spacing::MD)
                    .style(styles::card),
            );
        }

        if let (Some(info), Some(eval)) = (&self.response_info, &self.evaluation) {
            let (grade_color, grade_desc): (fn(&Theme) -> text::Style, String) = match eval.grade {
                HttpGrade::APlus => (text::success, fl!("ui-tool-headers-grade-aplus")),
                HttpGrade::A => (text::success, fl!("ui-tool-headers-grade-a")),
                HttpGrade::BPlus | HttpGrade::B => (text::primary, fl!("ui-tool-headers-grade-b")),
                HttpGrade::C => (text::warning, fl!("ui-tool-headers-grade-c")),
                HttpGrade::D => (text::danger, fl!("ui-tool-headers-grade-d")),
                HttpGrade::F => (text::danger, fl!("ui-tool-headers-grade-f")),
            };

            let copy_report_btn = super::copy_button(
                fl!("ui-tool-headers-copy-report"),
                state.copied(CopySlot::HttpHeadersReport),
                send(HttpHeadersMessage::CopyReport),
                super::COPY_PADDING,
            );

            // Grade Summary Card
            let grade_badge = container(
                text(eval.grade.label())
                    .size(font_size::HEADLINE)
                    .font(super::BOX_FONT)
                    .style(grade_color),
            )
            .padding(spacing::MD)
            .style(styles::card);

            let grade_summary = row![
                grade_badge,
                column![
                    text(format!("HTTP Status: {}", info.status_code))
                        .size(font_size::SUBTITLE)
                        .font(super::BOX_FONT),
                    text(grade_desc)
                        .size(font_size::BODY)
                        .style(text::secondary),
                ]
                .spacing(spacing::XS)
                .width(Length::Fill),
                copy_report_btn,
            ]
            .spacing(spacing::MD)
            .align_y(Alignment::Center);

            body = body.push(
                container(grade_summary)
                    .padding(spacing::MD)
                    .style(styles::card),
            );

            // Security Headers Card
            let mut sec_col =
                column![text(fl!("ui-tool-headers-sec-headers-title")).size(font_size::SUBTITLE)]
                    .spacing(spacing::SM);

            for check in &eval.security_checks {
                let status_cell = match check.status {
                    HeaderCheckStatus::Pass => text(fl!("ui-tool-headers-status-pass"))
                        .size(font_size::CAPTION)
                        .style(text::success)
                        .width(Length::Fixed(60.0)),
                    HeaderCheckStatus::Warn => text(fl!("ui-tool-headers-status-warn"))
                        .size(font_size::CAPTION)
                        .style(text::warning)
                        .width(Length::Fixed(60.0)),
                    HeaderCheckStatus::Fail => text(fl!("ui-tool-headers-status-fail"))
                        .size(font_size::CAPTION)
                        .style(text::danger)
                        .width(Length::Fixed(60.0)),
                };

                let val_display = if check.value.is_empty() {
                    fl!("ui-tool-headers-missing")
                } else {
                    check.value.clone()
                };

                let mut row_col = column![
                    row![
                        status_cell,
                        text(&check.name)
                            .size(font_size::BODY)
                            .font(super::BOX_FONT)
                            .width(Length::Fixed(220.0)),
                        text(val_display)
                            .size(font_size::CAPTION)
                            .font(super::BOX_FONT)
                            .style(text::secondary)
                            .width(Length::Fill),
                    ]
                    .spacing(spacing::SM)
                    .align_y(Alignment::Center)
                ];

                if let Some(rec_key) = check.recommendation_key {
                    row_col = row_col.push(
                        text(recommendation_text(rec_key))
                            .size(font_size::CAPTION)
                            .style(text::warning),
                    );
                }

                sec_col = sec_col.push(
                    container(row_col)
                        .padding(spacing::XS)
                        .style(styles::card)
                        .width(Length::Fill),
                );
            }

            body = body.push(container(sec_col).padding(spacing::MD).style(styles::card));

            // Information Disclosure Card
            let mut disc_col =
                column![text(fl!("ui-tool-headers-disclosure-title")).size(font_size::SUBTITLE)]
                    .spacing(spacing::SM);

            for check in &eval.disclosure_checks {
                let status_cell = match check.status {
                    HeaderCheckStatus::Pass => text(fl!("ui-tool-headers-status-clean"))
                        .size(font_size::CAPTION)
                        .style(text::success)
                        .width(Length::Fixed(70.0)),
                    HeaderCheckStatus::Warn | HeaderCheckStatus::Fail => {
                        text(fl!("ui-tool-headers-status-exposed"))
                            .size(font_size::CAPTION)
                            .style(text::danger)
                            .width(Length::Fixed(70.0))
                    }
                };

                let val_display = if check.value.is_empty() {
                    fl!("ui-tool-headers-not-exposed")
                } else {
                    check.value.clone()
                };

                let disc_row = row![
                    status_cell,
                    text(&check.name)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(220.0)),
                    text(val_display)
                        .size(font_size::CAPTION)
                        .font(super::BOX_FONT)
                        .style(text::secondary)
                        .width(Length::Fill),
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center);

                disc_col = disc_col.push(
                    container(disc_row)
                        .padding(spacing::XS)
                        .style(styles::card)
                        .width(Length::Fill),
                );
            }

            body = body.push(container(disc_col).padding(spacing::MD).style(styles::card));

            // Raw Response Headers Card
            let raw_card = column![
                text(fl!("ui-tool-headers-raw-title")).size(font_size::SUBTITLE),
                container(
                    text(&info.header_section)
                        .size(font_size::CAPTION)
                        .font(super::BOX_FONT)
                )
                .padding(spacing::SM)
                .style(styles::card)
                .width(Length::Fill),
            ]
            .spacing(spacing::SM);

            body = body.push(container(raw_card).padding(spacing::MD).style(styles::card));
        }

        super::content_column(body)
    }
}

fn recommendation_text(key: &'static str) -> String {
    match key {
        "ui-tool-headers-rec-hsts" => fl!("ui-tool-headers-rec-hsts"),
        "ui-tool-headers-rec-csp" => fl!("ui-tool-headers-rec-csp"),
        "ui-tool-headers-rec-xfo" => fl!("ui-tool-headers-rec-xfo"),
        "ui-tool-headers-rec-xcto" => fl!("ui-tool-headers-rec-xcto"),
        "ui-tool-headers-rec-referrer" => fl!("ui-tool-headers-rec-referrer"),
        "ui-tool-headers-rec-permissions" => fl!("ui-tool-headers-rec-permissions"),
        "ui-tool-headers-rec-xxss" => fl!("ui-tool-headers-rec-xxss"),
        "ui-tool-headers-rec-cookie" => fl!("ui-tool-headers-rec-cookie"),
        "ui-tool-headers-rec-disclosure" => fl!("ui-tool-headers-rec-disclosure"),
        _ => String::new(),
    }
}
