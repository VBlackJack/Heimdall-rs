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

//! Banner Grabber tool pane for probing network ports and inspecting service banners, as
//! the C# `BannerGrabViewModel`: [`MAX_CONCURRENT`] ports probed at once, each result shown
//! as it comes, and Stop ends the run at once.

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
use heimdall_core::tools::banner_grab::{
    self, BannerProbeResult, CONNECT_TIMEOUT, ExportLabels, MAX_CONCURRENT, READ_TIMEOUT,
};
use heimdall_core::tools::net_input;
use iced::futures::{SinkExt as _, StreamExt as _};
use iced::task::Handle;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Default common ports string.
const DEFAULT_PORTS: &str = "21,22,23,25,80,110,143,443,3306,3389,5432,6379,8080";
const WEB_PORTS: &str = "80,443,8000,8080,8443,8888,9000,9090";
const MAIL_PORTS: &str = "25,110,143,465,587,993,995";
const DB_PORTS: &str = "1433,1521,3306,5432,6379,27017";

/// Results buffered between the run and the pane.
const CHANNEL_SIZE: usize = 32;

/// The port looked up with the host: each probe sets its own.
const LOOKUP_PORT: u16 = 0;

/// Messages emitted by the Banner Grabber tool.
#[derive(Debug, Clone)]
pub enum BannerMessage {
    /// Target host changed.
    HostChanged(String),
    /// Ports specification changed.
    PortsChanged(String),
    /// Select common ports preset.
    SelectPresetCommon,
    /// Select web ports preset.
    SelectPresetWeb,
    /// Select mail ports preset.
    SelectPresetMail,
    /// Select database ports preset.
    SelectPresetDb,
    /// Start banner grabbing.
    Start,
    /// Stop current scan.
    Stop,
    /// A port of run `.0` was probed.
    Probed(u64, BannerProbeResult),
    /// The host of run `.0` could not be looked up.
    Unresolved(u64, DownReason),
    /// Run `.0` probed all its ports.
    Finished(u64),
    /// Copy tabulated text to clipboard.
    CopyResults,
    /// Export CSV to clipboard.
    ExportCsv,
}

/// What the tool says went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Problem {
    /// No host, the C# `ToolValidationHostRequired`.
    HostRequired,
    /// Not a host the C# `InputValidator` "Address" rule accepts, the C# `ErrorInvalidHost`.
    InvalidHost,
    /// No port could be read, the C# `ToolValidationPortRangeRequired`.
    PortRangeRequired,
    /// The host's name could not be looked up.
    Unresolved(DownReason),
}

/// The Banner Grabber tool pane state.
#[derive(Debug)]
pub struct BannerPane {
    host: String,
    ports: String,
    /// The run whose results are shown: those of an earlier one are let go.
    run: u64,
    /// The run under way, aborted when stopped or when the tab closes.
    running: Option<Handle>,
    problem: Option<Problem>,
    /// The results of the run shown, ordered by port.
    results: Vec<BannerProbeResult>,
    /// Ports the run shown probes.
    total: usize,
}

impl Default for BannerPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            ports: DEFAULT_PORTS.to_string(),
            run: 0,
            running: None,
            problem: None,
            results: Vec::new(),
            total: 0,
        }
    }
}

/// Outcome of updating the Banner Grabber tool pane.
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
    Run(Task<BannerMessage>),
    Copy(CopySlot, String),
}

