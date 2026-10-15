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

//! The TCP Port Scanner tool view, as the C# `PortScannerView` and `PortScanViewModel`: the
//! host resolved once, then its ports probed [`port_scanner::MAX_CONCURRENT`] at a time, the
//! results shown in port order as they come, with summary metrics and a CSV export.
//!
//! A scan is one stream, aborted by Stop or when the tab closes, which drops the
//! connections under way; each batch of results carries its scan's number, so one of an
//! earlier scan never lands in a later one. Above
//! [`port_scanner::LARGE_PORT_COUNT_WARNING`] ports the scan is confirmed first, as the C#
//! asks.

use std::time::Duration;

use heimdall_app::TabId;
use heimdall_app::net_tools::{self, ResolveFailure};
use iced::futures::{SinkExt as _, StreamExt as _};
use iced::task::Handle;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::banner_grab;
use heimdall_core::tools::port_scanner::{
    self, DEFAULT_COMMON_PORTS, DEFAULT_PORT_TIMEOUT_MS, PortProbeResult, PortScanSummary,
    PortStatus,
};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Rows shown at most: past it, only the open ports are, the CSV export holding every port.
const MAX_SHOWN_ROWS: usize = 500;

/// Results gathered into one message, at most, when several are ready at once.
const RESULT_BATCH: usize = 64;

/// Messages buffered between the scan and the view.
const EVENT_BUFFER: usize = 16;

/// Messages emitted by the port scanner view.
#[derive(Debug, Clone)]
pub enum PortScanMessage {
    /// Host input changed.
    HostChanged(String),
    /// Ports range input changed.
    PortsChanged(String),
    /// Timeout input changed.
    TimeoutChanged(String),
    /// Start port scan.
    Start,
    /// Start the scan of more ports than [`port_scanner::LARGE_PORT_COUNT_WARNING`], confirmed.
    Confirm,
    /// Cancel in-progress scan.
    Stop,
    /// The host of scan `.0` did not resolve.
    Unresolved(u64, ResolveFailure),
    /// Ports of scan `.0` probed.
    Probed(u64, Vec<PortProbeResult>),
    /// Scan `.0` probed every port.
    Ended(u64),
    /// Copy CSV export.
    CopyCsv,
}

/// What a scan is asked for, once validated.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScanRequest {
    host: String,
    ports: Vec<u16>,
    timeout_ms: u64,
}

/// The TCP Port Scanner tool pane state.
#[derive(Debug)]
pub struct PortScanPane {
    host: String,
    ports: String,
    timeout: String,
    running: bool,
    /// The number of the latest scan; the results of any other are ignored.
    run: u64,
    /// Aborts the scan under way, also when the pane is dropped.
    stop: Option<Handle>,
    /// The port count of a scan waiting for its confirmation.
    confirming: Option<usize>,
    error: Option<String>,
    /// Every result, in port order.
    results: Vec<PortProbeResult>,
    /// The ports of the scan.
    total: usize,
    summary: PortScanSummary,
}

impl Default for PortScanPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            ports: DEFAULT_COMMON_PORTS.to_string(),
            timeout: DEFAULT_PORT_TIMEOUT_MS.to_string(),
            running: false,
            run: 0,
            stop: None,
            confirming: None,
            error: None,
            results: Vec::new(),
            total: 0,
            summary: PortScanSummary::default(),
        }
    }
}

/// Outcome of updating the Port Scanner tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

/// The text of a validation error key of [`port_scanner`].
fn input_error(key: &str) -> String {
    match key {
        "ui-tool-portscan-error-host-required" => fl!("ui-tool-portscan-error-host-required"),
        "ui-tool-portscan-error-host-invalid" => fl!("ui-tool-portscan-error-host-invalid"),
        "ui-tool-portscan-error-timeout-invalid" => fl!("ui-tool-portscan-error-timeout-invalid"),
        _ => fl!("ui-tool-portscan-error-ports-invalid"),
    }
}

