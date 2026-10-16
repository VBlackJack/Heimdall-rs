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

//! The ARP Table Monitor tool view: local ARP cache inspection,
//! hardware vendor identification, anomaly detection, and CSV export.

use heimdall_app::TabId;
use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::arp_monitor::{self, ArpEntry, ArpStatus};

use super::network_run::{RunSlot, blocking_once};
use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// The format of the moments an entry was seen, as the C# shows them.
const SEEN_FORMAT: &str = "%H:%M:%S";

/// Messages emitted by the ARP monitor view.
#[derive(Debug, Clone)]
pub enum ArpMonitorMessage {
    /// Scan ARP table now.
    Scan,
    /// Scan `.0` read the table, `None` when it could not; the outer `None` when its work
    /// failed.
    Finished(u64, Option<Option<Vec<(String, String)>>>),
    /// Copy CSV export.
    CopyCsv,
}

/// The ARP Table Monitor tool pane state.
#[derive(Debug, Default)]
pub struct ArpMonitorPane {
    run: RunSlot,
    error: Option<String>,
    entries: Vec<ArpEntry>,
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

    /// What `message` changes, and what it asks.
    fn apply(&mut self, message: ArpMonitorMessage) -> Step {
        match message {
            ArpMonitorMessage::Scan => {
                if self.run.running() {
                    return Step::None;
                }
                self.error = None;
                let (run, stop) = self.run.begin();
                Step::Read(run, stop)
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
                        self.entries = arp_monitor::detect_changes(&self.entries, &table, &now);
                    }
                    None => self.error = Some(fl!("ui-tool-arpmon-error-read")),
                }
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

        let scan_btn = if self.run.running() {
            super::action_button(fl!("ui-tool-arpmon-scanning"), false, None)
        } else {
            super::action_button(
                fl!("ui-tool-arpmon-scan"),
                true,
                Some(send(ArpMonitorMessage::Scan)),
            )
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
            scan_btn,
            copy_csv_btn,
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let input_card = container(column![top_row])
            .padding(spacing::MD)
            .style(styles::card);

        let mut body = column![input_card].spacing(spacing::MD);

        if let Some(err) = &self.error {
            body = body.push(
                container(text(err.as_str()).size(font_size::BODY).style(text::danger))
                    .padding(spacing::MD)
                    .style(styles::card),
            );
        }

        if !self.entries.is_empty() {
            let table_headers = row![
                text(fl!("ui-tool-arpmon-col-ip"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
                text(fl!("ui-tool-arpmon-col-mac"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(160.0)),
                text(fl!("ui-tool-arpmon-col-vendor"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-arpmon-col-status"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
                text(fl!("ui-tool-arpmon-col-first-seen"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
                text(fl!("ui-tool-arpmon-col-last-seen"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for e in &self.entries {
                let style = match e.status {
                    ArpStatus::New => text::primary,
                    ArpStatus::Known => text::secondary,
                    ArpStatus::Changed => text::warning,
                    ArpStatus::Conflict => text::danger,
                };
                let status_el = text(status_label(e.status))
                    .size(font_size::BODY)
                    .style(style)
                    .width(Length::Fixed(90.0));
                let vendor = if e.vendor.is_empty() {
                    fl!("ui-tool-arpmon-vendor-unknown")
                } else {
                    e.vendor.clone()
                };

                let r = row![
                    text(&e.ip)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(&e.mac)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(160.0)),
                    text(vendor)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    status_el,
                    text(&e.first_seen)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                    text(&e.last_seen)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(90.0)),
                ]
                .spacing(spacing::SM);

                entry_rows = entry_rows.push(r);
            }

            let card = container(entry_rows)
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        }

        super::content_column(body)
    }
}

/// An entry's status, as the view and the CSV say it.
fn status_label(status: ArpStatus) -> String {
    match status {
        ArpStatus::New => fl!("ui-tool-arpmon-status-new"),
        ArpStatus::Known => fl!("ui-tool-arpmon-status-known"),
        ArpStatus::Changed => fl!("ui-tool-arpmon-status-changed"),
        ArpStatus::Conflict => fl!("ui-tool-arpmon-status-conflict"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(mac: &str) -> Option<Option<Vec<(String, String)>>> {
        Some(Some(vec![("10.0.0.1".to_owned(), mac.to_owned())]))
    }

    #[test]
    fn a_scan_at_a_time_and_changes_found_against_what_is_shown() {
        let mut pane = ArpMonitorPane::default();
        let Step::Read(run, _) = pane.apply(ArpMonitorMessage::Scan) else {
            panic!("a read");
        };
        assert!(
            matches!(pane.apply(ArpMonitorMessage::Scan), Step::None),
            "one at a time"
        );
        pane.apply(ArpMonitorMessage::Finished(run, table("00-50-56-01-02-03")));
        assert_eq!(pane.entries[0].status, ArpStatus::New);
        pane.apply(ArpMonitorMessage::Finished(run, table("AA-50-56-01-02-03")));
        assert_eq!(
            pane.entries[0].status,
            ArpStatus::New,
            "an old run's end is let go"
        );
        let Step::Read(run, _) = pane.apply(ArpMonitorMessage::Scan) else {
            panic!("a read");
        };
        pane.apply(ArpMonitorMessage::Finished(run, table("AA-50-56-01-02-03")));
        assert_eq!(pane.entries[0].status, ArpStatus::Changed);
        let Step::Copy(CopySlot::ArpMonCsv, csv) = pane.apply(ArpMonitorMessage::CopyCsv) else {
            panic!("a CSV");
        };
        assert_eq!(csv.lines().count(), 2);
    }

    #[test]
    fn a_table_that_could_not_be_read_is_said() {
        let mut pane = ArpMonitorPane::default();
        let Step::Read(run, _) = pane.apply(ArpMonitorMessage::Scan) else {
            panic!("a read");
        };
        pane.apply(ArpMonitorMessage::Finished(run, Some(None)));
        assert!(pane.error.is_some());
    }
}
