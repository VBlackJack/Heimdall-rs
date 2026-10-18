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

//! The SSL/TLS Certificate Inspector tool view: remote certificate inspection,
//! expiration status badges, hostname match verification, SANs display, and CSV export.
//!
//! The certificates are read by a real TLS handshake, 1.2 or 1.3, as the C#
//! `RetrieveCertificateAsync`'s `SslStream`: the chain the server presents, the version
//! negotiated, every port probed [`MAX_CONCURRENT_PROBES`] at a time and stopped for real.
//!
//! A server that shares nothing with that handshake, TLS 1.0 or 1.1 alone or TLS 1.2
//! suites rustls refuses, is read again in clear (owner decision of 2026-10-17, as the C#
//! `SslStream` reads it where the system still allows those versions): its certificates
//! as its answer to a legacy hello presents them, shown under a warning that nothing
//! proves the server holds their key, with the way to the TLS Auditor. Never trusted.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use heimdall_app::tools::{ToolContext, ToolId};
use heimdall_app::{Message as AppMessage, TabId, ToolsMessage};
use iced::futures::{SinkExt as _, StreamExt as _};
use iced::widget::{Column, column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::banner_grab::clean_banner;
use heimdall_core::tools::cert_inspector::{
    self, CertExpirationStatus, CertInspectionSummary, DEFAULT_PORT, EXTENDED_SCAN_PORTS,
    MAX_CONCURRENT_PROBES, MAX_SCAN_PORTS, PROBE_TIMEOUT_MS, QUICK_SCAN_PORTS,
};
use heimdall_core::tools::legacy_tls::{
    self, HelloShape, LegacyHandshake, LegacyReader, LegacyTlsError,
};
use heimdall_core::tools::net_host::checked_host;
use heimdall_core::tools::tls_audit::TlsProtocol;

use super::file_export::FileExport;
use super::network_run::RunSlot;
use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// What the CSV export's suggested name starts with, as the C# `certscan_{host}_{now}.csv`.
const EXPORT_PREFIX: &str = "certscan";

/// Messages a probe stream holds before it waits for the window.
const STREAM_BUFFER: usize = 16;
/// The most characters of a failure's own words shown: what a server or the system says
/// is bounded.
const MAX_DETAIL_CHARS: usize = 200;
/// The width of the labels of a certificate's details.
const LABEL_WIDTH: f32 = 140.0;
/// Bytes read from a legacy TLS answer at a time; the whole answer is bounded by
/// [`legacy_tls::MAX_ANSWER_BYTES`].
const LEGACY_READ_CHUNK: usize = 4096;

/// Why a port's certificate could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// No connection: refused, unreachable, the name unknown. The system's words.
    Connect(String),
    /// The host is neither a DNS name nor an address the handshake can name.
    Name,
    /// The TLS handshake failed: no TLS there, or a cipher not spoken. Its words.
    Handshake(String),
    /// The server speaks only a TLS version older than 1.2, and its answer to the legacy
    /// hello gave no certificate: why, said plainly rather than as rustls words its
    /// refusal.
    Legacy(LegacyTlsError),
    /// The server presented no certificate.
    NoCertificate,
    /// The certificate presented does not decode.
    Unreadable,
    /// Past [`PROBE_TIMEOUT_MS`].
    Timeout,
}

/// Messages emitted by the certificate inspector view.
#[derive(Debug, Clone)]
pub enum CertInspectorMessage {
    /// Host input changed.
    HostChanged(String),
    /// Ports input changed.
    PortsChanged(String),
    /// A scan profile chosen: its ports fill the Ports box, as the C# profile buttons.
    ProfileChosen(ScanProfile),
    /// Inspect target certificates.
    Inspect,
    /// Stop the inspection, as the C# `Cancel`.
    Stop,
    /// Run `.0` probed port `.1`.
    Probed(u64, u16, Result<Box<CertInspectionSummary>, ProbeError>),
    /// Run `.0` probed every port.
    Done(u64),
    /// Copy plain text details.
    CopyDetails,
    /// Save the CSV export to a file, as the C# `OnExportCsvClick`.
    ExportCsv,
}

/// A preset list of ports to scan, as the C# `quick` and `extended` profiles
/// (`CertInspectorViewModel.GetScanPorts`); typing the ports is the `custom` one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanProfile {
    Quick,
    Extended,
}

impl ScanProfile {
    /// The ports of the profile.
    fn ports(self) -> &'static [u16] {
        match self {
            Self::Quick => &QUICK_SCAN_PORTS,
            Self::Extended => &EXTENDED_SCAN_PORTS,
        }
    }

    /// The profile's button label, with its port count as the C# tooltip says it.
    fn label(self) -> String {
        let count = self.ports().len();
        match self {
            Self::Quick => fl!("ui-tool-cert-profile-quick", count = count),
            Self::Extended => fl!("ui-tool-cert-profile-extended", count = count),
        }
    }
}

/// The SSL/TLS Certificate Inspector tool pane state.
#[derive(Debug)]
pub struct CertInspectorPane {
    host: String,
    ports: String,
    run: RunSlot,
    /// The host of the run shown, as its results were read for it.
    inspected_host: String,
    /// Ports the run shown probes.
    total: usize,
    /// Ports it probed so far.
    probed: usize,
    error: Option<String>,
    /// What the run shown found, said when it is over.
    notice: Option<String>,
    /// The certificates read, by port.
    results: Vec<CertInspectionSummary>,
    /// The ports whose certificate could not be read, and why.
    failures: Vec<(u16, ProbeError)>,
}

impl Default for CertInspectorPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            ports: DEFAULT_PORT.to_string(),
            run: RunSlot::default(),
            inspected_host: String::new(),
            total: 0,
            probed: 0,
            error: None,
            notice: None,
            results: Vec::new(),
            failures: Vec::new(),
        }
    }
}

/// Outcome of updating the Certificate Inspector tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
    /// Save this export to the file the user picks.
    Save(FileExport),
}

/// What a message asks of the pane, before its tab is known.
#[derive(Debug)]
enum Step {
    None,
    /// Probe `.1`'s ports `.2` as run `.0`.
    Probe(u64, String, Vec<u16>),
    Copy(CopySlot, String),
    /// Save this export to the file the user picks.
    Save(FileExport),
}

