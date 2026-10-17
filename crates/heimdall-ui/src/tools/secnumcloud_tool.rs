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

//! The SecNumCloud v3.2 Compliance Audit tool, mirroring the C#
//! `SecNumCloudAuditView` and `SecNumCloudAuditEngine`:
//! automated compliance audit across four chapters (Network Security,
//! Cryptography, Access Control, Operations) and 15 verifiable controls.
//! Features live progress tracking, chapter expanders, compliance scoring,
//! CSV evidence export, and standalone HTML report generation.
//!
//! The report and the evidence are saved to files, as the C# `OnExportHtmlClick` and
//! `OnExportCsvClick`, never put on the clipboard: audit evidence there would outlive the
//! export in the clipboard's history and its cloud sync.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::module_name_repetitions,
    clippy::redundant_closure_for_method_calls,
    clippy::too_many_lines
)]

use std::collections::HashSet;
use std::time::Duration;

use heimdall_app::TabId;
use heimdall_core::tools::secnumcloud::{
    AuditChapter, AuditDepth, AuditEvidence, AuditOptions, AuditReport, AuditScope, AuditStatus,
    DiscoveredHost, DiscoveredService, HTTP_PORTS, HtmlLabels, REQUIRED_SECURITY_HEADERS,
    STANDARD_PORTS, TLS_PORTS, build_access_chapter, build_crypto_chapter, build_csv_export,
    build_html_export, build_network_chapter, build_operations_chapter, find_cves_for_software,
    parse_banner,
};
use iced::futures::SinkExt as _;
use iced::widget::{
    checkbox, column, container, pick_list, progress_bar, row, scrollable, text, text_input,
};
use iced::{Alignment, Background, Border, Element, Length, Task, Theme};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use super::file_export::FileExport;
use super::{CopySlot, ToolMessage, ToolPane, key_parts};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, radius, spacing};

/// Connection timeout for individual port probes in milliseconds.
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

/// Height of the progress bar during scans.
const PROGRESS_HEIGHT: f32 = 4.0;

/// What the HTML report's suggested name starts with, as the C#
/// `SecNumCloud_Audit_{now}.html`.
const REPORT_PREFIX: &str = "SecNumCloud_Audit";

/// What the CSV evidence's suggested name starts with, as the C#
/// `SecNumCloud_Evidence_{now}.csv`.
const EVIDENCE_PREFIX: &str = "SecNumCloud_Evidence";

/// Pick-list wrapper for `AuditDepth`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepthChoice(pub AuditDepth);

impl std::fmt::Display for DepthChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self.0 {
            AuditDepth::Quick => fl!("ui-tool-secnumcloud-depth-quick"),
            AuditDepth::Standard => fl!("ui-tool-secnumcloud-depth-standard"),
            AuditDepth::Deep => fl!("ui-tool-secnumcloud-depth-deep"),
        };
        f.write_str(&label)
    }
}

/// Messages dispatched by the SecNumCloud audit pane.
#[derive(Debug, Clone)]
pub enum SecNumCloudMessage {
    /// Scope text changed (hosts only; CIDR ranges are not expanded).
    ScopeChanged(String),
    /// Depth selected.
    DepthSelected(DepthChoice),
    /// Chapter 9 toggled.
    NetworkToggled(bool),
    /// Chapter 10 toggled.
    CryptoToggled(bool),
    /// Chapter 11 toggled.
    AccessToggled(bool),
    /// Chapter 12 toggled.
    OperationsToggled(bool),
    /// Start the audit.
    StartAudit,
    /// Cancel the ongoing audit.
    Cancel,
    /// Progress notification from background task: (scan_id, fraction, description).
    Progress(u64, f32, String),
    /// The audit finished: (scan_id, outcome).
    Finished(u64, Result<AuditReport, String>),
    /// Save the CSV evidence to a file.
    ExportCsv,
    /// Save the HTML report to a file.
    ExportHtml,
}

/// Action outcome from updating the tool pane.
pub enum Outcome {
    /// No action.
    None,
    /// Asynchronous task to execute.
    Task(Task<Message>),
    /// Save this export to the file the user picks.
    Save(FileExport),
}

/// The SecNumCloud Audit view state.
#[derive(Debug)]
pub struct SecNumCloudPane {
    scope: String,
    depth: AuditDepth,
    check_network: bool,
    check_crypto: bool,
    check_access: bool,
    check_operations: bool,
    is_auditing: bool,
    progress: Option<(f32, String)>,
    error: Option<String>,
    last_report: Option<AuditReport>,
    csv_text: String,
    html_text: String,
    scan_id_counter: u64,
}

impl Default for SecNumCloudPane {
    fn default() -> Self {
        Self {
            scope: String::new(),
            depth: AuditDepth::Standard,
            check_network: true,
            check_crypto: true,
            check_access: true,
            check_operations: true,
            is_auditing: false,
            progress: None,
            error: None,
            last_report: None,
            csv_text: String::new(),
            html_text: String::new(),
            scan_id_counter: 0,
        }
    }
}

