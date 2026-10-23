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

//! The ARP Table Monitor tool view, as the C# `ArpMonitorView`: the local ARP cache read
//! now or every few seconds while monitoring, each entry's hardware vendor and status since
//! the read before, alerts for what changed, and CSV export.

use std::fmt;
use std::time::Duration;

use heimdall_app::TabId;
use iced::widget::{column, container, pick_list, row, text};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::arp_monitor::{
    self, ArpAlert, ArpEntry, ArpStatus, DEFAULT_INTERVAL_SECS, MONITOR_INTERVALS_SECS,
};

use super::network_run::{RunSlot, blocking_once};
use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// The format of the moments an entry was seen, as the C# shows them.
const SEEN_FORMAT: &str = "%H:%M:%S";
/// Alerts kept until dismissed, the oldest let go past them.
const MAX_ALERTS: usize = 50;
/// Addresses an alert lists before it says how many more there are.
const MAX_LISTED_ADDRESSES: usize = 10;
/// Width of the IP address column.
const IP_WIDTH: f32 = 140.0;
/// Width of the MAC address column.
const MAC_WIDTH: f32 = 180.0;
/// Width of the status and time columns.
const SHORT_WIDTH: f32 = 90.0;
/// Width of an alert's time.
const ALERT_TIME_WIDTH: f32 = 70.0;

/// Messages emitted by the ARP monitor view.
#[derive(Debug, Clone)]
pub enum ArpMonitorMessage {
    /// Scan ARP table now.
    Scan,
    /// Start or stop monitoring, as the C# `BtnToggle`.
    ToggleMonitoring,
    /// The refresh interval chosen.
    IntervalSelected(IntervalChoice),
    /// Monitoring's timer: time to read the table again.
    Tick,
    /// Scan `.0` read the table, `None` when it could not; the outer `None` when its work
    /// failed.
    Finished(u64, Option<Option<Vec<(String, String)>>>),
    /// Dismiss the alerts, as the C# `DismissAlertCommand`.
    DismissAlerts,
    /// Copy CSV export.
    CopyCsv,
}

/// A refresh interval, as the picker shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntervalChoice(u64);

impl fmt::Display for IntervalChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&fl!("ui-tool-arpmon-interval-secs", secs = self.0))
    }
}

/// The intervals the picker offers.
const INTERVAL_CHOICES: [IntervalChoice; MONITOR_INTERVALS_SECS.len()] = {
    let mut choices = [IntervalChoice(0); MONITOR_INTERVALS_SECS.len()];
    let mut at = 0;
    while at < choices.len() {
        choices[at] = IntervalChoice(MONITOR_INTERVALS_SECS[at]);
        at += 1;
    }
    choices
};

/// The ARP Table Monitor tool pane state.
#[derive(Debug)]
pub struct ArpMonitorPane {
    run: RunSlot,
    /// The table is read again every `interval_secs`, as the C# polling loop.
    monitoring: bool,
    interval_secs: u64,
    error: Option<String>,
    entries: Vec<ArpEntry>,
    /// What reads found, with the moment, oldest first, until dismissed.
    alerts: Vec<(String, ArpAlert)>,
    /// When the table was last read, as the C# `LastRefreshText`.
    last_refresh: Option<String>,
}

impl Default for ArpMonitorPane {
    fn default() -> Self {
        Self {
            run: RunSlot::default(),
            monitoring: false,
            interval_secs: DEFAULT_INTERVAL_SECS,
            error: None,
            entries: Vec::new(),
            alerts: Vec::new(),
            last_refresh: None,
        }
    }
}

/// Outcome of updating the ARP Monitor tool pane.
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
    /// Read the table as run `.0`, watching stop `.1`.
    Read(u64, std::sync::Arc<std::sync::atomic::AtomicBool>),
    Copy(CopySlot, String),
}

impl ArpMonitorPane {
    /// Whether the table is read again on a timer.
    #[must_use]
    pub fn monitoring(&self) -> bool {
        self.monitoring
    }