/// Why the host gave no address, in the user's words.
fn unresolved_text(failure: &ResolveFailure) -> String {
    match failure {
        ResolveFailure::TimedOut | ResolveFailure::NoAddress => {
            fl!("ui-tool-portscan-error-unresolved")
        }
        ResolveFailure::Failed(why) => {
            fl!(
                "ui-tool-portscan-error-unresolved-reason",
                reason = why.as_str()
            )
        }
    }
}

/// Scan `run` of `request`: the host resolved once, before any port is timed, then its
/// ports probed [`port_scanner::MAX_CONCURRENT`] at a time on every address it has.
fn scan_run(tab: TabId, run: u64, request: ScanRequest) -> (Task<Message>, Handle) {
    let events = iced::stream::channel(EVENT_BUFFER, async move |mut out| {
        let resolving = Duration::from_millis(port_scanner::RESOLVE_TIMEOUT_MS);
        let addresses = match net_tools::resolve(&request.host, resolving).await {
            Ok(addresses) => addresses,
            Err(failure) => {
                let _ = out.send(PortScanMessage::Unresolved(run, failure)).await;
                return;
            }
        };
        let addresses = &addresses;
        let timeout_ms = request.timeout_ms;
        let mut probed = iced::futures::stream::iter(request.ports)
            .map(|port| net_tools::port_scan::probe(addresses, port, timeout_ms))
            .buffer_unordered(port_scanner::MAX_CONCURRENT)
            .ready_chunks(RESULT_BATCH);
        while let Some(results) = probed.next().await {
            if out
                .send(PortScanMessage::Probed(run, results))
                .await
                .is_err()
            {
                return;
            }
        }
        let _ = out.send(PortScanMessage::Ended(run)).await;
    });
    Task::run(events, move |message| {
        Message::Tool(tab, super::ToolMessage::PortScan(message))
    })
    .abortable()
}