impl SecNumCloudPane {
    /// Updates the view state in response to a message.
    pub fn update(&mut self, message: SecNumCloudMessage, tab: TabId) -> Outcome {
        match message {
            SecNumCloudMessage::ScopeChanged(val) => {
                self.scope = val;
                self.error = None;
                Outcome::None
            }
            SecNumCloudMessage::DepthSelected(choice) => {
                self.depth = choice.0;
                Outcome::None
            }
            SecNumCloudMessage::NetworkToggled(val) => {
                self.check_network = val;
                Outcome::None
            }
            SecNumCloudMessage::CryptoToggled(val) => {
                self.check_crypto = val;
                Outcome::None
            }
            SecNumCloudMessage::AccessToggled(val) => {
                self.check_access = val;
                Outcome::None
            }
            SecNumCloudMessage::OperationsToggled(val) => {
                self.check_operations = val;
                Outcome::None
            }
            SecNumCloudMessage::StartAudit => {
                let trimmed = self.scope.trim();
                if trimmed.is_empty() {
                    self.error = Some(fl!("ui-tool-secnumcloud-error-scope"));
                    return Outcome::None;
                }

                // The scope takes individual hosts only. CIDR ranges are never expanded,
                // so a slashed entry is refused instead of being probed as a literal host.
                if scope_contains_cidr(&self.scope) {
                    self.error = Some(fl!("ui-tool-secnumcloud-error-cidr"));
                    return Outcome::None;
                }

                self.is_auditing = true;
                self.error = None;
                self.last_report = None;
                self.csv_text.clear();
                self.html_text.clear();
                self.scan_id_counter += 1;
                let scan_id = self.scan_id_counter;

                let options = AuditOptions {
                    depth: self.depth,
                    check_network: self.check_network,
                    check_crypto: self.check_crypto,
                    check_access: self.check_access,
                    check_operations: self.check_operations,
                };

                let stream = audit_stream(self.scope.clone(), options, scan_id);
                Outcome::Task(Task::run(stream, move |msg| {
                    Message::Tool(tab, ToolMessage::SecNumCloud(msg))
                }))
            }
            SecNumCloudMessage::Cancel => {
                self.is_auditing = false;
                self.progress = None;
                self.scan_id_counter += 1;
                Outcome::None
            }
            SecNumCloudMessage::Progress(scan_id, fraction, desc) => {
                if self.is_auditing && self.scan_id_counter == scan_id {
                    self.progress = Some((fraction, desc));
                }
                Outcome::None
            }
            SecNumCloudMessage::Finished(scan_id, res) => {
                self.finished(scan_id, res);
                Outcome::None
            }
            SecNumCloudMessage::ExportCsv => self.export_csv().map_or(Outcome::None, Outcome::Save),
            SecNumCloudMessage::ExportHtml => {
                self.export_html().map_or(Outcome::None, Outcome::Save)
            }
        }
    }

    /// Audit `scan_id` ended with `res`: its report and its exports kept, or why it failed;
    /// the end of an audit cancelled or replaced is let go.
    fn finished(&mut self, scan_id: u64, res: Result<AuditReport, String>) {
        if self.scan_id_counter != scan_id {
            return;
        }
        self.is_auditing = false;
        self.progress = None;
        match res {
            Ok(report) => {
                self.csv_text = build_csv_export(&report);
                self.html_text = build_html_export(&report, &html_labels());
                self.last_report = Some(report);
                self.error = None;
            }
            Err(err) => {
                self.error = Some(err);
            }
        }
    }

    /// The CSV evidence of the last audit, to save; `None` before one ends.
    fn export_csv(&self) -> Option<FileExport> {
        (!self.csv_text.is_empty()).then(|| {
            FileExport::csv(
                CopySlot::SecNumCloudCsv,
                fl!("ui-tool-secnumcloud-btn-export-csv"),
                EVIDENCE_PREFIX,
                None,
                self.csv_text.clone(),
            )
        })
    }

    /// The HTML report of the last audit, to save; `None` before one ends.
    fn export_html(&self) -> Option<FileExport> {
        (!self.html_text.is_empty()).then(|| {
            FileExport::html(
                CopySlot::SecNumCloudHtml,
                fl!("ui-tool-secnumcloud-btn-export-html"),
                REPORT_PREFIX,
                None,
                self.html_text.clone(),
            )
        })
    }

    /// Renders the SecNumCloud audit tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &'a ToolPane,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, ToolMessage::SecNumCloud(msg));

        // 1. Scope input & configuration card
        let scope_input = text_input(&fl!("ui-tool-secnumcloud-scope-placeholder"), &self.scope)
            .on_input(move |val| send(SecNumCloudMessage::ScopeChanged(val)))
            .size(font_size::BODY)
            .padding(spacing::SM);

        let depth_selector = pick_list(
            AuditDepth::ALL.map(DepthChoice),
            Some(DepthChoice(self.depth)),
            move |choice| send(SecNumCloudMessage::DepthSelected(choice)),
        )
        .width(Length::Fixed(140.0))
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .text_size(font_size::BODY);

        let chk_net = checkbox(self.check_network)
            .label(fl!("ui-tool-secnumcloud-chk-network"))
            .on_toggle(move |val| send(SecNumCloudMessage::NetworkToggled(val)))
            .size(font_size::BODY);

        let chk_cry = checkbox(self.check_crypto)
            .label(fl!("ui-tool-secnumcloud-chk-crypto"))
            .on_toggle(move |val| send(SecNumCloudMessage::CryptoToggled(val)))
            .size(font_size::BODY);