    /// How often the table is read again while monitoring.
    #[must_use]
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_secs)
    }

    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: ArpMonitorMessage) -> Outcome {
        match self.apply(message) {
            Step::None => Outcome::None,
            Step::Copy(slot, content) => Outcome::Copy(slot, content),
            Step::Read(run, stop) => {
                let stream =
                    blocking_once(stop, arp_monitor::read_system_arp_table, move |table| {
                        ArpMonitorMessage::Finished(run, table)
                    });
                let task = Task::run(stream, move |message| {
                    Message::Tool(tab, super::ToolMessage::ArpMon(message))
                });
                Outcome::Task(self.run.attach(task))
            }
        }
    }

    /// A read of the table begun, unless one is going, as the C# `IsRefreshing` guard.
    fn read(&mut self) -> Step {
        if self.run.running() {
            return Step::None;
        }
        let (run, stop) = self.run.begin();
        Step::Read(run, stop)
    }

    /// What `message` changes, and what it asks.
    fn apply(&mut self, message: ArpMonitorMessage) -> Step {
        match message {
            ArpMonitorMessage::Scan => self.read(),
            ArpMonitorMessage::ToggleMonitoring => {
                if self.monitoring {
                    // Stopped for real: the read going is ended too.
                    self.monitoring = false;
                    self.run.stop();
                    Step::None
                } else {
                    // A read at once, then one every interval, as the C# `StartAsync`.
                    self.monitoring = true;
                    self.read()
                }
            }
            ArpMonitorMessage::IntervalSelected(choice) => {
                // Fixed while monitoring, as the C# disables its picker.
                if !self.monitoring && MONITOR_INTERVALS_SECS.contains(&choice.0) {
                    self.interval_secs = choice.0;
                }
                Step::None
            }
            ArpMonitorMessage::Tick => {
                if self.monitoring {
                    self.read()
                } else {
                    Step::None
                }
            }
            ArpMonitorMessage::Finished(run, table) => {
                if !self.run.finish(run) {
                    return Step::None;
                }
                match table.flatten() {
                    Some(table) => {
                        let now = chrono::Local::now().format(SEEN_FORMAT).to_string();
                        // Compared with what is shown now, not with what was shown when
                        // the scan began.
                        let refresh = arp_monitor::detect_changes(&self.entries, &table, &now);
                        self.entries = refresh.entries;
                        self.alerts
                            .extend(refresh.alerts.into_iter().map(|alert| (now.clone(), alert)));
                        let excess = self.alerts.len().saturating_sub(MAX_ALERTS);
                        self.alerts.drain(..excess);
                        self.last_refresh = Some(now);
                        self.error = None;
                    }
                    None => self.error = Some(fl!("ui-tool-arpmon-error-read")),
                }
                Step::None
            }
            ArpMonitorMessage::DismissAlerts => {
                self.alerts.clear();
                Step::None
            }
            ArpMonitorMessage::CopyCsv => {
                if self.entries.is_empty() {
                    return Step::None;
                }
                let header = [
                    fl!("ui-tool-arpmon-col-ip"),
                    fl!("ui-tool-arpmon-col-mac"),
                    fl!("ui-tool-arpmon-col-vendor"),
                    fl!("ui-tool-arpmon-col-status"),
                    fl!("ui-tool-arpmon-col-first-seen"),
                    fl!("ui-tool-arpmon-col-last-seen"),
                ];
                let csv = arp_monitor::build_arp_csv(
                    &self.entries,
                    header.each_ref().map(String::as_str),
                    super::vendor_label,
                    status_label,
                );
                Step::Copy(CopySlot::ArpMonCsv, csv)
            }
        }
    }

    /// Renders the ARP Table Monitor tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::ArpMon(msg));

        let toggle_btn = if self.monitoring {
            super::action_button(
                fl!("ui-tool-arpmon-stop"),
                false,
                Some(send(ArpMonitorMessage::ToggleMonitoring)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-arpmon-start"),
                true,
                Some(send(ArpMonitorMessage::ToggleMonitoring)),
            )
        };
        let scan_btn = if self.run.running() {
            super::action_button(fl!("ui-tool-arpmon-scanning"), false, None)
        } else {
            super::action_button(
                fl!("ui-tool-arpmon-scan"),
                false,
                Some(send(ArpMonitorMessage::Scan)),
            )
        };
        let interval: Element<'a, Message> = if self.monitoring {
            text(IntervalChoice(self.interval_secs).to_string())
                .size(font_size::BODY)
                .style(text::secondary)
                .into()
        } else {
            pick_list(
                &INTERVAL_CHOICES[..],
                Some(IntervalChoice(self.interval_secs)),
                move |choice| send(ArpMonitorMessage::IntervalSelected(choice)),
            )
            .style(styles::pick_list)
            .into()
        };

        let copy_csv_btn = super::copy_button(
            fl!("ui-tool-arpmon-copy-csv"),
            state.copied(CopySlot::ArpMonCsv),
            send(ArpMonitorMessage::CopyCsv),
            super::COPY_PADDING,
        );

        let top_row = row![
            text(fl!("ui-tool-arpmon-name"))
                .size(font_size::SUBTITLE)
                .width(Length::Fill),
            toggle_btn,
            scan_btn,
            text(fl!("ui-tool-arpmon-interval"))
                .size(font_size::BODY)
                .style(text::secondary),
            interval,
            copy_csv_btn,
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let input_card = container(column![top_row])
            .padding(spacing::MD)
            .style(styles::card);

        let mut body = column![input_card].spacing(spacing::MD);

        if !self.alerts.is_empty() {
            body = body.push(self.alerts_card(send));
        }

        if let Some(err) = &self.error {
            body = body.push(
                container(
                    text(err.as_str())
                        .size(font_size::BODY)
                        .style(styles::error_text),
                )
                .padding(spacing::MD)
                .style(styles::card),
            );
        }

        if self.entries.is_empty() {
            if self.error.is_none() {
                let empty = if self.monitoring {
                    fl!("ui-tool-arpmon-empty-running")
                } else {
                    fl!("ui-tool-arpmon-empty")
                };
                body = body.push(text(empty).size(font_size::BODY).style(text::secondary));
            }
        } else {
            body = body.push(self.entries_card());
        }

        super::table_column(body)
    }

    /// The alerts, oldest first, and their dismiss button, as the C# alert banner: an
    /// anomaly's title when one is among them.
    fn alerts_card<'a>(
        &'a self,
        send: impl Fn(ArpMonitorMessage) -> Message,
    ) -> Element<'a, Message> {
        let alarming = self.alerts.iter().any(|(_, alert)| {
            matches!(
                alert,
                ArpAlert::MacChanged { .. } | ArpAlert::Conflict { .. }
            )
        });
        let title = if alarming {
            text(fl!("ui-tool-arpmon-alert-title"))
                .size(font_size::SUBTITLE)
                .style(styles::error_text)
        } else {
            text(fl!("ui-tool-arpmon-changes-title"))
                .size(font_size::SUBTITLE)
                .style(styles::warning_text)
        };
        let header = row![
            container(title).width(Length::Fill),
            super::action_button(
                fl!("ui-tool-arpmon-dismiss-alert"),
                false,
                Some(send(ArpMonitorMessage::DismissAlerts)),
            ),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);
        let mut lines = column![header].spacing(spacing::XS);
        for (at, alert) in &self.alerts {
            let style = match alert {
                ArpAlert::MacChanged { .. } | ArpAlert::Conflict { .. } => styles::error_text,
                ArpAlert::Appeared(_) => styles::accent_text,
                ArpAlert::Gone(_) => text::secondary,
            };
            lines = lines.push(
                row![
                    text(at.as_str())
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .style(text::secondary)
                        .width(Length::Fixed(ALERT_TIME_WIDTH)),
                    text(alert_text(alert))
                        .size(font_size::BODY)
                        .style(style)
                        .width(Length::Fill),
                ]
                .spacing(spacing::SM),
            );
        }
        container(lines)
            .padding(spacing::MD)
            .style(styles::card)
            .into()
    }

    /// The entries, in the order first seen, with their count and the last read's moment.
    fn entries_card(&self) -> Element<'_, Message> {
        let caption = |label: String, width: Length| {
            text(label)
                .size(font_size::CAPTION)
                .style(text::secondary)
                .width(width)
        };
        let table_headers = row![
            caption(fl!("ui-tool-arpmon-col-ip"), Length::Fixed(IP_WIDTH)),
            caption(fl!("ui-tool-arpmon-col-mac"), Length::Fixed(MAC_WIDTH)),
            caption(fl!("ui-tool-arpmon-col-vendor"), Length::FillPortion(2)),
            caption(fl!("ui-tool-arpmon-col-status"), Length::Fixed(SHORT_WIDTH)),
            caption(
                fl!("ui-tool-arpmon-col-first-seen"),
                Length::Fixed(SHORT_WIDTH)
            ),
            caption(
                fl!("ui-tool-arpmon-col-last-seen"),
                Length::Fixed(SHORT_WIDTH)
            ),
        ]
        .spacing(spacing::SM);

        let mut entry_rows = column![table_headers].spacing(spacing::XS);

        for e in &self.entries {
            let style = match e.status {
                ArpStatus::New => styles::accent_text,
                ArpStatus::Stable => text::secondary,
                ArpStatus::Changed => styles::warning_text,
                ArpStatus::Gone | ArpStatus::Conflict => styles::error_text,
            };
            let status_el = text(status_label(e.status))
                .size(font_size::BODY)
                .style(style)
                .width(Length::Fixed(SHORT_WIDTH));
            let vendor = match super::vendor_label(e.vendor) {
                unknown if unknown.is_empty() => fl!("ui-tool-arpmon-vendor-unknown"),
                known => known,
            };
            // What the MAC address was, or which others claim the address.
            let aside = match e.status {
                ArpStatus::Changed => e
                    .previous_mac
                    .as_ref()
                    .map(|previous| fl!("ui-tool-arpmon-mac-was", mac = previous.as_str())),
                ArpStatus::Conflict => Some(fl!(
                    "ui-tool-arpmon-mac-also",
                    macs = e.conflicting_macs.join(", ")
                )),
                _ => None,
            };
            let mut mac = column![text(&e.mac).size(font_size::BODY).font(super::BOX_FONT)];
            if let Some(aside) = aside {
                mac = mac.push(
                    text(aside)
                        .size(font_size::CAPTION)
                        .font(super::BOX_FONT)
                        .style(style),
                );
            }

            let r = row![
                text(&e.ip)
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .width(Length::Fixed(IP_WIDTH)),
                container(mac).width(Length::Fixed(MAC_WIDTH)),
                text(vendor)
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .width(Length::FillPortion(2)),
                status_el,
                text(&e.first_seen)
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .width(Length::Fixed(SHORT_WIDTH)),
                text(&e.last_seen)
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .width(Length::Fixed(SHORT_WIDTH)),
            ]
            .spacing(spacing::SM);

            entry_rows = entry_rows.push(r);
        }

        let mut footer = row![
            text(fl!("ui-tool-arpmon-total", count = self.entries.len()))
                .size(font_size::BODY)
                .font(super::BOX_FONT)
                .style(text::secondary)
                .width(Length::Fill)
        ];
        if let Some(at) = &self.last_refresh {
            footer = footer.push(
                text(fl!("ui-tool-arpmon-last-refresh", time = at.as_str()))
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .style(text::secondary),
            );
        }

        container(column![entry_rows, footer].spacing(spacing::MD))
            .padding(spacing::MD)
            .style(styles::card)
            .into()
    }
}

