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

//! The HTTP Header Analyzer tool view, as the C# `HttpHeaderAnalyzerView` and
//! `HttpHeaderAnalyzerViewModel`: the URL read, its headers fetched by Heimdall's own client
//! (`heimdall_app::net_tools::http`), their security posture graded, information disclosure
//! detected, and the report copied.
//!
//! A fetch is one task, aborted by Stop, as the C# `Cancel`, or when the tab closes; its
//! answer carries its fetch's number, so an earlier one never replaces a later one. Enter
//! while a fetch goes on starts nothing, as the C# `CanCheck`. An HTTPS certificate the
//! system does not trust is shown as a finding, with why.

use heimdall_app::TabId;
use heimdall_app::net_tools::http::{self, CertificateCheck, HttpFetch, HttpFetchError};
use iced::task::Handle;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::http_headers::{
    self, HeaderCheckStatus, HeaderEvaluation, HttpGrade, HttpTarget, ReportText,
};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::texts::validation_issue;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the HTTP Header Analyzer view.
#[derive(Debug, Clone)]
pub enum HttpHeadersMessage {
    /// Target URL input changed.
    UrlChanged(String),
    /// Start header analysis.
    Analyze,
    /// Cancel the analysis under way.
    Stop,
    /// Fetch `.0` completed.
    Finished(u64, Result<HttpFetch, HttpFetchError>),
    /// Copy report text to clipboard.
    CopyReport,
}

/// The HTTP Header Analyzer tool pane state.
#[derive(Debug, Default)]
pub struct HttpHeadersPane {
    url: String,
    analyzing: bool,
    /// The number of the latest fetch; the answer of any other is ignored.
    run: u64,
    /// Aborts the fetch under way, also when the pane is dropped.
    stop: Option<Handle>,
    /// The target of the latest fetch.
    target: Option<HttpTarget>,
    error: Option<String>,
    fetched: Option<HttpFetch>,
    evaluation: Option<HeaderEvaluation>,
    report_text: String,
}

/// Outcome of updating the HTTP Header Analyzer tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

/// A text of the report, in the user's words.
fn report_text(said: ReportText<'_>) -> String {
    match said {
        ReportText::Title(target) => fl!("ui-tool-headers-report-title", target = target),
        ReportText::Grade(grade) => fl!("ui-tool-headers-report-grade", grade = grade),
        ReportText::SecurityHeaders => fl!("ui-tool-headers-sec-headers-title"),
        ReportText::Disclosure => fl!("ui-tool-headers-disclosure-title"),
        ReportText::RawHeaders => fl!("ui-tool-headers-raw-title"),
        ReportText::Status(HeaderCheckStatus::Pass) => fl!("ui-tool-headers-status-pass"),
        ReportText::Status(HeaderCheckStatus::Warn) => fl!("ui-tool-headers-status-warn"),
        ReportText::Status(HeaderCheckStatus::Fail) => fl!("ui-tool-headers-status-fail"),
        ReportText::DisclosureStatus(HeaderCheckStatus::Pass) => {
            fl!("ui-tool-headers-status-clean")
        }
        ReportText::DisclosureStatus(_) => fl!("ui-tool-headers-status-exposed"),
        ReportText::Missing => fl!("ui-tool-headers-missing"),
        ReportText::NotExposed => fl!("ui-tool-headers-not-exposed"),
        ReportText::MissingFlags(flags) => fl!("ui-tool-headers-cookie-missing", flags = flags),
    }
}

/// What the server's certificate came to, in the user's words; `None` over plain HTTP.
fn certificate_text(check: CertificateCheck) -> Option<String> {
    match check {
        CertificateCheck::NotTls => None,
        CertificateCheck::Trusted => Some(fl!("ui-tool-headers-certificate-trusted")),
        CertificateCheck::Untrusted(issue) => Some(fl!(
            "ui-tool-headers-certificate-untrusted",
            reason = validation_issue(issue)
        )),
    }
}