impl PortScanPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: PortScanMessage) -> Outcome {
        let confirmed = match message {
            PortScanMessage::Start => false,
            PortScanMessage::Confirm => true,
            other => return self.apply(other),
        };
        match self.start(confirmed) {
            Some(request) => {
                let (task, handle) = scan_run(tab, self.run, request);
                self.stop = Some(handle.abort_on_drop());
                Outcome::Task(task)
            }
            None => Outcome::None,
        }
    }

    /// Handles every message but those starting a scan, which need the tab for its task.
    fn apply(&mut self, message: PortScanMessage) -> Outcome {
        match message {
            PortScanMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
                self.confirming = None;
            }
            PortScanMessage::PortsChanged(val) => {
                self.ports = val;
                self.error = None;
                self.confirming = None;
            }
            PortScanMessage::TimeoutChanged(val) => {
                self.timeout = val;
                self.error = None;
                self.confirming = None;
            }
            PortScanMessage::Start | PortScanMessage::Confirm => {}
            PortScanMessage::Stop => self.finish(),
            PortScanMessage::Unresolved(run, failure) => {
                if run == self.run && self.running {
                    self.error = Some(unresolved_text(&failure));
                    self.finish();
                }
            }
            PortScanMessage::Probed(run, results) => {
                if run == self.run && self.running {
                    // Nearly in order already: the sort merges the new run in.
                    self.results.extend(results);
                    self.results.sort_by_key(|result| result.port);
                    self.summary = port_scanner::compute_port_summary(&self.results);
                }
            }
            PortScanMessage::Ended(run) => {
                if run == self.run {
                    self.finish();
                }
            }
            PortScanMessage::CopyCsv => {
                if !self.results.is_empty() {
                    let csv = port_scanner::format_port_scan_csv(&self.results);
                    return Outcome::Copy(CopySlot::PortScanCsv, csv);
                }
            }
        }
        Outcome::None
    }

    /// A new scan's request, the pane reset for it; `None` while a scan goes on, when an
    /// input is refused, said in the error card, or when a large scan waits for its
    /// confirmation.
    fn start(&mut self, confirmed: bool) -> Option<ScanRequest> {
        if self.running {
            return None;
        }
        let checked = port_scanner::validate_host(&self.host).and_then(|host| {
            let ports = banner_grab::parse_ports(&self.ports);
            if ports.is_empty() {
                return Err("ui-tool-portscan-error-ports-invalid");
            }
            let timeout_ms = port_scanner::validate_timeout(&self.timeout)?;
            Ok(ScanRequest {
                host,
                ports,
                timeout_ms,
            })
        });
        let request = match checked {
            Ok(request) => request,
            Err(key) => {
                self.error = Some(input_error(key));
                return None;
            }
        };
        let count = request.ports.len();
        if port_scanner::needs_confirmation(count) && !(confirmed && self.confirming == Some(count))
        {
            self.confirming = Some(count);
            return None;
        }
        self.confirming = None;
        self.run = self.run.wrapping_add(1);
        self.running = true;
        self.error = None;
        self.results.clear();
        self.total = count;
        self.summary = PortScanSummary::default();
        Some(request)
    }

    /// The scan over: stopped, its task aborted.
    fn finish(&mut self) {
        self.running = false;
        if let Some(stop) = self.stop.take() {
            stop.abort();
        }
    }

    /// The rows shown: every result, or past [`MAX_SHOWN_ROWS`] the open ports alone.
    fn shown(&self) -> Vec<&PortProbeResult> {
        if self.results.len() <= MAX_SHOWN_ROWS {
            return self.results.iter().collect();
        }
        self.results
            .iter()
            .filter(|result| result.status == PortStatus::Open)
            .take(MAX_SHOWN_ROWS)
            .collect()
    }

    /// Renders the Port Scanner view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::PortScan(msg));

        let mut host_input = text_input(&fl!("ui-tool-portscan-host-watermark"), &self.host)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .width(Length::FillPortion(3));
        let mut ports_input = text_input(&fl!("ui-tool-portscan-ports-watermark"), &self.ports)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .width(Length::FillPortion(3));
        let mut timeout_input =
            text_input(&fl!("ui-tool-portscan-timeout-watermark"), &self.timeout)
                .font(super::BOX_FONT)
                .size(font_size::BODY_LARGE)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .width(Length::FillPortion(1));
        if !self.running {
            host_input = host_input
                .on_input(move |v| send(PortScanMessage::HostChanged(v)))
                .on_submit(send(PortScanMessage::Start));
            ports_input = ports_input.on_input(move |v| send(PortScanMessage::PortsChanged(v)));
            timeout_input =
                timeout_input.on_input(move |v| send(PortScanMessage::TimeoutChanged(v)));
        }

        let scan_button = if self.running {
            super::action_button(
                fl!("ui-tool-portscan-stop"),
                false,
                Some(send(PortScanMessage::Stop)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-portscan-start"),
                true,
                Some(send(PortScanMessage::Start)),
            )
        };

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-portscan-host")), host_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(3)),
            column![
                super::field_label(fl!("ui-tool-portscan-ports")),
                ports_input
            ]
            .spacing(spacing::XS)
            .width(Length::FillPortion(3)),
            column![
                super::field_label(fl!("ui-tool-portscan-timeout")),
                timeout_input
            ]
            .spacing(spacing::XS)
            .width(Length::FillPortion(1)),
            container(scan_button).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::End);

        let input_card = container(column![inputs_row])
            .padding(spacing::MD)
            .style(styles::card);
        let mut body = column![input_card].spacing(spacing::MD);

        if let Some(count) = self.confirming {
            let warning = row![
                text(fl!(
                    "ui-tool-portscan-large-warning",
                    count = count.to_string()
                ))
                .size(font_size::BODY)
                .style(text::warning)
                .width(Length::Fill),
                super::action_button(
                    fl!("ui-tool-portscan-confirm"),
                    false,
                    Some(send(PortScanMessage::Confirm)),
                ),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);
            body = body.push(container(warning).padding(spacing::MD).style(styles::card));
        }

        if let Some(err) = &self.error {
            body = body.push(
                container(text(err.as_str()).size(font_size::BODY).style(text::danger))
                    .padding(spacing::MD)
                    .style(styles::card),
            );
        }

        if self.running {
            body = body.push(
                text(fl!(
                    "ui-tool-portscan-progress",
                    done = self.results.len().to_string(),
                    total = self.total.to_string()
                ))
                .size(font_size::CAPTION)
                .style(text::secondary),
            );
        }

        if !self.results.is_empty() {
            let metric_card = |label: String, val: String| {
                container(
                    column![
                        text(label).size(font_size::CAPTION).style(text::secondary),
                        text(val).size(font_size::SUBTITLE).font(super::BOX_FONT),
                    ]
                    .spacing(spacing::XS),
                )
                .padding(spacing::SM)
                .style(styles::card)
                .width(Length::FillPortion(1))
            };

            let metrics_row = row![
                metric_card(
                    fl!("ui-tool-portscan-total"),
                    self.summary.total.to_string()
                ),
                metric_card(fl!("ui-tool-portscan-open"), self.summary.open.to_string()),
                metric_card(
                    fl!("ui-tool-portscan-closed"),
                    self.summary.closed.to_string()
                ),
                metric_card(
                    fl!("ui-tool-portscan-filtered"),
                    self.summary.filtered.to_string()
                ),
            ]
            .spacing(spacing::SM);

            let copy_csv_btn = super::copy_button(
                fl!("ui-tool-portscan-copy-csv"),
                state.copied(CopySlot::PortScanCsv),
                send(PortScanMessage::CopyCsv),
                super::COPY_PADDING,
            );

            let header_row = row![
                text(fl!("ui-tool-portscan-results-title"))
                    .size(font_size::SUBTITLE)
                    .width(Length::Fill),
                copy_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let mut list_col = column![].spacing(spacing::XS);
            let shown = self.shown();
            if shown.len() < self.results.len() {
                list_col = list_col.push(
                    text(fl!(
                        "ui-tool-portscan-shown-open",
                        shown = shown.len().to_string(),
                        total = self.results.len().to_string()
                    ))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
                );
            }
            for r in shown {
                let status_cell = match r.status {
                    PortStatus::Open => text(fl!("ui-tool-portscan-status-open"))
                        .size(font_size::BODY)
                        .style(text::success)
                        .width(Length::Fixed(90.0)),
                    PortStatus::Closed => text(fl!("ui-tool-portscan-status-closed"))
                        .size(font_size::BODY)
                        .style(text::secondary)
                        .width(Length::Fixed(90.0)),
                    PortStatus::Filtered => text(fl!("ui-tool-portscan-status-filtered"))
                        .size(font_size::BODY)
                        .style(text::warning)
                        .width(Length::Fixed(90.0)),
                };

                let lat_text = if r.latency_ms >= 0.0 {
                    fl!(
                        "ui-tool-portscan-latency",
                        ms = format!("{:.1}", r.latency_ms)
                    )
                } else {
                    fl!("ui-tool-portscan-no-latency")
                };

                let item_row = row![
                    text(fl!("ui-tool-portscan-port", port = r.port.to_string()))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                    text(r.service)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    status_cell,
                    text(lat_text)
                        .size(font_size::CAPTION)
                        .style(text::secondary)
                        .width(Length::Fixed(80.0)),
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center);

                list_col = list_col.push(
                    container(item_row)
                        .padding(spacing::XS)
                        .style(styles::card)
                        .width(Length::Fill),
                );
            }

            let results_card =
                container(column![header_row, metrics_row, list_col].spacing(spacing::MD))
                    .padding(spacing::MD)
                    .style(styles::card);

            body = body.push(results_card);
        }

        super::content_column(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use heimdall_core::tools::port_scanner::service_name_for_port;

    fn filled(host: &str, ports: &str) -> PortScanPane {
        let mut pane = PortScanPane::default();
        pane.apply(PortScanMessage::HostChanged(host.into()));
        pane.apply(PortScanMessage::PortsChanged(ports.into()));
        pane
    }

    fn closed(port: u16) -> PortProbeResult {
        PortProbeResult {
            port,
            service: service_name_for_port(port),
            status: PortStatus::Closed,
            latency_ms: -1.0,
        }
    }

    /// The scan started, its abort handle kept as `update` keeps it; returned, a handle
    /// that only watches it.
    fn started(pane: &mut PortScanPane) -> Handle {
        assert!(pane.start(false).is_some(), "{:?}", pane.error);
        let (_, handle) = Task::<()>::none().abortable();
        let watcher = handle.clone();
        pane.stop = Some(handle.abort_on_drop());
        watcher
    }

    #[test]
    fn enter_during_a_scan_starts_no_second_one_and_stop_aborts_it() {
        let mut pane = filled("192.0.2.1", "22,80");
        let handle = started(&mut pane);
        let run = pane.run;
        assert_eq!(pane.start(false), None);
        assert_eq!(pane.run, run);
        pane.apply(PortScanMessage::Stop);
        assert!(!pane.running);
        assert!(handle.is_aborted());
        pane.apply(PortScanMessage::Probed(run, vec![closed(22)]));
        assert!(pane.results.is_empty(), "nothing lands after Stop");
    }

    #[test]
    fn results_of_an_earlier_scan_never_land_in_a_later_one() {
        let mut pane = filled("192.0.2.1", "22,80");
        started(&mut pane);
        let first = pane.run;
        pane.apply(PortScanMessage::Stop);
        started(&mut pane);
        pane.apply(PortScanMessage::Probed(first, vec![closed(22)]));
        pane.apply(PortScanMessage::Ended(first));
        assert!(pane.results.is_empty() && pane.running);
    }

    #[test]
    fn results_are_kept_in_port_order_as_they_come() {
        let mut pane = filled("192.0.2.1", "22,80,443");
        started(&mut pane);
        pane.apply(PortScanMessage::Probed(
            pane.run,
            vec![closed(443), closed(22)],
        ));
        pane.apply(PortScanMessage::Probed(pane.run, vec![closed(80)]));
        let ports: Vec<u16> = pane.results.iter().map(|r| r.port).collect();
        assert_eq!(ports, [22, 80, 443]);
        assert_eq!(pane.summary.closed, 3);
        pane.apply(PortScanMessage::Ended(pane.run));
        assert!(!pane.running);
    }

    #[test]
    fn a_host_that_does_not_resolve_is_said_not_shown_filtered() {
        let mut pane = filled("nothing.invalid", "22");
        started(&mut pane);
        pane.apply(PortScanMessage::Unresolved(
            pane.run,
            ResolveFailure::NoAddress,
        ));
        assert!(!pane.running);
        assert!(pane.results.is_empty());
        assert_eq!(
            pane.error.as_deref(),
            Some(fl!("ui-tool-portscan-error-unresolved").as_str())
        );
    }

    #[test]
    fn inputs_are_validated_not_replaced() {
        let mut pane = filled("-sS", "22");
        assert_eq!(pane.start(false), None);
        assert_eq!(
            pane.error.as_deref(),
            Some(fl!("ui-tool-portscan-error-host-invalid").as_str())
        );
        let mut pane = filled("192.0.2.1", "22");
        pane.apply(PortScanMessage::TimeoutChanged("99999".into()));
        assert_eq!(pane.start(false), None);
        assert_eq!(
            pane.error.as_deref(),
            Some(fl!("ui-tool-portscan-error-timeout-invalid").as_str()),
            "not replaced by the default"
        );
    }

    #[test]
    fn a_scan_of_more_than_ten_thousand_ports_waits_for_its_confirmation() {
        let mut pane = filled("192.0.2.1", "1-20000");
        assert_eq!(pane.start(false), None);
        assert_eq!(pane.confirming, Some(20000));
        assert!(!pane.running);
        let request = pane.start(true).expect("confirmed");
        assert_eq!(request.ports.len(), 20000);
        assert!(pane.running && pane.confirming.is_none());
        let mut pane = filled("192.0.2.1", "1-20000");
        assert_eq!(
            pane.start(true),
            None,
            "never confirmed before it was asked"
        );
        pane.apply(PortScanMessage::PortsChanged("1-30000".into()));
        assert_eq!(pane.confirming, None, "a new range is asked again");
    }

    #[test]
    fn past_the_shown_rows_only_the_open_ports_are_listed() {
        let mut pane = filled("192.0.2.1", "1-1000");
        started(&mut pane);
        let mut results: Vec<PortProbeResult> = (1..=1000).map(closed).collect();
        results[442].status = PortStatus::Open;
        pane.apply(PortScanMessage::Probed(pane.run, results));
        let shown = pane.shown();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].port, 443);
    }
}