/// The addresses of an alert, the first [`MAX_LISTED_ADDRESSES`] of them and how many more.
fn listed(ips: &[String]) -> String {
    let shown = ips
        .iter()
        .take(MAX_LISTED_ADDRESSES)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    match ips.len().saturating_sub(MAX_LISTED_ADDRESSES) {
        0 => shown,
        more => fl!("ui-tool-arpmon-alert-more", list = shown, more = more),
    }
}

/// What an alert says.
fn alert_text(alert: &ArpAlert) -> String {
    match alert {
        ArpAlert::MacChanged {
            ip,
            previous,
            current,
        } => fl!(
            "ui-tool-arpmon-alert-mac-changed",
            ip = ip.as_str(),
            previous = previous.as_str(),
            current = current.as_str()
        ),
        ArpAlert::Conflict { ip, macs } => fl!(
            "ui-tool-arpmon-alert-conflict",
            ip = ip.as_str(),
            macs = macs.join(", ")
        ),
        ArpAlert::Appeared(ips) => fl!(
            "ui-tool-arpmon-alert-new",
            count = ips.len(),
            list = listed(ips)
        ),
        ArpAlert::Gone(ips) => fl!(
            "ui-tool-arpmon-alert-gone",
            count = ips.len(),
            list = listed(ips)
        ),
    }
}

