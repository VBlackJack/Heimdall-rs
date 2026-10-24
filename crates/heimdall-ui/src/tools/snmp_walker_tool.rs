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

//! The SNMP MIB Walker tool view: SNMPv2c MIB subtree traversal,
//! OID resolution, type decoding, the test of the common communities, and CSV export.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use heimdall_app::TabId;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};
use zeroize::Zeroizing;

use heimdall_core::tools::net_host::checked_host;
use heimdall_core::tools::snmp_walker::{
    self, COMMON_COMMUNITIES, COMMUNITY_TIMEOUT_MS, CommunityResult, DEFAULT_COMMUNITY,
    DEFAULT_OID, DEFAULT_PORT, DEFAULT_TIMEOUT_MS, MAX_COMMUNITY_LEN, MAX_WALK_RESULTS, SnmpEntry,
    WalkEnd, WalkError,
};

use super::file_export::FileExport;
use super::network_run::{RunSlot, blocking_run};
use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// What the CSV export's suggested name starts with, as the C# `snmpwalk_{host}_{now}.csv`.
const EXPORT_PREFIX: &str = "snmpwalk";

/// Messages emitted by the SNMP walker view.
#[derive(Clone)]
pub enum SnmpWalkerMessage {
    /// Host input changed.
    HostChanged(String),
    /// Community input changed.
    CommunityChanged(String),
    /// Root OID input changed.
    OidChanged(String),
    /// Start SNMP walk.
    Start,
    /// Stop walk.
    Stop,
    /// Walk `.0` read an entry.
    Entry(u64, SnmpEntry),
    /// Walk `.0` ended as `.1` says; `None` when its work failed.
    Ended(u64, Option<Result<WalkEnd, WalkError>>),
    /// Try the common communities on the host, as the C# `TestCommunitiesAsync`.
    TestCommunities,
    /// Test `.0` tried a community.
    CommunityTried(u64, CommunityResult),
    /// Test `.0` ended as `.1` says; `None` when its work failed.
    CommunitiesEnded(u64, Option<Result<WalkEnd, WalkError>>),
    /// Save the CSV export to a file, as the C# `OnExportCsvClick`.
    ExportCsv,
}

impl fmt::Debug for SnmpWalkerMessage {
    /// The community typed is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HostChanged(host) => write!(f, "HostChanged({host:?})"),
            Self::CommunityChanged(_) => f.write_str("CommunityChanged(..)"),
            Self::OidChanged(oid) => write!(f, "OidChanged({oid:?})"),
            Self::Start => f.write_str("Start"),
            Self::Stop => f.write_str("Stop"),
            Self::Entry(run, entry) => write!(f, "Entry({run}, {entry:?})"),
            Self::Ended(run, end) => write!(f, "Ended({run}, {end:?})"),
            Self::TestCommunities => f.write_str("TestCommunities"),
            // The communities tried are the common ones, never one typed.
            Self::CommunityTried(run, result) => write!(f, "CommunityTried({run}, {result:?})"),
            Self::CommunitiesEnded(run, end) => write!(f, "CommunitiesEnded({run}, {end:?})"),
            Self::ExportCsv => f.write_str("ExportCsv"),
        }
    }
}

/// The SNMP MIB Walker tool pane state.
pub struct SnmpWalkerPane {
    host: String,
    /// The community, a password of SNMPv2c: hidden in its box, wiped when replaced.
    community: Zeroizing<String>,
    oid: String,
    run: RunSlot,
    error: Option<String>,
    /// How the last walk ended, when it says something: stopped, the agent gone silent.
    notice: Option<String>,
    entries: Vec<SnmpEntry>,
    /// The run going is a community test, not a walk.
    testing: bool,
    /// What the last community test found, community after community.
    communities: Vec<CommunityResult>,
}

impl fmt::Debug for SnmpWalkerPane {
    /// The community is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SnmpWalkerPane")
            .field("host", &self.host)
            .field("oid", &self.oid)
            .field("run", &self.run)
            .field("entries", &self.entries.len())
            .field("communities", &self.communities)
            .finish_non_exhaustive()
    }
}

