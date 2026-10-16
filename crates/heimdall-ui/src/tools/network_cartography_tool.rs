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

//! The Network Cartography tool view: subnet host expansion, port recon sweep,
//! role classification heuristics, and CSV export.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use heimdall_app::TabId;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::network_cartography::{
    self, CartographySummary, CidrError, DiscoveredNode, LARGE_SUBNET_THRESHOLD, MAX_PREFIX,
    MIN_PREFIX, NodeRole, PROBE_TIMEOUT_MS,
};

use super::network_run::{RunSlot, blocking_run};
use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the network cartography view.
#[derive(Debug, Clone)]
pub enum NetworkCartographyMessage {
    /// CIDR input changed.
    CidrChanged(String),
    /// Start network discovery.
    Start,
    /// Scan the large subnet asked about, as the C# confirmation's yes.
    ConfirmLarge,
    /// Stop discovery.
    Stop,
    /// Discovery `.0` probed every port of a host: the node found, `None` when nothing
    /// answered.
    Host(u64, Option<DiscoveredNode>),
    /// Discovery `.0` is over; `false` when its work failed.
    Ended(u64, bool),
    /// Copy CSV export.
    CopyCsv,
}

/// The Network Cartography tool pane state.
#[derive(Debug, Default)]
pub struct NetworkCartographyPane {
    cidr: String,
    run: RunSlot,
    error: Option<String>,
    /// What the last scan says beside its results: a stop.
    notice: Option<String>,
    /// The hosts of a subnet larger than [`LARGE_SUBNET_THRESHOLD`], waiting for the user's
    /// yes, as the C# asks before scanning one.
    pending: Option<Vec<Ipv4Addr>>,
    /// The hosts the last scan probes.
    total: usize,
    nodes: Vec<DiscoveredNode>,
    summary: CartographySummary,
}

/// Outcome of updating the Network Cartography tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

/// What a message asks of the pane, before its tab is known.
#[derive(Debug)]
enum Step {
    None,
    /// Probe hosts `.2` as run `.0`, watching stop `.1`.
    Discover(u64, Arc<AtomicBool>, Vec<Ipv4Addr>),
    Copy(CopySlot, String),
}