impl CertInspectorPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: CertInspectorMessage) -> Outcome {
        match self.apply(message) {
            Step::None => Outcome::None,
            Step::Copy(slot, content) => Outcome::Copy(slot, content),
            Step::Save(export) => Outcome::Save(export),
            Step::Probe(run, host, ports) => {
                let task = Task::run(probes(run, host, ports), move |message| {
                    Message::Tool(tab, super::ToolMessage::Cert(message))
                });
                Outcome::Task(self.run.attach(task))
            }
        }
    }

    /// What `message` changes, and what it asks.
    fn apply(&mut self, message: CertInspectorMessage) -> Step {
        match message {
            CertInspectorMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
                Step::None
            }
            CertInspectorMessage::ProfileChosen(profile) => {
                self.ports = cert_inspector::ports_text(profile.ports());
                Step::None
            }
            CertInspectorMessage::PortsChanged(val) => {
                self.ports = val;
                self.error = None;
                Step::None
            }
            CertInspectorMessage::Inspect => self.inspect(),
            CertInspectorMessage::Stop => {
                if self.run.running() {
                    self.run.stop();
                    self.notice = Some(self.found_text(true));
                }
                Step::None
            }
            CertInspectorMessage::Probed(run, port, result) => {
                if !self.run.is_current(run) {
                    return Step::None;
                }
                self.probed += 1;
                match result {
                    Ok(summary) => {
                        let at = self.results.partition_point(|shown| shown.port < port);
                        self.results.insert(at, *summary);
                    }
                    Err(error) => self.failures.push((port, error)),
                }
                Step::None
            }
            CertInspectorMessage::Done(run) => {
                if self.run.finish(run) {
                    self.done();
                }
                Step::None
            }
            CertInspectorMessage::CopyDetails => {
                if self.results.is_empty() {
                    return Step::None;
                }
                let details: Vec<String> = self
                    .results
                    .iter()
                    .map(|summary| details_text(summary, &self.inspected_host))
                    .collect();
                Step::Copy(CopySlot::CertDetails, details.join("\n"))
            }
            CertInspectorMessage::ExportCsv => {
                if self.results.is_empty() {
                    return Step::None;
                }
                let header = [
                    fl!("ui-tool-cert-col-port"),
                    fl!("ui-tool-cert-subject"),
                    fl!("ui-tool-cert-issuer"),
                    fl!("ui-tool-cert-valid-from"),
                    fl!("ui-tool-cert-valid-to"),
                    fl!("ui-tool-cert-days-remaining"),
                    fl!("ui-tool-cert-tls-version"),
                    fl!("ui-tool-cert-key-size"),
                    fl!("ui-tool-cert-hostname-match"),
                    fl!("ui-tool-cert-sans"),
                    fl!("ui-tool-cert-col-read"),
                ];
                let (yes, no) = (fl!("ui-tool-cert-match-yes"), fl!("ui-tool-cert-match-no"));
                let csv = cert_inspector::build_scan_csv(
                    &self.results,
                    header.each_ref().map(String::as_str),
                    (&yes, &no),
                    read_text,
                );
                Step::Save(FileExport::csv(
                    CopySlot::CertCsv,
                    fl!("ui-tool-cert-export-csv"),
                    EXPORT_PREFIX,
                    Some(&self.inspected_host),
                    csv,
                ))
            }
        }
    }

    /// An inspection begun, as the C# `CheckAsync`: one at a time, the host checked, the
    /// ports parsed and bounded.
    fn inspect(&mut self) -> Step {
        if self.run.running() {
            return Step::None;
        }
        let host = self.host.trim().to_owned();
        if host.is_empty() {
            self.error = Some(fl!("ui-tool-cert-error-host-required"));
            return Step::None;
        }
        let Some(host) = checked_host(&host) else {
            self.error = Some(fl!("ui-tool-cert-error-invalid-host"));
            return Step::None;
        };
        let ports = if self.ports.trim().is_empty() {
            vec![DEFAULT_PORT]
        } else {
            cert_inspector::parse_ports(&self.ports)
        };
        if ports.is_empty() {
            self.error = Some(fl!("ui-tool-cert-error-invalid-port"));
            return Step::None;
        }
        if ports.len() > MAX_SCAN_PORTS {
            self.error = Some(fl!(
                "ui-tool-cert-error-too-many-ports",
                count = ports.len(),
                max = MAX_SCAN_PORTS
            ));
            return Step::None;
        }
        self.error = None;
        self.notice = None;
        self.results.clear();
        self.failures.clear();
        self.total = ports.len();
        self.probed = 0;
        self.inspected_host.clone_from(&host);
        let (run, _) = self.run.begin();
        Step::Probe(run, host, ports)
    }

    /// The run is over: a single port's failure said, as the C# single check; a scan's
    /// failures skipped and what it found counted, as the C# scan.
    fn done(&mut self) {
        if self.total == 1
            && let [(port, error)] = self.failures.as_slice()
        {
            self.error = Some(failure_text(&self.inspected_host, *port, error));
            return;
        }
        self.notice = Some(self.found_text(false));
    }

    /// What the run found: the certificates read on the ports probed, `stopped` or not.
    fn found_text(&self, stopped: bool) -> String {
        let found = fl!(
            "ui-tool-cert-scan-found",
            certificates = self.results.len(),
            ports = self.probed
        );
        if stopped {
            fl!("ui-tool-cert-stopped", found = found)
        } else {
            found
        }
    }

    /// Renders the Certificate Inspector tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::Cert(msg));

        let host_input = text_input(&fl!("ui-tool-cert-host-watermark"), &self.host)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(CertInspectorMessage::HostChanged(v)))
            .on_submit(send(CertInspectorMessage::Inspect))
            .width(Length::FillPortion(3));

        let ports_input = text_input(&fl!("ui-tool-cert-ports-watermark"), &self.ports)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(CertInspectorMessage::PortsChanged(v)))
            .on_submit(send(CertInspectorMessage::Inspect))
            .width(Length::FillPortion(1));

        let inspect_button = if self.run.running() {
            super::action_button(
                fl!("ui-tool-cert-stop"),
                true,
                Some(send(CertInspectorMessage::Stop)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-cert-inspect"),
                true,
                Some(send(CertInspectorMessage::Inspect)),
            )
        };

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-cert-host")), host_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(3)),
            column![super::field_label(fl!("ui-tool-cert-ports")), ports_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(1)),
            container(inspect_button).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::End);

        let profiles_row = row![
            super::field_label(fl!("ui-tool-cert-profiles")),
            super::action_button(
                ScanProfile::Quick.label(),
                false,
                Some(send(CertInspectorMessage::ProfileChosen(
                    ScanProfile::Quick
                ))),
            ),
            super::action_button(
                ScanProfile::Extended.label(),
                false,
                Some(send(CertInspectorMessage::ProfileChosen(
                    ScanProfile::Extended
                ))),
            ),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let input_card = container(column![inputs_row, profiles_row].spacing(spacing::SM))
            .padding(spacing::MD)
            .style(styles::card);

        let mut body = column![input_card].spacing(spacing::MD);

        if self.run.running() {
            body = body.push(
                text(fl!(
                    "ui-tool-cert-progress",
                    done = self.probed,
                    total = self.total
                ))
                .size(font_size::BODY)
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
        if let Some(notice) = &self.notice {
            body = body.push(
                text(notice.as_str())
                    .size(font_size::BODY)
                    .style(text::secondary),
            );
        }

        if !self.results.is_empty() {
            let copy_details_btn = super::copy_button(
                fl!("ui-tool-cert-copy-details"),
                state.copied(CopySlot::CertDetails),
                send(CertInspectorMessage::CopyDetails),
                super::COPY_PADDING,
            );
            let export_csv_btn = super::export_button(
                fl!("ui-tool-cert-export-csv"),
                state.copied(CopySlot::CertCsv),
                send(CertInspectorMessage::ExportCsv),
            );
            body = body.push(
                row![
                    iced::widget::space::horizontal(),
                    copy_details_btn,
                    export_csv_btn
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center),
            );
        }

        for r in &self.results {
            body = body.push(certificate_card(r, &self.inspected_host));
        }

        super::content_column(body)
    }
}

/// A badge of `label` in `style`.
fn badge<'a>(label: String, style: fn(&Theme) -> text::Style) -> Element<'a, Message> {
    container(text(label).size(font_size::CAPTION).style(style))
        .padding([2.0, 6.0])
        .style(styles::card)
        .into()
}

/// A labelled line of a certificate's details.
fn detail_row<'a>(label: String, value: String) -> Element<'a, Message> {
    row![
        text(label)
            .size(font_size::CAPTION)
            .style(text::secondary)
            .width(Length::Fixed(LABEL_WIDTH)),
        text(value).size(font_size::BODY).font(super::BOX_FONT),
    ]
    .spacing(spacing::SM)
    .into()
}

/// The style of a badge that says something good of a certificate: the success colour,
/// but neutral for a certificate read in clear, whose good dates or name prove nothing.
fn good(r: &CertInspectionSummary) -> fn(&Theme) -> text::Style {
    if r.legacy.is_some() {
        text::secondary
    } else {
        text::success
    }
}

/// What opens the TLS Auditor on `host`:`port`, through the tool context of the palette
/// and the sidebar: its boxes filled, nothing started.
fn audit_message(host: &str, port: u16) -> Message {
    let context = ToolContext {
        host: Some(host.to_owned()),
        port: Some(port),
        argument: None,
    };
    Message::App(AppMessage::Tools(ToolsMessage::Open {
        tool: ToolId::TlsAudit,
        title: super::tab_title(ToolId::TlsAudit, &context),
        context,
    }))
}

/// The warning over a certificate read in clear from a legacy TLS answer: the version
/// and suite, that nothing of it is trusted, and the TLS Auditor on the same port.
fn legacy_banner<'a>(
    r: &CertInspectionSummary,
    cipher_suite: &str,
    host: &str,
) -> Element<'a, Message> {
    container(
        column![
            text(fl!(
                "ui-tool-cert-legacy-title",
                version = r.tls_version.as_str(),
                cipher = cipher_suite
            ))
            .size(font_size::BODY)
            .style(text::warning),
            text(fl!("ui-tool-cert-legacy-warning")).size(font_size::BODY),
            super::action_button(
                fl!("ui-tool-cert-legacy-audit"),
                false,
                Some(audit_message(host, r.port)),
            ),
        ]
        .spacing(spacing::SM),
    )
    .padding(spacing::MD)
    .width(Length::Fill)
    .style(styles::warning_badge)
    .into()
}