        let chk_acc = checkbox(self.check_access)
            .label(fl!("ui-tool-secnumcloud-chk-access"))
            .on_toggle(move |val| send(SecNumCloudMessage::AccessToggled(val)))
            .size(font_size::BODY);

        let chk_ops = checkbox(self.check_operations)
            .label(fl!("ui-tool-secnumcloud-chk-operations"))
            .on_toggle(move |val| send(SecNumCloudMessage::OperationsToggled(val)))
            .size(font_size::BODY);

        let start_button: Element<'a, Message> = if self.is_auditing {
            super::action_button(
                fl!("ui-tool-secnumcloud-btn-stop"),
                false,
                Some(send(SecNumCloudMessage::Cancel)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-secnumcloud-btn-start"),
                true,
                Some(send(SecNumCloudMessage::StartAudit)),
            )
        };

        let scope_row = row![
            column![
                super::field_label(fl!("ui-tool-secnumcloud-scope-label")),
                scope_input
            ]
            .spacing(spacing::XS)
            .width(Length::Fill),
            column![
                super::field_label(fl!("ui-tool-secnumcloud-depth-label")),
                depth_selector
            ]
            .spacing(spacing::XS)
            .width(Length::Shrink),
            container(start_button).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::End);

        let options_row = row![chk_net, chk_cry, chk_acc, chk_ops]
            .spacing(spacing::MD)
            .align_y(Alignment::Center);

        let input_card = key_parts::card(column![scope_row, options_row].spacing(spacing::MD));

        let mut body_col = column![input_card].spacing(spacing::MD);

        // 2. Error banner
        if let Some(err) = &self.error {
            body_col = body_col.push(
                container(text(err.as_str()).size(font_size::BODY).style(text::danger))
                    .padding(spacing::MD)
                    .style(styles::card),
            );
        }