/// Why no response was read, in the user's words.
fn fetch_error(error: &HttpFetchError, host: &str) -> String {
    match error {
        HttpFetchError::TimedOut => fl!("ui-tool-headers-error-timeout"),
        HttpFetchError::Unresolved(_) => fl!("ui-tool-headers-error-unresolved", host = host),
        HttpFetchError::Connection(why) => {
            fl!("ui-tool-headers-error-connection", reason = why.as_str())
        }
        HttpFetchError::Tls(why) => fl!("ui-tool-headers-error-tls", reason = why.as_str()),
        HttpFetchError::Certificate(issue) => fl!(
            "ui-tool-headers-error-certificate",
            reason = validation_issue(*issue)
        ),
        HttpFetchError::Empty => fl!("ui-tool-headers-error-empty"),
    }
}

impl HttpHeadersPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: HttpHeadersMessage) -> Outcome {
        if !matches!(message, HttpHeadersMessage::Analyze) {
            return self.apply(message);
        }
        let Some(target) = self.start() else {
            return Outcome::None;
        };
        let run = self.run;
        let (task, handle) =
            Task::perform(async move { http::fetch(&target).await }, move |answer| {
                Message::Tool(
                    tab,
                    super::ToolMessage::HttpHeaders(HttpHeadersMessage::Finished(run, answer)),
                )
            })
            .abortable();
        self.stop = Some(handle.abort_on_drop());
        Outcome::Task(task)
    }

    /// Handles every message but [`HttpHeadersMessage::Analyze`], which needs the tab for its
    /// task.
    fn apply(&mut self, message: HttpHeadersMessage) -> Outcome {
        match message {
            HttpHeadersMessage::UrlChanged(val) => {
                self.url = val;
                self.error = None;
            }
            HttpHeadersMessage::Analyze => {}
            HttpHeadersMessage::Stop => self.finish(),
            HttpHeadersMessage::Finished(run, answer) => {
                if run == self.run && self.analyzing {
                    self.finish();
                    self.finished(answer);
                }
            }
            HttpHeadersMessage::CopyReport => {
                if !self.report_text.is_empty() {
                    return Outcome::Copy(CopySlot::HttpHeadersReport, self.report_text.clone());
                }
            }
        }
        Outcome::None
    }

    /// A new fetch's target, the pane reset for it; `None` while a fetch goes on, or when
    /// the URL is refused, said in the error card.
    fn start(&mut self) -> Option<HttpTarget> {
        if self.analyzing {
            return None;
        }
        match http_headers::parse_target(&self.url) {
            Ok(target) => {
                self.run = self.run.wrapping_add(1);
                self.analyzing = true;
                self.error = None;
                self.fetched = None;
                self.evaluation = None;
                self.report_text.clear();
                self.target = Some(target.clone());
                Some(target)
            }
            Err(key) => {
                self.error = Some(match key {
                    "ui-tool-headers-error-url-required" => {
                        fl!("ui-tool-headers-error-url-required")
                    }
                    _ => fl!("ui-tool-headers-error-invalid-url"),
                });
                None
            }
        }
    }

    /// The fetch over: stopped, its task aborted.
    fn finish(&mut self) {
        self.analyzing = false;
        if let Some(stop) = self.stop.take() {
            stop.abort();
        }
    }

    /// The answer of the latest fetch shown: its evaluation and report, or why it failed.
    fn finished(&mut self, answer: Result<HttpFetch, HttpFetchError>) {
        let (url, host) = self
            .target
            .as_ref()
            .map(|target| (target.url(), target.host.clone()))
            .unwrap_or_default();
        match answer {
            Ok(fetched) => {
                let evaluation = http_headers::evaluate_headers(&fetched.info.headers);
                let certificate = certificate_text(fetched.certificate);
                self.report_text = http_headers::build_text_report(
                    &url,
                    &evaluation,
                    &fetched.info.header_section,
                    certificate.as_deref(),
                    &report_text,
                );
                self.fetched = Some(fetched);
                self.evaluation = Some(evaluation);
            }
            Err(error) => self.error = Some(fetch_error(&error, &host)),
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

        let mut url_input = text_input(&fl!("ui-tool-headers-url-watermark"), &self.url)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .width(Length::Fill);
        if !self.analyzing {
            url_input = url_input
                .on_input(move |v| send(HttpHeadersMessage::UrlChanged(v)))
                .on_submit(send(HttpHeadersMessage::Analyze));
        }

        let analyze_button = if self.analyzing {
            super::action_button(
                fl!("ui-tool-headers-stop"),
                false,
                Some(send(HttpHeadersMessage::Stop)),
            )
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

        if self.analyzing {
            body = body.push(
                text(fl!("ui-tool-headers-analyzing"))
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

        if let (Some(fetched), Some(eval)) = (&self.fetched, &self.evaluation) {
            let info = &fetched.info;
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
                    text(fl!(
                        "ui-tool-headers-http-status",
                        code = info.status_code.to_string()
                    ))
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

            // The certificate, a finding of its own when the system refused it.
            if let Some(said) = certificate_text(fetched.certificate) {
                let style = if matches!(fetched.certificate, CertificateCheck::Untrusted(_)) {
                    text::danger
                } else {
                    text::success
                };
                body = body.push(
                    container(text(said).size(font_size::BODY).style(style))
                        .padding(spacing::MD)
                        .style(styles::card)
                        .width(Length::Fill),
                );
            }

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

                if let Some(flags) = &check.detail {
                    row_col = row_col.push(
                        text(fl!(
                            "ui-tool-headers-cookie-missing",
                            flags = flags.as_str()
                        ))
                        .size(font_size::CAPTION)
                        .style(text::warning),
                    );
                }

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

#[cfg(test)]
mod tests {
    use super::*;
    use heimdall_core::tools::http_headers::parse_http_response;
    use heimdall_tls::ValidationIssue;

    fn typed(url: &str) -> HttpHeadersPane {
        let mut pane = HttpHeadersPane::default();
        pane.apply(HttpHeadersMessage::UrlChanged(url.into()));
        pane
    }

    fn started(pane: &mut HttpHeadersPane) -> Handle {
        assert!(pane.start().is_some(), "{:?}", pane.error);
        let (_, handle) = Task::<()>::none().abortable();
        let watcher = handle.clone();
        pane.stop = Some(handle.abort_on_drop());
        watcher
    }

    fn fetched(certificate: CertificateCheck) -> HttpFetch {
        HttpFetch {
            info: parse_http_response("HTTP/1.1 200 OK\r\nServer: test\r\n\r\n"),
            certificate,
        }
    }

    #[test]
    fn stop_cancels_the_fetch_and_its_late_answer_is_dropped() {
        let mut pane = typed("example.com");
        let handle = started(&mut pane);
        assert_eq!(pane.start(), None, "one fetch at a time");
        pane.apply(HttpHeadersMessage::Stop);
        assert!(!pane.analyzing && handle.is_aborted());
        pane.apply(HttpHeadersMessage::Finished(
            pane.run,
            Ok(fetched(CertificateCheck::Trusted)),
        ));
        assert!(pane.fetched.is_none());
    }

    #[test]
    fn an_earlier_answer_never_replaces_a_later_one() {
        let mut pane = typed("example.com");
        started(&mut pane);
        let first = pane.run;
        pane.apply(HttpHeadersMessage::Stop);
        started(&mut pane);
        pane.apply(HttpHeadersMessage::Finished(
            first,
            Err(HttpFetchError::Empty),
        ));
        assert!(pane.error.is_none() && pane.analyzing);
    }

    #[test]
    fn a_certificate_refused_is_a_finding_of_the_report() {
        let mut pane = typed("https://self-signed.example/");
        started(&mut pane);
        pane.apply(HttpHeadersMessage::Finished(
            pane.run,
            Ok(fetched(CertificateCheck::Untrusted(
                ValidationIssue::SelfSigned,
            ))),
        ));
        let finding = fl!(
            "ui-tool-headers-certificate-untrusted",
            reason = validation_issue(ValidationIssue::SelfSigned)
        );
        assert!(pane.report_text.contains(&finding), "{}", pane.report_text);
        assert!(pane.report_text.contains("Server: test"));
    }

    #[test]
    fn a_url_that_could_bend_the_request_is_refused() {
        let mut pane = typed("https://example.com/a b");
        assert_eq!(pane.start(), None);
        assert_eq!(pane.error, Some(fl!("ui-tool-headers-error-invalid-url")));
    }
}