/// The card of one port's certificate, read from `host`.
fn certificate_card<'a>(r: &CertInspectionSummary, host: &str) -> Element<'a, Message> {
    let status_badge = match r.expiration_status {
        CertExpirationStatus::Valid => badge(fl!("ui-tool-cert-status-valid"), good(r)),
        CertExpirationStatus::Warning => badge(fl!("ui-tool-cert-status-warning"), text::warning),
        CertExpirationStatus::Expired => badge(fl!("ui-tool-cert-status-expired"), text::danger),
    };
    let match_badge = if r.hostname_matches {
        badge(fl!("ui-tool-cert-match-yes"), good(r))
    } else {
        badge(fl!("ui-tool-cert-match-no"), text::danger)
    };
    let header_row = row![
        text(fl!(
            "ui-tool-cert-port-title",
            port = r.port,
            name = cert_inspector::extract_cn(&r.subject)
        ))
        .size(font_size::SUBTITLE)
        .width(Length::Fill),
        status_badge,
        match_badge,
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);

    let chain: Vec<String> = r
        .chain
        .iter()
        .map(|element| {
            fl!(
                "ui-tool-cert-chain-element",
                subject = element.subject.as_str(),
                expiry = element.expiry.as_str()
            )
        })
        .collect();
    let mut details: Column<'a, Message> = column![
        detail_row(fl!("ui-tool-cert-subject"), r.subject.clone()),
        detail_row(fl!("ui-tool-cert-issuer"), r.issuer.clone()),
        detail_row(fl!("ui-tool-cert-valid-from"), r.not_before.clone()),
        detail_row(fl!("ui-tool-cert-valid-to"), valid_to(r)),
        detail_row(fl!("ui-tool-cert-tls-version"), r.tls_version.clone()),
    ]
    .spacing(spacing::XS);
    if let Some(legacy) = &r.legacy {
        details = details.push(detail_row(
            fl!("ui-tool-cert-cipher-suite"),
            legacy.cipher_suite.clone(),
        ));
    }
    details = details.extend([
        detail_row(fl!("ui-tool-cert-sig-alg"), r.sig_algorithm.clone()),
        detail_row(fl!("ui-tool-cert-key-size"), key_size(r)),
        detail_row(fl!("ui-tool-cert-serial"), r.serial.clone()),
        detail_row(fl!("ui-tool-cert-sha256"), r.thumbprint.clone()),
        detail_row(fl!("ui-tool-cert-sans"), sans(r)),
    ]);
    if !chain.is_empty() {
        details = details.push(detail_row(fl!("ui-tool-cert-chain"), chain.join("\n")));
    }

    let mut card = column![header_row].spacing(spacing::MD);
    if let Some(legacy) = &r.legacy {
        card = card.push(legacy_banner(r, &legacy.cipher_suite, host));
    }
    container(card.push(details))
        .padding(spacing::MD)
        .style(styles::card)
        .into()
}