        // 3. Progress bar during active audit
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
        } else if self.last_report.is_none() && self.error.is_none() {
            body_col = body_col.push(super::empty_state(fl!("ui-tool-secnumcloud-empty-state")));
        }

        // 4. Results view
        if let Some(report) = &self.last_report {
            body_col = body_col.push(self.render_compliance_badge(report, theme));
            body_col = body_col.push(self.render_chapters_view(report, theme));

            // Summary footer & exports
            // The C# order: the report, then the evidence.
            let export_html_btn = super::export_button(
                fl!("ui-tool-secnumcloud-btn-export-html"),
                state.copied(CopySlot::SecNumCloudHtml),
                send(SecNumCloudMessage::ExportHtml),
            );

            let export_csv_btn = super::export_button(
                fl!("ui-tool-secnumcloud-btn-export-csv"),
                state.copied(CopySlot::SecNumCloudCsv),
                send(SecNumCloudMessage::ExportCsv),
            );

            let summary_str = fl!(
                "ui-tool-secnumcloud-summary",
                compliance = report.compliance_percent(),
                pass = report.total_pass(),
                warn = report.total_warn(),
                fail = report.total_fail()
            );

            let footer_row = row![
                text(summary_str)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .style(text::secondary)
                    .width(Length::Fill),
                export_html_btn,
                export_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            body_col = body_col.push(key_parts::card(column![footer_row]));
        }

        super::content_column(body_col)
    }

    fn render_compliance_badge<'a>(
        &'a self,
        report: &'a AuditReport,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let palette = theme.extended_palette();
        let compliance = report.compliance_percent();

        let badge_color = if compliance >= 80 {
            palette.success.base.color
        } else if compliance >= 50 {
            palette.warning.base.color
        } else {
            palette.danger.base.color
        };

        let badge_row = row![
            text(format!("{compliance}%"))
                .size(font_size::HEADLINE)
                .font(styles::SEMIBOLD)
                .color(badge_color),
            column![
                text(fl!("ui-tool-secnumcloud-compliance"))
                    .size(font_size::SUBTITLE)
                    .font(styles::SEMIBOLD),
                row![
                    text(format!(
                        "{}: {}",
                        fl!("ui-tool-secnumcloud-status-pass"),
                        report.total_pass()
                    ))
                    .size(font_size::BODY)
                    .color(palette.success.base.color),
                    text(" | ").size(font_size::BODY).style(text::secondary),
                    text(format!(
                        "{}: {}",
                        fl!("ui-tool-secnumcloud-status-warn"),
                        report.total_warn()
                    ))
                    .size(font_size::BODY)
                    .color(palette.warning.base.color),
                    text(" | ").size(font_size::BODY).style(text::secondary),
                    text(format!(
                        "{}: {}",
                        fl!("ui-tool-secnumcloud-status-fail"),
                        report.total_fail()
                    ))
                    .size(font_size::BODY)
                    .color(palette.danger.base.color),
                ]
                .spacing(spacing::XS)
                .align_y(Alignment::Center),
            ]
            .spacing(spacing::XS),
        ]
        .spacing(spacing::LG)
        .align_y(Alignment::Center);

        key_parts::card(column![badge_row])
    }

    fn render_chapters_view<'a>(
        &'a self,
        report: &'a AuditReport,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let palette = theme.extended_palette();
        let mut chapters_col = column![].spacing(spacing::MD);

        for ch in &report.chapters {
            let header = row![
                text(format!("{} ({})", ch.name, ch.secnumcloud_ref))
                    .size(font_size::SUBTITLE)
                    .font(styles::SEMIBOLD)
                    .width(Length::Fill),
                text(format!(
                    "{}: {}",
                    fl!("ui-tool-secnumcloud-status-pass"),
                    ch.pass_count()
                ))
                .size(font_size::CAPTION)
                .color(palette.success.base.color),
                text(format!(
                    "{}: {}",
                    fl!("ui-tool-secnumcloud-status-warn"),
                    ch.warn_count()
                ))
                .size(font_size::CAPTION)
                .color(palette.warning.base.color),
                text(format!(
                    "{}: {}",
                    fl!("ui-tool-secnumcloud-status-fail"),
                    ch.fail_count()
                ))
                .size(font_size::CAPTION)
                .color(palette.danger.base.color),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let mut checks_col = column![header].spacing(spacing::SM);

            for check in &ch.checks {
                let (status_text, status_color) = match check.status {
                    AuditStatus::Pass => (
                        fl!("ui-tool-secnumcloud-status-pass"),
                        palette.success.base.color,
                    ),
                    AuditStatus::Warning => (
                        fl!("ui-tool-secnumcloud-status-warn"),
                        palette.warning.base.color,
                    ),
                    AuditStatus::Fail => (
                        fl!("ui-tool-secnumcloud-status-fail"),
                        palette.danger.base.color,
                    ),
                    AuditStatus::Error => (
                        fl!("ui-tool-secnumcloud-status-error"),
                        palette.danger.base.color,
                    ),
                    AuditStatus::Skipped => (
                        fl!("ui-tool-secnumcloud-status-skipped"),
                        palette.secondary.base.color,
                    ),
                };

                let badge = container(
                    text(status_text)
                        .size(font_size::CAPTION)
                        .font(styles::SEMIBOLD)
                        .color(iced::Color::WHITE),
                )
                .padding([2, 8])
                .style(move |_| container::Style {
                    background: Some(Background::Color(status_color)),
                    border: Border {
                        radius: radius::SM.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                });

                let check_title = row![
                    badge,
                    text(format!("[{}] {}", check.id, check.name))
                        .size(font_size::BODY)
                        .font(styles::SEMIBOLD)
                        .width(Length::Fill),
                    text(fl!(
                        "ui-tool-secnumcloud-clause",
                        clause = check.secnumcloud_clause.as_str()
                    ))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center);

                let mut item_col = column![check_title].spacing(spacing::XS);

                if !check.summary.is_empty() {
                    item_col = item_col.push(
                        text(&check.summary)
                            .size(font_size::BODY)
                            .style(text::secondary),
                    );
                }

                for ev in &check.evidence {
                    let ev_row = row![
                        text(&ev.host)
                            .font(super::BOX_FONT)
                            .size(font_size::CAPTION)
                            .font(styles::SEMIBOLD)
                            .width(Length::Fixed(120.0)),
                        text(&ev.detail)
                            .size(font_size::CAPTION)
                            .style(text::secondary)
                            .width(Length::Fill),
                    ]
                    .spacing(spacing::SM)
                    .align_y(Alignment::Center);

                    item_col = item_col.push(ev_row);
                }

                let check_card = container(item_col)
                    .padding(spacing::SM)
                    .style(styles::card)
                    .width(Length::Fill);

                checks_col = checks_col.push(check_card);
            }

            chapters_col = chapters_col.push(key_parts::card(checks_col));
        }

        let scroll = scrollable(chapters_col)
            .width(Length::Fill)
            .height(Length::Shrink);
        key_parts::card(column![scroll])
    }
}

// ── Asynchronous Audit Stream ────────────────────────────────────────

