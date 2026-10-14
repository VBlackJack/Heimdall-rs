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

//! The DNS Security Checker tool, as the C# `DnsSecurityView` and
//! `DnsSecurityCheckerViewModel`: validates domain security posture across six key
//! DNS controls: SPF, DKIM, DMARC, CAA, DNSSEC, and MX.
//!
//! Provides asynchronous query execution, live progress streaming, summary status
//! badge, per-control result cards, and formatted report copying.

use heimdall_app::TabId;
use heimdall_core::tools::dns_security::{
    self, DEFAULT_DKIM_SELECTORS, DnsCheckFinding, DnsCheckResult, DnsCheckStatus,
    DnsSecurityReport, DnsSummaryStatus,
};
use iced::futures::SinkExt;
use iced::widget::{column, container, progress_bar, row, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Task, Theme};

use super::{CopySlot, ToolPane, key_parts};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, radius, spacing};

/// Height of the progress bar in pixels.
const PROGRESS_HEIGHT: f32 = 6.0;

/// The messages of the DNS Security Checker tool.
#[derive(Debug, Clone)]
pub enum DnsSecMessage {
    /// The domain input changed.
    DomainChanged(String),
    /// Start auditing DNS security records.
    Check,
    /// Cancel the ongoing audit.
    Cancel,
    /// Progress update during query execution.
    Progress(u64, f32, String),
    /// DNS security check completed.
    Finished(u64, Result<DnsSecurityReport, String>),
    /// Copy the complete report to the clipboard.
    CopyReport,
}

/// What the DNS Security Checker asks of the surrounding tab.
pub enum Outcome {
    /// Nothing to do.
    None,
    /// Copy text into the given slot.
    Copy(CopySlot, String),
    /// Run an asynchronous command or stream.
    Task(Task<Message>),
}

/// State of the DNS Security Checker pane.
#[derive(Debug)]
pub struct DnsSecPane {
    domain: String,
    is_checking: bool,
    check_id: u64,
    progress: Option<(f32, String)>,
    error: Option<String>,
    report: Option<DnsSecurityReport>,
    report_text: String,
}

impl Default for DnsSecPane {
    fn default() -> Self {
        Self::new()
    }
}