/// An entry's status, as the view and the CSV say it.
fn status_label(status: ArpStatus) -> String {
    match status {
        ArpStatus::New => fl!("ui-tool-arpmon-status-new"),
        ArpStatus::Stable => fl!("ui-tool-arpmon-status-stable"),
        ArpStatus::Changed => fl!("ui-tool-arpmon-status-changed"),
        ArpStatus::Gone => fl!("ui-tool-arpmon-status-gone"),
        ArpStatus::Conflict => fl!("ui-tool-arpmon-status-conflict"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    fn table(mac: &str) -> Option<Option<Vec<(String, String)>>> {
        Some(Some(vec![("10.0.0.1".to_owned(), mac.to_owned())]))
    }

    fn read(pane: &mut ArpMonitorPane, message: ArpMonitorMessage) -> u64 {
        match pane.apply(message) {
            Step::Read(run, _) => run,
            other => panic!("no read: {other:?}"),
        }
    }

    #[test]
    fn a_scan_at_a_time_and_changes_found_against_what_is_shown() {
        let mut pane = ArpMonitorPane::default();
        let run = read(&mut pane, ArpMonitorMessage::Scan);
        assert!(
            matches!(pane.apply(ArpMonitorMessage::Scan), Step::None),
            "one at a time"
        );
        pane.apply(ArpMonitorMessage::Finished(run, table("00-50-56-01-02-03")));
        assert_eq!(pane.entries[0].status, ArpStatus::New);
        assert!(pane.alerts.is_empty(), "nothing alerted on the first read");
        assert!(pane.last_refresh.is_some());
        pane.apply(ArpMonitorMessage::Finished(run, table("AA-50-56-01-02-03")));
        assert_eq!(
            pane.entries[0].status,
            ArpStatus::New,
            "an old run's end is let go"
        );
        let run = read(&mut pane, ArpMonitorMessage::Scan);
        pane.apply(ArpMonitorMessage::Finished(run, table("AA-50-56-01-02-03")));
        assert_eq!(pane.entries[0].status, ArpStatus::Changed);
        assert!(matches!(
            pane.alerts.as_slice(),
            [(_, ArpAlert::MacChanged { .. })]
        ));
        let Step::Copy(CopySlot::ArpMonCsv, csv) = pane.apply(ArpMonitorMessage::CopyCsv) else {
            panic!("a CSV");
        };
        assert_eq!(csv.lines().count(), 2);
        pane.apply(ArpMonitorMessage::DismissAlerts);
        assert!(pane.alerts.is_empty());
    }

    #[test]
    fn monitoring_reads_at_once_then_on_each_tick_and_its_stop_ends_the_read() {
        let mut pane = ArpMonitorPane::default();
        assert_eq!(pane.interval(), Duration::from_secs(DEFAULT_INTERVAL_SECS));
        pane.apply(ArpMonitorMessage::IntervalSelected(IntervalChoice(5)));
        assert_eq!(pane.interval(), Duration::from_secs(5));
        pane.apply(ArpMonitorMessage::IntervalSelected(IntervalChoice(7)));
        assert_eq!(
            pane.interval(),
            Duration::from_secs(5),
            "only those offered"
        );
        assert!(
            matches!(pane.apply(ArpMonitorMessage::Tick), Step::None),
            "no timer before monitoring"
        );

        let Step::Read(run, stop) = pane.apply(ArpMonitorMessage::ToggleMonitoring) else {
            panic!("a read at once");
        };
        assert!(pane.monitoring());
        pane.apply(ArpMonitorMessage::IntervalSelected(IntervalChoice(60)));
        assert_eq!(
            pane.interval(),
            Duration::from_secs(5),
            "fixed while monitoring"
        );
        assert!(
            matches!(pane.apply(ArpMonitorMessage::Tick), Step::None),
            "a tick during a read waits for it"
        );
        pane.apply(ArpMonitorMessage::Finished(run, table("00-50-56-01-02-03")));
        let run = read(&mut pane, ArpMonitorMessage::Tick);
        pane.apply(ArpMonitorMessage::Finished(run, Some(Some(Vec::new()))));
        assert_eq!(pane.entries[0].status, ArpStatus::Gone);
        assert!(matches!(pane.alerts.as_slice(), [(_, ArpAlert::Gone(_))]));
        assert!(!stop.load(Ordering::Relaxed));

        let Step::Read(run, stop) = pane.apply(ArpMonitorMessage::Tick) else {
            panic!("a read");
        };
        pane.apply(ArpMonitorMessage::ToggleMonitoring);
        assert!(!pane.monitoring());
        assert!(stop.load(Ordering::Relaxed), "the read going is stopped");
        pane.apply(ArpMonitorMessage::Finished(run, table("00-50-56-01-02-03")));
        assert_eq!(pane.entries[0].status, ArpStatus::Gone, "its end let go");
        assert!(matches!(pane.apply(ArpMonitorMessage::Tick), Step::None));
    }

    #[test]
    fn a_table_that_could_not_be_read_is_said() {
        let mut pane = ArpMonitorPane::default();
        let run = read(&mut pane, ArpMonitorMessage::Scan);
        pane.apply(ArpMonitorMessage::Finished(run, Some(None)));
        assert!(pane.error.is_some());
        let run = read(&mut pane, ArpMonitorMessage::Scan);
        pane.apply(ArpMonitorMessage::Finished(run, table("00-50-56-01-02-03")));
        assert!(pane.error.is_none(), "cleared by a read that worked");
    }

    #[test]
    fn alerts_are_bounded_and_list_a_few_addresses() {
        let mut pane = ArpMonitorPane::default();
        for n in 0..(MAX_ALERTS + 5) {
            pane.alerts
                .push((n.to_string(), ArpAlert::Gone(vec!["10.0.0.1".to_owned()])));
        }
        let run = read(&mut pane, ArpMonitorMessage::Scan);
        pane.apply(ArpMonitorMessage::Finished(run, Some(Some(Vec::new()))));
        assert_eq!(pane.alerts.len(), MAX_ALERTS, "the oldest let go");
        assert_eq!(pane.alerts[0].0, "5");
        let ips: Vec<String> = (1..=12).map(|n| format!("10.0.0.{n}")).collect();
        let said = listed(&ips);
        assert!(
            said.contains("10.0.0.10") && !said.contains("10.0.0.11"),
            "{said}"
        );
        assert!(said.contains(" 2 "), "how many more: {said}");
        assert_eq!(listed(&ips[..2]), "10.0.0.1, 10.0.0.2");
    }
}