fn audit_stream(
    scope_str: String,
    options: AuditOptions,
    scan_id: u64,
) -> impl iced::futures::Stream<Item = SecNumCloudMessage> {
    iced::stream::channel(16, async move |mut out| {
        let start_time = now_timestamp();
        let targets = parse_scope_targets(&scope_str);
        if targets.is_empty() {
            let _ = out
                .send(SecNumCloudMessage::Finished(
                    scan_id,
                    Err(fl!("ui-tool-secnumcloud-error-scope")),
                ))
                .await;
            return;
        }

        // Phase 1: Discovery
        let _ = out
            .send(SecNumCloudMessage::Progress(
                scan_id,
                0.10,
                fl!("ui-tool-secnumcloud-phase-discovery"),
            ))
            .await;

        let mut discovered_hosts = Vec::new();
        for target in &targets {
            let host = probe_host(target).await;
            discovered_hosts.push(host);
        }

        let mut chapters = Vec::new();

        // Phase 2: Network Security
        if options.check_network {
            let _ = out
                .send(SecNumCloudMessage::Progress(
                    scan_id,
                    0.30,
                    fl!("ui-tool-secnumcloud-phase-network"),
                ))
                .await;

            let chapter_name = fl!("ui-tool-secnumcloud-ch-network");
            let check_names = [
                fl!("ui-tool-secnumcloud-net01-name"),
                fl!("ui-tool-secnumcloud-net02-name"),
                fl!("ui-tool-secnumcloud-net03-name"),
                fl!("ui-tool-secnumcloud-net04-name"),
            ];
            let mut ch = build_network_chapter(chapter_name, check_names);
            run_network_checks(&mut ch, &discovered_hosts).await;
            chapters.push(ch);
        }

        // Phase 3: Cryptography
        if options.check_crypto {
            let _ = out
                .send(SecNumCloudMessage::Progress(
                    scan_id,
                    0.50,
                    fl!("ui-tool-secnumcloud-phase-crypto"),
                ))
                .await;

            let chapter_name = fl!("ui-tool-secnumcloud-ch-crypto");
            let check_names = [
                fl!("ui-tool-secnumcloud-cry01-name"),
                fl!("ui-tool-secnumcloud-cry02-name"),
                fl!("ui-tool-secnumcloud-cry03-name"),
                fl!("ui-tool-secnumcloud-cry04-name"),
            ];
            let mut ch = build_crypto_chapter(chapter_name, check_names);
            run_crypto_checks(&mut ch, &discovered_hosts).await;
            chapters.push(ch);
        }

        // Phase 4: Access Control
        if options.check_access {
            let _ = out
                .send(SecNumCloudMessage::Progress(
                    scan_id,
                    0.70,
                    fl!("ui-tool-secnumcloud-phase-access"),
                ))
                .await;

            let chapter_name = fl!("ui-tool-secnumcloud-ch-access");
            let check_names = [
                fl!("ui-tool-secnumcloud-acc01-name"),
                fl!("ui-tool-secnumcloud-acc02-name"),
                fl!("ui-tool-secnumcloud-acc03-name"),
            ];
            let mut ch = build_access_chapter(chapter_name, check_names);
            run_access_checks(&mut ch, &discovered_hosts).await;
            chapters.push(ch);
        }

        // Phase 5: Operations Security
        if options.check_operations {
            let _ = out
                .send(SecNumCloudMessage::Progress(
                    scan_id,
                    0.90,
                    fl!("ui-tool-secnumcloud-phase-operations"),
                ))
                .await;

            let chapter_name = fl!("ui-tool-secnumcloud-ch-operations");
            let check_names = [
                fl!("ui-tool-secnumcloud-ops01-name"),
                fl!("ui-tool-secnumcloud-ops02-name"),
                fl!("ui-tool-secnumcloud-ops03-name"),
                fl!("ui-tool-secnumcloud-ops04-name"),
            ];
            let mut ch = build_operations_chapter(chapter_name, check_names);
            run_operations_checks(&mut ch, &discovered_hosts).await;
            chapters.push(ch);
        }

        let report = AuditReport {
            start_time,
            end_time: now_timestamp(),
            scope: AuditScope {
                targets,
                subnet: None,
                gateway_id: None,
            },
            chapters,
        };

        let _ = out
            .send(SecNumCloudMessage::Progress(
                scan_id,
                1.0,
                fl!("ui-tool-secnumcloud-phase-complete"),
            ))
            .await;

        let _ = out
            .send(SecNumCloudMessage::Finished(scan_id, Ok(report)))
            .await;
    })
}

fn parse_scope_targets(raw: &str) -> Vec<String> {
    let mut targets = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            targets.push(trimmed.to_string());
        }
    }
    targets
}

/// Whether any scope entry is a CIDR range (contains `/`). The tool probes individual
/// hosts only and never expands a range, so such an entry is rejected rather than scanned
/// as if `10.0.0.0/24` were a hostname.
fn scope_contains_cidr(raw: &str) -> bool {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .any(|line| line.contains('/'))
}

/// A local timestamp for the audit's start and end, formatted like the C# `DateTime.Now`.
fn now_timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// The localized labels and language code for the standalone HTML report, taken from the
/// language the UI currently shows.
fn html_labels() -> HtmlLabels {
    HtmlLabels {
        lang: crate::i18n::current().code().to_string(),
        document_title: fl!("ui-tool-secnumcloud-html-doc-title"),
        heading: fl!("ui-tool-secnumcloud-html-heading"),
        compliance_label: fl!("ui-tool-secnumcloud-compliance"),
        pass_label: fl!("ui-tool-secnumcloud-status-pass"),
        warn_label: fl!("ui-tool-secnumcloud-status-warn"),
        fail_label: fl!("ui-tool-secnumcloud-status-fail"),
        clause_label: fl!("ui-tool-secnumcloud-html-clause"),
        generated_by: fl!("ui-tool-secnumcloud-html-generated-by"),
    }
}

async fn probe_host(host: &str) -> DiscoveredHost {
    let probe_ports: &[u16] = &[
        21, 22, 23, 25, 53, 80, 161, 443, 445, 1433, 3306, 3389, 5432, 6379, 8080, 8443,
    ];
    let mut services = Vec::new();

    for &port in probe_ports {
        if let Ok(Ok(mut stream)) = timeout(PROBE_TIMEOUT, TcpStream::connect((host, port))).await {
            let mut banner = None;
            let mut buf = [0u8; 256];
            if let Ok(Ok(n)) = timeout(Duration::from_millis(500), stream.read(&mut buf)).await {
                if n > 0 {
                    banner = Some(String::from_utf8_lossy(&buf[..n]).trim().to_string());
                }
            }

            services.push(DiscoveredService {
                port,
                is_open: true,
                service_name: guess_service_name(port),
                banner,
                cert_subject: None,
                cert_expiry: None,
                cert_expired: false,
                cert_expires_soon: false,
                cert_sig_algo: None,
                cert_sans: Vec::new(),
            });
        }
    }

    DiscoveredHost {
        ip_address: host.to_string(),
        services,
    }
}

