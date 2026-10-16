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

//! The SSL/TLS Auditor tool, as the C# `TlsAuditView` and `TlsAuditViewModel`:
//! probes a target host and port for TLS protocol support (SSL 3.0 through TLS 1.3),
//! enumerates TLS 1.2 cipher suites, inspects the server certificate, calculates
//! a security grade (A+ through F), and generates granular findings and recommendations.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::module_name_repetitions,
    clippy::redundant_closure_for_method_calls,
    clippy::too_many_lines
)]

use std::time::Duration;

use heimdall_app::TabId;
use heimdall_core::tools::tls_audit::{
    self, CipherStrength, KNOWN_TLS12_CIPHER_SUITES, PROTOCOL_DEFINITIONS, TlsAuditResult,
    TlsCertificateSummary, TlsCipherInfo, TlsFinding, TlsFindingKind, TlsFindingSeverity, TlsGrade,
    TlsProtocol, TlsProtocolRating, TlsProtocolTestResult,
};
use iced::futures::SinkExt as _;
use iced::widget::{column, container, progress_bar, row, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Task, Theme};

use super::{CopySlot, ToolMessage, ToolPane, key_parts};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, radius, spacing};

/// Probe connection timeout.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Height of the progress bar in the auditing banner.
const PROGRESS_HEIGHT: f32 = 4.0;

/// Messages dispatched by the TLS Auditor.
#[derive(Debug, Clone)]
pub enum TlsAuditMessage {
    /// Target host name or IP address edited.
    HostChanged(String),
    /// Target port edited.
    PortChanged(String),
    /// Start auditing the target.
    Audit,
    /// Cancel the ongoing audit.
    Cancel,
    /// Progress update during audit: (audit_id, percent 0.0..=1.0, step description).
    Progress(u64, f32, String),
    /// The audit finished with success or failure: (audit_id, result).
    Finished(u64, Result<TlsAuditResult, String>),
    /// Copy the full text report to clipboard.
    CopyReport,
}

/// The TLS Auditor view state.
pub struct TlsAuditPane {
    host: String,
    port: String,
    is_auditing: bool,
    progress: Option<(f32, String)>,
    error: Option<String>,
    result: Option<TlsAuditResult>,
    report_text: String,
    audit_id: u64,
}

impl Default for TlsAuditPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: "443".to_owned(),
            is_auditing: false,
            progress: None,
            error: None,
            result: None,
            report_text: String::new(),
            audit_id: 0,
        }
    }
}

impl std::fmt::Debug for TlsAuditPane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsAuditPane")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("is_auditing", &self.is_auditing)
            .field("has_result", &self.result.is_some())
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

/// The outcome of updating the TLS Auditor.
#[derive(Debug)]
pub enum Outcome {
    None,
    Copy(CopySlot, String),
    Task(Task<Message>),
}

