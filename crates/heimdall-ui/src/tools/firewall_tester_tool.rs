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

//! The Firewall Egress Tester tool view: outbound egress port connectivity testing,
//! latency measurement, matrix summary, and CSV export.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use heimdall_app::TabId;
use iced::widget::{column, container, row, text, text_editor, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::firewall_tester::{
    self, DEFAULT_CONNECT_TIMEOUT_MS, FwInputError, FwProbeResult, FwProbeStatus, FwProbeSummary,
    MAX_HOSTS, MAX_PORTS,
};

use super::file_export::FileExport;
use super::network_run::{RunSlot, blocking_run};
use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// What the CSV export's suggested name starts with, as the C# `firewall_test_{now}.csv`.
const EXPORT_PREFIX: &str = "firewall_test";

/// Height of the hosts box, as the C# `TxtHosts` (`Height="80"`).
const HOSTS_BOX_HEIGHT: f32 = 80.0;

/// Messages emitted by the firewall tester view.
#[derive(Debug, Clone)]
pub enum FirewallTesterMessage {
    /// An action in the hosts box, one host per line as the C# `TxtHosts`.
    HostsAction(text_editor::Action),
    /// Ports input changed.
    PortsChanged(String),
    /// Start test.
    Start,
    /// Stop test.
    Stop,
    /// Test `.0` probed a host's port.
    Probed(u64, FwProbeResult),
    /// Test `.0` is over; `false` when its work failed.
    Ended(u64, bool),
    /// Save the CSV export to a file, as the C# `OnExportCsvClick`.
    ExportCsv,
}

/// The Firewall Egress Tester tool pane state.
#[derive(Debug)]
pub struct FirewallTesterPane {
    /// The hosts box: several lines, as the C# `AcceptsReturn` box.
    hosts: text_editor::Content,
    ports: String,
    run: RunSlot,
    error: Option<String>,
    /// What the last test says beside its results: the inputs cut to the C# limits, a stop.
    notice: Option<String>,
    results: Vec<FwProbeResult>,
    summary: FwProbeSummary,
    /// The hosts and ports the results are for, as the C# `GetLastHosts` and
    /// `GetLastPorts`: the matrix is theirs, whatever the boxes hold since.
    last_hosts: Vec<String>,
    last_ports: Vec<u16>,
    /// Probes the last test makes.
    total: usize,
}

impl Default for FirewallTesterPane {
    fn default() -> Self {
        Self {
            hosts: text_editor::Content::new(),
            ports: "80,443,8080".to_string(),
            run: RunSlot::default(),
            error: None,
            notice: None,
            results: Vec::new(),
            summary: FwProbeSummary::default(),
            last_hosts: Vec::new(),
            last_ports: Vec::new(),
            total: 0,
        }
    }
}

/// Outcome of updating the Firewall Tester tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    /// Save this export to the file the user picks.
    Save(FileExport),
}

/// What a message asks of the pane, before its tab is known.
#[derive(Debug)]
enum Step {
    None,
    /// Probe the last hosts and ports as run `.0`, watching stop `.1`.
    Test(u64, Arc<AtomicBool>),
    /// Save this export to the file the user picks.
    Save(FileExport),
}