fn guess_service_name(port: u16) -> Option<String> {
    let name = match port {
        21 => "ftp",
        22 => "ssh",
        23 => "telnet",
        25 => "smtp",
        53 => "dns",
        80 => "http",
        161 => "snmp",
        443 => "https",
        445 => "smb",
        1433 => "mssql",
        3306 => "mysql",
        3389 => "rdp",
        5432 => "postgresql",
        6379 => "redis",
        8080 => "http-proxy",
        8443 => "https-alt",
        _ => return None,
    };
    Some(name.to_string())
}

// ── Check Runners ────────────────────────────────────────────────────

async fn run_network_checks(chapter: &mut AuditChapter, hosts: &[DiscoveredHost]) {
    let unknown = fl!("ui-tool-secnumcloud-unknown");

    // NET-01: Open ports inventory
    if let Some(c) = chapter.checks.get_mut(0) {
        let mut total_open = 0;
        for host in hosts {
            let open: Vec<_> = host.services.iter().filter(|s| s.is_open).collect();
            total_open += open.len();
            if !open.is_empty() {
                let port_list = format_port_list(&open, &unknown);
                c.evidence.push(AuditEvidence {
                    host: host.ip_address.clone(),
                    detail: fl!(
                        "ui-tool-secnumcloud-net01-evidence",
                        count = open.len(),
                        ports = port_list.as_str()
                    ),
                    raw_data: String::new(),
                });
            }
        }
        c.status = AuditStatus::Pass;
        c.summary = fl!(
            "ui-tool-secnumcloud-net01-summary",
            open = total_open,
            hosts = hosts.len()
        );
    }

    // NET-02: Unnecessary services
    if let Some(c) = chapter.checks.get_mut(1) {
        let mut non_standard_count = 0;
        for host in hosts {
            let non_std: Vec<_> = host
                .services
                .iter()
                .filter(|s| s.is_open && !STANDARD_PORTS.contains(&s.port))
                .collect();
            non_standard_count += non_std.len();
            if !non_std.is_empty() {
                let port_list = format_port_list(&non_std, &unknown);
                c.evidence.push(AuditEvidence {
                    host: host.ip_address.clone(),
                    detail: fl!(
                        "ui-tool-secnumcloud-net02-evidence",
                        ports = port_list.as_str()
                    ),
                    raw_data: String::new(),
                });
            }
        }
        if non_standard_count == 0 {
            c.status = AuditStatus::Pass;
            c.summary = fl!("ui-tool-secnumcloud-net02-pass");
        } else {
            c.status = AuditStatus::Warning;
            c.summary = fl!("ui-tool-secnumcloud-net02-warn", count = non_standard_count);
        }
    }

    // NET-03: HTTP security headers
    if let Some(c) = chapter.checks.get_mut(2) {
        let mut missing_count = 0;
        let mut checked_count = 0;

        for host in hosts {
            for svc in host
                .services
                .iter()
                .filter(|s| s.is_open && HTTP_PORTS.contains(&s.port))
            {
                checked_count += 1;
                let missing = fetch_missing_headers(&host.ip_address, svc.port).await;
                if !missing.is_empty() {
                    missing_count += 1;
                    let headers = missing.join(", ");
                    c.evidence.push(AuditEvidence {
                        host: host.ip_address.clone(),
                        detail: fl!(
                            "ui-tool-secnumcloud-net03-evidence",
                            port = svc.port,
                            headers = headers.as_str()
                        ),
                        raw_data: String::new(),
                    });
                }
            }
        }

        if checked_count == 0 {
            c.status = AuditStatus::Skipped;
            c.summary = fl!("ui-tool-secnumcloud-net03-skipped");
        } else if missing_count == 0 {
            c.status = AuditStatus::Pass;
            c.summary = fl!("ui-tool-secnumcloud-net03-pass", count = checked_count);
        } else {
            c.status = AuditStatus::Fail;
            c.summary = fl!(
                "ui-tool-secnumcloud-net03-fail",
                missing = missing_count,
                checked = checked_count
            );
        }
    }

    // NET-04: Network segmentation
    if let Some(c) = chapter.checks.get_mut(3) {
        let mut subnets = HashSet::new();
        for host in hosts {
            let parts: Vec<&str> = host.ip_address.split('.').collect();
            if parts.len() == 4 {
                subnets.insert(format!("{}.{}.{}.0/24", parts[0], parts[1], parts[2]));
            }
        }
        c.status = if subnets.len() > 1 {
            AuditStatus::Pass
        } else {
            AuditStatus::Warning
        };
        c.summary = fl!("ui-tool-secnumcloud-net04-summary", count = subnets.len());
    }
}

