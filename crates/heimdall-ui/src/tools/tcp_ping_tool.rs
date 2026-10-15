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

//! TCP Ping tool pane for measuring connection handshake latency, as the C#
//! `TcpPingViewModel`: the probes come back one by one, and Stop ends the run at once.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::module_name_repetitions,
    clippy::redundant_closure_for_method_calls,
    clippy::too_many_lines
)]

use heimdall_app::TabId;
use heimdall_app::network_tools;
use heimdall_app::reachability::DownReason;
use heimdall_core::tools::net_input;
use heimdall_core::tools::tcp_ping::{
    self, CONNECT_TIMEOUT, DEFAULT_COUNT, DEFAULT_PORT, DELAY_BETWEEN_PROBES, TcpPingSummary,
};
use iced::futures::SinkExt as _;
use iced::task::Handle;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Probes buffered between the run and the pane.
const CHANNEL_SIZE: usize = 16;

/// Milliseconds in a second, for a probe's time.
const MILLIS_PER_SECOND: f64 = 1000.0;

/// One probe of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    /// Its number in the run, from 1.
    pub seq: usize,
    /// Its handshake time in milliseconds, or why it got no answer.
    pub outcome: Result<f64, DownReason>,
}

/// Messages emitted by the TCP Ping tool.
#[derive(Debug, Clone)]
pub enum TcpPingMessage {
    /// Target host changed.
    HostChanged(String),
    /// Target port changed.
    PortChanged(String),
    /// Probe count changed.
    CountChanged(String),
    /// Start a run.
    Start,
    /// Stop the run under way.
    Stop,
    /// A probe of run `.0` ended.
    Probed(u64, Probe),
    /// The host of run `.0` could not be looked up.
    Unresolved(u64, DownReason),
    /// Run `.0` sent all its probes.
    Finished(u64),
    /// Copy summary and results log to clipboard.
    CopyResults,
}

/// What the tool says went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Problem {
    /// No host, the C# `ToolValidationHostRequired`.
    HostRequired,
    /// Not a host the C# `InputValidator` "Address" rule accepts, `ToolValidationInvalidHost`.
    InvalidHost,
    /// Not a port from 1 to 65535.
    InvalidPort,
    /// Not a count from 1 to 10000, the C# `ToolTcpPingErrorCount`.
    InvalidCount,
    /// The host's name could not be looked up.
    Unresolved(DownReason),
}

/// The TCP Ping tool pane state.
#[derive(Debug)]
pub struct TcpPingPane {
    host: String,
    port: String,
    count: String,
    /// The run whose probes are shown: those of an earlier one are let go.
    run: u64,
    /// The run under way, aborted when stopped or when the tab closes.
    running: Option<Handle>,
    problem: Option<Problem>,
    /// `host:port` of the run shown.
    target: String,
    /// Probes the run shown asked for.
    total: usize,
    probes: Vec<Probe>,
}

impl Default for TcpPingPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: DEFAULT_PORT.to_string(),
            count: DEFAULT_COUNT.to_string(),
            run: 0,
            running: None,
            problem: None,
            target: String::new(),
            total: 0,
            probes: Vec::new(),
        }
    }
}

/// Outcome of updating the TCP Ping tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

/// What a message did, before its tab is known.
#[derive(Debug)]
enum Applied {
    None,
    Run(Task<TcpPingMessage>),
    Copy(CopySlot, String),
}