/// How a certificate was read, as its export says it: by a verified handshake, or in
/// clear from a legacy TLS answer, with the suite.
fn read_text(r: &CertInspectionSummary) -> String {
    r.legacy.as_ref().map_or_else(
        || fl!("ui-tool-cert-read-verified"),
        |legacy| {
            fl!(
                "ui-tool-cert-read-unverified",
                cipher = legacy.cipher_suite.as_str()
            )
        },
    )
}

/// The last moment of a certificate and the days left, or past.
fn valid_to(r: &CertInspectionSummary) -> String {
    let days = if r.days_remaining < 0 {
        fl!(
            "ui-tool-cert-days-expired",
            count = r.days_remaining.unsigned_abs()
        )
    } else {
        fl!("ui-tool-cert-days-left", count = r.days_remaining)
    };
    format!("{} ({days})", r.not_after)
}

/// The key size, in bits, or a dash when unknown.
fn key_size(r: &CertInspectionSummary) -> String {
    if r.key_size_bits > 0 {
        fl!("ui-tool-cert-key-bits", bits = r.key_size_bits)
    } else {
        "-".to_owned()
    }
}

/// The subject alternative names, or a dash when there is none, as the C#.
fn sans(r: &CertInspectionSummary) -> String {
    if r.sans.is_empty() {
        "-".to_owned()
    } else {
        r.sans.join(", ")
    }
}

/// The plain-text report of one certificate, as the C# `BuildDetailsText`; a certificate
/// read in clear says so first, with its suite.
fn details_text(summary: &CertInspectionSummary, host: &str) -> String {
    let legacy = summary.legacy.as_ref().map(|legacy| {
        [
            fl!(
                "ui-tool-cert-legacy-title",
                version = summary.tls_version.as_str(),
                cipher = legacy.cipher_suite.as_str()
            ),
            fl!("ui-tool-cert-legacy-warning"),
        ]
    });
    let lines = [
        fl!("ui-tool-cert-port-header", port = summary.port),
        format!(
            "{}: {host}:{}",
            fl!("ui-tool-cert-detail-host"),
            summary.port
        ),
        format!(
            "{}: {}",
            fl!("ui-tool-cert-tls-version"),
            summary.tls_version
        ),
        format!(
            "{}: {}",
            fl!("ui-tool-cert-hostname-match"),
            if summary.hostname_matches {
                fl!("ui-tool-cert-match-yes")
            } else {
                fl!("ui-tool-cert-match-no")
            }
        ),
        format!("{}: {}", fl!("ui-tool-cert-subject"), summary.subject),
        format!("{}: {}", fl!("ui-tool-cert-issuer"), summary.issuer),
        format!("{}: {}", fl!("ui-tool-cert-valid-from"), summary.not_before),
        format!("{}: {}", fl!("ui-tool-cert-valid-to"), valid_to(summary)),
        format!("{}: {}", fl!("ui-tool-cert-serial"), summary.serial),
        format!("{}: {}", fl!("ui-tool-cert-sha256"), summary.thumbprint),
        format!("{}: {}", fl!("ui-tool-cert-sig-alg"), summary.sig_algorithm),
        format!("{}: {}", fl!("ui-tool-cert-key-size"), key_size(summary)),
        format!("{}: {}", fl!("ui-tool-cert-sans"), sans(summary)),
    ];
    let mut text = lines[0].clone();
    for line in legacy.iter().flatten().chain(&lines[1..]) {
        text.push('\n');
        text.push_str(line);
    }
    text.push('\n');
    text
}

/// Why port `port` of `host` gave no certificate, said.
fn failure_text(host: &str, port: u16, error: &ProbeError) -> String {
    match error {
        ProbeError::Connect(detail) => fl!(
            "ui-tool-cert-error-connection",
            host = host,
            port = port,
            detail = detail.as_str()
        ),
        ProbeError::Name => fl!("ui-tool-cert-error-invalid-host"),
        ProbeError::Handshake(detail) => fl!(
            "ui-tool-cert-error-handshake",
            host = host,
            port = port,
            detail = detail.as_str()
        ),
        ProbeError::Legacy(LegacyTlsError::Version(code))
            if *code < TlsProtocol::Tls10.version_code() =>
        {
            fl!(
                "ui-tool-cert-error-legacy-version",
                host = host,
                port = port,
                version = version_label(*code)
            )
        }
        ProbeError::Legacy(error) => fl!(
            "ui-tool-cert-error-legacy",
            host = host,
            port = port,
            detail = legacy_detail(*error)
        ),
        ProbeError::NoCertificate => fl!("ui-tool-cert-error-no-certificate"),
        ProbeError::Unreadable => fl!("ui-tool-cert-error-unreadable"),
        ProbeError::Timeout => fl!("ui-tool-cert-error-timeout"),
    }
}