impl DnsSecPane {
    /// Creates a new, blank DNS Security Checker tool state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            domain: String::new(),
            is_checking: false,
            check_id: 0,
            progress: None,
            error: None,
            report: None,
            report_text: String::new(),
        }
    }

    /// Handles a tool message and returns the required outcome.
    pub fn update(&mut self, msg: DnsSecMessage, tab: TabId) -> Outcome {
        match msg {
            DnsSecMessage::DomainChanged(domain) => {
                self.domain = domain;
                Outcome::None
            }
            DnsSecMessage::Check => {
                let normalized = dns_security::normalize_domain_input(&self.domain);
                if normalized.is_empty() {
                    self.error = Some(fl!("ui-tool-dnssec-error-domain-required"));
                    self.report = None;
                    self.report_text.clear();
                    return Outcome::None;
                }

                if !normalized.contains('.') || normalized.len() < 3 {
                    self.error = Some(fl!("ui-tool-dnssec-error-invalid-domain"));
                    self.report = None;
                    self.report_text.clear();
                    return Outcome::None;
                }

                self.is_checking = true;
                self.check_id += 1;
                let id = self.check_id;
                self.error = None;
                self.progress = Some((
                    0.0,
                    fl!(
                        "ui-tool-dnssec-progress",
                        record = "SPF",
                        domain = normalized.as_str()
                    ),
                ));

                let stream = check_stream(normalized, id);
                Outcome::Task(Task::run(stream, move |m| {
                    Message::Tool(tab, super::ToolMessage::DnsSec(m))
                }))
            }
            DnsSecMessage::Cancel => {
                if self.is_checking {
                    self.is_checking = false;
                    self.check_id += 1;
                    self.progress = None;
                }
                Outcome::None
            }
            DnsSecMessage::Progress(id, fraction, desc) => {
                if id == self.check_id && self.is_checking {
                    self.progress = Some((fraction, desc));
                }
                Outcome::None
            }
            DnsSecMessage::Finished(id, outcome) => {
                if id == self.check_id && self.is_checking {
                    self.is_checking = false;
                    self.progress = None;
                    match outcome {
                        Ok(rep) => {
                            self.report_text = format_report_text(&rep);
                            self.report = Some(rep);
                            self.error = None;
                        }
                        Err(err) => {
                            self.error = Some(err);
                            self.report = None;
                            self.report_text.clear();
                        }
                    }
                }
                Outcome::None
            }
            DnsSecMessage::CopyReport => {
                if !self.report_text.is_empty() {
                    return Outcome::Copy(CopySlot::DnsSecReport, self.report_text.clone());
                }
                Outcome::None
            }
        }
    }

    /// Renders the DNS Security Checker tool page.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane, theme: &Theme) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::DnsSec(msg));

        let domain_input = text_input(&fl!("ui-tool-dnssec-domain-watermark"), &self.domain)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |d| send(DnsSecMessage::DomainChanged(d)))
            .on_submit(send(DnsSecMessage::Check));

        let check_button = if self.is_checking {
            super::action_button(
                fl!("ui-tool-dnssec-cancel"),
                false,
                Some(send(DnsSecMessage::Cancel)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-dnssec-check"),
                true,
                Some(send(DnsSecMessage::Check)),
            )
        };

        let controls_row = row![
            column![super::field_label(fl!("ui-tool-dnssec-domain")), domain_input]
                .spacing(spacing::XS)
                .width(Length::Fill),
            container(check_button).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::End);

        let input_card = key_parts::card(column![controls_row]);
        let mut body_col = column![input_card].spacing(spacing::MD);

        if let Some(err) = &self.error {
            body_col = body_col.push(
                container(
                    text(err.as_str())
                        .size(font_size::BODY)
                        .style(text::danger),
                )
                .padding(spacing::MD)
                .style(styles::card),
            );
        }

        if self.is_checking {
            if let Some((fraction, desc)) = &self.progress {
                let progress_col = column![
                    row![
                        text(desc)
                            .size(font_size::BODY)
                            .style(text::secondary)
                            .width(Length::Fill),
                        text(format!("{:.0}%", fraction * 100.0))
                            .size(font_size::BODY)
                            .style(text::secondary),
                    ]
                    .align_y(Alignment::Center),
                    progress_bar(0.0..=1.0, *fraction)
                        .length(Length::Fill)
                        .girth(PROGRESS_HEIGHT),
                ]
                .spacing(spacing::SM);

                body_col = body_col.push(
                    container(progress_col)
                        .padding(spacing::MD)
                        .style(styles::card),
                );
            }
        } else if self.report.is_none() && self.error.is_none() {
            body_col = body_col.push(super::empty_state(fl!("ui-tool-dnssec-empty-state")));
        }

        if let Some(rep) = &self.report {
            let summary_card = self.summary_card(rep, theme);
            let results_card = self.results_card(rep, theme);

            let copy_btn = super::copy_button(
                fl!("ui-tool-dnssec-copy-report"),
                state.copied(CopySlot::DnsSecReport),
                send(DnsSecMessage::CopyReport),
                super::COPY_PADDING,
            );

            body_col = body_col.push(summary_card);
            body_col = body_col.push(results_card);
            body_col = body_col.push(container(copy_btn).align_x(Alignment::Start));
        }

        super::content_column(body_col)
    }

    fn summary_card<'a>(
        &'a self,
        rep: &'a DnsSecurityReport,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let (summary_text, is_success, is_warn) = match rep.summary {
            DnsSummaryStatus::AllPass => (
                fl!(
                    "ui-tool-dnssec-summary-all-pass",
                    passed = rep.pass_count,
                    total = rep.total
                ),
                true,
                false,
            ),
            DnsSummaryStatus::Good => (
                fl!(
                    "ui-tool-dnssec-summary-good",
                    passed = rep.pass_count,
                    total = rep.total
                ),
                true,
                false,
            ),
            DnsSummaryStatus::Partial => (
                fl!(
                    "ui-tool-dnssec-summary-partial",
                    passed = rep.pass_count,
                    total = rep.total
                ),
                false,
                true,
            ),
            DnsSummaryStatus::Bad => (
                fl!(
                    "ui-tool-dnssec-summary-bad",
                    passed = rep.pass_count,
                    total = rep.total
                ),
                false,
                false,
            ),
        };

        let badge = container(
            text(summary_text)
                .font(styles::SEMIBOLD)
                .size(font_size::BODY_LARGE)
                .style(move |t: &Theme| {
                    let palette = t.extended_palette();
                    text::Style {
                        color: Some(if is_success {
                            palette.success.strong.text
                        } else if is_warn {
                            palette.warning.strong.text
                        } else {
                            palette.danger.strong.text
                        }),
                    }
                }),
        )
        .padding([spacing::SM, spacing::MD])
        .style(move |t: &Theme| {
            let palette = t.extended_palette();
            let (bg, border_color) = if is_success {
                (palette.success.weak.color, palette.success.base.color)
            } else if is_warn {
                (palette.warning.weak.color, palette.warning.base.color)
            } else {
                (palette.danger.weak.color, palette.danger.base.color)
            };
            container::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    radius: radius::SM.into(),
                    width: 1.0,
                    color: border_color,
                },
                ..container::Style::default()
            }
        });

        key_parts::card(column![badge])
    }

    fn results_card<'a>(
        &'a self,
        rep: &'a DnsSecurityReport,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let mut card_col = column![].spacing(spacing::MD);

        for res in &rep.results {
            let (status_label, is_success, is_warn) = match res.status {
                DnsCheckStatus::Pass => (fl!("ui-tool-dnssec-pass"), true, false),
                DnsCheckStatus::Warn => (fl!("ui-tool-dnssec-warn"), false, true),
                DnsCheckStatus::Fail => (fl!("ui-tool-dnssec-fail"), false, false),
            };

            let badge = container(
                text(status_label)
                    .font(styles::SEMIBOLD)
                    .size(font_size::CAPTION)
                    .style(move |t: &Theme| {
                        let palette = t.extended_palette();
                        text::Style {
                            color: Some(if is_success {
                                palette.success.strong.text
                            } else if is_warn {
                                palette.warning.strong.text
                            } else {
                                palette.danger.strong.text
                            }),
                        }
                    }),
            )
            .padding([spacing::XS, spacing::SM])
            .style(move |t: &Theme| {
                let palette = t.extended_palette();
                let (bg, border_color) = if is_success {
                    (palette.success.weak.color, palette.success.base.color)
                } else if is_warn {
                    (palette.warning.weak.color, palette.warning.base.color)
                } else {
                    (palette.danger.weak.color, palette.danger.base.color)
                };
                container::Style {
                    background: Some(Background::Color(bg)),
                    border: Border {
                        radius: radius::SM.into(),
                        width: 1.0,
                        color: border_color,
                    },
                    ..container::Style::default()
                }
            });

            let name_text = text(res.kind.name())
                .font(styles::SEMIBOLD)
                .size(font_size::BODY)
                .style(text::primary);

            let header_row = row![name_text, badge]
                .spacing(spacing::SM)
                .align_y(Alignment::Center);

            let record_val = if res.raw_record.is_empty() {
                fl!("ui-tool-dnssec-no-record")
            } else {
                res.raw_record.clone()
            };

            let record_text = text(record_val)
                .font(super::BOX_FONT)
                .size(font_size::CAPTION)
                .style(text::secondary);

            let detail_str = finding_message(&res.finding);
            let detail_text = text(detail_str)
                .size(font_size::CAPTION)
                .style(text::secondary);

            let item_box = column![header_row, record_text, detail_text]
                .spacing(spacing::XS)
                .padding([spacing::XS, 0.0]);

            card_col = card_col.push(item_box);
        }

        key_parts::card(card_col)
    }
}