impl TcpPingPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: TcpPingMessage) -> Outcome {
        match self.apply(message) {
            Applied::None => Outcome::None,
            Applied::Run(task) => Outcome::Task(
                task.map(move |message| Message::Tool(tab, ToolMessage::TcpPing(message))),
            ),
            Applied::Copy(slot, content) => Outcome::Copy(slot, content),
        }
    }

    /// Handles a message, as [`Self::update`] without the tab.
    fn apply(&mut self, message: TcpPingMessage) -> Applied {
        match message {
            TcpPingMessage::HostChanged(val) => {
                self.host = val;
                self.problem = None;
                Applied::None
            }
            TcpPingMessage::PortChanged(val) => {
                self.port = val;
                self.problem = None;
                Applied::None
            }
            TcpPingMessage::CountChanged(val) => {
                self.count = val;
                self.problem = None;
                Applied::None
            }
            TcpPingMessage::Start => self.start(),
            TcpPingMessage::Stop => {
                if let Some(running) = self.running.take() {
                    running.abort();
                }
                Applied::None
            }
            TcpPingMessage::Probed(run, probe) => {
                if run == self.run && self.running.is_some() {
                    self.probes.push(probe);
                }
                Applied::None
            }
            TcpPingMessage::Unresolved(run, reason) => {
                if run == self.run && self.running.is_some() {
                    self.running = None;
                    self.problem = Some(Problem::Unresolved(reason));
                }
                Applied::None
            }
            TcpPingMessage::Finished(run) => {
                if run == self.run {
                    self.running = None;
                }
                Applied::None
            }
            TcpPingMessage::CopyResults => {
                let mut out = String::new();
                if let Some(summary) = self.summary() {
                    out.push_str(&summary);
                    out.push_str(super::NEW_LINE);
                    out.push_str(super::NEW_LINE);
                }
                for probe in &self.probes {
                    out.push_str(&self.probe_line(probe));
                    out.push_str(super::NEW_LINE);
                }
                Applied::Copy(CopySlot::TcpPingResults, out)
            }
        }
    }

    /// Starts a run, the inputs checked in the C# `StartAsync` order.
    fn start(&mut self) -> Applied {
        if self.running.is_some() {
            return Applied::None;
        }
        let host = self.host.trim().to_owned();
        let checked = if host.is_empty() {
            Err(Problem::HostRequired)
        } else if !net_input::is_address(&host) {
            Err(Problem::InvalidHost)
        } else {
            net_input::parse_port(&self.port)
                .ok_or(Problem::InvalidPort)
                .and_then(|port| {
                    tcp_ping::parse_count(&self.count)
                        .map(|count| (port, count))
                        .ok_or(Problem::InvalidCount)
                })
        };
        let (port, count) = match checked {
            Ok(checked) => checked,
            Err(problem) => {
                self.problem = Some(problem);
                return Applied::None;
            }
        };

        self.run += 1;
        self.problem = None;
        self.probes.clear();
        self.target = format!("{host}:{port}");
        self.total = count;
        let (task, handle) =
            Task::run(probes(host, port, count, self.run), |message| message).abortable();
        self.running = Some(handle.abort_on_drop());
        Applied::Run(task)
    }

    /// The summary of the probes shown, once the run is over, as the C# `BuildSummaryText`:
    /// no time when none answered.
    fn summary(&self) -> Option<String> {
        if self.running.is_some() {
            return None;
        }
        let latencies: Vec<Option<f64>> = self
            .probes
            .iter()
            .map(|probe| probe.outcome.as_ref().ok().copied())
            .collect();
        let summary = TcpPingSummary::from_latencies(&latencies)?;
        let pct = format!("{:.0}", summary.loss_percentage());
        Some(if summary.all_lost() {
            fl!(
                "ui-tool-tcpping-summary-all-lost",
                lost = summary.lost,
                total = summary.total,
                pct = pct
            )
        } else {
            fl!(
                "ui-tool-tcpping-summary",
                min = format!("{:.1}", summary.min_ms),
                avg = format!("{:.1}", summary.avg_ms),
                max = format!("{:.1}", summary.max_ms),
                lost = summary.lost,
                total = summary.total,
                pct = pct
            )
        })
    }

    /// A probe's line, as the C# `FormatProbeLine`.
    fn probe_line(&self, probe: &Probe) -> String {
        match &probe.outcome {
            Ok(millis) => fl!(
                "ui-tool-tcpping-probe-ok",
                seq = probe.seq,
                total = self.total,
                target = self.target.as_str(),
                ms = format!("{millis:.1}")
            ),
            Err(reason) => fl!(
                "ui-tool-tcpping-probe-failed",
                seq = probe.seq,
                total = self.total,
                target = self.target.as_str(),
                reason = super::down_reason_text(reason)
            ),
        }
    }

    /// What the tool says went wrong, in the user's language.
    fn problem_text(problem: &Problem) -> String {
        match problem {
            Problem::HostRequired => fl!("ui-tool-net-error-host-required"),
            Problem::InvalidHost => fl!("ui-tool-net-error-invalid-host"),
            Problem::InvalidPort => fl!("ui-tool-net-error-port"),
            Problem::InvalidCount => fl!("ui-tool-tcpping-error-count"),
            Problem::Unresolved(reason) => super::down_reason_text(reason),
        }
    }

    /// Renders the TCP Ping tool pane.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &'a ToolPane,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let colors = crate::themes::colors_of(theme);
        let default_port = DEFAULT_PORT.to_string();
        let default_count = DEFAULT_COUNT.to_string();

        let host_input = column![
            text(fl!("ui-tool-tcpping-host-label"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text_input(&fl!("ui-tool-tcpping-host-placeholder"), &self.host)
                .on_input(move |v| Message::Tool(
                    tab,
                    ToolMessage::TcpPing(TcpPingMessage::HostChanged(v))
                ))
                .on_submit(Message::Tool(
                    tab,
                    ToolMessage::TcpPing(TcpPingMessage::Start)
                ))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::FillPortion(5))
        .spacing(spacing::XS);

        let port_input = column![
            text(fl!("ui-tool-tcpping-port-label"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text_input(&default_port, &self.port)
                .on_input(move |v| Message::Tool(
                    tab,
                    ToolMessage::TcpPing(TcpPingMessage::PortChanged(v))
                ))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::FillPortion(2))
        .spacing(spacing::XS);

        let count_input = column![
            text(fl!("ui-tool-tcpping-count-label"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text_input(&default_count, &self.count)
                .on_input(move |v| Message::Tool(
                    tab,
                    ToolMessage::TcpPing(TcpPingMessage::CountChanged(v))
                ))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::FillPortion(2))
        .spacing(spacing::XS);

        let start_btn = if self.running.is_some() {
            button(
                text(fl!("ui-tool-tcpping-btn-stop"))
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD),
            )
            .padding(super::PRIMARY_PADDING)
            .style(styles::danger)
            .on_press(Message::Tool(
                tab,
                ToolMessage::TcpPing(TcpPingMessage::Stop),
            ))
        } else {
            button(
                text(fl!("ui-tool-tcpping-btn-start"))
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD),
            )
            .padding(super::PRIMARY_PADDING)
            .style(styles::primary)
            .on_press(Message::Tool(
                tab,
                ToolMessage::TcpPing(TcpPingMessage::Start),
            ))
        };

        let controls_row = row![
            host_input,
            port_input,
            count_input,
            container(start_btn).align_y(Alignment::End),
        ]
        .spacing(spacing::MD)
        .align_y(Alignment::End);

        let mut body_col = column!().spacing(spacing::MD);

        if let Some(problem) = &self.problem {
            body_col = body_col.push(
                text(Self::problem_text(problem))
                    .size(font_size::BODY)
                    .color(colors.red),
            );
        }

        if let Some(summary) = self.summary() {
            let summary_card = container(
                row![
                    text(summary)
                        .size(font_size::BODY)
                        .font(styles::SEMIBOLD)
                        .width(Length::Fill),
                    super::copy_button(
                        fl!("ui-tool-tcpping-btn-copy"),
                        state.copied(CopySlot::TcpPingResults),
                        Message::Tool(tab, ToolMessage::TcpPing(TcpPingMessage::CopyResults),),
                        super::COPY_PADDING,
                    ),
                ]
                .align_y(Alignment::Center),
            )
            .padding([8.0, 12.0])
            .width(Length::Fill)
            .style(styles::card);

            body_col = body_col.push(summary_card);
        }

        if self.probes.is_empty() && self.running.is_none() {
            body_col = body_col.push(
                container(
                    text(fl!("ui-tool-tcpping-empty-state"))
                        .size(font_size::BODY)
                        .style(text::secondary),
                )
                .padding(super::EMPTY_STATE_PADDING),
            );
        } else {
            let mut list = column!().spacing(spacing::XS);
            for probe in &self.probes {
                let color = if probe.outcome.is_ok() {
                    colors.green
                } else {
                    colors.red
                };
                list = list.push(
                    text(self.probe_line(probe))
                        .size(font_size::CAPTION)
                        .font(super::BOX_FONT)
                        .color(color),
                );
            }

            body_col = body_col.push(scrollable(list).height(Length::Fill));
        }

        let content = column![controls_row, body_col]
            .spacing(spacing::LG)
            .padding(super::BODY_PADDING)
            .max_width(super::CONTENT_MAX_WIDTH);

        content.into()
    }
}

/// The probes of a run, as the C# `StartAsync` loop: the host looked up once, then `count`
/// probes [`DELAY_BETWEEN_PROBES`] apart, each sent back as it ends.
fn probes(
    host: String,
    port: u16,
    count: usize,
    run: u64,
) -> impl iced::futures::Stream<Item = TcpPingMessage> {
    iced::stream::channel(CHANNEL_SIZE, async move |mut out| {
        let addresses = match network_tools::resolve(&host, port, CONNECT_TIMEOUT).await {
            Ok(addresses) => addresses,
            Err(reason) => {
                let _ = out.send(TcpPingMessage::Unresolved(run, reason)).await;
                return;
            }
        };
        for seq in 1..=count {
            let outcome = network_tools::connect_time(&addresses, CONNECT_TIMEOUT)
                .await
                .map(|elapsed| elapsed.as_secs_f64() * MILLIS_PER_SECOND);
            if out
                .send(TcpPingMessage::Probed(run, Probe { seq, outcome }))
                .await
                .is_err()
            {
                return;
            }
            if seq < count {
                tokio::time::sleep(DELAY_BETWEEN_PROBES).await;
            }
        }
        let _ = out.send(TcpPingMessage::Finished(run)).await;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started(pane: &mut TcpPingPane) -> Handle {
        pane.apply(TcpPingMessage::HostChanged("localhost".to_owned()));
        assert!(matches!(pane.apply(TcpPingMessage::Start), Applied::Run(_)));
        pane.running.clone().expect("a run under way")
    }

    #[test]
    fn stop_aborts_the_run_and_its_late_probes_are_ignored() {
        let mut pane = TcpPingPane::default();
        let run = started(&mut pane);
        let probe = Probe {
            seq: 1,
            outcome: Ok(1.0),
        };
        pane.apply(TcpPingMessage::Probed(pane.run, probe.clone()));
        pane.apply(TcpPingMessage::Stop);
        assert!(run.is_aborted());
        assert!(pane.running.is_none());
        pane.apply(TcpPingMessage::Probed(pane.run, probe.clone()));
        assert_eq!(pane.probes, [probe]);
    }

    #[test]
    fn a_stale_run_cannot_touch_a_new_one() {
        let mut pane = TcpPingPane::default();
        drop(started(&mut pane));
        let old = pane.run;
        pane.apply(TcpPingMessage::Stop);
        drop(started(&mut pane));
        let late = Probe {
            seq: 3,
            outcome: Ok(2.0),
        };
        pane.apply(TcpPingMessage::Probed(old, late));
        pane.apply(TcpPingMessage::Finished(old));
        assert!(pane.probes.is_empty());
        assert!(pane.running.is_some(), "the new run goes on");
    }

    #[test]
    fn start_is_refused_while_a_run_is_under_way() {
        let mut pane = TcpPingPane::default();
        drop(started(&mut pane));
        let run = pane.run;
        assert!(matches!(pane.apply(TcpPingMessage::Start), Applied::None));
        assert_eq!(pane.run, run);
    }

    #[test]
    fn the_inputs_are_checked_instead_of_replaced() {
        let mut pane = TcpPingPane::default();
        let refused = |pane: &mut TcpPingPane, problem: Problem| {
            assert!(matches!(pane.apply(TcpPingMessage::Start), Applied::None));
            assert_eq!(pane.problem, Some(problem));
            assert!(pane.running.is_none());
        };
        refused(&mut pane, Problem::HostRequired);
        pane.apply(TcpPingMessage::HostChanged("bad host".to_owned()));
        refused(&mut pane, Problem::InvalidHost);
        pane.apply(TcpPingMessage::HostChanged("example.com".to_owned()));
        for port in ["http", "0", "65536"] {
            pane.apply(TcpPingMessage::PortChanged(port.to_owned()));
            refused(&mut pane, Problem::InvalidPort);
        }
        pane.apply(TcpPingMessage::PortChanged("22".to_owned()));
        pane.apply(TcpPingMessage::CountChanged("0".to_owned()));
        refused(&mut pane, Problem::InvalidCount);
    }
}