/// The name of version `code`: SSL 3.0, else its number.
fn version_label(code: u16) -> String {
    if code == TlsProtocol::Ssl3.version_code() {
        TlsProtocol::Ssl3.name().to_owned()
    } else {
        format!("0x{code:04X}")
    }
}

/// Why a legacy TLS answer gave no certificate, said.
fn legacy_detail(error: LegacyTlsError) -> String {
    match error {
        LegacyTlsError::Alert(code) => fl!("ui-tool-cert-legacy-alert", code = code),
        LegacyTlsError::NotTls => fl!("ui-tool-cert-legacy-not-tls"),
        LegacyTlsError::TooLarge => fl!("ui-tool-cert-legacy-too-large"),
        // A version past TLS 1.0 it was not offered is no answer to the hello.
        LegacyTlsError::Malformed | LegacyTlsError::Version(_) => {
            fl!("ui-tool-cert-legacy-malformed")
        }
        LegacyTlsError::NoCertificate => fl!("ui-tool-cert-legacy-no-certificate"),
        LegacyTlsError::Closed => fl!("ui-tool-cert-legacy-closed"),
    }
}

/// What the system or the server said, fit to show: control characters out, bounded.
fn detail(raw: &str) -> String {
    clean_banner(raw)
        .unwrap_or_default()
        .chars()
        .take(MAX_DETAIL_CHARS)
        .collect()
}

/// Now, in Unix seconds.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// The certificate of `host`:`port`, read by a TLS handshake within [`PROBE_TIMEOUT_MS`];
/// a server sharing nothing with it read again in clear, at the same address and within
/// the same time.
async fn probe(host: String, port: u16) -> Result<Box<CertInspectionSummary>, ProbeError> {
    let timeout = Duration::from_millis(PROBE_TIMEOUT_MS);
    let read = async {
        let tcp = tokio::net::TcpStream::connect((host.as_str(), port))
            .await
            .map_err(|error| ProbeError::Connect(detail(&error.to_string())))?;
        // The address reached: a read in clear goes to it, never to another the name
        // resolves to by then.
        let address = tcp
            .peer_addr()
            .map_err(|error| ProbeError::Connect(detail(&error.to_string())))?;
        let name = heimdall_tls::inspection_server_name(&host).ok_or(ProbeError::Name)?;
        let tls = match heimdall_tls::inspection_connector()
            .connect(name, tcp)
            .await
        {
            Ok(tls) => tls,
            Err(error) if heimdall_tls::may_speak_legacy_only(&error) => {
                return read_legacy(address, &host, &error).await;
            }
            Err(error) => return Err(ProbeError::Handshake(detail(&error.to_string()))),
        };
        let chain =
            heimdall_tls::presented_chain(tls.get_ref().1).ok_or(ProbeError::NoCertificate)?;
        cert_inspector::build_summary(
            &chain.certificates,
            chain.version.label(),
            &host,
            port,
            now(),
        )
        .map(Box::new)
        .ok_or(ProbeError::Unreadable)
    };
    tokio::time::timeout(timeout, read)
        .await
        .unwrap_or(Err(ProbeError::Timeout))
}

/// Why a legacy TLS read gave nothing.
enum LegacyFailure {
    /// The connection, of this kind: the system's words.
    Io(std::io::ErrorKind, String),
    /// The answer.
    Answer(LegacyTlsError),
}

impl LegacyFailure {
    /// Whether the server closed or reset the connection on the hello, as an old server
    /// intolerant of its version or its extensions does.
    fn closed(&self) -> bool {
        matches!(
            self,
            Self::Answer(LegacyTlsError::Closed)
                | Self::Io(
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::UnexpectedEof,
                    _
                )
        )
    }
}

/// The certificate of `host` at `address` read in clear after the handshake failed with
/// `refusal`, marked so: a full legacy hello, then a minimal one when the server closes on
/// it. A server that turns out to speak what rustls speaks is not shown as a legacy one:
/// the handshake's own words are kept. When the read gives nothing, a server that said it
/// speaks only an old version is told why, and so is one speaking SSL 3.0 or older; any
/// other keeps the handshake's own words.
async fn read_legacy(
    address: std::net::SocketAddr,
    host: &str,
    refusal: &std::io::Error,
) -> Result<Box<CertInspectionSummary>, ProbeError> {
    let refused = || ProbeError::Handshake(detail(&refusal.to_string()));
    let obsolete = heimdall_tls::failed_on_obsolete_version(refusal);
    let read = match legacy_handshake(address, host, HelloShape::Full).await {
        Err(failure) if failure.closed() => {
            legacy_handshake(address, host, HelloShape::Minimal).await
        }
        read => read,
    };
    match read {
        Ok(handshake) if handshake.speaks_modern_tls() => Err(refused()),
        Ok(handshake) => {
            cert_inspector::build_legacy_summary(&handshake, host, address.port(), now())
                .map(Box::new)
                .ok_or(ProbeError::Unreadable)
        }
        Err(LegacyFailure::Answer(LegacyTlsError::Version(code)))
            if code < TlsProtocol::Tls10.version_code() =>
        {
            Err(ProbeError::Legacy(LegacyTlsError::Version(code)))
        }
        Err(LegacyFailure::Answer(error)) if obsolete => Err(ProbeError::Legacy(error)),
        Err(LegacyFailure::Io(_, said)) if obsolete => Err(ProbeError::Connect(said)),
        Err(_) => Err(refused()),
    }
}

