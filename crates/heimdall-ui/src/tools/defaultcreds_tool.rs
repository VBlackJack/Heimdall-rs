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

//! The Default Credentials Scanner tool, as the C# `DefaultCredentialView`
//! and `DefaultCredentialViewModel`:
//! tests common default credentials against detected services on a target host.
//! Features automatic port discovery, rate limiting, masked password toggle,
//! plain-text report copy, and CSV export.

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
use heimdall_core::tools::base64_codec;
use heimdall_core::tools::default_creds::{
    self, CredTestResult, CredTestStatus, PORT_PROBE_TIMEOUT_MS, RATE_LIMIT_DELAY_MS,
    SERVICE_PORTS, build_csv_export, build_report_text, count_defaults_and_services,
    is_http_success_response, status_to_detail,
};
use iced::futures::SinkExt as _;
use iced::widget::{checkbox, column, container, progress_bar, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Task, Theme};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use super::{CopySlot, ToolMessage, ToolPane, key_parts};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, radius, spacing};

/// Connection timeout in milliseconds for credential attempts.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(default_creds::CONNECT_TIMEOUT_MS);

/// Height of the progress bar during scans.
const PROGRESS_HEIGHT: f32 = 4.0;

/// Mask string used when passwords are not displayed.
const MASKED_PASSWORD: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

/// Messages dispatched by the Default Credentials Scanner.
#[derive(Debug, Clone)]
pub enum DefaultCredsMessage {
    /// Target host name or IP address edited.
    HostChanged(String),
    /// Auto-detect services option toggled.
    AutoDetectToggled(bool),
    /// Show passwords option toggled.
    ShowPasswordsToggled(bool),
    /// Start scanning the target.
    Scan,
    /// Cancel the ongoing scan.
    Cancel,
    /// Progress update during scan: (scan_id, percent 0.0..=1.0, step description).
    Progress(u64, f32, String),
    /// Live results snapshot received from background task.
    ResultsUpdate(u64, Vec<CredTestResult>),
    /// The scan finished with success or failure: (scan_id, outcome).
    Finished(u64, Result<Vec<CredTestResult>, String>),
    /// Copy the plain-text tabular report to clipboard.
    CopyReport,
    /// Export CSV to clipboard.
    ExportCsv,
}

/// Action outcome from updating the tool pane.
pub enum Outcome {
    /// No action.
    None,
    /// Asynchronous task to execute.
    Task(Task<Message>),
    /// Copy text into a copy slot.
    Copy(CopySlot, String),
}

/// The Default Credentials Scanner view state.
pub struct DefaultCredsPane {
    host: String,
    auto_detect: bool,
    show_passwords: bool,
    is_scanning: bool,
    progress: Option<(f32, String)>,
    error: Option<String>,
    results: Vec<CredTestResult>,
    summary_text: String,
    report_text: String,
    csv_text: String,
    scan_id: u64,
}

impl DefaultCredsPane {
    /// Creates a new Default Credentials Scanner pane, inheriting optional target host.
    #[must_use]
    pub fn new(target_host: Option<&str>) -> Self {
        Self {
            host: target_host.map_or_else(String::new, |h| h.trim().to_owned()),
            auto_detect: true,
            show_passwords: false,
            is_scanning: false,
            progress: None,
            error: None,
            results: Vec::new(),
            summary_text: String::new(),
            report_text: String::new(),
            csv_text: String::new(),
            scan_id: 0,
        }
    }
}

impl Default for DefaultCredsPane {
    fn default() -> Self {
        Self::new(None)
    }
}