impl NetworkCartographyPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: NetworkCartographyMessage) -> Outcome {
        match self.apply(message) {
            Step::None => Outcome::None,
            Step::Copy(slot, content) => Outcome::Copy(slot, content),
            Step::Discover(run, stop, ips) => {
                let stream = blocking_run(
                    stop,
                    move |stop, host| {
                        network_cartography::discover(&ips, PROBE_TIMEOUT_MS, stop, host);
                    },
                    move |node: Option<DiscoveredNode>| NetworkCartographyMessage::Host(run, node),
                    move |ended| NetworkCartographyMessage::Ended(run, ended.is_some()),
                );
                let task = Task::run(stream, move |message| {
                    Message::Tool(tab, super::ToolMessage::NetMap(message))
                });
                Outcome::Task(self.run.attach(task))
            }
        }
    }

    /// What `message` changes, and what it asks.
    fn apply(&mut self, message: NetworkCartographyMessage) -> Step {
        match message {
            NetworkCartographyMessage::CidrChanged(val) => {
                self.cidr = val;
                self.error = None;
                self.pending = None;
                Step::None
            }
            NetworkCartographyMessage::Start => self.start(),
            NetworkCartographyMessage::ConfirmLarge => match self.pending.take() {
                Some(ips) if !self.run.running() => self.discover(ips),
                _ => Step::None,
            },
            NetworkCartographyMessage::Stop => {
                if self.run.running() {
                    self.run.stop();
                    self.notice = Some(fl!("ui-tool-netmap-stopped"));
                }
                Step::None
            }
            NetworkCartographyMessage::Host(run, node) => {
                if !self.run.is_current(run) {
                    return Step::None;
                }
                self.summary.hosts_scanned += 1;
                if let Some(node) = node {
                    self.summary.hosts_up += 1;
                    self.summary.open_ports_count += node.open_ports.len();
                    let key = address(&node);
                    let at = self.nodes.partition_point(|shown| address(shown) < key);
                    self.nodes.insert(at, node);
                }
                Step::None
            }
            NetworkCartographyMessage::Ended(run, completed) => {
                if self.run.finish(run) && !completed {
                    self.error = Some(fl!("ui-tool-netmap-error-failed"));
                }
                Step::None
            }
            NetworkCartographyMessage::CopyCsv => {
                if self.nodes.is_empty() {
                    return Step::None;
                }
                let header = [
                    fl!("ui-tool-netmap-col-ip"),
                    fl!("ui-tool-netmap-col-hostname"),
                    fl!("ui-tool-netmap-col-mac"),
                    fl!("ui-tool-netmap-col-vendor"),
                    fl!("ui-tool-netmap-col-ports"),
                    fl!("ui-tool-netmap-col-role"),
                    fl!("ui-tool-netmap-col-latency"),
                ];
                let csv = network_cartography::build_nodes_csv(
                    &self.nodes,
                    header.each_ref().map(String::as_str),
                    role_label,
                );
                Step::Copy(CopySlot::NetMapCsv, csv)
            }
        }
    }

    /// A scan asked, as the C# `ScanAsync`: the subnet normalized and kept within `/16` to
    /// `/30`, a large one asked about first.
    fn start(&mut self) -> Step {
        if self.run.running() {
            return Step::None;
        }
        let cidr = network_cartography::normalize_subnet(&self.cidr);
        if cidr.is_empty() {
            self.error = Some(fl!("ui-tool-netmap-error-cidr-required"));
            return Step::None;
        }
        self.cidr.clone_from(&cidr);
        let ips = match network_cartography::expand_subnet_ips(&cidr) {
            Ok(ips) => ips,
            Err(CidrError::Invalid) => {
                self.error = Some(fl!("ui-tool-netmap-error-cidr-required"));
                return Step::None;
            }
            Err(CidrError::PrefixOutOfRange) => {
                self.error = Some(fl!(
                    "ui-tool-netmap-error-prefix",
                    min = MIN_PREFIX,
                    max = MAX_PREFIX
                ));
                return Step::None;
            }
        };
        if ips.len() > LARGE_SUBNET_THRESHOLD {
            self.error = None;
            self.pending = Some(ips);
            return Step::None;
        }
        self.discover(ips)
    }

    /// The scan of `ips` begun.
    fn discover(&mut self, ips: Vec<Ipv4Addr>) -> Step {
        self.error = None;
        self.notice = None;
        self.pending = None;
        self.nodes.clear();
        self.summary = CartographySummary::default();
        self.total = ips.len();
        let (run, stop) = self.run.begin();
        Step::Discover(run, stop, ips)
    }

    /// Renders the Network Cartography tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::NetMap(msg));

        let cidr_input = text_input(&fl!("ui-tool-netmap-subnet-watermark"), &self.cidr)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(NetworkCartographyMessage::CidrChanged(v)))
            .on_submit(send(NetworkCartographyMessage::Start))
            .width(Length::FillPortion(3));

        let action_btn = if self.run.running() {
            super::action_button(
                fl!("ui-tool-netmap-stop"),
                true,
                Some(send(NetworkCartographyMessage::Stop)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-netmap-discover"),
                true,
                Some(send(NetworkCartographyMessage::Start)),
            )
        };

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-netmap-subnet")), cidr_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(3)),
            container(action_btn).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::End);

        let input_card = container(column![inputs_row])
            .padding(spacing::MD)
            .style(styles::card);

        let mut body = column![input_card].spacing(spacing::MD);

        if let Some(ips) = &self.pending {
            body = body.push(
                container(
                    row![
                        text(fl!("ui-tool-netmap-warning-large", count = ips.len()))
                            .size(font_size::BODY)
                            .style(text::warning)
                            .width(Length::Fill),
                        super::action_button(
                            fl!("ui-tool-netmap-scan-anyway"),
                            false,
                            Some(send(NetworkCartographyMessage::ConfirmLarge)),
                        ),
                    ]
                    .spacing(spacing::SM)
                    .align_y(Alignment::Center),
                )
                .padding(spacing::MD)
                .style(styles::card),
            );
        }
        if self.run.running() || self.summary.hosts_scanned > 0 {
            body = body.push(
                text(fl!(
                    "ui-tool-netmap-progress",
                    done = self.summary.hosts_scanned,
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

        if !self.nodes.is_empty() {
            let summary_cards = row![
                container(
                    column![
                        text(fl!("ui-tool-netmap-hosts-scanned"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.hosts_scanned.to_string())
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
                        text(fl!("ui-tool-netmap-hosts-up"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.hosts_up.to_string())
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
                        text(fl!("ui-tool-netmap-ports-open"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                        text(self.summary.open_ports_count.to_string())
                            .size(font_size::TITLE)
                            .font(super::BOX_FONT)
                            .style(text::primary),
                    ]
                    .spacing(spacing::XS)
                )
                .padding(spacing::SM)
                .style(styles::card)
                .width(Length::FillPortion(1)),
            ]
            .spacing(spacing::SM);

            body = body.push(summary_cards);

            let copy_csv_btn = super::copy_button(
                fl!("ui-tool-netmap-copy-csv"),
                state.copied(CopySlot::NetMapCsv),
                send(NetworkCartographyMessage::CopyCsv),
                super::COPY_PADDING,
            );

            let header_row = row![
                text(fl!("ui-tool-netmap-name"))
                    .size(font_size::SUBTITLE)
                    .width(Length::Fill),
                copy_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let table_headers = row![
                text(fl!("ui-tool-netmap-col-ip"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-netmap-col-role"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-netmap-col-ports"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-netmap-col-latency"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
            ]
            .spacing(spacing::SM);

            let mut node_rows = column![table_headers].spacing(spacing::XS);

            for n in &self.nodes {
                let ports_str = n
                    .open_ports
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");

                let r = row![
                    text(&n.ip)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(role_label(n.role))
                        .size(font_size::BODY)
                        .style(text::primary)
                        .width(Length::FillPortion(2)),
                    text(ports_str)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    text(fl!("ui-tool-netmap-latency-ms", ms = n.response_time_ms))
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                ]
                .spacing(spacing::SM);

                node_rows = node_rows.push(r);
            }

            let card = container(column![header_row, node_rows].spacing(spacing::MD))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        }

        super::content_column(body)
    }
}

/// A node's address, by which nodes are shown in order.
fn address(node: &DiscoveredNode) -> Option<Ipv4Addr> {
    node.ip.parse().ok()
}

/// A role, as the view and the CSV say it.
fn role_label(role: NodeRole) -> String {
    match role {
        NodeRole::Directory => fl!("ui-tool-netmap-role-directory"),
        NodeRole::Database => fl!("ui-tool-netmap-role-database"),
        NodeRole::Mail => fl!("ui-tool-netmap-role-mail"),
        NodeRole::FileStorage => fl!("ui-tool-netmap-role-file"),
        NodeRole::Web => fl!("ui-tool-netmap-role-web"),
        NodeRole::RemoteManagement => fl!("ui-tool-netmap-role-remote"),
        NodeRole::NetworkAppliance => fl!("ui-tool-netmap-role-appliance"),
        NodeRole::ActiveNode => fl!("ui-tool-netmap-role-active"),
        NodeRole::NoPortsOpen => fl!("ui-tool-netmap-role-no-ports"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    fn node(ip: &str, ports: &[u16]) -> DiscoveredNode {
        DiscoveredNode {
            ip: ip.to_owned(),
            hostname: None,
            mac: None,
            vendor: None,
            open_ports: ports.to_vec(),
            role: network_cartography::classify_role(ports),
            response_time_ms: 2,
        }
    }

    fn started(pane: &mut NetworkCartographyPane, cidr: &str) -> (u64, Arc<AtomicBool>, usize) {
        pane.apply(NetworkCartographyMessage::CidrChanged(cidr.to_owned()));
        match pane.apply(NetworkCartographyMessage::Start) {
            Step::Discover(run, stop, ips) => (run, stop, ips.len()),
            other => panic!("no scan: {other:?}"),
        }
    }

    #[test]
    fn the_scope_is_bounded_and_a_large_subnet_asked_about_first() {
        let mut pane = NetworkCartographyPane::default();
        pane.apply(NetworkCartographyMessage::CidrChanged(
            "10.0.0.0/8".to_owned(),
        ));
        assert!(matches!(
            pane.apply(NetworkCartographyMessage::Start),
            Step::None
        ));
        assert!(pane.error.is_some(), "wider than /16");
        pane.apply(NetworkCartographyMessage::CidrChanged(
            "10.0.0.0/16".to_owned(),
        ));
        assert!(matches!(
            pane.apply(NetworkCartographyMessage::Start),
            Step::None
        ));
        assert!(pane.pending.is_some(), "65534 hosts: asked first");
        assert!(!pane.run.running());
        let Step::Discover(_, _, ips) = pane.apply(NetworkCartographyMessage::ConfirmLarge) else {
            panic!("scanned once confirmed");
        };
        assert_eq!(ips.len(), 65_534);
        pane.apply(NetworkCartographyMessage::Stop);
        let (_, _, count) = started(&mut pane, "192.168.7.9");
        assert_eq!(count, 254, "a /24 when no prefix is typed, as the C#");
        assert_eq!(pane.cidr, "192.168.7.9/24");
    }

    #[test]
    fn hosts_stream_in_sorted_counted_and_copied() {
        let mut pane = NetworkCartographyPane::default();
        let (run, stop, _) = started(&mut pane, "10.0.0.0/28");
        pane.apply(NetworkCartographyMessage::Host(
            run,
            Some(node("10.0.0.10", &[80, 443])),
        ));
        pane.apply(NetworkCartographyMessage::Host(run, None));
        pane.apply(NetworkCartographyMessage::Host(
            run,
            Some(node("10.0.0.9", &[22])),
        ));
        let shown: Vec<&str> = pane.nodes.iter().map(|n| n.ip.as_str()).collect();
        assert_eq!(shown, ["10.0.0.9", "10.0.0.10"], "by address, not by text");
        assert_eq!(pane.summary.hosts_scanned, 3);
        assert_eq!(pane.summary.hosts_up, 2);
        assert_eq!(pane.summary.open_ports_count, 3);
        let Step::Copy(CopySlot::NetMapCsv, csv) = pane.apply(NetworkCartographyMessage::CopyCsv)
        else {
            panic!("the CSV is copied");
        };
        assert_eq!(csv.lines().count(), 3);
        pane.apply(NetworkCartographyMessage::Stop);
        assert!(stop.load(Ordering::Relaxed));
        pane.apply(NetworkCartographyMessage::Host(
            run,
            Some(node("10.0.0.11", &[80])),
        ));
        assert_eq!(pane.nodes.len(), 2, "nothing after the stop");
    }
}