fn finding_message(finding: &DnsCheckFinding) -> String {
    match finding {
        DnsCheckFinding::Missing => fl!("ui-tool-dnssec-finding-missing"),
        DnsCheckFinding::SpfPermissive => fl!("ui-tool-dnssec-finding-spf-permissive"),
        DnsCheckFinding::SpfNeutral => fl!("ui-tool-dnssec-finding-spf-neutral"),
        DnsCheckFinding::SpfNoAll => fl!("ui-tool-dnssec-finding-spf-no-all"),
        DnsCheckFinding::SpfGood => fl!("ui-tool-dnssec-finding-spf-good"),
        DnsCheckFinding::DmarcNone => fl!("ui-tool-dnssec-finding-dmarc-none"),
        DnsCheckFinding::DmarcEnforced(p) => {
            fl!("ui-tool-dnssec-finding-dmarc-enforced", policy = p.as_str())
        }
        DnsCheckFinding::DmarcPctZero(p) => {
            fl!("ui-tool-dnssec-finding-dmarc-pct-zero", policy = p.as_str())
        }
        DnsCheckFinding::DkimFound(s) => {
            fl!("ui-tool-dnssec-finding-dkim-found", selector = s.as_str())
        }
        DnsCheckFinding::DkimRevoked(s) => {
            fl!("ui-tool-dnssec-finding-dkim-revoked", selector = s.as_str())
        }
        DnsCheckFinding::NoDkim => fl!("ui-tool-dnssec-finding-dkim-none"),
        DnsCheckFinding::NoCaa => fl!("ui-tool-dnssec-finding-caa-missing"),
        DnsCheckFinding::CaaPresent(issuers) => {
            fl!("ui-tool-dnssec-finding-caa-present", issuers = issuers.as_str())
        }
        DnsCheckFinding::DnssecPresent => fl!("ui-tool-dnssec-finding-dnssec-present"),
        DnsCheckFinding::DnssecMissing => fl!("ui-tool-dnssec-finding-dnssec-missing"),
        DnsCheckFinding::MxServers(servers) => {
            fl!("ui-tool-dnssec-finding-mx-servers", servers = servers.as_str())
        }
        DnsCheckFinding::NoMx => fl!("ui-tool-dnssec-finding-no-mx"),
        DnsCheckFinding::Error(err) => fl!("ui-tool-dnssec-finding-error", error = err.as_str()),
    }
}