impl TlsAuditPane {
    /// Applies `message` and returns the outcome.
    pub fn update(&mut self, message: TlsAuditMessage, tab: TabId) -> Outcome {
        match message {
            TlsAuditMessage::HostChanged(host) => {
                self.host = host;
                Outcome::None
            }
            TlsAuditMessage::PortChanged(port) => {
                self.port = port;
                Outcome::None
            }
            TlsAuditMessage::Cancel => {
                self.audit_id = self.audit_id.wrapping_add(1);
                self.is_auditing = false;
                self.progress = None;
                Outcome::None
            }
            TlsAuditMessage::Audit => {
                if self.is_auditing {
                    return Outcome::None;
                }

                let trimmed_host = self.host.trim();
                if trimmed_host.is_empty() {
                    self.error = Some(fl!("ui-tool-tlsaudit-error-host-required"));
                    self.result = None;
                    self.report_text.clear();
                    return Outcome::None;
                }

                let parsed_port = match self.port.trim().parse::<u16>() {
                    Ok(p) if p > 0 => p,
                    _ => {
                        self.error = Some(fl!("ui-tool-tlsaudit-error-invalid-port"));
                        self.result = None;
                        self.report_text.clear();
                        return Outcome::None;
                    }
                };

                self.error = None;
                self.result = None;
                self.report_text.clear();
                self.is_auditing = true;
                self.audit_id = self.audit_id.wrapping_add(1);
                let current_id = self.audit_id;

                self.progress = Some((
                    0.0,
                    fl!(
                        "ui-tool-tlsaudit-progress",
                        step = PROTOCOL_DEFINITIONS[0].name()
                    ),
                ));

                let host_str = trimmed_host.to_owned();
                let task = Task::run(
                    audit_stream(host_str, parsed_port, current_id),
                    move |msg| Message::Tool(tab, ToolMessage::TlsAudit(msg)),
                );
                Outcome::Task(task)
            }
            TlsAuditMessage::Progress(id, fraction, description) => {
                if id == self.audit_id && self.is_auditing {
                    self.progress = Some((fraction, description));
                }
                Outcome::None
            }
            TlsAuditMessage::Finished(id, outcome) => {
                if id == self.audit_id && self.is_auditing {
                    self.is_auditing = false;
                    self.progress = None;
                    match outcome {
                        Ok(res) => {
                            let parsed_port = self.port.trim().parse::<u16>().unwrap_or(443);
                            self.report_text =
                                format_report_text(self.host.trim(), parsed_port, &res);
                            self.result = Some(res);
                            self.error = None;
                        }
                        Err(err) => {
                            self.error = Some(err);
                            self.result = None;
                            self.report_text.clear();
                        }
                    }
                }
                Outcome::None
            }
            TlsAuditMessage::CopyReport => {
                if !self.report_text.is_empty() {
                    return Outcome::Copy(CopySlot::TlsAuditReport, self.report_text.clone());
                }
                Outcome::None
            }
        }
    }

    /// Renders the TLS Auditor tool page.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane, theme: &Theme) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, ToolMessage::TlsAudit(msg));