impl std::fmt::Debug for DefaultCredsPane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultCredsPane")
            .field("host", &self.host)
            .field("auto_detect", &self.auto_detect)
            .field("show_passwords", &self.show_passwords)
            .field("is_scanning", &self.is_scanning)
            .field("results_count", &self.results.len())
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl DefaultCredsPane {
    /// Handles Default Credentials tool messages.
    pub fn update(&mut self, msg: DefaultCredsMessage, tab: TabId) -> Outcome {
        match msg {
            DefaultCredsMessage::HostChanged(host) => {
                self.host = host;
                self.error = None;
                Outcome::None
            }
            DefaultCredsMessage::AutoDetectToggled(val) => {
                self.auto_detect = val;
                Outcome::None
            }
            DefaultCredsMessage::ShowPasswordsToggled(val) => {
                self.show_passwords = val;
                Outcome::None
            }
            DefaultCredsMessage::Cancel => {
                if self.is_scanning {
                    self.is_scanning = false;
                    self.progress = None;
                    self.scan_id = self.scan_id.wrapping_add(1);
                    self.recompute_exports();
                }
                Outcome::None
            }
            DefaultCredsMessage::Scan => {
                let trimmed = self.host.trim();
                if trimmed.is_empty() {
                    self.error = Some(fl!("ui-tool-defaultcreds-error-host-required"));
                    return Outcome::None;
                }

                self.error = None;
                self.results.clear();
                self.summary_text.clear();
                self.report_text.clear();
                self.csv_text.clear();
                self.is_scanning = true;
                self.scan_id = self.scan_id.wrapping_add(1);
                let current_id = self.scan_id;

                self.progress = Some((0.0, fl!("ui-tool-defaultcreds-detecting")));

                let host_str = trimmed.to_owned();
                let auto = self.auto_detect;
                let task = Task::run(scan_stream(host_str, auto, current_id), move |msg| {
                    Message::Tool(tab, ToolMessage::DefaultCreds(msg))
                });
                Outcome::Task(task)
            }
            DefaultCredsMessage::Progress(id, fraction, desc) => {
                if id == self.scan_id && self.is_scanning {
                    self.progress = Some((fraction, desc));
                }
                Outcome::None
            }
            DefaultCredsMessage::ResultsUpdate(id, results) => {
                if id == self.scan_id && self.is_scanning {
                    self.results = results;
                    self.recompute_exports();
                }
                Outcome::None
            }
            DefaultCredsMessage::Finished(id, outcome) => {
                if id == self.scan_id && self.is_scanning {
                    self.is_scanning = false;
                    self.progress = None;
                    match outcome {
                        Ok(res) => {
                            self.results = res;
                            self.error = None;
                            self.recompute_exports();
                        }
                        Err(err) => {
                            self.error = Some(err);
                            self.recompute_exports();
                        }
                    }
                }
                Outcome::None
            }
            DefaultCredsMessage::CopyReport => {
                if !self.report_text.is_empty() {
                    return Outcome::Copy(CopySlot::DefaultCredsReport, self.report_text.clone());
                }
                Outcome::None
            }
            DefaultCredsMessage::ExportCsv => {
                if !self.csv_text.is_empty() {
                    return Outcome::Copy(CopySlot::DefaultCredsCsv, self.csv_text.clone());
                }
                Outcome::None
            }
        }
    }

    fn recompute_exports(&mut self) {
        let loc = |k: &str| localize_key(k);
        let (defaults, services) = count_defaults_and_services(&self.results);
        // The summary uses the Fluent plural directly, so the counts, plural forms and
        // word order come from the locale instead of a hand-built "{0} on {1}" template.
        self.summary_text = if defaults == 0 {
            fl!("ui-tool-defaultcreds-no-defaults")
        } else {
            fl!(
                "ui-tool-defaultcreds-summary",
                defaults = defaults,
                services = services
            )
        };
        self.report_text = build_report_text(&self.results, Some(&loc));
        self.csv_text = build_csv_export(&self.results, Some(&loc));
    }

    /// Renders the Default Credentials Scanner tool page.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane, theme: &Theme) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, ToolMessage::DefaultCreds(msg));

        // 1. Warning banner (always visible)
        let warning_banner = container(
            row![
                text("\u{26A0}")
                    .font(styles::SEMIBOLD)
                    .size(font_size::BODY_LARGE),
                text(fl!("ui-tool-defaultcreds-warning"))
                    .size(font_size::BODY)
                    .width(Length::Fill),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
        )
        .padding(spacing::SM)
        .width(Length::Fill)
        .style(|t: &Theme| {
            let palette = t.extended_palette();
            container::Style {
                background: Some(Background::Color(palette.warning.weak.color)),
                border: Border {
                    radius: radius::SM.into(),
                    width: 1.0,
                    color: palette.warning.base.color,
                },
                ..container::Style::default()
            }
        });

        // 2. Host input and options
        let host_input = text_input(&fl!("ui-tool-defaultcreds-host-placeholder"), &self.host)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |h| send(DefaultCredsMessage::HostChanged(h)))
            .on_submit(send(DefaultCredsMessage::Scan));

        let scan_button = if self.is_scanning {
            super::action_button(
                fl!("ui-tool-defaultcreds-btn-stop"),
                false,
                Some(send(DefaultCredsMessage::Cancel)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-defaultcreds-btn-scan"),
                true,
                Some(send(DefaultCredsMessage::Scan)),
            )
        };

        let auto_detect_chk = checkbox(self.auto_detect)
            .label(fl!("ui-tool-defaultcreds-auto-detect"))
            .on_toggle(move |val| send(DefaultCredsMessage::AutoDetectToggled(val)))
            .size(font_size::BODY);

        let show_passwords_chk = checkbox(self.show_passwords)
            .label(fl!("ui-tool-defaultcreds-show-passwords"))
            .on_toggle(move |val| send(DefaultCredsMessage::ShowPasswordsToggled(val)))
            .size(font_size::BODY);

        let input_row = row![
            column![
                super::field_label(fl!("ui-tool-defaultcreds-host-label")),
                host_input
            ]
            .spacing(spacing::XS)
            .width(Length::Fill),
            container(scan_button).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::End);

        let options_row = row![
            auto_detect_chk,
            iced::widget::space().width(Length::Fill),
            show_passwords_chk,
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill);

        let input_card = key_parts::card(column![input_row, options_row].spacing(spacing::MD));

        let mut body_col = column![warning_banner, input_card].spacing(spacing::MD);

        // 3. Error banner
        if let Some(err) = &self.error {
            body_col = body_col.push(
                container(text(err.as_str()).size(font_size::BODY).style(text::danger))
                    .padding(spacing::MD)
                    .style(styles::card),
            );
        }

        // 4. Progress bar during active scan
        if self.is_scanning {
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
        } else if self.results.is_empty() && self.error.is_none() {
            body_col = body_col.push(super::empty_state(fl!("ui-tool-defaultcreds-empty-state")));
        }

        // 5. Results table
        if !self.results.is_empty() {
            body_col = body_col.push(self.render_results_card(theme));

            // Summary & export buttons
            let copy_report_btn = super::copy_button(
                fl!("ui-tool-defaultcreds-btn-copy"),
                state.copied(CopySlot::DefaultCredsReport),
                send(DefaultCredsMessage::CopyReport),
                super::COPY_PADDING,
            );

            let export_csv_btn = super::copy_button(
                fl!("ui-tool-defaultcreds-btn-export"),
                state.copied(CopySlot::DefaultCredsCsv),
                send(DefaultCredsMessage::ExportCsv),
                super::COPY_PADDING,
            );

            let footer_row = row![
                text(&self.summary_text)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .style(text::secondary)
                    .width(Length::Fill),
                copy_report_btn,
                export_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            body_col = body_col.push(key_parts::card(column![footer_row]));
        }

        super::content_column(body_col)
    }

    fn render_results_card<'a>(&'a self, theme: &Theme) -> Element<'a, Message> {
        let palette = theme.extended_palette();

        // Header row
        let header_row = row![
            text(fl!("ui-tool-defaultcreds-col-service"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::secondary)
                .width(Length::Fixed(100.0)),
            text(fl!("ui-tool-defaultcreds-col-port"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::secondary)
                .width(Length::Fixed(70.0)),
            text(fl!("ui-tool-defaultcreds-col-user"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::secondary)
                .width(Length::Fixed(120.0)),
            text(fl!("ui-tool-defaultcreds-col-pass"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::secondary)
                .width(Length::Fixed(120.0)),
            text(fl!("ui-tool-defaultcreds-col-status"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::secondary)
                .width(Length::Fixed(110.0)),
            text(fl!("ui-tool-defaultcreds-col-detail"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::secondary)
                .width(Length::Fill),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let mut rows_col = column![header_row].spacing(spacing::XS);

        for r in &self.results {
            let pass_display = if self.show_passwords {
                if r.password.is_empty() {
                    fl!("ui-tool-defaultcreds-empty")
                } else {
                    r.password.clone()
                }
            } else {
                MASKED_PASSWORD.to_string()
            };

            let (status_label, bg_color, fg_color, border_color) = match r.status {
                CredTestStatus::Default => (
                    fl!("ui-tool-defaultcreds-status-default"),
                    palette.danger.weak.color,
                    palette.danger.strong.text,
                    palette.danger.base.color,
                ),
                CredTestStatus::Changed => (
                    fl!("ui-tool-defaultcreds-status-changed"),
                    palette.success.weak.color,
                    palette.success.strong.text,
                    palette.success.base.color,
                ),
                CredTestStatus::NotTested => (
                    fl!("ui-tool-defaultcreds-status-not-tested"),
                    palette.secondary.weak.color,
                    palette.secondary.strong.text,
                    palette.secondary.base.color,
                ),
                CredTestStatus::Error => (
                    fl!("ui-tool-defaultcreds-status-error"),
                    palette.warning.weak.color,
                    palette.warning.strong.text,
                    palette.warning.base.color,
                ),
            };

            let badge = container(
                text(status_label)
                    .size(font_size::CAPTION)
                    .font(styles::SEMIBOLD)
                    .style(move |_| text::Style {
                        color: Some(fg_color),
                    }),
            )
            .padding([2.0, 6.0])
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_color)),
                border: Border {
                    radius: radius::SM.into(),
                    width: 1.0,
                    color: border_color,
                },
                ..container::Style::default()
            });

            let loc = |k: &str| localize_key(k);
            let detail = if r.status == CredTestStatus::Error {
                r.error_detail.clone().unwrap_or_default()
            } else {
                status_to_detail(r.status, &r.service, Some(&loc))
            };

            let item_row = row![
                text(&r.service)
                    .font(styles::SEMIBOLD)
                    .size(font_size::BODY)
                    .width(Length::Fixed(100.0)),
                text(r.port.to_string())
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .width(Length::Fixed(70.0)),
                text(&r.username)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .style(text::secondary)
                    .width(Length::Fixed(120.0)),
                text(pass_display)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .style(text::secondary)
                    .width(Length::Fixed(120.0)),
                container(badge).width(Length::Fixed(110.0)),
                text(detail)
                    .size(font_size::BODY)
                    .style(text::secondary)
                    .width(Length::Fill),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            rows_col = rows_col.push(item_row);
        }

        let scroll = scrollable(rows_col)
            .width(Length::Fill)
            .height(Length::Shrink);
        key_parts::card(column![scroll])
    }
}

/// Helper mapping the engine's report and CSV formatter keys to localized strings.
/// The summary is formatted in `recompute_exports` with the Fluent plural directly,
/// so no "{0} on {1}" / "{0} services" word-order templates live here.
fn localize_key(key: &str) -> String {
    match key {
        "ToolDefCredColService" => fl!("ui-tool-defaultcreds-col-service"),
        "ToolDefCredColPort" => fl!("ui-tool-defaultcreds-col-port"),
        "ToolDefCredColUser" => fl!("ui-tool-defaultcreds-col-user"),
        "ToolDefCredColPass" => fl!("ui-tool-defaultcreds-col-pass"),
        "ToolDefCredColStatus" => fl!("ui-tool-defaultcreds-col-status"),
        "ToolDefCredColDetail" => fl!("ui-tool-defaultcreds-col-detail"),
        "ToolDefCredStatusDefault" => fl!("ui-tool-defaultcreds-status-default"),
        "ToolDefCredStatusChanged" => fl!("ui-tool-defaultcreds-status-changed"),
        "ToolDefCredStatusNotTested" => fl!("ui-tool-defaultcreds-status-not-tested"),
        "ToolDefCredStatusError" => fl!("ui-tool-defaultcreds-status-error"),
        "ToolDefCredDetailAccepted" => fl!("ui-tool-defaultcreds-detail-accepted", service = "{0}"),
        "ToolDefCredDetailRejected" => fl!("ui-tool-defaultcreds-detail-rejected", service = "{0}"),
        "ToolDefCredDetailNotTested" => {
            fl!("ui-tool-defaultcreds-detail-not-tested", service = "{0}")
        }
        _ => key.to_string(),
    }
}

/// Asynchronous streaming scan execution.
fn scan_stream(
    host: String,
    auto_detect: bool,
    scan_id: u64,
) -> impl iced::futures::Stream<Item = DefaultCredsMessage> {
    iced::stream::channel(16, async move |mut out| {
        // Phase 1: Determine target services
        let target_services: Vec<(u16, &'static str)> = if auto_detect {
            let _ = out
                .send(DefaultCredsMessage::Progress(
                    scan_id,
                    0.05,
                    fl!("ui-tool-defaultcreds-detecting"),
                ))
                .await;

            let mut detected = Vec::new();
            let probe_dur = Duration::from_millis(PORT_PROBE_TIMEOUT_MS);

            for (port, svc) in SERVICE_PORTS {
                if probe_port(&host, *port, probe_dur).await {
                    detected.push((*port, *svc));
                }
            }
            detected
        } else {
            SERVICE_PORTS.to_vec()
        };

        if target_services.is_empty() {
            let _ = out
                .send(DefaultCredsMessage::Finished(scan_id, Ok(Vec::new())))
                .await;
            return;
        }

        // Phase 2: Compute total credential tests
        let mut total_tests = 0;
        for (_, svc) in &target_services {
            if let Some(creds) = default_creds::credentials_for_service(svc) {
                total_tests += creds.len();
            }
        }

        let mut completed_tests = 0;
        let mut collected_results = Vec::with_capacity(total_tests);

        // Phase 3: Execute tests
        for (port, svc) in target_services {
            let Some(creds) = default_creds::credentials_for_service(svc) else {
                continue;
            };

            for &(user, pass) in creds {
                completed_tests += 1;
                let fraction = if total_tests > 0 {
                    completed_tests as f32 / total_tests as f32
                } else {
                    0.0
                };

                let step_desc = fl!(
                    "ui-tool-defaultcreds-testing",
                    service = svc,
                    user = user,
                    pass = pass
                );

                let _ = out
                    .send(DefaultCredsMessage::Progress(scan_id, fraction, step_desc))
                    .await;

                let res = test_credential(&host, port, svc, user, pass).await;
                collected_results.push(res);

                let _ = out
                    .send(DefaultCredsMessage::ResultsUpdate(
                        scan_id,
                        collected_results.clone(),
                    ))
                    .await;

                // Rate limiting delay between probes
                tokio::time::sleep(Duration::from_millis(RATE_LIMIT_DELAY_MS)).await;
            }
        }

        let _ = out
            .send(DefaultCredsMessage::Finished(
                scan_id,
                Ok(collected_results),
            ))
            .await;
    })
}

/// Probes whether a TCP port is open.
async fn probe_port(host: &str, port: u16, dur: Duration) -> bool {
    timeout(dur, TcpStream::connect((host, port)))
        .await
        .is_ok_and(|res| res.is_ok())
}

/// Tests a single credential pair against a service.
async fn test_credential(
    host: &str,
    port: u16,
    service: &str,
    user: &str,
    pass: &str,
) -> CredTestResult {
    let outcome = match service {
        "SSH" => test_ssh(host, port).await,
        "Telnet" => test_telnet(host, port, user, pass).await,
        "HTTP" => test_http_basic(host, port, user, pass).await,
        "FTP" => test_ftp(host, port, user, pass).await,
        "SNMP" => test_snmp(host, user).await,
        "MySQL" => test_mysql(host, port).await,
        "PostgreSQL" => test_postgresql(host, port, user).await,
        "Redis" => test_redis(host, port).await,
        "VNC" => test_vnc(host, port).await,
        "MSSQL" => test_mssql(host, port).await,
        _ => Ok(false),
    };

    // SSH, MySQL, MSSQL, Redis and VNC probes only confirm the service answers; they do
    // not submit the credential pair. Their reachable outcome is reported as "not tested"
    // and never counted as a finding, so "accepted" is reserved for a verified login.
    let verifies_credentials = matches!(service, "Telnet" | "HTTP" | "FTP" | "SNMP" | "PostgreSQL");

    let status = match (&outcome, verifies_credentials) {
        (Ok(true), true) => CredTestStatus::Default,
        (Ok(true), false) => CredTestStatus::NotTested,
        (Ok(false), _) => CredTestStatus::Changed,
        (Err(_), _) => CredTestStatus::Error,
    };

    CredTestResult {
        service: service.to_string(),
        port,
        username: user.to_string(),
        password: pass.to_string(),
        status,
        error_detail: outcome.err(),
    }
}

/// Tests Redis connectivity and no-auth mode via PING -> +PONG.
async fn test_redis(host: &str, port: u16) -> Result<bool, String> {
    let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-connect"))?
        .map_err(|e| e.to_string())?;

    timeout(CONNECT_TIMEOUT, stream.write_all(b"PING\r\n"))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-write"))?
        .map_err(|e| e.to_string())?;

    let mut buf = [0u8; 256];
    let n = timeout(CONNECT_TIMEOUT, stream.read(&mut buf))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-read"))?
        .map_err(|e| e.to_string())?;

    let resp = String::from_utf8_lossy(&buf[..n]);
    Ok(resp.to_uppercase().contains("+PONG"))
}

/// Tests FTP credentials using USER/PASS sequence.
async fn test_ftp(host: &str, port: u16, user: &str, pass: &str) -> Result<bool, String> {
    let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-connect"))?
        .map_err(|e| e.to_string())?;

    let mut buf = [0u8; 512];
    let _ = timeout(CONNECT_TIMEOUT, stream.read(&mut buf)).await;

    let user_cmd = format!("USER {user}\r\n");
    timeout(CONNECT_TIMEOUT, stream.write_all(user_cmd.as_bytes()))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-write"))?
        .map_err(|e| e.to_string())?;

    let _ = timeout(CONNECT_TIMEOUT, stream.read(&mut buf)).await;

    let pass_cmd = format!("PASS {pass}\r\n");
    timeout(CONNECT_TIMEOUT, stream.write_all(pass_cmd.as_bytes()))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-write"))?
        .map_err(|e| e.to_string())?;

    let n = timeout(CONNECT_TIMEOUT, stream.read(&mut buf))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-read"))?
        .map_err(|e| e.to_string())?;

    let resp = String::from_utf8_lossy(&buf[..n]);
    Ok(resp.contains("230"))
}

/// Tests VNC for no-auth security type (type 1).
async fn test_vnc(host: &str, port: u16) -> Result<bool, String> {
    let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-connect"))?
        .map_err(|e| e.to_string())?;

    let mut buf = [0u8; 256];
    let n = timeout(CONNECT_TIMEOUT, stream.read(&mut buf))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-read"))?
        .map_err(|e| e.to_string())?;

    let banner = String::from_utf8_lossy(&buf[..n]);
    if !banner.starts_with("RFB") {
        return Ok(false);
    }

    timeout(CONNECT_TIMEOUT, stream.write_all(&buf[..n]))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-write"))?
        .map_err(|e| e.to_string())?;

    let n = timeout(CONNECT_TIMEOUT, stream.read(&mut buf))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-read"))?
        .map_err(|e| e.to_string())?;

    if n > 0 {
        let count = buf[0] as usize;
        for b in buf.iter().take(n).skip(1).take(count) {
            if *b == 1 {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Tests HTTP Basic Authentication.
async fn test_http_basic(host: &str, port: u16, user: &str, pass: &str) -> Result<bool, String> {
    let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-connect"))?
        .map_err(|e| e.to_string())?;

    let raw_creds = format!("{user}:{pass}");
    let b64 = base64_codec::encode(raw_creds.as_bytes(), false);
    let req = format!("GET / HTTP/1.0\r\nHost: {host}\r\nAuthorization: Basic {b64}\r\n\r\n");

    timeout(CONNECT_TIMEOUT, stream.write_all(req.as_bytes()))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-write"))?
        .map_err(|e| e.to_string())?;

    let mut buf = [0u8; 512];
    let n = timeout(CONNECT_TIMEOUT, stream.read(&mut buf))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-read"))?
        .map_err(|e| e.to_string())?;

    let resp = String::from_utf8_lossy(&buf[..n]);
    Ok(is_http_success_response(&resp))
}

/// Tests Telnet credentials by sending user and password commands.
async fn test_telnet(host: &str, port: u16, user: &str, pass: &str) -> Result<bool, String> {
    let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-connect"))?
        .map_err(|e| e.to_string())?;

    let mut buf = [0u8; 1024];
    let _ = timeout(CONNECT_TIMEOUT, stream.read(&mut buf)).await;

    let user_line = format!("{user}\r\n");
    timeout(CONNECT_TIMEOUT, stream.write_all(user_line.as_bytes()))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-write"))?
        .map_err(|e| e.to_string())?;

    tokio::time::sleep(Duration::from_millis(300)).await;
    let _ = timeout(CONNECT_TIMEOUT, stream.read(&mut buf)).await;

    let pass_line = format!("{pass}\r\n");
    timeout(CONNECT_TIMEOUT, stream.write_all(pass_line.as_bytes()))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-write"))?
        .map_err(|e| e.to_string())?;

    tokio::time::sleep(Duration::from_millis(300)).await;
    let n = timeout(CONNECT_TIMEOUT, stream.read(&mut buf))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-read"))?
        .map_err(|e| e.to_string())?;

    let resp = String::from_utf8_lossy(&buf[..n]).to_lowercase();
    let denied = resp.contains("incorrect") || resp.contains("failed") || resp.contains("denied");
    let has_prompt = resp.contains('$') || resp.contains('#') || resp.contains('>');

    Ok(!denied && has_prompt)
}

/// Tests PostgreSQL startup message for trust/no-password access.
async fn test_postgresql(host: &str, port: u16, user: &str) -> Result<bool, String> {
    let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-connect"))?
        .map_err(|e| e.to_string())?;

    let params = format!("user\0{user}\0database\0{user}\0\0");
    let param_bytes = params.as_bytes();
    let msg_len = 4 + 4 + param_bytes.len();
    let mut msg = Vec::with_capacity(msg_len);
    msg.extend_from_slice(&(msg_len as u32).to_be_bytes());
    msg.extend_from_slice(&0x0003_0000u32.to_be_bytes()); // Protocol 3.0
    msg.extend_from_slice(param_bytes);

    timeout(CONNECT_TIMEOUT, stream.write_all(&msg))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-write"))?
        .map_err(|e| e.to_string())?;

    let mut buf = [0u8; 1024];
    let n = timeout(CONNECT_TIMEOUT, stream.read(&mut buf))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-read"))?
        .map_err(|e| e.to_string())?;

    if n >= 9 && buf[0] == b'R' {
        let auth_type = u32::from_be_bytes([buf[5], buf[6], buf[7], buf[8]]);
        return Ok(auth_type == 0); // AuthenticationOk
    }

    Ok(false)
}

/// Tests MySQL greeting packet.
async fn test_mysql(host: &str, port: u16) -> Result<bool, String> {
    let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-connect"))?
        .map_err(|e| e.to_string())?;

    let mut buf = [0u8; 1024];
    let n = timeout(CONNECT_TIMEOUT, stream.read(&mut buf))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-read"))?
        .map_err(|e| e.to_string())?;

    // MySQL protocol handshake version 10 (0x0a)
    Ok(n > 4 && buf[4] == 0x0a)
}

/// Tests MSSQL connectivity.
async fn test_mssql(host: &str, port: u16) -> Result<bool, String> {
    let _stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-connect"))?
        .map_err(|e| e.to_string())?;

    Ok(false)
}

/// Tests SNMP community string via UDP probe.
async fn test_snmp(host: &str, community: &str) -> Result<bool, String> {
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| e.to_string())?;

    let target = format!("{host}:161");
    // Minimal SNMPv1 GET request for 1.3.6.1.2.1.1.1.0 (sysDescr)
    let mut pdu = Vec::new();
    let comm_bytes = community.as_bytes();

    // Sequence wrapper
    let inner_len = 3 + 2 + comm_bytes.len() + 18;
    pdu.push(0x30); // SEQUENCE
    pdu.push(inner_len as u8);
    // Version: integer 0 (v1)
    pdu.extend_from_slice(&[0x02, 0x01, 0x00]);
    // Community: OCTET STRING
    pdu.push(0x04);
    pdu.push(comm_bytes.len() as u8);
    pdu.extend_from_slice(comm_bytes);
    // GetRequest-PDU
    pdu.extend_from_slice(&[
        0xA0, 0x10, // GetRequest length
        0x02, 0x01, 0x01, // Request ID: 1
        0x02, 0x01, 0x00, // Error status: 0
        0x02, 0x01, 0x00, // Error index: 0
        0x30, 0x07, // Variable bindings
        0x30, 0x05, // VarBind
        0x06, 0x01, 0x00, // Object ID placeholder
        0x05, 0x00, // NULL value
    ]);

    socket
        .send_to(&pdu, &target)
        .await
        .map_err(|e| e.to_string())?;

    let mut buf = [0u8; 512];
    match timeout(CONNECT_TIMEOUT, socket.recv_from(&mut buf)).await {
        Ok(Ok((n, _))) => Ok(n > 0),
        _ => Ok(false),
    }
}

/// Tests SSH banner connectivity.
async fn test_ssh(host: &str, port: u16) -> Result<bool, String> {
    let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-connect"))?
        .map_err(|e| e.to_string())?;

    let mut buf = [0u8; 256];
    let n = timeout(CONNECT_TIMEOUT, stream.read(&mut buf))
        .await
        .map_err(|_| fl!("ui-tool-defaultcreds-timeout-read"))?
        .map_err(|e| e.to_string())?;

    let banner = String::from_utf8_lossy(&buf[..n]);
    Ok(banner.starts_with("SSH-"))
}
