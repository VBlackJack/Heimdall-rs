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

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use heimdall_app::TabId;
use iced::futures::{SinkExt as _, StreamExt as _};
use iced::widget::{Column, column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::banner_grab::clean_banner;
use heimdall_core::tools::cert_inspector::{
    self, CertExpirationStatus, CertInspectionSummary, DEFAULT_PORT, MAX_CONCURRENT_PROBES,
    MAX_SCAN_PORTS, PROBE_TIMEOUT_MS,
};
use heimdall_core::tools::net_host::checked_host;

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

/// Why a port's certificate could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// No connection: refused, unreachable, the name unknown. The system's words.
    Connect(String),
    /// The host is neither a DNS name nor an address the handshake can name.
    Name,
    /// The TLS handshake failed: no TLS there, or a cipher not spoken. Its words.
    Handshake(String),
    /// The server speaks only TLS 1.1 or older, which Heimdall refuses (owner decision of
    /// 2026-10-16): said plainly rather than as rustls words its refusal.
    ObsoleteVersion,
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
                ];
                let (yes, no) = (fl!("ui-tool-cert-match-yes"), fl!("ui-tool-cert-match-no"));
                let csv = cert_inspector::build_scan_csv(
                    &self.results,
                    header.each_ref().map(String::as_str),
                    (&yes, &no),
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

        let input_card = container(column![inputs_row])
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
            body = body.push(certificate_card(r));
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

/// The card of one port's certificate.
fn certificate_card<'a>(r: &CertInspectionSummary) -> Element<'a, Message> {
    let status_badge = match r.expiration_status {
        CertExpirationStatus::Valid => badge(fl!("ui-tool-cert-status-valid"), text::success),
        CertExpirationStatus::Warning => badge(fl!("ui-tool-cert-status-warning"), text::warning),
        CertExpirationStatus::Expired => badge(fl!("ui-tool-cert-status-expired"), text::danger),
    };
    let match_badge = if r.hostname_matches {
        badge(fl!("ui-tool-cert-match-yes"), text::success)
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
        detail_row(fl!("ui-tool-cert-sig-alg"), r.sig_algorithm.clone()),
        detail_row(fl!("ui-tool-cert-key-size"), key_size(r)),
        detail_row(fl!("ui-tool-cert-serial"), r.serial.clone()),
        detail_row(fl!("ui-tool-cert-sha256"), r.thumbprint.clone()),
        detail_row(fl!("ui-tool-cert-sans"), sans(r)),
    ]
    .spacing(spacing::XS);
    if !chain.is_empty() {
        details = details.push(detail_row(fl!("ui-tool-cert-chain"), chain.join("\n")));
    }

    container(column![header_row, details].spacing(spacing::MD))
        .padding(spacing::MD)
        .style(styles::card)
        .into()
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

/// The plain-text report of one certificate, as the C# `BuildDetailsText`.
fn details_text(summary: &CertInspectionSummary, host: &str) -> String {
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
    let mut text = lines.join("\n");
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
        ProbeError::ObsoleteVersion => {
            fl!("ui-tool-cert-error-obsolete-tls", host = host, port = port)
        }
        ProbeError::NoCertificate => fl!("ui-tool-cert-error-no-certificate"),
        ProbeError::Unreadable => fl!("ui-tool-cert-error-unreadable"),
        ProbeError::Timeout => fl!("ui-tool-cert-error-timeout"),
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

/// The certificate of `host`:`port`, read by a TLS handshake within [`PROBE_TIMEOUT_MS`].
async fn probe(host: String, port: u16) -> Result<Box<CertInspectionSummary>, ProbeError> {
    let timeout = Duration::from_millis(PROBE_TIMEOUT_MS);
    let read = async {
        let tcp = tokio::net::TcpStream::connect((host.as_str(), port))
            .await
            .map_err(|error| ProbeError::Connect(detail(&error.to_string())))?;
        let name = heimdall_tls::inspection_server_name(&host).ok_or(ProbeError::Name)?;
        let tls = heimdall_tls::inspection_connector()
            .connect(name, tcp)
            .await
            .map_err(|error| {
                if heimdall_tls::failed_on_obsolete_version(&error) {
                    ProbeError::ObsoleteVersion
                } else {
                    ProbeError::Handshake(detail(&error.to_string()))
                }
            })?;
        let chain =
            heimdall_tls::presented_chain(tls.get_ref().1).ok_or(ProbeError::NoCertificate)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        cert_inspector::build_summary(&chain.certificates, chain.version.label(), &host, port, now)
            .map(Box::new)
            .ok_or(ProbeError::Unreadable)
    };
    tokio::time::timeout(timeout, read)
        .await
        .unwrap_or(Err(ProbeError::Timeout))
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

    /// A server speaking TLS 1.0 alone, on a port of the loopback: it answers the first
    /// client hello with a server hello choosing TLS 1.0 (`0x0301`), as an old appliance.
    async fn tls10_only_server() -> u16 {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        const TLS10: [u8; 2] = [0x03, 0x01];
        const RSA_AES_128_CBC_SHA: [u8; 2] = [0x00, 0x2F];
        const RANDOM_LEN: usize = 32;
        const CLIENT_HELLO_READ: usize = 4096;
        let mut hello = TLS10.to_vec();
        hello.extend([0x5A; RANDOM_LEN]);
        // No session identifier, the suite, no compression, no extension.
        hello.extend([0, RSA_AES_128_CBC_SHA[0], RSA_AES_128_CBC_SHA[1], 0]);
        let mut message = vec![0x02, 0, 0, u8::try_from(hello.len()).expect("short")];
        message.extend(hello);
        let mut record = vec![0x16, TLS10[0], TLS10[1]];
        record.extend(u16::try_from(message.len()).expect("short").to_be_bytes());
        record.extend(message);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("listener");
        let port = listener.local_addr().expect("address").port();
        tokio::spawn(async move {
            let (mut client, _) = listener.accept().await.expect("client");
            let mut client_hello = vec![0; CLIENT_HELLO_READ];
            let _ = client.read(&mut client_hello).await;
            let _ = client.write_all(&record).await;
            // Its alert read, the connection closes.
            let _ = client.read(&mut client_hello).await;
        });
        port
    }

    #[tokio::test]
    async fn a_server_speaking_only_tls_1_0_is_said_obsolete_not_a_failed_handshake() {
        let port = tls10_only_server().await;
        let refusal = probe("127.0.0.1".to_owned(), port)
            .await
            .expect_err("refused");
        assert_eq!(refusal, ProbeError::ObsoleteVersion);
        let said = failure_text("127.0.0.1", port, &refusal);
        assert!(said.contains("TLS 1.0"), "{said}");
    }

    #[test]
    fn what_a_server_or_the_system_says_is_cleaned_and_bounded() {
        assert_eq!(detail("refused\r\n\u{1b}[31m"), "refused [31m");
        assert_eq!(detail(&"x".repeat(1000)).chars().count(), MAX_DETAIL_CHARS);
    }
}