        let host_input = text_input(&fl!("ui-tool-tlsaudit-host"), &self.host)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |h| send(TlsAuditMessage::HostChanged(h)))
            .on_submit(send(TlsAuditMessage::Audit));

        let port_input = text_input(&fl!("ui-tool-tlsaudit-port"), &self.port)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .width(Length::Fixed(90.0))
            .style(styles::text_input)
            .on_input(move |p| send(TlsAuditMessage::PortChanged(p)))
            .on_submit(send(TlsAuditMessage::Audit));

        let audit_button = if self.is_auditing {
            super::action_button(
                fl!("ui-tool-tlsaudit-cancel"),
                false,
                Some(send(TlsAuditMessage::Cancel)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-tlsaudit-audit"),
                true,
                Some(send(TlsAuditMessage::Audit)),
            )
        };

        let controls_row = row![
            column![super::field_label(fl!("ui-tool-tlsaudit-host")), host_input]
                .spacing(spacing::XS)
                .width(Length::Fill),
            column![super::field_label(fl!("ui-tool-tlsaudit-port")), port_input]
                .spacing(spacing::XS)
                .width(Length::Fixed(90.0)),
            container(audit_button).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::End);

        let input_card = key_parts::card(column![controls_row]);

        let mut body_col = column![input_card].spacing(spacing::MD);

        if let Some(err) = &self.error {
            body_col = body_col.push(
                container(text(err.as_str()).size(font_size::BODY).style(text::danger))
                    .padding(spacing::MD)
                    .style(styles::card),
            );
        }

        if self.is_auditing {
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
                        .girth(PROGRESS_HEIGHT)
                        .style(styles::progress),
                ]
                .spacing(spacing::SM);

                body_col = body_col.push(
                    container(progress_col)
                        .padding(spacing::MD)
                        .style(styles::card),
                );
            }
        } else if self.result.is_none() && self.error.is_none() {
            body_col = body_col.push(super::empty_state(fl!("ui-tool-tlsaudit-empty-state")));
        }

        if let Some(res) = &self.result {
            let grade_row = self.grade_badge_row(res.grade);
            let protocols_card = self.protocols_card(&res.protocols, theme);
            let ciphers_card = self.ciphers_card(&res.ciphers, theme);
            let cert_card = res
                .certificate
                .as_ref()
                .map(|cert| self.certificate_card(cert));
            let findings_card = self.findings_card(&res.findings, theme);

            let copy_btn = super::copy_button(
                fl!("ui-tool-tlsaudit-copy-report"),
                state.copied(CopySlot::TlsAuditReport),
                send(TlsAuditMessage::CopyReport),
                super::COPY_PADDING,
            );

            body_col = body_col.push(grade_row);
            body_col = body_col.push(protocols_card);
            if !res.ciphers.is_empty() {
                body_col = body_col.push(ciphers_card);
            }
            if let Some(card) = cert_card {
                body_col = body_col.push(card);
            }
            body_col = body_col.push(findings_card);
            body_col = body_col.push(container(copy_btn).align_x(Alignment::Start));
        }

        super::content_column(body_col)
    }

    fn grade_badge_row<'a>(&'a self, grade: TlsGrade) -> Element<'a, Message> {
        let grade_str = tls_audit::grade_to_str(grade);

        let badge = container(
            text(format!("{}: {}", fl!("ui-tool-tlsaudit-grade"), grade_str))
                .size(font_size::SUBTITLE)
                .font(styles::SEMIBOLD)
                .style(move |t: &Theme| {
                    let palette = t.extended_palette();
                    match grade {
                        TlsGrade::APlus | TlsGrade::A => text::Style {
                            color: Some(palette.success.base.color),
                        },
                        TlsGrade::B | TlsGrade::C => text::Style {
                            color: Some(palette.warning.base.color),
                        },
                        TlsGrade::D | TlsGrade::F => text::Style {
                            color: Some(palette.danger.base.color),
                        },
                        TlsGrade::T => text::Style {
                            color: Some(palette.secondary.base.color),
                        },
                    }
                }),
        )
        .padding([8.0, 16.0])
        .style(move |t: &Theme| {
            let palette = t.extended_palette();
            let (bg, border_color) = match grade {
                TlsGrade::APlus | TlsGrade::A => {
                    (palette.success.weak.color, palette.success.base.color)
                }
                TlsGrade::B | TlsGrade::C => {
                    (palette.warning.weak.color, palette.warning.base.color)
                }
                TlsGrade::D | TlsGrade::F => (palette.danger.weak.color, palette.danger.base.color),
                TlsGrade::T => (palette.secondary.weak.color, palette.secondary.base.color),
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

        row![badge].into()
    }

    fn protocols_card<'a>(
        &'a self,
        protocols: &'a [TlsProtocolTestResult],
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let mut card_col = column![key_parts::section_title(fl!("ui-tool-tlsaudit-protocols"))]
            .spacing(spacing::SM);

        for p in protocols {
            let status_text = if p.supported {
                fl!("ui-tool-tlsaudit-supported")
            } else {
                fl!("ui-tool-tlsaudit-not-supported")
            };

            let is_good = match p.rating {
                TlsProtocolRating::Strong => p.supported,
                TlsProtocolRating::Weak | TlsProtocolRating::Critical => !p.supported,
            };

            let badge = container(
                text(status_text)
                    .size(font_size::CAPTION)
                    .font(styles::SEMIBOLD)
                    .style(move |t: &Theme| {
                        let palette = t.extended_palette();
                        text::Style {
                            color: Some(if is_good {
                                palette.success.base.color
                            } else {
                                palette.danger.base.color
                            }),
                        }
                    }),
            )
            .padding([2.0, 8.0])
            .style(move |t: &Theme| {
                let palette = t.extended_palette();
                let (bg, border_color) = if is_good {
                    (palette.success.weak.color, palette.success.base.color)
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

            let item = row![
                text(&p.name)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .width(Length::Fill),
                badge,
            ]
            .align_y(Alignment::Center);

            card_col = card_col.push(item);
        }

        key_parts::card(card_col)
    }

    fn ciphers_card<'a>(
        &'a self,
        ciphers: &'a [TlsCipherInfo],
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let mut card_col =
            column![key_parts::section_title(fl!("ui-tool-tlsaudit-ciphers"))].spacing(spacing::SM);

        for c in ciphers {
            let strength_text = match c.strength {
                CipherStrength::Strong => fl!("ui-tool-tlsaudit-strong"),
                CipherStrength::Acceptable => fl!("ui-tool-tlsaudit-acceptable"),
                CipherStrength::Weak => fl!("ui-tool-tlsaudit-weak"),
            };

            let strength = c.strength;
            let badge = container(
                text(strength_text)
                    .size(font_size::CAPTION)
                    .font(styles::SEMIBOLD)
                    .style(move |t: &Theme| {
                        let palette = t.extended_palette();
                        match strength {
                            CipherStrength::Strong => text::Style {
                                color: Some(palette.success.base.color),
                            },
                            CipherStrength::Acceptable => text::Style {
                                color: Some(palette.warning.base.color),
                            },
                            CipherStrength::Weak => text::Style {
                                color: Some(palette.danger.base.color),
                            },
                        }
                    }),
            )
            .padding([2.0, 8.0])
            .style(move |t: &Theme| {
                let palette = t.extended_palette();
                let (bg, border_color) = match strength {
                    CipherStrength::Strong => {
                        (palette.success.weak.color, palette.success.base.color)
                    }
                    CipherStrength::Acceptable => {
                        (palette.warning.weak.color, palette.warning.base.color)
                    }
                    CipherStrength::Weak => (palette.danger.weak.color, palette.danger.base.color),
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

            let details = format!("{}/{}/{}", c.key_exchange, c.authentication, c.encryption);

            let item = row![
                text(&c.suite_name)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .width(Length::Fill),
                text(details)
                    .size(font_size::CAPTION)
                    .style(text::secondary),
                badge,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            card_col = card_col.push(item);
        }

        key_parts::card(card_col)
    }

    fn certificate_card<'a>(&'a self, cert: &'a TlsCertificateSummary) -> Element<'a, Message> {
        let mut card_col = column![key_parts::section_title(fl!(
            "ui-tool-tlsaudit-cert-section"
        ))]
        .spacing(spacing::SM);

        let bits_str = if cert.key_size_bits > 0 {
            fl!("ui-tool-tlsaudit-bits", bits = cert.key_size_bits)
        } else {
            String::new()
        };

        let rows = [
            (fl!("ui-tool-tlsaudit-cert-subject"), cert.subject.clone()),
            (fl!("ui-tool-tlsaudit-cert-issuer"), cert.issuer.clone()),
            (fl!("ui-tool-tlsaudit-cert-expires"), cert.not_after.clone()),
            (fl!("ui-tool-tlsaudit-cert-key-size"), bits_str),
        ];

        for (label, value) in rows {
            if !value.is_empty() {
                let item = row![
                    text(label)
                        .size(font_size::BODY)
                        .style(text::secondary)
                        .width(super::RESULT_LABEL_WIDTH),
                    iced::widget::space().width(super::RESULT_LABEL_GAP),
                    text(value)
                        .font(super::BOX_FONT)
                        .size(font_size::BODY)
                        .width(Length::Fill),
                ]
                .align_y(Alignment::Center);

                card_col = card_col.push(item);
            }
        }

        key_parts::card(card_col)
    }

    fn findings_card<'a>(
        &'a self,
        findings: &'a [TlsFinding],
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let mut card_col = column![key_parts::section_title(fl!("ui-tool-tlsaudit-findings"))]
            .spacing(spacing::SM);

        for f in findings {
            let (icon, is_error, is_warn) = match f.severity {
                TlsFindingSeverity::Pass => ("\u{2714}", false, false),
                TlsFindingSeverity::Info => ("\u{2139}", false, false),
                TlsFindingSeverity::Warning => ("\u{26A0}", false, true),
                TlsFindingSeverity::Critical => ("\u{26A0}", true, false),
            };

            let msg = finding_message(&f.kind);

            let icon_text = text(icon)
                .font(styles::SEMIBOLD)
                .size(font_size::BODY)
                .style(move |t: &Theme| {
                    let palette = t.extended_palette();
                    text::Style {
                        color: Some(if is_error {
                            palette.danger.base.color
                        } else if is_warn {
                            palette.warning.base.color
                        } else {
                            palette.success.base.color
                        }),
                    }
                });

            let item = row![
                container(icon_text).width(Length::Fixed(24.0)),
                text(msg)
                    .size(font_size::BODY)
                    .style(text::primary)
                    .width(Length::Fill),
            ]
            .align_y(Alignment::Center);

            card_col = card_col.push(item);
        }

        key_parts::card(card_col)
    }
}

/// Translates a [`TlsFindingKind`] to its localized user-facing message.
fn finding_message(kind: &TlsFindingKind) -> String {
    match kind {
        TlsFindingKind::Ssl3Supported => fl!("ui-tool-tlsaudit-finding-ssl3"),
        TlsFindingKind::Tls10Supported => fl!("ui-tool-tlsaudit-finding-tls10"),
        TlsFindingKind::Tls11Supported => fl!("ui-tool-tlsaudit-finding-tls11"),
        TlsFindingKind::NoTls13 => fl!("ui-tool-tlsaudit-finding-no-tls13"),
        TlsFindingKind::WeakCipher(suite) => {
            fl!(
                "ui-tool-tlsaudit-finding-weak-cipher",
                suite = suite.as_str()
            )
        }
        TlsFindingKind::NoPfs => fl!("ui-tool-tlsaudit-finding-no-pfs"),
        TlsFindingKind::AllGood => fl!("ui-tool-tlsaudit-finding-all-good"),
    }
}

/// Formats the complete audit report suitable for clipboard copying.
fn format_report_text(host: &str, port: u16, res: &TlsAuditResult) -> String {
    let mut sb = String::new();
    sb.push_str(&fl!(
        "ui-tool-tlsaudit-report-title",
        host = host,
        port = port
    ));
    sb.push('\n');
    sb.push_str(&fl!(
        "ui-tool-tlsaudit-report-grade",
        grade = tls_audit::grade_to_str(res.grade)
    ));
    sb.push_str("\n\n");

    sb.push_str(&fl!("ui-tool-tlsaudit-report-protocols"));
    sb.push('\n');
    for proto in &res.protocols {
        let status = if proto.supported {
            fl!("ui-tool-tlsaudit-supported")
        } else {
            fl!("ui-tool-tlsaudit-not-supported")
        };
        sb.push_str(&format!("  {}: {}\n", proto.name, status));
    }
    sb.push('\n');

    if !res.ciphers.is_empty() {
        sb.push_str(&fl!("ui-tool-tlsaudit-report-ciphers"));
        sb.push('\n');
        for cipher in &res.ciphers {
            sb.push_str(&format!(
                "  {}  [{}/{}/{}]\n",
                cipher.suite_name, cipher.key_exchange, cipher.authentication, cipher.encryption
            ));
        }
        sb.push('\n');
    }

    if let Some(cert) = &res.certificate {
        sb.push_str(&fl!("ui-tool-tlsaudit-report-cert"));
        sb.push('\n');
        sb.push_str(&format!(
            "  {}: {}\n",
            fl!("ui-tool-tlsaudit-cert-subject"),
            cert.subject
        ));
        sb.push_str(&format!(
            "  {}: {}\n",
            fl!("ui-tool-tlsaudit-cert-issuer"),
            cert.issuer
        ));
        sb.push_str(&format!(
            "  {}: {}\n",
            fl!("ui-tool-tlsaudit-cert-expires"),
            cert.not_after
        ));
        if cert.key_size_bits > 0 {
            sb.push_str(&format!(
                "  {}: {}\n",
                fl!("ui-tool-tlsaudit-cert-key-size"),
                fl!("ui-tool-tlsaudit-bits", bits = cert.key_size_bits)
            ));
        }
        sb.push('\n');
    }

    if !res.findings.is_empty() {
        sb.push_str(&fl!("ui-tool-tlsaudit-report-findings"));
        sb.push('\n');
        for finding in &res.findings {
            let icon = match finding.severity {
                TlsFindingSeverity::Pass => "\u{2714}",
                TlsFindingSeverity::Info => "\u{2139}",
                TlsFindingSeverity::Warning | TlsFindingSeverity::Critical => "\u{26A0}",
            };
            let msg = finding_message(&finding.kind);
            sb.push_str(&format!("  {} {}\n", icon, msg));
        }
    }

    sb
}

/// Asynchronous streaming probe execution.
fn audit_stream(
    host: String,
    port: u16,
    audit_id: u64,
) -> impl iced::futures::Stream<Item = TlsAuditMessage> {
    iced::stream::channel(16, async move |mut out| {
        let total_steps = (PROTOCOL_DEFINITIONS.len() + KNOWN_TLS12_CIPHER_SUITES.len() + 1) as f32;
        let mut current_step = 0.0f32;

        let mut protocol_results = Vec::with_capacity(PROTOCOL_DEFINITIONS.len());
        let mut any_connected = false;
        let mut last_error = String::new();

        // 1. Probe protocols
        for proto in PROTOCOL_DEFINITIONS {
            current_step += 1.0;
            let percent = current_step / total_steps;
            let desc = fl!("ui-tool-tlsaudit-progress", step = proto.name());
            let _ = out
                .send(TlsAuditMessage::Progress(audit_id, percent, desc))
                .await;

            let ciphers: &[u16] = match proto {
                TlsProtocol::Ssl3 => &[0x000A, 0x0005, 0x0004],
                TlsProtocol::Tls13 => &[0x1301, 0x1302, 0x1303, 0xC02F, 0xC030],
                _ => &[
                    0xC030, 0xC02F, 0xC02C, 0xC02B, 0x009F, 0x009E, 0x009D, 0x009C, 0x003D, 0x003C,
                    0x0035, 0x002F, 0xC028, 0xC027, 0xC014, 0xC013,
                ],
            };

            let hello = tls_audit::build_client_hello(proto, &host, ciphers);
            match probe_packet(&host, port, &hello, PROBE_TIMEOUT).await {
                Ok(resp) => {
                    any_connected = true;
                    let supported = resp
                        .as_deref()
                        .map_or(false, |r| tls_audit::is_server_hello_accepted(r, proto));
                    protocol_results.push(TlsProtocolTestResult {
                        name: proto.name().to_owned(),
                        supported,
                        rating: proto.rating(),
                    });
                }
                Err(e) => {
                    last_error = e;
                    protocol_results.push(TlsProtocolTestResult {
                        name: proto.name().to_owned(),
                        supported: false,
                        rating: proto.rating(),
                    });
                }
            }
        }

        if !any_connected {
            let err_msg = fl!(
                "ui-tool-tlsaudit-error-connection",
                error = last_error.as_str()
            );
            let _ = out
                .send(TlsAuditMessage::Finished(audit_id, Err(err_msg)))
                .await;
            return;
        }

        // 2. Probe cipher suites (if TLS 1.2 or modern is supported)
        let mut cipher_results = Vec::new();
        let mut cert_der: Option<Vec<u8>> = None;

        let tls12_supported = protocol_results
            .iter()
            .any(|p| (p.name == "TLS 1.2" || p.name == "TLS 1.3") && p.supported);

        if tls12_supported {
            for suite in KNOWN_TLS12_CIPHER_SUITES {
                current_step += 1.0;
                let percent = current_step / total_steps;
                let desc = fl!("ui-tool-tlsaudit-progress-ciphers");
                let _ = out
                    .send(TlsAuditMessage::Progress(audit_id, percent, desc))
                    .await;

                let hello = tls_audit::build_client_hello(TlsProtocol::Tls12, &host, &[suite.id]);
                if let Ok(Some(resp)) = probe_packet(&host, port, &hello, PROBE_TIMEOUT).await {
                    if tls_audit::is_server_hello_accepted(&resp, TlsProtocol::Tls12) {
                        cipher_results.push(tls_audit::build_cipher_info(suite.name));

                        if cert_der.is_none() {
                            cert_der = tls_audit::extract_certificate_from_handshake(&resp);
                        }
                    }
                }
            }
        }

        // 3. Extract certificate if not already done
        if cert_der.is_none() {
            let full_hello = tls_audit::build_client_hello(
                TlsProtocol::Tls12,
                &host,
                &[
                    0xC030, 0xC02F, 0xC02C, 0xC02B, 0x009F, 0x009E, 0x009D, 0x009C, 0x003D, 0x003C,
                    0x0035, 0x002F, 0xC028, 0xC027, 0xC014, 0xC013,
                ],
            );
            if let Ok(Some(flight)) =
                probe_handshake_flight(&host, port, &full_hello, PROBE_TIMEOUT).await
            {
                cert_der = tls_audit::extract_certificate_from_handshake(&flight);
            }
        }

        let cert_summary = cert_der.and_then(|der| {
            sealvault::cert::inspect(&der).map(|c| TlsCertificateSummary {
                subject: c.subject,
                issuer: c.issuer,
                not_after: c.not_after,
                key_size_bits: c.key_size_bits,
            })
        });

        let grade = tls_audit::calculate_grade(&protocol_results, &cipher_results);
        let findings = tls_audit::build_findings(&protocol_results, &cipher_results);

        let final_result = TlsAuditResult {
            grade,
            protocols: protocol_results,
            ciphers: cipher_results,
            certificate: cert_summary,
            findings,
        };

        let _ = out
            .send(TlsAuditMessage::Finished(audit_id, Ok(final_result)))
            .await;
    })
}

/// Connects to (host, port), sends `packet`, and reads the initial server response.
async fn probe_packet(
    host: &str,
    port: u16,
    packet: &[u8],
    timeout_duration: Duration,
) -> Result<Option<Vec<u8>>, String> {
    let connect_fut = tokio::net::TcpStream::connect((host, port));
    let mut stream = match tokio::time::timeout(timeout_duration, connect_fut).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(e.to_string()),
        Err(_) => return Err("Connection timed out".to_owned()),
    };

    // Both the timeout and the write itself can fail; the inner write error was
    // previously discarded, hiding a failed send as a successful one.
    match tokio::time::timeout(
        timeout_duration,
        tokio::io::AsyncWriteExt::write_all(&mut stream, packet),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e.to_string()),
        Err(e) => return Err(e.to_string()),
    }

    let mut buf = vec![0u8; 4096];
    match tokio::time::timeout(
        timeout_duration,
        tokio::io::AsyncReadExt::read(&mut stream, &mut buf),
    )
    .await
    {
        Ok(Ok(n)) if n > 0 => {
            buf.truncate(n);
            Ok(Some(buf))
        }
        Ok(Ok(_)) => Ok(None),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Ok(None),
    }
}

/// Probes a TLS endpoint reading a larger flight to locate the server certificate.
async fn probe_handshake_flight(
    host: &str,
    port: u16,
    packet: &[u8],
    timeout_duration: Duration,
) -> Result<Option<Vec<u8>>, String> {
    let connect_fut = tokio::net::TcpStream::connect((host, port));
    let mut stream = match tokio::time::timeout(timeout_duration, connect_fut).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(e.to_string()),
        Err(_) => return Err("Connection timed out".to_owned()),
    };

    match tokio::time::timeout(
        timeout_duration,
        tokio::io::AsyncWriteExt::write_all(&mut stream, packet),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e.to_string()),
        Err(e) => return Err(e.to_string()),
    }

    let mut buf = vec![0u8; 16384];
    let mut total_read = 0;
    let deadline = tokio::time::Instant::now() + timeout_duration;

    while total_read < buf.len() && tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(
            remaining,
            tokio::io::AsyncReadExt::read(&mut stream, &mut buf[total_read..]),
        )
        .await
        {
            Ok(Ok(n)) if n > 0 => {
                total_read += n;
                if tls_audit::extract_certificate_from_handshake(&buf[..total_read]).is_some() {
                    break;
                }
            }
            _ => break,
        }
    }

    if total_read > 0 {
        buf.truncate(total_read);
        Ok(Some(buf))
    } else {
        Ok(None)
    }
}