fn format_report_text(report: &DnsSecurityReport) -> String {
    let mut sb = String::new();
    sb.push_str(&fl!(
        "ui-tool-dnssec-report-title",
        domain = report.domain.as_str()
    ));
    sb.push('\n');
    sb.push_str(&fl!(
        "ui-tool-dnssec-report-summary",
        passed = report.pass_count,
        total = report.total
    ));
    sb.push_str("\n\n");

    for res in &report.results {
        let status_str = match res.status {
            DnsCheckStatus::Pass => fl!("ui-tool-dnssec-pass"),
            DnsCheckStatus::Warn => fl!("ui-tool-dnssec-warn"),
            DnsCheckStatus::Fail => fl!("ui-tool-dnssec-fail"),
        };
        let detail = finding_message(&res.finding);
        let val = if res.raw_record.is_empty() {
            fl!("ui-tool-dnssec-no-record")
        } else {
            res.raw_record.clone()
        };
        sb.push_str(&format!("[{status_str}] {}\n", res.kind.name()));
        sb.push_str(&format!("  Record: {val}\n"));
        sb.push_str(&format!("  Detail: {detail}\n\n"));
    }

    sb
}

/// Executes nslookup silently and returns the raw output string.
fn query_dns(record_type: &str, query_target: &str) -> String {
    let mut cmd = std::process::Command::new("nslookup");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.arg(format!("-type={record_type}"));
    cmd.arg(query_target);

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stdout.trim().is_empty() {
                stderr.to_string()
            } else {
                stdout.to_string()
            }
        }
        Err(e) => format!("Error executing nslookup: {e}"),
    }
}