impl Default for SnmpWalkerPane {
    fn default() -> Self {
        Self {
            host: String::new(),
            community: Zeroizing::new(String::new()),
            oid: DEFAULT_OID.to_owned(),
            run: RunSlot::default(),
            error: None,
            notice: None,
            entries: Vec::new(),
            testing: false,
            communities: Vec::new(),
        }
    }
}

/// Outcome of updating the SNMP MIB Walker tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    /// Save this export to the file the user picks.
    Save(FileExport),
}

/// A walk to make: its run, its stop, and what it reads.
struct Walk {
    run: u64,
    stop: Arc<AtomicBool>,
    host: String,
    community: Zeroizing<String>,
    oid: String,
}

/// A community test to make: its run, its stop, and the host.
struct CommunityTest {
    run: u64,
    stop: Arc<AtomicBool>,
    host: String,
}

/// What a message asks of the pane, before its tab is known.
enum Step {
    None,
    Walk(Walk),
    Test(CommunityTest),
    /// Save this export to the file the user picks.
    Save(FileExport),
}

impl SnmpWalkerPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: SnmpWalkerMessage) -> Outcome {
        match self.apply(message) {
            Step::None => Outcome::None,
            Step::Save(export) => Outcome::Save(export),
            Step::Walk(walk) => {
                let Walk {
                    run,
                    stop,
                    host,
                    community,
                    oid,
                } = walk;
                let stream = blocking_run(
                    stop,
                    move |stop, entry| {
                        snmp_walker::walk(
                            &host,
                            DEFAULT_PORT,
                            &community,
                            &oid,
                            MAX_WALK_RESULTS,
                            DEFAULT_TIMEOUT_MS,
                            stop,
                            entry,
                        )
                    },
                    move |entry: SnmpEntry| SnmpWalkerMessage::Entry(run, entry),
                    move |end| SnmpWalkerMessage::Ended(run, end),
                );
                let task = Task::run(stream, move |message| {
                    Message::Tool(tab, super::ToolMessage::SnmpWalk(message))
                });
                Outcome::Task(self.run.attach(task))
            }
            Step::Test(test) => {
                let CommunityTest { run, stop, host } = test;
                let stream = blocking_run(
                    stop,
                    move |stop, tried| {
                        snmp_walker::test_communities(
                            &host,
                            DEFAULT_PORT,
                            &COMMON_COMMUNITIES,
                            COMMUNITY_TIMEOUT_MS,
                            stop,
                            tried,
                        )
                    },
                    move |result: CommunityResult| SnmpWalkerMessage::CommunityTried(run, result),
                    move |end| SnmpWalkerMessage::CommunitiesEnded(run, end),
                );
                let task = Task::run(stream, move |message| {
                    Message::Tool(tab, super::ToolMessage::SnmpWalk(message))
                });
                Outcome::Task(self.run.attach(task))
            }
        }
    }

    /// What `message` changes, and what it asks.
    fn apply(&mut self, message: SnmpWalkerMessage) -> Step {
        match message {
            SnmpWalkerMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
                Step::None
            }
            SnmpWalkerMessage::CommunityChanged(val) => {
                self.community = Zeroizing::new(val);
                self.error = None;
                Step::None
            }
            SnmpWalkerMessage::OidChanged(val) => {
                self.oid = val;
                self.error = None;
                Step::None
            }
            SnmpWalkerMessage::Start => self.start(),
            SnmpWalkerMessage::Stop => {
                if self.run.running() {
                    self.run.stop();
                    self.notice = Some(if self.testing {
                        fl!("ui-tool-snmpwalk-test-stopped")
                    } else {
                        fl!("ui-tool-snmpwalk-stopped")
                    });
                    self.testing = false;
                }
                Step::None
            }
            SnmpWalkerMessage::Entry(run, entry) => {
                if self.run.is_current(run) {
                    self.entries.push(entry);
                }
                Step::None
            }
            SnmpWalkerMessage::Ended(run, end) => {
                if self.run.finish(run) {
                    self.ended(end);
                }
                Step::None
            }
            SnmpWalkerMessage::TestCommunities => self.test(),
            SnmpWalkerMessage::CommunityTried(run, result) => {
                if self.run.is_current(run) {
                    self.communities.push(result);
                }
                Step::None
            }
            SnmpWalkerMessage::CommunitiesEnded(run, end) => {
                if self.run.finish(run) {
                    self.testing = false;
                    self.ended(end);
                }
                Step::None
            }
            SnmpWalkerMessage::ExportCsv => {
                if self.entries.is_empty() {
                    return Step::None;
                }
                let header = [
                    fl!("ui-tool-snmpwalk-col-oid"),
                    fl!("ui-tool-snmpwalk-col-name"),
                    fl!("ui-tool-snmpwalk-col-type"),
                    fl!("ui-tool-snmpwalk-col-value"),
                ];
                let csv = snmp_walker::build_snmp_csv(
                    &self.entries,
                    header.each_ref().map(String::as_str),
                );
                Step::Save(FileExport::csv(
                    CopySlot::SnmpWalkCsv,
                    fl!("ui-tool-snmpwalk-export-csv"),
                    EXPORT_PREFIX,
                    Some(&self.host),
                    csv,
                ))
            }
        }
    }

    /// The host typed, checked; `None` with the error said.
    fn checked_target(&mut self) -> Option<String> {
        let host = self.host.trim().to_owned();
        if host.is_empty() {
            self.error = Some(fl!("ui-tool-snmpwalk-error-host-required"));
            return None;
        }
        let host = checked_host(&host);
        if host.is_none() {
            self.error = Some(fl!("ui-tool-snmpwalk-error-invalid-host"));
        }
        host
    }

    /// A community test begun, as the C# `TestCommunitiesAsync`: the host checked, the
    /// community typed left aside.
    fn test(&mut self) -> Step {
        if self.run.running() {
            return Step::None;
        }
        let Some(host) = self.checked_target() else {
            return Step::None;
        };
        self.error = None;
        self.notice = None;
        self.communities.clear();
        self.testing = true;
        let (run, stop) = self.run.begin();
        Step::Test(CommunityTest { run, stop, host })
    }

    /// A walk begun, as the C# `WalkAsync`: the host checked, the community and the
    /// subtree given their defaults when empty.
    fn start(&mut self) -> Step {
        if self.run.running() {
            return Step::None;
        }
        let Some(host) = self.checked_target() else {
            return Step::None;
        };
        let community = match self.community.trim() {
            "" => DEFAULT_COMMUNITY,
            typed => typed,
        };
        if community.len() > MAX_COMMUNITY_LEN {
            self.error = Some(fl!(
                "ui-tool-snmpwalk-error-community-too-long",
                max = MAX_COMMUNITY_LEN
            ));
            return Step::None;
        }
        let oid = match self.oid.trim() {
            "" => DEFAULT_OID.to_owned(),
            typed => typed.to_owned(),
        };
        if snmp_walker::parse_oid_string(&oid).is_none() {
            self.error = Some(fl!("ui-tool-snmpwalk-error-invalid-oid"));
            return Step::None;
        }
        let community = Zeroizing::new(community.to_owned());
        self.error = None;
        self.notice = None;
        self.entries.clear();
        let (run, stop) = self.run.begin();
        Step::Walk(Walk {
            run,
            stop,
            host,
            community,
            oid,
        })
    }

    /// The walk that ran ended as `end` says: what it read is kept, as the C# keeps it
    /// when the agent goes silent.
    fn ended(&mut self, end: Option<Result<WalkEnd, WalkError>>) {
        let error = match end {
            Some(Ok(WalkEnd::Complete | WalkEnd::Stopped)) => return,
            Some(Ok(WalkEnd::TimedOut)) => {
                self.notice = Some(fl!("ui-tool-snmpwalk-agent-silent"));
                return;
            }
            Some(Err(WalkError::InvalidOid)) => fl!("ui-tool-snmpwalk-error-invalid-oid"),
            Some(Err(WalkError::CommunityTooLong)) => fl!(
                "ui-tool-snmpwalk-error-community-too-long",
                max = MAX_COMMUNITY_LEN
            ),
            Some(Err(WalkError::Unresolved)) => fl!("ui-tool-snmpwalk-error-unresolved"),
            Some(Err(WalkError::Socket)) => fl!("ui-tool-snmpwalk-error-socket"),
            Some(Err(WalkError::NoResponse)) => fl!("ui-tool-snmpwalk-error-no-response"),
            None => fl!("ui-tool-snmpwalk-error-failed"),
        };
        self.error = Some(error);
    }

    /// Renders the SNMP MIB Walker tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::SnmpWalk(msg));

        let host_input = text_input(&fl!("ui-tool-snmpwalk-host-watermark"), &self.host)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(SnmpWalkerMessage::HostChanged(v)))
            .on_submit(send(SnmpWalkerMessage::Start))
            .width(Length::FillPortion(2));

        let comm_input = text_input(
            &fl!(
                "ui-tool-snmpwalk-community-watermark",
                community = DEFAULT_COMMUNITY
            ),
            &self.community,
        )
        .secure(true)
        .font(super::BOX_FONT)
        .size(font_size::BODY_LARGE)
        .padding(super::INPUT_PADDING)
        .style(styles::text_input)
        .on_input(move |v| send(SnmpWalkerMessage::CommunityChanged(v)))
        .on_submit(send(SnmpWalkerMessage::Start))
        .width(Length::FillPortion(1));

        let oid_input = text_input(DEFAULT_OID, &self.oid)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(SnmpWalkerMessage::OidChanged(v)))
            .on_submit(send(SnmpWalkerMessage::Start))
            .width(Length::FillPortion(2));

        let action_btn = if self.run.running() {
            super::action_button(
                fl!("ui-tool-snmpwalk-stop"),
                true,
                Some(send(SnmpWalkerMessage::Stop)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-snmpwalk-start"),
                true,
                Some(send(SnmpWalkerMessage::Start)),
            )
        };
        let test_btn = super::action_button(
            if self.testing {
                fl!("ui-tool-snmpwalk-testing")
            } else {
                fl!("ui-tool-snmpwalk-test-communities")
            },
            false,
            (!self.run.running()).then(|| send(SnmpWalkerMessage::TestCommunities)),
        );

        let inputs_row = row![
            column![super::field_label(fl!("ui-tool-snmpwalk-host")), host_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(2)),
            column![
                super::field_label(fl!("ui-tool-snmpwalk-community")),
                comm_input
            ]
            .spacing(spacing::XS)
            .width(Length::FillPortion(1)),
            column![super::field_label(fl!("ui-tool-snmpwalk-oid")), oid_input]
                .spacing(spacing::XS)
                .width(Length::FillPortion(2)),
            container(action_btn).align_y(Alignment::End),
            container(test_btn).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::End);

        let input_card = container(column![inputs_row])
            .padding(spacing::MD)
            .style(styles::card);

        let mut body = column![input_card].spacing(spacing::MD);

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
        if let Some(notice) = &self.notice {
            body = body.push(
                container(
                    text(notice.as_str())
                        .size(font_size::BODY)
                        .style(text::secondary),
                )
                .padding(spacing::MD)
                .style(styles::card),
            );
        }

        if !self.communities.is_empty() {
            body = body.push(self.communities_card());
        }

        if !self.entries.is_empty() {
            let export_csv_btn = super::export_button(
                fl!("ui-tool-snmpwalk-export-csv"),
                state.copied(CopySlot::SnmpWalkCsv),
                send(SnmpWalkerMessage::ExportCsv),
            );

            let header_row = row![
                text(fl!(
                    "ui-tool-snmpwalk-entry-count",
                    count = self.entries.len()
                ))
                .size(font_size::SUBTITLE)
                .width(Length::Fill),
                export_csv_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            let table_headers = row![
                text(fl!("ui-tool-snmpwalk-col-oid"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-snmpwalk-col-name"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-snmpwalk-col-type"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(100.0)),
                text(fl!("ui-tool-snmpwalk-col-value"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(3)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for e in &self.entries {
                let r = row![
                    text(&e.oid)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    text(&e.name)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    text(&e.val_type)
                        .size(font_size::BODY)
                        .style(text::secondary)
                        .width(Length::Fixed(100.0)),
                    text(&e.value)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(3)),
                ]
                .spacing(spacing::SM);

                entry_rows = entry_rows.push(r);
            }

            let card = container(column![header_row, entry_rows].spacing(spacing::MD))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        }

        super::content_column(body)
    }

    /// What the last community test found, as the C# results grid: each community tried,
    /// whether the agent answered it and the name it gave.
    fn communities_card(&self) -> Element<'_, Message> {
        let caption = |label: String, width: Length| {
            text(label)
                .size(font_size::CAPTION)
                .style(text::secondary)
                .width(width)
        };
        let mut rows = column![
            text(fl!(
                "ui-tool-snmpwalk-accepted-count",
                count = self.communities.iter().filter(|r| r.accepted).count()
            ))
            .size(font_size::SUBTITLE),
            row![
                caption(fl!("ui-tool-snmpwalk-community"), Length::FillPortion(1)),
                caption(fl!("ui-tool-snmpwalk-col-status"), Length::FillPortion(1)),
                caption(fl!("ui-tool-snmpwalk-col-sys-name"), Length::FillPortion(2)),
            ]
            .spacing(spacing::SM),
        ]
        .spacing(spacing::XS);
        for result in &self.communities {
            let (status, style): (String, fn(&Theme) -> text::Style) = if result.accepted {
                (
                    fl!("ui-tool-snmpwalk-community-accepted"),
                    styles::success_text,
                )
            } else {
                (fl!("ui-tool-snmpwalk-community-rejected"), text::secondary)
            };
            // What the agent named itself is the agent's text.
            let name = result
                .sys_name
                .as_deref()
                .map(heimdall_app::server_text)
                .unwrap_or_default();
            rows = rows.push(
                row![
                    text(&result.community)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(1)),
                    text(status)
                        .size(font_size::BODY)
                        .style(style)
                        .width(Length::FillPortion(1)),
                    text(name)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                ]
                .spacing(spacing::SM),
            );
        }
        container(rows)
            .padding(spacing::MD)
            .style(styles::card)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    fn entry(n: u32) -> SnmpEntry {
        SnmpEntry {
            oid: format!("1.3.6.1.2.1.1.{n}.0"),
            name: String::new(),
            val_type: "STRING".to_owned(),
            value: format!("v{n}"),
        }
    }

    fn started(pane: &mut SnmpWalkerPane) -> Walk {
        pane.apply(SnmpWalkerMessage::HostChanged("10.0.0.1".to_owned()));
        match pane.apply(SnmpWalkerMessage::Start) {
            Step::Walk(walk) => walk,
            _ => panic!("no walk"),
        }
    }

    #[test]
    fn an_empty_community_is_the_csharp_default_and_never_shown_in_debug() {
        let mut pane = SnmpWalkerPane::default();
        assert!(pane.community.is_empty(), "nothing typed for the user");
        let walk = started(&mut pane);
        assert_eq!(walk.community.as_str(), DEFAULT_COMMUNITY);
        assert_eq!(walk.oid, DEFAULT_OID);
        pane.apply(SnmpWalkerMessage::Stop);
        pane.apply(SnmpWalkerMessage::CommunityChanged("s3cret".to_owned()));
        let walk = started(&mut pane);
        assert_eq!(walk.community.as_str(), "s3cret");
        assert!(!format!("{pane:?}").contains("s3cret"));
        assert!(
            !format!(
                "{:?}",
                SnmpWalkerMessage::CommunityChanged("s3cret".to_owned())
            )
            .contains("s3cret")
        );
    }

    #[test]
    fn entries_stream_in_and_a_stop_lets_the_late_ones_go() {
        let mut pane = SnmpWalkerPane::default();
        let walk = started(&mut pane);
        pane.apply(SnmpWalkerMessage::Entry(walk.run, entry(1)));
        pane.apply(SnmpWalkerMessage::Entry(walk.run, entry(2)));
        assert_eq!(pane.entries.len(), 2);
        pane.apply(SnmpWalkerMessage::Stop);
        assert!(walk.stop.load(Ordering::Relaxed));
        pane.apply(SnmpWalkerMessage::Entry(walk.run, entry(3)));
        pane.apply(SnmpWalkerMessage::Ended(
            walk.run,
            Some(Ok(WalkEnd::Stopped)),
        ));
        assert_eq!(pane.entries.len(), 2, "kept, none added after the stop");
        let Step::Save(export) = pane.apply(SnmpWalkerMessage::ExportCsv) else {
            panic!("a CSV");
        };
        assert_eq!(export.slot(), CopySlot::SnmpWalkCsv);
        assert!(export.suggested_name().starts_with("snmpwalk_"));
        let csv = export.content();
        assert_eq!(csv.lines().count(), 3);
    }

    fn tried(community: &str, accepted: bool) -> CommunityResult {
        CommunityResult {
            community: community.to_owned(),
            accepted,
            sys_name: accepted.then(|| "core-sw1".to_owned()),
        }
    }

    #[test]
    fn a_community_test_lists_each_community_and_is_one_run_with_the_walk() {
        let mut pane = SnmpWalkerPane::default();
        assert!(matches!(
            pane.apply(SnmpWalkerMessage::TestCommunities),
            Step::None
        ));
        assert!(pane.error.is_some(), "a host first");
        pane.apply(SnmpWalkerMessage::HostChanged("10.0.0.1".to_owned()));
        pane.apply(SnmpWalkerMessage::CommunityChanged("s3cret".to_owned()));
        let Step::Test(test) = pane.apply(SnmpWalkerMessage::TestCommunities) else {
            panic!("a test");
        };
        assert_eq!(test.host, "10.0.0.1");
        assert!(pane.testing);
        assert!(
            matches!(pane.apply(SnmpWalkerMessage::Start), Step::None),
            "no walk while the test runs"
        );
        pane.apply(SnmpWalkerMessage::CommunityTried(
            test.run,
            tried("public", false),
        ));
        pane.apply(SnmpWalkerMessage::CommunityTried(
            test.run,
            tried("monitor", true),
        ));
        pane.apply(SnmpWalkerMessage::CommunitiesEnded(
            test.run,
            Some(Ok(WalkEnd::Complete)),
        ));
        assert!(!pane.testing && !pane.run.running());
        assert_eq!(pane.communities.len(), 2);
        assert!(pane.communities[1].accepted);
        assert!(
            !format!("{pane:?}").contains("s3cret"),
            "the community typed is still never shown"
        );

        let Step::Test(test) = pane.apply(SnmpWalkerMessage::TestCommunities) else {
            panic!("a test");
        };
        assert!(pane.communities.is_empty(), "a new test starts afresh");
        pane.apply(SnmpWalkerMessage::Stop);
        assert!(test.stop.load(Ordering::Relaxed));
        assert!(!pane.testing);
        pane.apply(SnmpWalkerMessage::CommunityTried(
            test.run,
            tried("public", true),
        ));
        assert!(pane.communities.is_empty(), "nothing after the stop");
        let Step::Test(test) = pane.apply(SnmpWalkerMessage::TestCommunities) else {
            panic!("a test");
        };
        pane.apply(SnmpWalkerMessage::CommunitiesEnded(
            test.run,
            Some(Err(WalkError::Unresolved)),
        ));
        assert!(pane.error.is_some(), "a host that does not resolve is said");
    }

    #[test]
    fn bad_inputs_and_a_silent_agent_are_said() {
        let mut pane = SnmpWalkerPane::default();
        pane.apply(SnmpWalkerMessage::HostChanged("-c x".to_owned()));
        assert!(matches!(pane.apply(SnmpWalkerMessage::Start), Step::None));
        assert!(pane.error.is_some());
        pane.apply(SnmpWalkerMessage::HostChanged("10.0.0.1".to_owned()));
        pane.apply(SnmpWalkerMessage::OidChanged("1.99999999999".to_owned()));
        assert!(matches!(pane.apply(SnmpWalkerMessage::Start), Step::None));
        assert!(pane.error.is_some());
        pane.apply(SnmpWalkerMessage::OidChanged(String::new()));
        let walk = started(&mut pane);
        pane.apply(SnmpWalkerMessage::Ended(
            walk.run,
            Some(Err(WalkError::NoResponse)),
        ));
        assert!(pane.error.is_some());
        let walk = started(&mut pane);
        pane.apply(SnmpWalkerMessage::Entry(walk.run, entry(1)));
        pane.apply(SnmpWalkerMessage::Ended(
            walk.run,
            Some(Ok(WalkEnd::TimedOut)),
        ));
        assert!(pane.error.is_none() && pane.notice.is_some());
        assert_eq!(pane.entries.len(), 1, "what the agent said is kept");
    }
}