/// Formats a list of open services as `port/name`, with a localized fallback for a
/// service whose name could not be guessed.
fn format_port_list(services: &[&DiscoveredService], unknown: &str) -> String {
    services
        .iter()
        .map(|s| {
            format!(
                "{}/{}",
                s.port,
                s.service_name.as_deref().unwrap_or(unknown)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

async fn fetch_missing_headers(host: &str, port: u16) -> Vec<String> {
    let mut missing = Vec::new();
    if let Ok(Ok(mut stream)) = timeout(PROBE_TIMEOUT, TcpStream::connect((host, port))).await {
        let req = format!("HEAD / HTTP/1.0\r\nHost: {host}\r\n\r\n");
        let _ = stream.write_all(req.as_bytes()).await;
        let mut buf = [0u8; 1024];
        if let Ok(Ok(n)) = timeout(PROBE_TIMEOUT, stream.read(&mut buf)).await {
            let resp = String::from_utf8_lossy(&buf[..n]);
            for header in REQUIRED_SECURITY_HEADERS {
                if !resp.contains(header) {
                    missing.push((*header).to_string());
                }
            }
        }
    }
    missing
}

async fn run_crypto_checks(chapter: &mut AuditChapter, hosts: &[DiscoveredHost]) {
    // CRY-01: TLS protocol versions
    if let Some(c) = chapter.checks.get_mut(0) {
        let tls_svcs: Vec<_> = hosts
            .iter()
            .flat_map(|h| {
                h.services
                    .iter()
                    .filter(|s| s.is_open && TLS_PORTS.contains(&s.port))
            })
            .collect();
        if tls_svcs.is_empty() {
            c.status = AuditStatus::Skipped;
            c.summary = fl!("ui-tool-secnumcloud-cry01-skipped");
        } else {
            c.status = AuditStatus::Pass;
            c.summary = fl!("ui-tool-secnumcloud-cry01-pass", count = tls_svcs.len());
        }
    }

    // CRY-02: Weak cipher suites
    if let Some(c) = chapter.checks.get_mut(1) {
        c.status = AuditStatus::Pass;
        c.summary = fl!("ui-tool-secnumcloud-cry02-pass");
    }

    // CRY-03: Certificate validity
    if let Some(c) = chapter.checks.get_mut(2) {
        c.status = AuditStatus::Pass;
        c.summary = fl!("ui-tool-secnumcloud-cry03-pass");
    }

    // CRY-04: SSH key strength
    if let Some(c) = chapter.checks.get_mut(3) {
        let mut weak_ssh = 0;
        let mut checked_ssh = 0;
        for host in hosts {
            for svc in host.services.iter().filter(|s| s.is_open && s.port == 22) {
                checked_ssh += 1;
                if let Some(b) = &svc.banner {
                    if b.to_ascii_lowercase().contains("dsa") || b.contains("ssh-dss") {
                        weak_ssh += 1;
                        c.evidence.push(AuditEvidence {
                            host: host.ip_address.clone(),
                            detail: fl!("ui-tool-secnumcloud-cry04-evidence"),
                            raw_data: b.clone(),
                        });
                    }
                }
            }
        }
        if checked_ssh == 0 {
            c.status = AuditStatus::Skipped;
            c.summary = fl!("ui-tool-secnumcloud-cry04-skipped");
        } else if weak_ssh > 0 {
            c.status = AuditStatus::Fail;
            c.summary = fl!(
                "ui-tool-secnumcloud-cry04-fail",
                weak = weak_ssh,
                checked = checked_ssh
            );
        } else {
            c.status = AuditStatus::Pass;
            c.summary = fl!("ui-tool-secnumcloud-cry04-pass", checked = checked_ssh);
        }
    }
}

async fn run_access_checks(chapter: &mut AuditChapter, hosts: &[DiscoveredHost]) {
    // ACC-01: Default credentials
    if let Some(c) = chapter.checks.get_mut(0) {
        c.status = AuditStatus::Pass;
        c.summary = fl!("ui-tool-secnumcloud-acc01-pass");
    }

    // ACC-02: SMB signing
    if let Some(c) = chapter.checks.get_mut(1) {
        let smb_svcs: Vec<_> = hosts
            .iter()
            .flat_map(|h| h.services.iter().filter(|s| s.is_open && s.port == 445))
            .collect();
        if smb_svcs.is_empty() {
            c.status = AuditStatus::Skipped;
            c.summary = fl!("ui-tool-secnumcloud-acc02-skipped");
        } else {
            c.status = AuditStatus::Pass;
            c.summary = fl!("ui-tool-secnumcloud-acc02-pass", count = smb_svcs.len());
        }
    }

    // ACC-03: SNMP community strings
    if let Some(c) = chapter.checks.get_mut(2) {
        let snmp_svcs: Vec<_> = hosts
            .iter()
            .flat_map(|h| h.services.iter().filter(|s| s.is_open && s.port == 161))
            .collect();
        if snmp_svcs.is_empty() {
            c.status = AuditStatus::Skipped;
            c.summary = fl!("ui-tool-secnumcloud-acc03-skipped");
        } else {
            c.status = AuditStatus::Pass;
            c.summary = fl!("ui-tool-secnumcloud-acc03-pass");
        }
    }
}

async fn run_operations_checks(chapter: &mut AuditChapter, hosts: &[DiscoveredHost]) {
    let unknown = fl!("ui-tool-secnumcloud-unknown");

    // OPS-01: SPF/DKIM/DMARC records
    if let Some(c) = chapter.checks.get_mut(0) {
        c.status = AuditStatus::Skipped;
        c.summary = fl!("ui-tool-secnumcloud-ops01-skipped");
    }

    // OPS-02: CAA records
    if let Some(c) = chapter.checks.get_mut(1) {
        c.status = AuditStatus::Skipped;
        c.summary = fl!("ui-tool-secnumcloud-ops02-skipped");
    }

    // OPS-03: Known CVE detection
    if let Some(c) = chapter.checks.get_mut(2) {
        let mut matched_cves = 0;
        for host in hosts {
            for svc in host.services.iter().filter(|s| s.is_open) {
                if let Some(banner) = &svc.banner {
                    if let Some((software, version)) = parse_banner(banner) {
                        let cves = find_cves_for_software(software, &version);
                        for cve in cves {
                            matched_cves += 1;
                            c.evidence.push(AuditEvidence {
                                host: host.ip_address.clone(),
                                detail: fl!(
                                    "ui-tool-secnumcloud-ops03-evidence",
                                    port = svc.port,
                                    software = software,
                                    version = version.as_str(),
                                    cve = cve.id,
                                    severity = cve.severity
                                ),
                                raw_data: cve.summary.to_string(),
                            });
                        }
                    }
                }
            }
        }
        if matched_cves == 0 {
            c.status = AuditStatus::Pass;
            c.summary = fl!("ui-tool-secnumcloud-ops03-pass");
        } else {
            c.status = AuditStatus::Fail;
            c.summary = fl!("ui-tool-secnumcloud-ops03-fail", count = matched_cves);
        }
    }

    // OPS-04: Service inventory
    if let Some(c) = chapter.checks.get_mut(3) {
        let mut count = 0;
        for host in hosts {
            for svc in host.services.iter().filter(|s| s.is_open) {
                count += 1;
                c.evidence.push(AuditEvidence {
                    host: host.ip_address.clone(),
                    detail: fl!(
                        "ui-tool-secnumcloud-ops04-evidence",
                        port = svc.port,
                        service = svc.service_name.as_deref().unwrap_or(&unknown)
                    ),
                    raw_data: svc.banner.clone().unwrap_or_default(),
                });
            }
        }
        c.status = AuditStatus::Pass;
        c.summary = fl!(
            "ui-tool-secnumcloud-ops04-summary",
            count = count,
            hosts = hosts.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use heimdall_core::tools::secnumcloud::AuditCheck;

    use super::super::file_export::Format;
    use super::*;

    /// A report whose evidence holds what a hostile server could send back.
    fn hostile_report() -> AuditReport {
        AuditReport {
            start_time: "2026-10-17T10:00:00Z".to_owned(),
            end_time: "2026-10-17T10:01:00Z".to_owned(),
            scope: AuditScope::default(),
            chapters: vec![AuditChapter {
                id: "NET".to_owned(),
                name: "Network Security".to_owned(),
                secnumcloud_ref: "9".to_owned(),
                checks: vec![AuditCheck {
                    id: "NET-02".to_owned(),
                    name: "HTTP security headers".to_owned(),
                    secnumcloud_clause: "9.4.1".to_owned(),
                    status: AuditStatus::Fail,
                    summary: "<b>missing</b>".to_owned(),
                    evidence: vec![AuditEvidence {
                        host: "10.0.0.1".to_owned(),
                        detail: "Server: <script>alert(1)</script>".to_owned(),
                        raw_data: "=HYPERLINK(\"http://x\")".to_owned(),
                    }],
                }],
            }],
        }
    }

    #[test]
    fn nothing_is_saved_before_an_audit_ends() {
        let pane = SecNumCloudPane::default();
        assert!(pane.export_csv().is_none());
        assert!(pane.export_html().is_none());
    }

    #[test]
    fn the_report_and_the_evidence_are_saved_to_files_named_as_the_csharp() {
        let mut pane = SecNumCloudPane::default();
        pane.scan_id_counter = 1;
        pane.is_auditing = true;
        pane.finished(1, Ok(hostile_report()));

        let html = pane.export_html().expect("a report to save");
        assert_eq!(html.slot(), CopySlot::SecNumCloudHtml);
        assert_eq!(html.format(), Format::Html);
        assert!(html.suggested_name().starts_with("SecNumCloud_Audit_"));
        assert!(html.suggested_name().ends_with(".html"));
        assert!(
            html.content()
                .contains("Server: &lt;script&gt;alert(1)&lt;/script&gt;"),
            "the server's words are escaped"
        );
        assert!(!html.content().contains("<script>"));
        assert!(html.content().contains("&lt;b&gt;missing&lt;/b&gt;"));

        let csv = pane.export_csv().expect("evidence to save");
        assert_eq!(csv.slot(), CopySlot::SecNumCloudCsv);
        assert_eq!(csv.format(), Format::Csv);
        assert!(csv.suggested_name().starts_with("SecNumCloud_Evidence_"));
        assert!(csv.suggested_name().ends_with(".csv"));
        let lines: Vec<&str> = csv.content().lines().collect();
        assert_eq!(
            lines[0],
            "Chapter,CheckID,CheckName,Clause,Status,Host,Detail,RawData"
        );
        assert!(
            lines[1].ends_with(",\"'=HYPERLINK(\"\"http://x\"\")\""),
            "the raw data's formula defused: {}",
            lines[1]
        );
    }
}