/// Asynchronous streaming DNS security checks.
fn check_stream(
    domain: String,
    check_id: u64,
) -> impl iced::futures::Stream<Item = DnsSecMessage> {
    iced::stream::channel(16, async move |mut out| {
        let total_steps = 6.0f32;
        let mut results = Vec::with_capacity(6);

        // 1. SPF
        let _ = out
            .send(DnsSecMessage::Progress(
                check_id,
                1.0 / total_steps,
                fl!("ui-tool-dnssec-progress", record = "SPF", domain = domain.as_str()),
            ))
            .await;

        let dom = domain.clone();
        let spf_raw = tokio::task::spawn_blocking(move || query_dns("TXT", &dom))
            .await
            .unwrap_or_default();
        results.push(dns_security::evaluate_spf(&spf_raw));

        // 2. DKIM
        let _ = out
            .send(DnsSecMessage::Progress(
                check_id,
                2.0 / total_steps,
                fl!("ui-tool-dnssec-progress", record = "DKIM", domain = domain.as_str()),
            ))
            .await;

        let mut dkim_result: Option<DnsCheckResult> = None;
        let mut first_warn: Option<DnsCheckResult> = None;

        for selector in DEFAULT_DKIM_SELECTORS {
            let selector_domain = format!("{selector}._domainkey.{domain}");
            let raw = tokio::task::spawn_blocking(move || query_dns("TXT", &selector_domain))
                .await
                .unwrap_or_default();

            let eval = dns_security::evaluate_dkim(Some(selector), Some(&raw));
            if eval.status == DnsCheckStatus::Pass {
                dkim_result = Some(eval);
                break;
            }
            if eval.status == DnsCheckStatus::Warn && first_warn.is_none() {
                first_warn = Some(eval);
            }
        }

        results.push(dkim_result.or(first_warn).unwrap_or_else(|| {
            dns_security::evaluate_dkim(None, None)
        }));

        // 3. DMARC
        let _ = out
            .send(DnsSecMessage::Progress(
                check_id,
                3.0 / total_steps,
                fl!("ui-tool-dnssec-progress", record = "DMARC", domain = domain.as_str()),
            ))
            .await;

        let dmarc_domain = format!("_dmarc.{domain}");
        let dmarc_raw = tokio::task::spawn_blocking(move || query_dns("TXT", &dmarc_domain))
            .await
            .unwrap_or_default();
        results.push(dns_security::evaluate_dmarc(&dmarc_raw));

        // 4. CAA
        let _ = out
            .send(DnsSecMessage::Progress(
                check_id,
                4.0 / total_steps,
                fl!("ui-tool-dnssec-progress", record = "CAA", domain = domain.as_str()),
            ))
            .await;

        let dom = domain.clone();
        let caa_raw = tokio::task::spawn_blocking(move || query_dns("CAA", &dom))
            .await
            .unwrap_or_default();
        results.push(dns_security::evaluate_caa(&caa_raw));

        // 5. DNSSEC
        let _ = out
            .send(DnsSecMessage::Progress(
                check_id,
                5.0 / total_steps,
                fl!("ui-tool-dnssec-progress", record = "DNSSEC", domain = domain.as_str()),
            ))
            .await;

        let dom = domain.clone();
        let dnskey_raw = tokio::task::spawn_blocking(move || query_dns("DNSKEY", &dom))
            .await
            .unwrap_or_default();

        let rrsig_raw = if !dns_security::contains_dns_records(&dnskey_raw, "DNSKEY") {
            let dom2 = domain.clone();
            tokio::task::spawn_blocking(move || query_dns("RRSIG", &dom2))
                .await
                .unwrap_or_default()
        } else {
            String::new()
        };

        results.push(dns_security::evaluate_dnssec(&dnskey_raw, &rrsig_raw));

        // 6. MX
        let _ = out
            .send(DnsSecMessage::Progress(
                check_id,
                6.0 / total_steps,
                fl!("ui-tool-dnssec-progress", record = "MX", domain = domain.as_str()),
            ))
            .await;

        let dom = domain.clone();
        let mx_raw = tokio::task::spawn_blocking(move || query_dns("MX", &dom))
            .await
            .unwrap_or_default();
        results.push(dns_security::evaluate_mx(&mx_raw));

        let report = dns_security::build_report(&domain, results);
        let _ = out.send(DnsSecMessage::Finished(check_id, Ok(report))).await;
    })
}