/// What `host` at `address` answers a legacy hello of `shape`, up to its certificates, on
/// a connection of its own closed once they are read: the handshake is never finished.
async fn legacy_handshake(
    address: std::net::SocketAddr,
    host: &str,
    shape: HelloShape,
) -> Result<LegacyHandshake, LegacyFailure> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let io = |error: std::io::Error| LegacyFailure::Io(error.kind(), detail(&error.to_string()));
    let mut tcp = tokio::net::TcpStream::connect(address).await.map_err(io)?;
    tcp.write_all(&legacy_tls::client_hello(host, shape))
        .await
        .map_err(io)?;
    let mut reader = LegacyReader::new();
    let mut chunk = vec![0; LEGACY_READ_CHUNK];
    loop {
        let read = tcp.read(&mut chunk).await.map_err(io)?;
        if read == 0 {
            return Err(LegacyFailure::Answer(LegacyTlsError::Closed));
        }
        if let Some(handshake) = reader.feed(&chunk[..read]).map_err(LegacyFailure::Answer)? {
            return Ok(handshake);
        }
    }
}

/// The messages of run `run` probing `ports` of `host`, [`MAX_CONCURRENT_PROBES`] at a
/// time, as the C# `SemaphoreSlim`: each port's outcome as it comes, then the end. Dropped
/// by a stop, every probe still going is dropped with it.
fn probes(
    run: u64,
    host: String,
    ports: Vec<u16>,
) -> impl iced::futures::Stream<Item = CertInspectorMessage> {
    iced::stream::channel(STREAM_BUFFER, async move |mut out| {
        let mut probed = iced::futures::stream::iter(ports)
            .map(|port| {
                let host = host.clone();
                async move { (port, probe(host, port).await) }
            })
            .buffer_unordered(MAX_CONCURRENT_PROBES);
        while let Some((port, result)) = probed.next().await {
            if out
                .send(CertInspectorMessage::Probed(run, port, result))
                .await
                .is_err()
            {
                return;
            }
        }
        let _ = out.send(CertInspectorMessage::Done(run)).await;
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use heimdall_core::tools::cert_inspector::CertExpirationStatus;

    use super::*;

    fn summary(port: u16, days: i64) -> Box<CertInspectionSummary> {
        Box::new(CertInspectionSummary {
            subject: "CN=files.lab".to_owned(),
            issuer: "CN=Lab CA".to_owned(),
            not_before: "2026-01-01T00:00:00Z".to_owned(),
            not_after: "2026-02-01T00:00:00Z".to_owned(),
            days_remaining: days,
            serial: "0A".to_owned(),
            thumbprint: "AB".to_owned(),
            sig_algorithm: "sha256RSA".to_owned(),
            key_size_bits: 2048,
            sans: vec!["files.lab".to_owned()],
            tls_version: "TLS 1.3".to_owned(),
            hostname_matches: true,
            expiration_status: if days < 0 {
                CertExpirationStatus::Expired
            } else {
                CertExpirationStatus::Valid
            },
            chain: Vec::new(),
            port,
            legacy: None,
        })
    }

    fn inspecting(pane: &mut CertInspectorPane, ports: &str) -> u64 {
        pane.apply(CertInspectorMessage::HostChanged("files.lab".to_owned()));
        pane.apply(CertInspectorMessage::PortsChanged(ports.to_owned()));
        match pane.apply(CertInspectorMessage::Inspect) {
            Step::Probe(run, host, _) => {
                assert_eq!(host, "files.lab");
                run
            }
            other => panic!("no probe: {other:?}"),
        }
    }

    #[test]
    fn a_scan_profile_fills_the_ports_box_with_its_ports_and_scans_them() {
        let mut pane = CertInspectorPane::default();
        pane.apply(CertInspectorMessage::HostChanged("files.lab".to_owned()));
        pane.apply(CertInspectorMessage::ProfileChosen(ScanProfile::Quick));
        assert_eq!(
            pane.ports,
            "443, 8443, 993, 995, 465, 636, 990, 5986, 3389, 853, 5223, 8883"
        );
        pane.apply(CertInspectorMessage::ProfileChosen(ScanProfile::Extended));
        assert_eq!(
            cert_inspector::parse_ports(&pane.ports).len(),
            EXTENDED_SCAN_PORTS.len()
        );
        assert!(matches!(
            pane.apply(CertInspectorMessage::Inspect),
            Step::Probe(_, _, ports) if ports.len() == EXTENDED_SCAN_PORTS.len()
        ));
        // A profile is a start: the box stays free to edit.
        pane.apply(CertInspectorMessage::PortsChanged("443".to_owned()));
        assert_eq!(pane.ports, "443");
    }

    #[test]
    fn ports_are_bounded_and_a_host_that_is_no_address_refused() {
        let mut pane = CertInspectorPane::default();
        pane.apply(CertInspectorMessage::HostChanged("files.lab".to_owned()));
        pane.apply(CertInspectorMessage::PortsChanged("1-65535".to_owned()));
        assert!(matches!(
            pane.apply(CertInspectorMessage::Inspect),
            Step::None
        ));
        assert!(pane.error.is_some(), "too many ports");
        pane.apply(CertInspectorMessage::PortsChanged("abc".to_owned()));
        assert!(matches!(
            pane.apply(CertInspectorMessage::Inspect),
            Step::None
        ));
        assert!(pane.error.is_some(), "no valid port");
        pane.apply(CertInspectorMessage::PortsChanged(String::new()));
        let Step::Probe(_, _, ports) = pane.apply(CertInspectorMessage::Inspect) else {
            panic!("the default port");
        };
        assert_eq!(ports, [DEFAULT_PORT]);
        pane.apply(CertInspectorMessage::Stop);
        pane.apply(CertInspectorMessage::HostChanged("-k".to_owned()));
        assert!(matches!(
            pane.apply(CertInspectorMessage::Inspect),
            Step::None
        ));
        assert!(pane.error.is_some());
    }

    #[test]
    fn results_come_sorted_by_port_and_a_stop_lets_late_ones_go() {
        let mut pane = CertInspectorPane::default();
        let run = inspecting(&mut pane, "443, 8443, 993");
        pane.apply(CertInspectorMessage::Probed(
            run,
            8443,
            Ok(summary(8443, 10)),
        ));
        pane.apply(CertInspectorMessage::Probed(run, 443, Ok(summary(443, -3))));
        let ports: Vec<u16> = pane.results.iter().map(|r| r.port).collect();
        assert_eq!(ports, [443, 8443]);
        pane.apply(CertInspectorMessage::Stop);
        assert!(!pane.run.running());
        pane.apply(CertInspectorMessage::Probed(run, 993, Ok(summary(993, 10))));
        pane.apply(CertInspectorMessage::Done(run));
        assert_eq!(pane.results.len(), 2, "nothing after the stop");
        assert!(pane.notice.is_some());
        let Step::Save(export) = pane.apply(CertInspectorMessage::ExportCsv) else {
            panic!("a CSV");
        };
        assert_eq!(export.slot(), CopySlot::CertCsv);
        assert!(export.suggested_name().starts_with("certscan_"));
        let csv = export.content();
        assert!(csv.contains("\n443,") && csv.contains(",-3,"), "{csv}");
        let Step::Copy(CopySlot::CertDetails, details) =
            pane.apply(CertInspectorMessage::CopyDetails)
        else {
            panic!("details");
        };
        assert!(details.contains("files.lab:443") && details.contains("files.lab:8443"));
    }

    #[test]
    fn a_single_port_says_why_it_failed_and_a_scan_counts_what_it_found() {
        let mut pane = CertInspectorPane::default();
        let run = inspecting(&mut pane, "443");
        pane.apply(CertInspectorMessage::Probed(
            run,
            443,
            Err(ProbeError::Handshake("no TLS\u{1b}[2J here".to_owned())),
        ));
        pane.apply(CertInspectorMessage::Done(run));
        assert!(pane.error.is_some());
        let run = inspecting(&mut pane, "443,465");
        pane.apply(CertInspectorMessage::Probed(run, 443, Ok(summary(443, 90))));
        pane.apply(CertInspectorMessage::Probed(
            run,
            465,
            Err(ProbeError::Timeout),
        ));
        pane.apply(CertInspectorMessage::Done(run));
        assert!(pane.error.is_none(), "a scan skips what failed, as the C#");
        assert!(pane.notice.is_some());
    }

    /// A fixture certificate for `old.lab`, valid from 2020 to 2090 (DER).
    fn fixture_certificate() -> Vec<u8> {
        let key = rcgen::KeyPair::generate().expect("key");
        let mut params = rcgen::CertificateParams::new(vec!["old.lab".to_owned()]).expect("params");
        params.not_before = rcgen::date_time_ymd(2020, 1, 1);
        params.not_after = rcgen::date_time_ymd(2090, 1, 1);
        params.self_signed(&key).expect("signed").der().to_vec()
    }

    /// TLS 1.0 and TLS 1.2 on the wire.
    const TLS10: u16 = 0x0301;
    const TLS12: u16 = 0x0303;
    /// `TLS_RSA_WITH_AES_128_CBC_SHA`, and `TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256`, which
    /// rustls speaks.
    const RSA_AES_128_CBC_SHA: u16 = 0x002F;
    const ECDHE_RSA_AES_128_GCM: u16 = 0xC02F;
    /// The record type of a handshake, and the handshake types of a `ServerHello`, a
    /// `Certificate` and a `ServerHelloDone`.
    const HANDSHAKE: u8 = 22;
    const SERVER_HELLO: u8 = 2;
    const CERTIFICATE: u8 = 11;
    const SERVER_HELLO_DONE: u8 = 14;
    /// The fatal `handshake_failure` alert.
    const HANDSHAKE_FAILURE: [u8; 7] = [0x15, 0x03, 0x03, 0, 2, 2, 40];
    /// Where a client hello's version sits: after the record and message headers.
    const HELLO_VERSION_AT: std::ops::Range<usize> = 9..11;

    /// A handshake message of type `kind` and body `body`.
    fn message(kind: u8, body: &[u8]) -> Vec<u8> {
        let length = u32::try_from(body.len()).expect("short").to_be_bytes();
        [&[kind], &length[1..], body].concat()
    }

    /// `messages` in one handshake record of `version`.
    fn record(version: u16, messages: &[u8]) -> Vec<u8> {
        let length = u16::try_from(messages.len()).expect("short").to_be_bytes();
        [&[HANDSHAKE], &version.to_be_bytes()[..], &length, messages].concat()
    }

    /// A `ServerHello` choosing `version` and `cipher_suite`: no session, no compression.
    fn server_hello(version: u16, cipher_suite: u16) -> Vec<u8> {
        let mut body = version.to_be_bytes().to_vec();
        body.extend([0x5A; 32]);
        body.push(0);
        body.extend(cipher_suite.to_be_bytes());
        body.push(0);
        message(SERVER_HELLO, &body)
    }

    /// The `Certificate` message of a list of `list_len` bytes holding `chain`.
    fn certificates(list_len: usize, chain: &[Vec<u8>]) -> Vec<u8> {
        let mut body = u32::try_from(list_len).expect("short").to_be_bytes()[1..].to_vec();
        for der in chain {
            body.extend(&u32::try_from(der.len()).expect("short").to_be_bytes()[1..]);
            body.extend(der);
        }
        message(CERTIFICATE, &body)
    }

    /// The whole answer of a server speaking `version`, choosing `cipher_suite` and
    /// presenting `der`: its hello, its certificate and its hello done.
    fn answer(version: u16, cipher_suite: u16, der: &[u8]) -> Vec<u8> {
        let mut messages = server_hello(version, cipher_suite);
        messages.extend(certificates(der.len() + 3, &[der.to_vec()]));
        messages.extend(message(SERVER_HELLO_DONE, &[]));
        record(version, &messages)
    }

    /// A server on a port of the loopback that answers each client what `answer` makes of
    /// the connection's number and the client's first bytes, nothing to close at once,
    /// then reads until it closes: an old appliance, or a hostile one. The port, and the
    /// count of connections taken.
    fn canned_server(
        answer: impl Fn(usize, &[u8]) -> Vec<u8> + Send + 'static,
    ) -> (u16, Arc<AtomicUsize>) {
        use std::io::{Read as _, Write as _};
        const CLIENT_HELLO_READ: usize = 4096;
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("listener");
        let port = listener.local_addr().expect("address").port();
        let taken = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&taken);
        std::thread::spawn(move || {
            while let Ok((mut client, _)) = listener.accept() {
                let number = count.fetch_add(1, Ordering::SeqCst);
                let mut hello = vec![0; CLIENT_HELLO_READ];
                let read = client.read(&mut hello).unwrap_or(0);
                let _ = client.write_all(&answer(number, &hello[..read]));
                // Nothing more comes: the client sees the end of what was sent, and its
                // alert or its close read, the connection closes.
                let _ = client.shutdown(std::net::Shutdown::Write);
                let _ = client.read(&mut hello);
            }
        });
        (port, taken)
    }

    /// What probing the loopback's `port` gives.
    async fn probed(port: u16) -> Result<Box<CertInspectionSummary>, ProbeError> {
        probe("127.0.0.1".to_owned(), port).await
    }

    #[tokio::test]
    async fn a_tls_1_0_server_has_its_certificate_read_in_clear_and_marked() {
        let canned = answer(TLS10, RSA_AES_128_CBC_SHA, &fixture_certificate());
        let (port, taken) = canned_server(move |_, _| canned.clone());
        let summary = probed(port).await.expect("read");
        assert_eq!(
            taken.load(Ordering::SeqCst),
            2,
            "the handshake refused, then the read in clear"
        );
        assert_eq!(summary.tls_version, "TLS 1.0");
        let legacy = summary.legacy.as_ref().expect("marked as read in clear");
        assert_eq!(legacy.cipher_suite, "TLS_RSA_WITH_AES_128_CBC_SHA");
        assert_eq!(summary.sans, ["old.lab"]);
        assert_eq!(summary.port, port);
        let details = details_text(&summary, "127.0.0.1");
        assert!(
            details.contains("TLS_RSA_WITH_AES_128_CBC_SHA"),
            "{details}"
        );
        assert!(details.contains("private key"), "the warning: {details}");
        assert!(read_text(&summary).contains("not verified"));
        // The way to the TLS Auditor, on the same host and port.
        let Message::App(AppMessage::Tools(ToolsMessage::Open { tool, context, .. })) =
            audit_message("127.0.0.1", port)
        else {
            panic!("no tool opened");
        };
        assert_eq!(tool, ToolId::TlsAudit);
        assert_eq!(context.host.as_deref(), Some("127.0.0.1"));
        assert_eq!(context.port, Some(port));
    }

    #[tokio::test]
    async fn an_old_server_closing_on_a_tls_1_2_hello_is_read_with_a_minimal_one() {
        let canned = answer(TLS10, RSA_AES_128_CBC_SHA, &fixture_certificate());
        let (port, taken) = canned_server(move |_, hello| {
            let tls10 = hello.get(HELLO_VERSION_AT) == Some(&TLS10.to_be_bytes()[..]);
            if tls10 { canned.clone() } else { Vec::new() }
        });
        let summary = probed(port).await.expect("read");
        assert_eq!(
            taken.load(Ordering::SeqCst),
            3,
            "rustls, the full hello and the minimal one"
        );
        assert_eq!(summary.tls_version, "TLS 1.0");
        assert!(summary.legacy.is_some());
    }

    #[tokio::test]
    async fn a_server_speaking_what_rustls_speaks_is_not_shown_as_a_legacy_one() {
        // The verified handshake refused (a client certificate asked, say), then a TLS 1.2
        // answer with a suite rustls speaks.
        let canned = answer(TLS12, ECDHE_RSA_AES_128_GCM, &fixture_certificate());
        let (port, taken) = canned_server(move |number, _| {
            if number == 0 {
                HANDSHAKE_FAILURE.to_vec()
            } else {
                canned.clone()
            }
        });
        let refusal = probed(port).await.expect_err("refused");
        assert_eq!(taken.load(Ordering::SeqCst), 2);
        assert!(matches!(refusal, ProbeError::Handshake(_)), "{refusal:?}");
    }

    #[tokio::test]
    async fn a_tls_1_0_server_whose_answer_is_no_chain_is_said_why() {
        let hello = record(TLS10, &server_hello(TLS10, RSA_AES_128_CBC_SHA));
        // A hello, then a record announcing far more than any chain.
        let oversized = [&hello[..], &[HANDSHAKE, 3, 1, 0xFF, 0xFF]].concat();
        // A hello, then a certificate list longer than its message.
        let garbled = [&hello[..], &record(TLS10, &certificates(9, &[vec![7]]))[..]].concat();
        for (canned, expected) in [
            (hello, LegacyTlsError::Closed),
            (oversized, LegacyTlsError::TooLarge),
            (garbled, LegacyTlsError::Malformed),
        ] {
            let (port, _) = canned_server(move |_, _| canned.clone());
            let refusal = probed(port).await.expect_err("refused");
            assert_eq!(refusal, ProbeError::Legacy(expected));
            let said = failure_text("127.0.0.1", port, &refusal);
            assert!(said.contains("obsolete"), "{said}");
        }
        // SSL 3.0 alone: named, never read.
        let ssl3 = answer(0x0300, RSA_AES_128_CBC_SHA, &fixture_certificate());
        let (port, _) = canned_server(move |_, _| ssl3.clone());
        let refusal = probed(port).await.expect_err("refused");
        assert_eq!(refusal, ProbeError::Legacy(LegacyTlsError::Version(0x0300)));
        let said = failure_text("127.0.0.1", port, &refusal);
        assert!(said.contains("SSL 3.0"), "{said}");
    }

    #[tokio::test]
    async fn a_server_refusing_every_suite_keeps_the_handshake_words_when_nothing_is_read() {
        // handshake_failure to every hello: the read in clear runs and finds nothing more.
        let (port, taken) = canned_server(|_, _| HANDSHAKE_FAILURE.to_vec());
        let refusal = probed(port).await.expect_err("refused");
        assert_eq!(taken.load(Ordering::SeqCst), 2, "the read in clear ran");
        assert!(matches!(refusal, ProbeError::Handshake(_)), "{refusal:?}");
    }

    #[test]
    fn what_a_server_or_the_system_says_is_cleaned_and_bounded() {
        assert_eq!(detail("refused\r\n\u{1b}[31m"), "refused [31m");
        assert_eq!(detail(&"x".repeat(1000)).chars().count(), MAX_DETAIL_CHARS);
    }
}