impl BannerPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: BannerMessage) -> Outcome {
        match self.apply(message) {
            Applied::None => Outcome::None,
            Applied::Run(task) => Outcome::Task(
                task.map(move |message| Message::Tool(tab, ToolMessage::Banner(message))),
            ),
            Applied::Copy(slot, content) => Outcome::Copy(slot, content),
        }
    }

    /// Handles a message, as [`Self::update`] without the tab.
    fn apply(&mut self, message: BannerMessage) -> Applied {
        match message {
            BannerMessage::HostChanged(val) => {
                self.host = val;
                self.problem = None;
                Applied::None
            }
            BannerMessage::PortsChanged(val) => {
                self.ports = val;
                self.problem = None;
                Applied::None
            }
            BannerMessage::SelectPresetCommon => {
                self.ports = DEFAULT_PORTS.to_string();
                Applied::None
            }
            BannerMessage::SelectPresetWeb => {
                self.ports = WEB_PORTS.to_string();
                Applied::None
            }
            BannerMessage::SelectPresetMail => {
                self.ports = MAIL_PORTS.to_string();
                Applied::None
            }
            BannerMessage::SelectPresetDb => {
                self.ports = DB_PORTS.to_string();
                Applied::None
            }
            BannerMessage::Start => self.start(),
            BannerMessage::Stop => {
                if let Some(running) = self.running.take() {
                    running.abort();
                }
                Applied::None
            }
            BannerMessage::Probed(run, result) => {
                if run == self.run && self.running.is_some() {
                    let at = self
                        .results
                        .partition_point(|shown| shown.port < result.port);
                    self.results.insert(at, result);
                }
                Applied::None
            }
            BannerMessage::Unresolved(run, reason) => {
                if run == self.run && self.running.is_some() {
                    self.running = None;
                    self.problem = Some(Problem::Unresolved(reason));
                }
                Applied::None
            }
            BannerMessage::Finished(run) => {
                if run == self.run {
                    self.running = None;
                }
                Applied::None
            }
            BannerMessage::CopyResults => Applied::Copy(
                CopySlot::BannerGrabResults,
                banner_grab::build_clipboard_text(
                    &self.results,
                    &export_labels(),
                    millis_text,
                    super::NEW_LINE,
                ),
            ),
            BannerMessage::ExportCsv => Applied::Copy(
                CopySlot::BannerGrabCsv,
                banner_grab::build_csv_export(
                    &self.results,
                    &export_labels(),
                    millis_text,
                    super::NEW_LINE,
                ),
            ),
        }
    }

    /// Starts a run, the inputs checked in the C# `GrabAsync` order.
    fn start(&mut self) -> Applied {
        if self.running.is_some() {
            return Applied::None;
        }
        let host = self.host.trim().to_string();
        let ports = banner_grab::parse_ports(&self.ports);
        let problem = if host.is_empty() {
            Some(Problem::HostRequired)
        } else if !net_input::is_address(&host) {
            Some(Problem::InvalidHost)
        } else if ports.is_empty() {
            Some(Problem::PortRangeRequired)
        } else {
            None
        };
        if let Some(problem) = problem {
            self.problem = Some(problem);
            self.results.clear();
            self.total = 0;
            return Applied::None;
        }

        self.run += 1;
        self.problem = None;
        self.results.clear();
        self.total = ports.len();
        let (task, handle) = Task::run(grabs(host, ports, self.run), |message| message).abortable();
        self.running = Some(handle.abort_on_drop());
        Applied::Run(task)
    }

    /// What the tool says went wrong, in the user's language.
    fn problem_text(problem: &Problem) -> String {
        match problem {
            Problem::HostRequired => fl!("ui-tool-net-error-host-required"),
            Problem::InvalidHost => fl!("ui-tool-net-error-invalid-host"),
            Problem::PortRangeRequired => fl!("ui-tool-net-error-port-range"),
            Problem::Unresolved(reason) => super::down_reason_text(reason),
        }
    }

    /// Renders the Banner Grabber tool pane.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &'a ToolPane,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let colors = crate::themes::colors_of(theme);
        let no_value = fl!("ui-tool-net-no-value");

        let host_input = column![
            text(fl!("ui-tool-banner-host-label"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text_input(&fl!("ui-tool-banner-host-placeholder"), &self.host)
                .on_input(move |v| Message::Tool(
                    tab,
                    ToolMessage::Banner(BannerMessage::HostChanged(v))
                ))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::FillPortion(3))
        .spacing(spacing::XS);

        let ports_input = column![
            text(fl!("ui-tool-banner-ports-label"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text_input(&fl!("ui-tool-banner-ports-placeholder"), &self.ports)
                .on_input(move |v| Message::Tool(
                    tab,
                    ToolMessage::Banner(BannerMessage::PortsChanged(v))
                ))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
        ]
        .width(Length::FillPortion(4))
        .spacing(spacing::XS);

        let grab_btn = if self.running.is_some() {
            button(
                text(fl!("ui-tool-banner-btn-stop"))
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD),
            )
            .padding(super::PRIMARY_PADDING)
            .style(styles::danger)
            .on_press(Message::Tool(tab, ToolMessage::Banner(BannerMessage::Stop)))
        } else {
            button(
                text(fl!("ui-tool-banner-btn-grab"))
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD),
            )
            .padding(super::PRIMARY_PADDING)
            .style(styles::primary)
            .on_press(Message::Tool(
                tab,
                ToolMessage::Banner(BannerMessage::Start),
            ))
        };

        let presets_row = row![
            button(text(fl!("ui-tool-banner-preset-common")).size(font_size::CAPTION))
                .padding([2.0, 6.0])
                .style(styles::subtle)
                .on_press(Message::Tool(
                    tab,
                    ToolMessage::Banner(BannerMessage::SelectPresetCommon)
                )),
            button(text(fl!("ui-tool-banner-preset-web")).size(font_size::CAPTION))
                .padding([2.0, 6.0])
                .style(styles::subtle)
                .on_press(Message::Tool(
                    tab,
                    ToolMessage::Banner(BannerMessage::SelectPresetWeb)
                )),
            button(text(fl!("ui-tool-banner-preset-mail")).size(font_size::CAPTION))
                .padding([2.0, 6.0])
                .style(styles::subtle)
                .on_press(Message::Tool(
                    tab,
                    ToolMessage::Banner(BannerMessage::SelectPresetMail)
                )),
            button(text(fl!("ui-tool-banner-preset-database")).size(font_size::CAPTION))
                .padding([2.0, 6.0])
                .style(styles::subtle)
                .on_press(Message::Tool(
                    tab,
                    ToolMessage::Banner(BannerMessage::SelectPresetDb)
                )),
        ]
        .spacing(spacing::XS);

        let controls_row = row![
            host_input,
            ports_input,
            container(grab_btn).align_y(Alignment::End),
        ]
        .spacing(spacing::MD)
        .align_y(Alignment::End);

        let mut body_col = column![controls_row, presets_row].spacing(spacing::SM);

        if let Some(problem) = &self.problem {
            body_col = body_col.push(
                text(Self::problem_text(problem))
                    .size(font_size::BODY)
                    .color(colors.red),
            );
        }

        if !self.results.is_empty() || self.running.is_some() {
            let actions_row = row![
                text(fl!(
                    "ui-tool-banner-progress",
                    completed = self.results.len(),
                    total = self.total
                ))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .color(colors.blue)
                .width(Length::Fill),
                super::copy_button(
                    fl!("ui-tool-banner-btn-copy"),
                    state.copied(CopySlot::BannerGrabResults),
                    Message::Tool(tab, ToolMessage::Banner(BannerMessage::CopyResults),),
                    super::COPY_PADDING,
                ),
                super::copy_button(
                    fl!("ui-tool-banner-btn-export"),
                    state.copied(CopySlot::BannerGrabCsv),
                    Message::Tool(tab, ToolMessage::Banner(BannerMessage::ExportCsv),),
                    super::COPY_PADDING,
                ),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let mut table_rows = column!().spacing(spacing::XS);
            // Header
            table_rows = table_rows.push(
                row![
                    text(fl!("ui-tool-banner-col-port"))
                        .size(font_size::CAPTION)
                        .font(styles::SEMIBOLD)
                        .style(text::secondary)
                        .width(Length::Fixed(60.0)),
                    text(fl!("ui-tool-banner-col-service"))
                        .size(font_size::CAPTION)
                        .font(styles::SEMIBOLD)
                        .style(text::secondary)
                        .width(Length::Fixed(120.0)),
                    text(fl!("ui-tool-banner-col-time"))
                        .size(font_size::CAPTION)
                        .font(styles::SEMIBOLD)
                        .style(text::secondary)
                        .width(Length::Fixed(80.0)),
                    text(fl!("ui-tool-banner-col-banner"))
                        .size(font_size::CAPTION)
                        .font(styles::SEMIBOLD)
                        .style(text::secondary)
                        .width(Length::Fill),
                ]
                .spacing(spacing::MD),
            );

            for r in &self.results {
                let banner_str = r.banner.clone().unwrap_or_else(|| no_value.clone());
                let banner_color = if r.banner.is_some() {
                    colors.green
                } else {
                    colors.comment
                };
                let time = r.response_ms.map_or_else(|| no_value.clone(), millis_text);

                table_rows = table_rows.push(
                    row![
                        text(r.port.to_string())
                            .size(font_size::CAPTION)
                            .font(super::BOX_FONT)
                            .width(Length::Fixed(60.0)),
                        text(&r.service)
                            .size(font_size::CAPTION)
                            .font(super::BOX_FONT)
                            .width(Length::Fixed(120.0)),
                        text(time)
                            .size(font_size::CAPTION)
                            .font(super::BOX_FONT)
                            .width(Length::Fixed(80.0)),
                        text(banner_str)
                            .size(font_size::CAPTION)
                            .font(super::BOX_FONT)
                            .color(banner_color)
                            .width(Length::Fill),
                    ]
                    .spacing(spacing::MD)
                    .align_y(Alignment::Center),
                );
            }

            body_col = body_col
                .push(actions_row)
                .push(scrollable(table_rows).height(Length::Fill));
        } else {
            body_col = body_col.push(
                container(
                    text(fl!("ui-tool-banner-empty-state"))
                        .size(font_size::BODY)
                        .style(text::secondary),
                )
                .padding(super::EMPTY_STATE_PADDING),
            );
        }

        let content = column![body_col]
            .spacing(spacing::LG)
            .padding(super::BODY_PADDING)
            .max_width(super::CONTENT_MAX_WIDTH);

        content.into()
    }
}

/// A time in milliseconds, in the user's language.
fn millis_text(millis: u64) -> String {
    fl!("ui-tool-net-millis", ms = millis)
}

/// The words of the exports, in the user's language, as the C# localizes its columns.
fn export_labels() -> ExportLabels {
    ExportLabels {
        port: fl!("ui-tool-banner-col-port"),
        service: fl!("ui-tool-banner-col-service"),
        banner: fl!("ui-tool-banner-col-banner"),
        time: fl!("ui-tool-banner-col-time"),
        none: fl!("ui-tool-net-no-value"),
    }
}

/// The probes of a run, as the C# `BannerGrabService.GrabAsync`: the host looked up once,
/// then [`MAX_CONCURRENT`] ports at a time, each result sent back as it ends.
fn grabs(
    host: String,
    ports: Vec<u16>,
    run: u64,
) -> impl iced::futures::Stream<Item = BannerMessage> {
    iced::stream::channel(CHANNEL_SIZE, async move |mut out| {
        let addresses = match network_tools::resolve(&host, LOOKUP_PORT, CONNECT_TIMEOUT).await {
            Ok(addresses) => addresses,
            Err(reason) => {
                let _ = out.send(BannerMessage::Unresolved(run, reason)).await;
                return;
            }
        };
        let mut probes = iced::futures::stream::iter(ports)
            .map(|port| network_tools::grab_banner(&addresses, port, CONNECT_TIMEOUT, READ_TIMEOUT))
            .buffer_unordered(MAX_CONCURRENT);
        while let Some(result) = probes.next().await {
            if out.send(BannerMessage::Probed(run, result)).await.is_err() {
                return;
            }
        }
        let _ = out.send(BannerMessage::Finished(run)).await;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started(pane: &mut BannerPane) -> Handle {
        pane.apply(BannerMessage::HostChanged("localhost".to_owned()));
        assert!(matches!(pane.apply(BannerMessage::Start), Applied::Run(_)));
        pane.running.clone().expect("a run under way")
    }

    #[test]
    fn results_come_one_by_one_in_port_order() {
        let mut pane = BannerPane::default();
        drop(started(&mut pane));
        pane.apply(BannerMessage::Probed(
            pane.run,
            BannerProbeResult::closed(80),
        ));
        pane.apply(BannerMessage::Probed(
            pane.run,
            BannerProbeResult::closed(22),
        ));
        let ports: Vec<u16> = pane.results.iter().map(|result| result.port).collect();
        assert_eq!(ports, [22, 80]);
        assert!(pane.running.is_some());
        pane.apply(BannerMessage::Finished(pane.run));
        assert!(pane.running.is_none());
    }

    #[test]
    fn stop_aborts_the_run_and_a_stale_run_is_ignored() {
        let mut pane = BannerPane::default();
        let first = started(&mut pane);
        let old = pane.run;
        pane.apply(BannerMessage::Stop);
        assert!(first.is_aborted());
        assert!(pane.running.is_none());
        pane.apply(BannerMessage::Probed(old, BannerProbeResult::closed(22)));
        assert!(pane.results.is_empty());

        drop(started(&mut pane));
        pane.apply(BannerMessage::Probed(old, BannerProbeResult::closed(23)));
        pane.apply(BannerMessage::Finished(old));
        assert!(pane.results.is_empty());
        assert!(pane.running.is_some(), "the new run goes on");
    }

    #[test]
    fn the_inputs_are_checked_as_the_csharp() {
        let mut pane = BannerPane::default();
        let refused = |pane: &mut BannerPane, problem: Problem| {
            assert!(matches!(pane.apply(BannerMessage::Start), Applied::None));
            assert_eq!(pane.problem, Some(problem));
        };
        refused(&mut pane, Problem::HostRequired);
        pane.apply(BannerMessage::HostChanged("bad host".to_owned()));
        refused(&mut pane, Problem::InvalidHost);
        pane.apply(BannerMessage::HostChanged("example.com".to_owned()));
        pane.apply(BannerMessage::PortsChanged("0, x, 70000".to_owned()));
        refused(&mut pane, Problem::PortRangeRequired);
    }
}