impl FirewallTesterPane {
    /// What its first box holds, for the tests of what a tool opens on.
    #[cfg(test)]
    pub(super) fn first_box(&self) -> String {
        super::box_text(&self.hosts)
    }

    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: FirewallTesterMessage) -> Outcome {
        match self.apply(message) {
            Step::None => Outcome::None,
            Step::Save(export) => Outcome::Save(export),
            Step::Test(run, stop) => {
                let hosts = self.last_hosts.clone();
                let ports = self.last_ports.clone();
                let stream = blocking_run(
                    stop,
                    move |stop, probed| {
                        firewall_tester::run_matrix(
                            &hosts,
                            &ports,
                            DEFAULT_CONNECT_TIMEOUT_MS,
                            stop,
                            probed,
                        );
                    },
                    move |result: FwProbeResult| FirewallTesterMessage::Probed(run, result),
                    move |ended| FirewallTesterMessage::Ended(run, ended.is_some()),
                );
                let task = Task::run(stream, move |message| {
                    Message::Tool(tab, super::ToolMessage::FwTest(message))
                });
                Outcome::Task(self.run.attach(task))
            }
        }
    }

    /// What `message` changes, and what it asks.
    fn apply(&mut self, message: FirewallTesterMessage) -> Step {
        match message {
            FirewallTesterMessage::HostsAction(action) => {
                if action.is_edit() {
                    self.error = None;
                }
                self.hosts.perform(action);
                Step::None
            }
            FirewallTesterMessage::PortsChanged(val) => {
                self.ports = val;
                self.error = None;
                Step::None
            }
            FirewallTesterMessage::Start => self.start(),
            FirewallTesterMessage::Stop => {
                if self.run.running() {
                    self.run.stop();
                    self.notice = Some(fl!("ui-tool-fwtest-stopped"));
                }
                Step::None
            }
            FirewallTesterMessage::Probed(run, result) => {
                if self.run.is_current(run) {
                    let key = self.order(&result);
                    let at = self
                        .results
                        .partition_point(|shown| self.order(shown) < key);
                    self.results.insert(at, result);
                    self.summary = firewall_tester::compute_summary(&self.results);
                }
                Step::None
            }
            FirewallTesterMessage::Ended(run, completed) => {
                if self.run.finish(run) && !completed {
                    self.error = Some(fl!("ui-tool-fwtest-error-failed"));
                }
                Step::None
            }
            FirewallTesterMessage::ExportCsv => {
                if self.results.is_empty() {
                    return Step::None;
                }
                let csv = firewall_tester::build_matrix_csv(
                    &self.results,
                    &self.last_hosts,
                    &self.last_ports,
                    &fl!("ui-tool-fwtest-col-host"),
                    |result| match (result.status, result.latency_ms) {
                        (FwProbeStatus::Open, Some(ms)) => fl!(
                            "ui-tool-fwtest-cell-open-ms",
                            label = status_label(result.status),
                            ms = ms
                        ),
                        (status, _) => status_label(status),
                    },
                    &fl!("ui-tool-fwtest-not-tested"),
                );
                Step::Save(FileExport::csv(
                    CopySlot::FwTestCsv,
                    fl!("ui-tool-fwtest-export-csv"),
                    EXPORT_PREFIX,
                    None,
                    csv,
                ))
            }
        }
    }

    /// A test begun, as the C# `ParseAndValidateInputs` then `TestAsync`: invalid hosts left
    /// out, the C# limits kept and said.
    fn start(&mut self) -> Step {
        if self.run.running() {
            return Step::None;
        }
        let inputs =
            match firewall_tester::validate_inputs(&super::box_text(&self.hosts), &self.ports) {
                Ok(inputs) => inputs,
                Err(error) => {
                    self.error = Some(match error {
                        FwInputError::NoHosts => fl!("ui-tool-fwtest-error-hosts-required"),
                        FwInputError::InvalidHost => fl!("ui-tool-fwtest-error-invalid-host"),
                        FwInputError::NoPorts => fl!("ui-tool-fwtest-error-no-ports"),
                    });
                    return Step::None;
                }
            };
        self.error = None;
        self.notice = (inputs.hosts_left_out > 0 || inputs.ports_left_out > 0).then(|| {
            fl!(
                "ui-tool-fwtest-limited",
                max_hosts = MAX_HOSTS,
                max_ports = MAX_PORTS
            )
        });
        self.results.clear();
        self.summary = FwProbeSummary::default();
        self.total = inputs.hosts.len() * inputs.ports.len();
        self.last_hosts = inputs.hosts;
        self.last_ports = inputs.ports;
        let (run, stop) = self.run.begin();
        Step::Test(run, stop)
    }

    /// Where `result` goes among those shown: by host as typed, then by port, as the C#
    /// sorts them.
    fn order(&self, result: &FwProbeResult) -> (usize, u16) {
        let host = self
            .last_hosts
            .iter()
            .position(|host| *host == result.host)
            .unwrap_or(usize::MAX);
        (host, result.port)
    }

    /// Renders the Firewall Egress Tester tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::FwTest(msg));

        let hosts_input = super::text_box(&self.hosts, Some(fl!("ui-tool-fwtest-hosts-watermark")))
            .on_action(move |action| send(FirewallTesterMessage::HostsAction(action)))
            .height(Length::Fixed(HOSTS_BOX_HEIGHT));

        let ports_input = text_input(&fl!("ui-tool-fwtest-ports-watermark"), &self.ports)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(FirewallTesterMessage::PortsChanged(v)))
            .width(Length::FillPortion(1));

        let action_btn = if self.run.running() {
            super::action_button(
                fl!("ui-tool-fwtest-stop"),
                true,
                Some(send(FirewallTesterMessage::Stop)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-fwtest-start"),
                true,
                Some(send(FirewallTesterMessage::Start)),
            )
        };

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-fwtest-hosts")), hosts_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(3)),
            column![super::field_label(fl!("ui-tool-fwtest-ports")), ports_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(1)),
            container(action_btn).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Start);

        let input_card = container(column![inputs_row])
            .padding(spacing::MD)
            .style(styles::card);

        let mut body = column![input_card].spacing(spacing::MD);

        if self.total > 0 && (self.run.running() || !self.results.is_empty()) {
            body = body.push(
                text(fl!(
                    "ui-tool-fwtest-progress",
                    done = self.results.len(),
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
            let summary_cards = row![
                container(
                    column![
                        text(fl!("ui-tool-fwtest-total"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.total.to_string())
                            .size(font_size::TITLE)
                            .font(super::BOX_FONT),
                    ]
                    .spacing(spacing::XS)
                )
                .padding(spacing::SM)
                .style(styles::card)
                .width(Length::FillPortion(1)),
                container(
                    column![
                        text(fl!("ui-tool-fwtest-open"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.open.to_string())
                            .size(font_size::TITLE)
                            .font(super::BOX_FONT)
                            .style(text::success),
                    ]
                    .spacing(spacing::XS)
                )
                .padding(spacing::SM)
                .style(styles::card)
                .width(Length::FillPortion(1)),
                container(
                    column![
                        text(fl!("ui-tool-fwtest-closed"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.closed.to_string())
                            .size(font_size::TITLE)
                            .font(super::BOX_FONT)
                            .style(text::secondary),
                    ]
                    .spacing(spacing::XS)
                )
                .padding(spacing::SM)
                .style(styles::card)
                .width(Length::FillPortion(1)),
                container(
                    column![
                        text(fl!("ui-tool-fwtest-timeout"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.timeout.to_string())
                            .size(font_size::TITLE)
                            .font(super::BOX_FONT)
                            .style(text::danger),
                    ]
                    .spacing(spacing::XS)
                )
                .padding(spacing::SM)
                .style(styles::card)
                .width(Length::FillPortion(1)),
            ]
            .spacing(spacing::SM);

            body = body.push(summary_cards);
            if self.summary.unresolved > 0 {
                body = body.push(
                    text(fl!(
                        "ui-tool-fwtest-unresolved-count",
                        count = self.summary.unresolved
                    ))
                    .size(font_size::BODY)
                    .style(text::warning),
                );
            }

            let export_csv_btn = super::export_button(
                fl!("ui-tool-fwtest-export-csv"),
                state.copied(CopySlot::FwTestCsv),
                send(FirewallTesterMessage::ExportCsv),
            );

            let header_row = row![
                text(fl!("ui-tool-fwtest-name"))
                    .size(font_size::SUBTITLE)
                    .width(Length::Fill),
                export_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let table_headers = row![
                text(fl!("ui-tool-fwtest-col-host"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-fwtest-col-port"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(80.0)),
                text(fl!("ui-tool-fwtest-col-status"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(110.0)),
                text(fl!("ui-tool-fwtest-col-latency"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
            ]
            .spacing(spacing::SM);

            let mut probe_rows = column![table_headers].spacing(spacing::XS);

            for r in &self.results {
                let style = match r.status {
                    FwProbeStatus::Open => text::success,
                    FwProbeStatus::Closed => text::secondary,
                    FwProbeStatus::Timeout => text::danger,
                    FwProbeStatus::Unresolved => text::warning,
                };
                let status_el = text(status_label(r.status))
                    .size(font_size::BODY)
                    .style(style)
                    .width(Length::Fixed(110.0));

                let lat_str = match r.latency_ms {
                    Some(ms) => fl!("ui-tool-fwtest-latency-ms", ms = ms),
                    None => "-".to_string(),
                };

                let row_el = row![
                    text(&r.host)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    text(r.port.to_string())
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(80.0)),
                    status_el,
                    text(lat_str)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                ]
                .spacing(spacing::SM);

                probe_rows = probe_rows.push(row_el);
            }

            let card = container(column![header_row, probe_rows].spacing(spacing::MD))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        }

        super::content_column(body)
    }
}

/// A probe's status, as the view and the CSV say it.
fn status_label(status: FwProbeStatus) -> String {
    match status {
        FwProbeStatus::Open => fl!("ui-tool-fwtest-open"),
        FwProbeStatus::Closed => fl!("ui-tool-fwtest-closed"),
        FwProbeStatus::Timeout => fl!("ui-tool-fwtest-timeout"),
        FwProbeStatus::Unresolved => fl!("ui-tool-fwtest-unresolved"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    fn probed(host: &str, port: u16, status: FwProbeStatus) -> FwProbeResult {
        FwProbeResult {
            host: host.to_owned(),
            port,
            status,
            latency_ms: (status == FwProbeStatus::Open).then_some(4),
        }
    }

    /// `hosts` typed in the hosts box, replacing what it held.
    fn type_hosts(pane: &mut FirewallTesterPane, hosts: &str) {
        pane.apply(FirewallTesterMessage::HostsAction(
            text_editor::Action::SelectAll,
        ));
        pane.apply(FirewallTesterMessage::HostsAction(
            text_editor::Action::Edit(text_editor::Edit::Paste(Arc::new(hosts.to_owned()))),
        ));
    }

    fn started(pane: &mut FirewallTesterPane, hosts: &str, ports: &str) -> (u64, Arc<AtomicBool>) {
        type_hosts(pane, hosts);
        pane.apply(FirewallTesterMessage::PortsChanged(ports.to_owned()));
        match pane.apply(FirewallTesterMessage::Start) {
            Step::Test(run, stop) => (run, stop),
            other => panic!("no test: {other:?}"),
        }
    }

    #[test]
    fn the_hosts_box_holds_one_host_per_line_as_the_csharp() {
        let mut pane = FirewallTesterPane::default();
        started(&mut pane, "a.lab\nb.lab\n\n  c.lab, a.lab\n", "80");
        assert_eq!(pane.last_hosts, ["a.lab", "b.lab", "c.lab"]);
        assert_eq!(pane.total, 3);
    }

    #[test]
    fn the_csharp_limits_hold_and_are_said() {
        let mut pane = FirewallTesterPane::default();
        let hosts: Vec<String> = (1..=70).map(|n| format!("10.0.1.{n}")).collect();
        started(&mut pane, &hosts.join(","), "1-100");
        assert_eq!(pane.last_hosts.len(), MAX_HOSTS);
        assert_eq!(pane.last_ports.len(), MAX_PORTS);
        assert_eq!(pane.total, MAX_HOSTS * MAX_PORTS);
        assert!(pane.notice.is_some(), "what was left out is said");
        pane.apply(FirewallTesterMessage::Stop);
        pane.apply(FirewallTesterMessage::PortsChanged(String::new()));
        assert!(matches!(
            pane.apply(FirewallTesterMessage::Start),
            Step::None
        ));
        assert!(pane.error.is_some(), "no port, as the C#");
    }

    #[test]
    fn results_stream_in_sorted_and_the_csv_is_the_run_s_whatever_the_boxes_hold() {
        let mut pane = FirewallTesterPane::default();
        let (run, _) = started(&mut pane, "b.lab a.lab", "443,80");
        pane.apply(FirewallTesterMessage::Probed(
            run,
            probed("a.lab", 443, FwProbeStatus::Closed),
        ));
        pane.apply(FirewallTesterMessage::Probed(
            run,
            probed("b.lab", 80, FwProbeStatus::Open),
        ));
        pane.apply(FirewallTesterMessage::Probed(
            run,
            probed("a.lab", 80, FwProbeStatus::Unresolved),
        ));
        let order: Vec<(&str, u16)> = pane
            .results
            .iter()
            .map(|r| (r.host.as_str(), r.port))
            .collect();
        assert_eq!(order, [("b.lab", 80), ("a.lab", 80), ("a.lab", 443)]);
        assert_eq!(pane.summary.unresolved, 1);
        pane.apply(FirewallTesterMessage::Ended(run, true));
        type_hosts(&mut pane, "other.lab");
        pane.apply(FirewallTesterMessage::PortsChanged("22".to_owned()));
        let Step::Save(export) = pane.apply(FirewallTesterMessage::ExportCsv) else {
            panic!("a CSV");
        };
        assert_eq!(export.slot(), CopySlot::FwTestCsv);
        assert!(export.suggested_name().starts_with("firewall_test_"));
        let csv = export.content();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 3, "the two hosts tested: {csv}");
        assert!(lines[0].ends_with(",80,443"), "the ports tested: {csv}");
        assert!(lines[1].starts_with("\"b.lab\""));
    }

    #[tokio::test]
    async fn a_refused_port_is_closed_not_filtered() {
        use std::net::{Ipv4Addr, SocketAddr};

        // The port stays taken by a socket bound but not listening, so a parallel test
        // cannot listen on it in between: the connection is refused, never accepted.
        let socket = tokio::net::TcpSocket::new_v4().expect("socket");
        socket
            .bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .expect("bind");
        let port = socket.local_addr().expect("addr").port();
        // Windows tries a refused connection again for about two seconds: the timeout
        // leaves it room.
        let probed = tokio::task::spawn_blocking(move || {
            firewall_tester::probe_host_port("127.0.0.1", port, 5000)
        })
        .await
        .expect("probed");
        assert_eq!(probed.status, FwProbeStatus::Closed);
        drop(socket);
    }

    #[test]
    fn a_stop_tells_the_probes_and_lets_late_results_go() {
        let mut pane = FirewallTesterPane::default();
        let (run, stop) = started(&mut pane, "a.lab", "80,443");
        pane.apply(FirewallTesterMessage::Probed(
            run,
            probed("a.lab", 80, FwProbeStatus::Open),
        ));
        pane.apply(FirewallTesterMessage::Stop);
        assert!(stop.load(Ordering::Relaxed));
        pane.apply(FirewallTesterMessage::Probed(
            run,
            probed("a.lab", 443, FwProbeStatus::Open),
        ));
        assert_eq!(pane.results.len(), 1);
        assert!(!pane.run.running());
    }
}
