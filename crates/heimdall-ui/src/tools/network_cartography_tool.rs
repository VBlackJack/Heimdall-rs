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

//! The Network Cartography tool view, as the C# `NetworkCartographyView`: subnet host
//! expansion, host discovery (ping sweep, ARP table, port probes), each host's name, MAC
//! address and vendor, role classification, the scan history and the knowledge base, and
//! CSV export.

use std::fmt;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use heimdall_app::TabId;
use iced::widget::{checkbox, column, container, pick_list, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::network_cartography::{
    self, CartographySummary, CidrError, DiscoverOptions, DiscoveredNode, DiscoveryEvent,
    LARGE_SUBNET_THRESHOLD, MAX_PREFIX, MIN_PREFIX, NodeRole, PROBE_TIMEOUT_MS, SystemProber,
};
use heimdall_core::tools::network_fingerprint::Fingerprint;
use heimdall_core::tools::network_history::{
    self, KnowledgeBase, ScanDiff, ScanSnapshot, ScanStore, SnapshotInfo,
};

use super::file_export::FileExport;
use super::network_run::{RunSlot, blocking_run};
use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// What the CSV export's suggested name starts with, as the C# `cartography_{subnet}_{now}.csv`.
const EXPORT_PREFIX: &str = "cartography";

/// How a snapshot's moment is shown in the history picker, in local time.
const SNAPSHOT_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
/// Below this age the knowledge base was updated "just now", as the C# `FormatTimeAgo`.
const JUST_NOW_SECS: u64 = 5 * 60;
const SECS_PER_MINUTE: u64 = 60;
const SECS_PER_HOUR: u64 = 60 * SECS_PER_MINUTE;
const SECS_PER_DAY: u64 = 24 * SECS_PER_HOUR;
/// Width of the IP address column.
const IP_WIDTH: f32 = 120.0;
/// Width of the MAC address column.
const MAC_WIDTH: f32 = 150.0;
/// Width of the latency column.
const LATENCY_WIDTH: f32 = 70.0;
/// What separates the facts of the line shown under a host.
const FINGERPRINT_SEPARATOR: &str = "  |  ";
/// How long the certificate of an HTTPS port is waited for, as the C# TLS inspection.
const TLS_SUBJECT_TIMEOUT_MS: u64 = 3000;

/// Messages emitted by the network cartography view.
#[derive(Debug, Clone)]
pub enum NetworkCartographyMessage {
    /// CIDR input changed.
    CidrChanged(String),
    /// Look up the names of the hosts found, as the C# `ReverseDns`.
    ReverseDnsToggled(bool),
    /// Use the knowledge base, as the C# `UseKnowledgeBase`.
    KnowledgeToggled(bool),
    /// Start network discovery.
    Start,
    /// Scan the large subnet asked about, as the C# confirmation's yes.
    ConfirmLarge,
    /// Stop discovery.
    Stop,
    /// Discovery `.0` found something.
    Event(u64, DiscoveryEvent),
    /// Discovery `.0` is over as `.1` says; `None` when its work failed.
    Ended(u64, Option<ScanEnd>),
    /// What the history and the knowledge base hold, read when the tab opens or changed.
    Stored(StoredView),
    /// The history's snapshot to compare the last scan with.
    CompareSelected(SnapshotChoice),
    /// Compare the last scan with the snapshot chosen, as the C# `CompareWithHistory`.
    Compare,
    /// The snapshot `.0` read for the comparison, `None` when it could not be.
    Compared(String, Option<ScanSnapshot>),
    /// Clear the knowledge base, once confirmed, as the C# `ClearKbAsync`.
    ClearKnowledge,
    /// The clearing confirmed, or not.
    ConfirmClear(bool),
    /// The knowledge base cleared, or why it could not be.
    Cleared(Result<StoredView, String>),
    /// Save the CSV export to a file, as the C# `OnExportCsvClick`.
    ExportCsv,
}

/// What the knowledge base holds, as the C# `KbStatsText` says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KnowledgeStats {
    hosts: usize,
    last_updated: u64,
}

impl KnowledgeStats {
    fn of(knowledge: &KnowledgeBase) -> Self {
        Self {
            hosts: knowledge.host_count(),
            last_updated: knowledge.last_updated,
        }
    }
}

/// What the store holds: its snapshots, the most recent first, and the knowledge base's
/// size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredView {
    history: Vec<SnapshotInfo>,
    knowledge: KnowledgeStats,
}

impl StoredView {
    /// What `store` holds now.
    fn read(store: &ScanStore) -> Self {
        Self {
            history: store.list_snapshots(),
            knowledge: KnowledgeStats::of(&store.load_knowledge()),
        }
    }
}

/// How a discovery ended.
#[derive(Debug, Clone)]
pub enum ScanEnd {
    /// Stopped: nothing saved, as the C# saves nothing of a cancelled scan.
    Stopped,
    /// Completed: its snapshot, the name it was saved under, and what the store holds now.
    Completed {
        snapshot: ScanSnapshot,
        saved: Option<String>,
        stored: Option<StoredView>,
    },
}

/// A snapshot of the history, as the picker shows it: its local moment and its subnet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotChoice(SnapshotInfo);

impl fmt::Display for SnapshotChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let moment = i64::try_from(self.0.timestamp)
            .ok()
            .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
            .map(|utc| {
                utc.with_timezone(&chrono::Local)
                    .format(SNAPSHOT_TIME_FORMAT)
                    .to_string()
            })
            .unwrap_or_default();
        f.write_str(&fl!(
            "ui-tool-netmap-history-entry",
            time = moment,
            subnet = self.0.subnet.as_str()
        ))
    }
}

/// The Network Cartography tool pane state.
#[derive(Debug)]
pub struct NetworkCartographyPane {
    cidr: String,
    reverse_dns: bool,
    use_knowledge: bool,
    /// Where the history and the knowledge base are kept; `None` without a data folder.
    store: Option<ScanStore>,
    run: RunSlot,
    error: Option<String>,
    /// What the last scan says beside its results: a stop, its snapshot saved.
    notice: Option<String>,
    /// The hosts of a subnet larger than [`LARGE_SUBNET_THRESHOLD`], waiting for the user's
    /// yes, as the C# asks before scanning one.
    pending: Option<Vec<Ipv4Addr>>,
    /// The hosts the last scan probes.
    total: usize,
    /// The sweep is over: the names of this many hosts are looked up.
    naming: Option<usize>,
    nodes: Vec<DiscoveredNode>,
    summary: CartographySummary,
    /// The last completed scan, which the history is compared with.
    last_snapshot: Option<ScanSnapshot>,
    /// The file it was saved under, left out of the picker.
    saved_name: Option<String>,
    history: Vec<SnapshotChoice>,
    compare_with: Option<SnapshotChoice>,
    /// What changed since the snapshot compared with, as the C# `DiffText`.
    diff: Option<ScanDiff>,
    knowledge: Option<KnowledgeStats>,
    /// The clearing of the knowledge base asked, waiting for the user's yes.
    confirm_clear: bool,
}

impl NetworkCartographyPane {
    /// The pane, its history and knowledge base kept in `data_dir`, Heimdall's data folder
    /// (none without one).
    #[must_use]
    pub fn new(data_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            cidr: String::new(),
            reverse_dns: DiscoverOptions::default().reverse_dns,
            use_knowledge: false,
            store: data_dir.map(ScanStore::new),
            run: RunSlot::default(),
            error: None,
            notice: None,
            pending: None,
            total: 0,
            naming: None,
            nodes: Vec::new(),
            summary: CartographySummary::default(),
            last_snapshot: None,
            saved_name: None,
            history: Vec::new(),
            compare_with: None,
            diff: None,
            knowledge: None,
            confirm_clear: false,
        }
    }
}

/// Outcome of updating the Network Cartography tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    /// Save this export to the file the user picks.
    Save(FileExport),
}

/// A discovery to run.
#[derive(Debug)]
struct Discovery {
    run: u64,
    stop: Arc<AtomicBool>,
    ips: Vec<Ipv4Addr>,
    subnet: String,
    options: DiscoverOptions,
    use_knowledge: bool,
    store: Option<ScanStore>,
}

/// What a message asks of the pane, before its tab is known.
#[derive(Debug)]
enum Step {
    None,
    Discover(Discovery),
    /// Read the snapshot named `.1` of store `.0` to compare with.
    Load(ScanStore, String),
    /// Clear the knowledge base of the store.
    Clear(ScanStore),
    /// Save this export to the file the user picks.
    Save(FileExport),
}

/// `work` run off the window, its end as `end` says.
fn blocking<T: Send + 'static>(
    tab: TabId,
    work: impl FnOnce() -> T + Send + 'static,
    end: impl FnOnce(Option<T>) -> NetworkCartographyMessage + Send + 'static,
) -> Task<Message> {
    Task::perform(
        async move { tokio::task::spawn_blocking(work).await.ok() },
        move |done| Message::Tool(tab, super::ToolMessage::NetMap(end(done))),
    )
}

/// The subject of the certificate `ip`:`port` presents, read by a handshake that takes any
/// certificate and sends nothing once it is over, as the C# `InspectTlsWithHttpAsync` reads
/// the certificate it does not trust. The discovery runs off the window's runtime, so the
/// handshake runs on one of its own.
fn tls_subject(ip: Ipv4Addr, port: u16) -> Option<String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    runtime.block_on(async {
        let read = async {
            let tcp = tokio::net::TcpStream::connect((ip, port)).await.ok()?;
            let name = heimdall_tls::inspection_server_name(&ip.to_string())?;
            let tls = heimdall_tls::inspection_connector()
                .connect(name, tcp)
                .await
                .ok()?;
            let chain = heimdall_tls::presented_chain(tls.get_ref().1)?;
            let leaf = chain.certificates.first()?;
            sealvault::cert::inspect(leaf).map(|summary| summary.subject)
        };
        tokio::time::timeout(Duration::from_millis(TLS_SUBJECT_TIMEOUT_MS), read)
            .await
            .ok()
            .flatten()
    })
}

/// A discovery's whole run, off the window: the knowledge base read when it is used, the
/// hosts discovered, then, once completed, its snapshot saved in the history and learned by
/// the knowledge base, as the C# `ScanAsync` does after its scan.
fn run_discovery(
    discovery: &Discovery,
    stop: &AtomicBool,
    on_event: &mut dyn FnMut(DiscoveryEvent),
) -> ScanEnd {
    let store = discovery.store.as_ref();
    let mut knowledge = store.map(ScanStore::load_knowledge).unwrap_or_default();
    let priors = if discovery.use_knowledge {
        knowledge.priors(&discovery.ips, network_history::now_secs())
    } else {
        std::collections::HashMap::new()
    };
    let prober = SystemProber::new(PROBE_TIMEOUT_MS).with_tls_subject(tls_subject);
    let nodes = network_cartography::discover(
        &discovery.ips,
        &priors,
        discovery.options,
        &prober,
        stop,
        on_event,
    );
    if stop.load(Ordering::Relaxed) {
        return ScanEnd::Stopped;
    }
    let snapshot = ScanSnapshot::new(&discovery.subnet, network_history::now_secs(), &nodes);
    let Some(store) = store else {
        return ScanEnd::Completed {
            snapshot,
            saved: None,
            stored: None,
        };
    };
    // Best effort, as the C# logs a failed save and keeps the results.
    let saved = store.save_snapshot(&snapshot).ok();
    knowledge.merge(&snapshot);
    let _ = store.save_knowledge(&knowledge);
    ScanEnd::Completed {
        snapshot,
        saved,
        stored: Some(StoredView::read(store)),
    }
}

impl NetworkCartographyPane {
    /// The work of the tab just opened, as the C# `Initialize`: what the history and the
    /// knowledge base hold, read.
    pub fn open(&mut self, tab: TabId) -> Task<Message> {
        let Some(store) = self.store.clone() else {
            return Task::none();
        };
        blocking(
            tab,
            move || StoredView::read(&store),
            |stored| match stored {
                Some(stored) => NetworkCartographyMessage::Stored(stored),
                None => NetworkCartographyMessage::Cleared(Err(String::new())),
            },
        )
    }

    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: NetworkCartographyMessage) -> Outcome {
        match self.apply(message) {
            Step::None => Outcome::None,
            Step::Save(export) => Outcome::Save(export),
            Step::Load(store, file_name) => {
                let read = file_name.clone();
                Outcome::Task(blocking(
                    tab,
                    move || store.load_snapshot(&read),
                    move |snapshot| {
                        NetworkCartographyMessage::Compared(file_name, snapshot.flatten())
                    },
                ))
            }
            Step::Clear(store) => Outcome::Task(blocking(
                tab,
                move || {
                    store
                        .save_knowledge(&KnowledgeBase::default())
                        .map(|()| StoredView::read(&store))
                        .map_err(|error| error.to_string())
                },
                |cleared| {
                    NetworkCartographyMessage::Cleared(
                        cleared.unwrap_or_else(|| Err(String::new())),
                    )
                },
            )),
            Step::Discover(discovery) => {
                let run = discovery.run;
                let stop = Arc::clone(&discovery.stop);
                let stream = blocking_run(
                    stop,
                    move |stop, event| run_discovery(&discovery, stop, event),
                    move |event: DiscoveryEvent| NetworkCartographyMessage::Event(run, event),
                    move |ended| NetworkCartographyMessage::Ended(run, ended),
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
            NetworkCartographyMessage::ReverseDnsToggled(on) => {
                self.reverse_dns = on;
                Step::None
            }
            NetworkCartographyMessage::KnowledgeToggled(on) => {
                self.use_knowledge = on;
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
                    self.naming = None;
                    self.notice = Some(fl!("ui-tool-netmap-stopped"));
                }
                Step::None
            }
            NetworkCartographyMessage::Event(run, event) => {
                if self.run.is_current(run) {
                    self.event(event);
                }
                Step::None
            }
            NetworkCartographyMessage::Ended(run, end) => {
                if self.run.finish(run) {
                    self.naming = None;
                    self.ended(end);
                }
                Step::None
            }
            NetworkCartographyMessage::Stored(stored) => {
                self.stored(stored);
                Step::None
            }
            NetworkCartographyMessage::CompareSelected(choice) => {
                self.compare_with = Some(choice);
                self.diff = None;
                Step::None
            }
            NetworkCartographyMessage::Compare => {
                match (&self.store, &self.compare_with, &self.last_snapshot) {
                    (Some(store), Some(choice), Some(_)) => {
                        Step::Load(store.clone(), choice.0.file_name.clone())
                    }
                    _ => Step::None,
                }
            }
            NetworkCartographyMessage::Compared(file_name, snapshot) => {
                let chosen = self
                    .compare_with
                    .as_ref()
                    .is_some_and(|choice| choice.0.file_name == file_name);
                match (snapshot, &self.last_snapshot) {
                    (Some(older), Some(newer)) if chosen => {
                        self.diff = Some(network_history::compare(&older, newer));
                    }
                    (None, _) if chosen => {
                        self.error = Some(fl!("ui-tool-netmap-error-history-read"));
                    }
                    _ => {}
                }
                Step::None
            }
            NetworkCartographyMessage::ClearKnowledge => {
                if !self.run.running() && self.store.is_some() {
                    self.confirm_clear = true;
                }
                Step::None
            }
            NetworkCartographyMessage::ConfirmClear(yes) => {
                self.confirm_clear = false;
                match &self.store {
                    Some(store) if yes && !self.run.running() => Step::Clear(store.clone()),
                    _ => Step::None,
                }
            }
            NetworkCartographyMessage::Cleared(cleared) => {
                match cleared {
                    Ok(stored) => {
                        self.stored(stored);
                        self.notice = Some(fl!("ui-tool-netmap-kb-cleared"));
                    }
                    Err(why) if why.is_empty() => {}
                    Err(why) => {
                        self.error = Some(fl!("ui-tool-netmap-error-kb-clear", reason = why));
                    }
                }
                Step::None
            }
            NetworkCartographyMessage::ExportCsv => {
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
                    fl!("ui-tool-netmap-col-os"),
                    fl!("ui-tool-netmap-col-os-confidence"),
                    fl!("ui-tool-netmap-col-netbios-name"),
                    fl!("ui-tool-netmap-col-netbios-domain"),
                    fl!("ui-tool-netmap-col-snmp-descr"),
                    fl!("ui-tool-netmap-col-snmp-name"),
                    fl!("ui-tool-netmap-col-snmp-location"),
                    fl!("ui-tool-netmap-col-ntlm-dns-name"),
                    fl!("ui-tool-netmap-col-os-build"),
                    fl!("ui-tool-netmap-col-smb-dialect"),
                    fl!("ui-tool-netmap-col-smb-signing"),
                    fl!("ui-tool-netmap-col-ssh-banner"),
                    fl!("ui-tool-netmap-col-ssh-hassh"),
                    fl!("ui-tool-netmap-col-tls-subject"),
                ];
                let csv = network_cartography::build_nodes_csv(
                    &self.nodes,
                    header.each_ref().map(String::as_str),
                    super::vendor_label,
                    role_label,
                );
                Step::Save(FileExport::csv(
                    CopySlot::NetMapCsv,
                    fl!("ui-tool-netmap-export-csv"),
                    EXPORT_PREFIX,
                    Some(&self.cidr),
                    csv,
                ))
            }
        }
    }

    /// What discovery found, shown.
    fn event(&mut self, event: DiscoveryEvent) {
        match event {
            DiscoveryEvent::Host(node) => {
                self.summary.hosts_scanned += 1;
                if let Some(node) = node {
                    self.summary.hosts_up += 1;
                    self.summary.open_ports_count += node.open_ports.len();
                    let key = address(&node);
                    let at = self.nodes.partition_point(|shown| address(shown) < key);
                    self.nodes.insert(at, node);
                }
            }
            DiscoveryEvent::Naming(count) => self.naming = Some(count),
            DiscoveryEvent::Details {
                ip,
                hostname,
                mac,
                vendor,
            } => {
                if let Some(node) = self.nodes.iter_mut().find(|node| node.ip == ip) {
                    node.hostname = hostname;
                    node.mac = mac;
                    node.vendor = vendor;
                }
            }
            DiscoveryEvent::Fingerprint { ip, fingerprint } => {
                if let Some(node) = self.nodes.iter_mut().find(|node| node.ip == ip) {
                    node.fingerprint = fingerprint;
                }
            }
        }
    }

    /// The discovery that ran ended as `end` says.
    fn ended(&mut self, end: Option<ScanEnd>) {
        match end {
            None => self.error = Some(fl!("ui-tool-netmap-error-failed")),
            Some(ScanEnd::Stopped) => {}
            Some(ScanEnd::Completed {
                snapshot,
                saved,
                stored,
            }) => {
                self.notice = saved.is_some().then(|| fl!("ui-tool-netmap-saved"));
                self.saved_name = saved;
                self.last_snapshot = Some(snapshot);
                if let Some(stored) = stored {
                    self.stored(stored);
                }
            }
        }
    }

    /// What the store holds, shown: its snapshots but the last scan's own, and the
    /// knowledge base's size.
    fn stored(&mut self, stored: StoredView) {
        self.history = stored
            .history
            .into_iter()
            .filter(|info| Some(&info.file_name) != self.saved_name.as_ref())
            .map(SnapshotChoice)
            .collect();
        if self
            .compare_with
            .as_ref()
            .is_some_and(|chosen| !self.history.contains(chosen))
        {
            self.compare_with = None;
            self.diff = None;
        }
        self.knowledge = Some(stored.knowledge);
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
        self.confirm_clear = false;
        self.naming = None;
        self.nodes.clear();
        self.summary = CartographySummary::default();
        self.last_snapshot = None;
        self.diff = None;
        self.total = ips.len();
        let subnet = network_cartography::canonical_subnet(&self.cidr).unwrap_or_default();
        let (run, stop) = self.run.begin();
        Step::Discover(Discovery {
            run,
            stop,
            ips,
            subnet,
            options: DiscoverOptions {
                reverse_dns: self.reverse_dns,
            },
            use_knowledge: self.use_knowledge,
            store: self.store.clone(),
        })
    }

    /// Renders the Network Cartography tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::NetMap(msg));
        let running = self.run.running();

        let cidr_input = text_input(&fl!("ui-tool-netmap-subnet-watermark"), &self.cidr)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |v| send(NetworkCartographyMessage::CidrChanged(v)))
            .on_submit(send(NetworkCartographyMessage::Start))
            .width(Length::FillPortion(3));

        let action_btn = if running {
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

        let option =
            |checked: bool, label: String, toggled: fn(bool) -> NetworkCartographyMessage| {
                let mut option = checkbox(checked)
                    .label(label)
                    .style(styles::checkbox)
                    .text_size(font_size::BODY);
                if !running {
                    option = option.on_toggle(move |on| send(toggled(on)));
                }
                option
            };
        let mut options_row = row![option(
            self.reverse_dns,
            fl!("ui-tool-netmap-reverse-dns"),
            NetworkCartographyMessage::ReverseDnsToggled
        ),]
        .spacing(spacing::MD)
        .align_y(Alignment::Center);
        if self.store.is_some() {
            options_row = options_row.push(option(
                self.use_knowledge,
                fl!("ui-tool-netmap-use-kb"),
                NetworkCartographyMessage::KnowledgeToggled,
            ));
            if let Some(knowledge) = self.knowledge {
                options_row = options_row.push(
                    text(knowledge_text(knowledge, network_history::now_secs()))
                        .size(font_size::BODY)
                        .style(text::secondary),
                );
                if knowledge.hosts > 0 {
                    options_row = options_row.push(super::action_button(
                        fl!("ui-tool-netmap-kb-clear"),
                        false,
                        (!running).then(|| send(NetworkCartographyMessage::ClearKnowledge)),
                    ));
                }
            }
        }

        let input_card = container(column![inputs_row, options_row].spacing(spacing::SM))
            .padding(spacing::MD)
            .style(styles::card);

        let mut body = column![input_card].spacing(spacing::MD);

        if self.confirm_clear {
            body = body.push(confirm_row(
                fl!("ui-tool-netmap-kb-clear-confirm"),
                vec![
                    super::action_button(
                        fl!("ui-tool-netmap-kb-clear"),
                        true,
                        Some(send(NetworkCartographyMessage::ConfirmClear(true))),
                    ),
                    super::action_button(
                        fl!("ui-tool-netmap-kb-keep"),
                        false,
                        Some(send(NetworkCartographyMessage::ConfirmClear(false))),
                    ),
                ],
            ));
        }
        if let Some(ips) = &self.pending {
            body = body.push(confirm_row(
                fl!("ui-tool-netmap-warning-large", count = ips.len()),
                vec![super::action_button(
                    fl!("ui-tool-netmap-scan-anyway"),
                    false,
                    Some(send(NetworkCartographyMessage::ConfirmLarge)),
                )],
            ));
        }
        if running || self.summary.hosts_scanned > 0 {
            let progress = match self.naming {
                Some(count) if running => fl!("ui-tool-netmap-naming", count = count),
                _ => fl!(
                    "ui-tool-netmap-progress",
                    done = self.summary.hosts_scanned,
                    total = self.total
                ),
            };
            body = body.push(text(progress).size(font_size::BODY).style(text::secondary));
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
        if self.last_snapshot.is_some() && !self.history.is_empty() && !running {
            body = body.push(self.history_card(send));
        }

        if !self.nodes.is_empty() {
            body = body.push(self.summary_cards());
            body = body.push(self.nodes_card(state, send));
        }

        super::content_column(body)
    }

    /// The snapshot to compare the last scan with, and what changed since, as the C#
    /// `CmbHistory` and `DiffText`.
    fn history_card<'a>(
        &'a self,
        send: impl Fn(NetworkCartographyMessage) -> Message + Copy + 'a,
    ) -> Element<'a, Message> {
        let picker = pick_list(
            &self.history[..],
            self.compare_with.clone(),
            move |choice| send(NetworkCartographyMessage::CompareSelected(choice)),
        )
        .style(styles::pick_list)
        .width(Length::FillPortion(2));
        let compare = super::action_button(
            fl!("ui-tool-netmap-compare"),
            false,
            self.compare_with
                .is_some()
                .then(|| send(NetworkCartographyMessage::Compare)),
        );
        let mut card = column![
            row![
                text(fl!("ui-tool-netmap-compare-with"))
                    .size(font_size::BODY)
                    .style(text::secondary),
                picker,
                compare,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center)
        ]
        .spacing(spacing::SM);
        if let Some(diff) = self.diff {
            card = card.push(
                text(diff_text(diff))
                    .size(font_size::BODY)
                    .style(text::primary),
            );
        }
        container(card)
            .padding(spacing::MD)
            .style(styles::card)
            .into()
    }

    /// The counts of the scan, as the C# summary cards.
    fn summary_cards(&self) -> Element<'_, Message> {
        let card = |label: String, value: usize, style: fn(&Theme) -> text::Style| {
            container(
                column![
                    text(label).size(font_size::CAPTION).style(text::secondary),
                    text(value.to_string())
                        .size(font_size::TITLE)
                        .font(super::BOX_FONT)
                        .style(style),
                ]
                .spacing(spacing::XS),
            )
            .padding(spacing::SM)
            .style(styles::card)
            .width(Length::FillPortion(1))
        };
        row![
            card(
                fl!("ui-tool-netmap-hosts-scanned"),
                self.summary.hosts_scanned,
                text::default
            ),
            card(
                fl!("ui-tool-netmap-hosts-up"),
                self.summary.hosts_up,
                text::success
            ),
            card(
                fl!("ui-tool-netmap-ports-open"),
                self.summary.open_ports_count,
                text::primary
            ),
        ]
        .spacing(spacing::SM)
        .into()
    }

    /// The hosts found, by address.
    fn nodes_card<'a>(
        &'a self,
        state: &ToolPane,
        send: impl Fn(NetworkCartographyMessage) -> Message,
    ) -> Element<'a, Message> {
        let export_csv_btn = super::export_button(
            fl!("ui-tool-netmap-export-csv"),
            state.copied(CopySlot::NetMapCsv),
            send(NetworkCartographyMessage::ExportCsv),
        );

        let header_row = row![
            text(fl!("ui-tool-netmap-name"))
                .size(font_size::SUBTITLE)
                .width(Length::Fill),
            export_csv_btn,
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let caption = |label: String, width: Length| {
            text(label)
                .size(font_size::CAPTION)
                .style(text::secondary)
                .width(width)
        };
        let table_headers = row![
            caption(fl!("ui-tool-netmap-col-ip"), Length::Fixed(IP_WIDTH)),
            caption(fl!("ui-tool-netmap-col-hostname"), Length::FillPortion(2)),
            caption(fl!("ui-tool-netmap-col-mac"), Length::Fixed(MAC_WIDTH)),
            caption(fl!("ui-tool-netmap-col-vendor"), Length::FillPortion(2)),
            caption(fl!("ui-tool-netmap-col-role"), Length::FillPortion(2)),
            caption(fl!("ui-tool-netmap-col-ports"), Length::FillPortion(2)),
            caption(
                fl!("ui-tool-netmap-col-latency"),
                Length::Fixed(LATENCY_WIDTH)
            ),
        ]
        .spacing(spacing::SM);

        let mut node_rows = column![table_headers].spacing(spacing::XS);

        for n in &self.nodes {
            let ports_str = n
                .open_ports
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            // A name and a MAC address come from the network or from a file: shown as text.
            let shown = |value: Option<&String>| {
                value
                    .map(|value| heimdall_app::server_text(value))
                    .unwrap_or_default()
            };

            let r = row![
                text(&n.ip)
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .width(Length::Fixed(IP_WIDTH)),
                text(shown(n.hostname.as_ref()))
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .width(Length::FillPortion(2)),
                text(shown(n.mac.as_ref()))
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .width(Length::Fixed(MAC_WIDTH)),
                text(super::vendor_label(n.vendor))
                    .size(font_size::BODY)
                    .width(Length::FillPortion(2)),
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
                    .width(Length::Fixed(LATENCY_WIDTH)),
            ]
            .spacing(spacing::SM);

            let entry: Element<'a, Message> = match fingerprint_line(&n.fingerprint) {
                Some(line) => column![
                    r,
                    text(line).size(font_size::CAPTION).style(text::secondary)
                ]
                .spacing(spacing::XS)
                .into(),
                None => r.into(),
            };
            node_rows = node_rows.push(entry);
        }

        container(column![header_row, node_rows].spacing(spacing::MD))
            .padding(spacing::MD)
            .style(styles::card)
            .into()
    }
}

/// A question and its buttons, in a card.
fn confirm_row<'a>(question: String, buttons: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut line = row![
        text(question)
            .size(font_size::BODY)
            .style(text::warning)
            .width(Length::Fill)
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);
    for button in buttons {
        line = line.push(button);
    }
    container(line)
        .padding(spacing::MD)
        .style(styles::card)
        .into()
}

/// What the knowledge base holds, as the C# `UpdateKbStats` says it at `now`.
fn knowledge_text(knowledge: KnowledgeStats, now: u64) -> String {
    if knowledge.hosts == 0 {
        return fl!("ui-tool-netmap-kb-empty");
    }
    let age = now.saturating_sub(knowledge.last_updated);
    let (minutes, hours, days): (u64, u64, u64) = (
        age / SECS_PER_MINUTE,
        age / SECS_PER_HOUR,
        age / SECS_PER_DAY,
    );
    let ago = if age < JUST_NOW_SECS {
        fl!("ui-tool-netmap-kb-just-now")
    } else if age < SECS_PER_HOUR {
        fl!("ui-tool-netmap-kb-minutes-ago", count = minutes)
    } else if age < SECS_PER_DAY {
        fl!("ui-tool-netmap-kb-hours-ago", count = hours)
    } else {
        fl!("ui-tool-netmap-kb-days-ago", count = days)
    };
    fl!(
        "ui-tool-netmap-kb-stats",
        count = knowledge.hosts,
        updated = ago
    )
}

/// What changed since a snapshot, as the C# `DiffText`: its parts joined, or nothing.
fn diff_text(diff: ScanDiff) -> String {
    if diff.is_empty() {
        return fl!("ui-tool-netmap-diff-none");
    }
    let mut parts = Vec::new();
    if diff.new_hosts > 0 {
        parts.push(fl!("ui-tool-netmap-diff-new", count = diff.new_hosts));
    }
    if diff.removed_hosts > 0 {
        parts.push(fl!(
            "ui-tool-netmap-diff-removed",
            count = diff.removed_hosts
        ));
    }
    if diff.changed_hosts > 0 {
        parts.push(fl!(
            "ui-tool-netmap-diff-changed",
            count = diff.changed_hosts
        ));
    }
    parts.join(" | ")
}

/// A node's address, by which nodes are shown in order.
fn address(node: &DiscoveredNode) -> Option<Ipv4Addr> {
    node.ip.parse().ok()
}

/// What the probes learned of a host, said in one line under it; `None` when nothing was
/// learned. Every text comes from the network: shown as text, never as the interface's own
/// words.
fn fingerprint_line(fingerprint: &Fingerprint) -> Option<String> {
    let shown = |value: &str| heimdall_app::server_text(value);
    let mut parts: Vec<String> = Vec::new();
    if let Some(os) = &fingerprint.os {
        parts.push(fl!(
            "ui-tool-netmap-fp-os",
            os = shown(&os.os),
            confidence = u64::from(os.confidence)
        ));
    }
    match (&fingerprint.netbios_name, &fingerprint.netbios_domain) {
        (Some(name), Some(domain)) => parts.push(fl!(
            "ui-tool-netmap-fp-netbios-domain",
            domain = shown(domain),
            name = shown(name)
        )),
        (Some(name), None) => parts.push(fl!("ui-tool-netmap-fp-netbios", name = shown(name))),
        (None, Some(domain)) => {
            parts.push(fl!("ui-tool-netmap-fp-workgroup", domain = shown(domain)))
        }
        (None, None) => {}
    }
    if let Some(snmp) = &fingerprint.snmp {
        if let Some(descr) = &snmp.descr {
            parts.push(fl!("ui-tool-netmap-fp-snmp", descr = shown(descr)));
        }
        if let Some(name) = &snmp.name {
            parts.push(fl!("ui-tool-netmap-fp-snmp-name", name = shown(name)));
        }
    }
    if let Some(ntlm) = &fingerprint.ntlm {
        if let Some(name) = &ntlm.dns_computer {
            parts.push(fl!("ui-tool-netmap-fp-windows", name = shown(name)));
        }
        if let Some(build) = &ntlm.os_build {
            parts.push(fl!("ui-tool-netmap-fp-build", build = shown(build)));
        }
    }
    if let Some(smb) = &fingerprint.smb {
        parts.push(fl!(
            "ui-tool-netmap-fp-smb",
            dialect = heimdall_core::tools::smb::format_dialect(smb.dialect_revision),
            required = if smb.signing_required { "yes" } else { "no" }
        ));
    }
    if let Some(banner) = &fingerprint.ssh_banner {
        parts.push(fl!("ui-tool-netmap-fp-ssh", banner = shown(banner)));
    }
    if let Some(hash) = &fingerprint.ssh_hassh {
        parts.push(fl!("ui-tool-netmap-fp-hassh", hash = shown(hash)));
    }
    if let Some(subject) = &fingerprint.tls_subject {
        parts.push(fl!("ui-tool-netmap-fp-tls", subject = shown(subject)));
    }
    (!parts.is_empty()).then(|| parts.join(FINGERPRINT_SEPARATOR))
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

    use heimdall_core::tools::mac_vendor::MacVendor;

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
            fingerprint: Box::default(),
        }
    }

    fn started(pane: &mut NetworkCartographyPane, cidr: &str) -> Discovery {
        pane.apply(NetworkCartographyMessage::CidrChanged(cidr.to_owned()));
        match pane.apply(NetworkCartographyMessage::Start) {
            Step::Discover(discovery) => discovery,
            other => panic!("no scan: {other:?}"),
        }
    }

    fn host(run: u64, node: Option<DiscoveredNode>) -> NetworkCartographyMessage {
        NetworkCartographyMessage::Event(run, DiscoveryEvent::Host(node))
    }

    #[test]
    fn what_the_probes_learned_is_said_in_one_line_and_exported() {
        assert_eq!(fingerprint_line(&Fingerprint::default()), None);
        let fingerprint = Fingerprint {
            netbios_name: Some("SRV01".to_owned()),
            netbios_domain: Some("LAB".to_owned()),
            ssh_banner: Some("SSH-2.0-Open\u{202E}SSH".to_owned()),
            os: Some(heimdall_core::tools::network_fingerprint::OsGuess {
                os: "Linux".to_owned(),
                source: "Ports".to_owned(),
                confidence: 40,
            }),
            ..Fingerprint::default()
        };
        let line = fingerprint_line(&fingerprint).expect("a line");
        assert!(line.contains("Linux") && line.contains("SRV01") && line.contains("LAB"));
        assert!(
            line.contains("SSH-2.0-OpenSSH"),
            "bidi controls dropped: {line}"
        );
        assert!(line.contains(FINGERPRINT_SEPARATOR));

        let mut pane = NetworkCartographyPane::new(None);
        let discovery = started(&mut pane, "10.0.0.0/28");
        let mut found = node("10.0.0.5", &[22]);
        found.fingerprint = Box::new(fingerprint);
        pane.apply(host(discovery.run, Some(found)));
        let Step::Save(export) = pane.apply(NetworkCartographyMessage::ExportCsv) else {
            panic!("the CSV is saved");
        };
        let csv = export.content();
        let header = csv.lines().next().expect("header");
        assert_eq!(header.split("\",\"").count(), 21, "{header}");
        assert!(csv.contains("\"SRV01\",\"LAB\""), "{csv}");
    }

    #[test]
    fn the_scope_is_bounded_and_a_large_subnet_asked_about_first() {
        let mut pane = NetworkCartographyPane::new(None);
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
        let Step::Discover(discovery) = pane.apply(NetworkCartographyMessage::ConfirmLarge) else {
            panic!("scanned once confirmed");
        };
        assert_eq!(discovery.ips.len(), 65_534);
        assert_eq!(discovery.subnet, "10.0.0.0/16");
        pane.apply(NetworkCartographyMessage::Stop);
        let discovery = started(&mut pane, "192.168.7.9");
        assert_eq!(
            discovery.ips.len(),
            254,
            "a /24 when no prefix is typed, as the C#"
        );
        assert_eq!(pane.cidr, "192.168.7.9/24");
        assert_eq!(discovery.subnet, "192.168.7.0/24");
        assert!(discovery.options.reverse_dns, "on by default, as the C#");
        assert!(!discovery.use_knowledge, "off by default, as the C#");
    }

    #[test]
    fn hosts_stream_in_sorted_are_detailed_counted_and_copied() {
        let mut pane = NetworkCartographyPane::new(None);
        let discovery = started(&mut pane, "10.0.0.0/28");
        let run = discovery.run;
        pane.apply(host(run, Some(node("10.0.0.10", &[80, 443]))));
        pane.apply(host(run, None));
        pane.apply(host(run, Some(node("10.0.0.9", &[22]))));
        let shown: Vec<&str> = pane.nodes.iter().map(|n| n.ip.as_str()).collect();
        assert_eq!(shown, ["10.0.0.9", "10.0.0.10"], "by address, not by text");
        assert_eq!(pane.summary.hosts_scanned, 3);
        assert_eq!(pane.summary.hosts_up, 2);
        assert_eq!(pane.summary.open_ports_count, 3);
        pane.apply(NetworkCartographyMessage::Event(
            run,
            DiscoveryEvent::Naming(2),
        ));
        assert_eq!(pane.naming, Some(2));
        pane.apply(NetworkCartographyMessage::Event(
            run,
            DiscoveryEvent::Details {
                ip: "10.0.0.10".to_owned(),
                hostname: Some("web.lab".to_owned()),
                mac: Some("00-50-56-00-00-10".to_owned()),
                vendor: Some(MacVendor::Named("VMware")),
            },
        ));
        assert_eq!(pane.nodes[1].hostname.as_deref(), Some("web.lab"));
        assert_eq!(pane.nodes[1].vendor, Some(MacVendor::Named("VMware")));
        let Step::Save(export) = pane.apply(NetworkCartographyMessage::ExportCsv) else {
            panic!("the CSV is saved");
        };
        assert_eq!(export.slot(), CopySlot::NetMapCsv);
        assert!(export.suggested_name().starts_with("cartography_"));
        let csv = export.content();
        assert_eq!(csv.lines().count(), 3);
        assert!(
            csv.contains("\"web.lab\",\"00-50-56-00-00-10\",\"VMware\""),
            "{csv}"
        );
        pane.apply(NetworkCartographyMessage::Event(
            run,
            DiscoveryEvent::Fingerprint {
                ip: "10.0.0.10".to_owned(),
                fingerprint: Box::new(Fingerprint {
                    ssh_banner: Some("SSH-2.0-OpenSSH_9.6".to_owned()),
                    ..Fingerprint::default()
                }),
            },
        ));
        assert_eq!(
            pane.nodes[1].fingerprint.ssh_banner.as_deref(),
            Some("SSH-2.0-OpenSSH_9.6")
        );
        assert!(pane.nodes[0].fingerprint.is_empty());
        pane.apply(NetworkCartographyMessage::Stop);
        assert!(discovery.stop.load(Ordering::Relaxed));
        assert_eq!(pane.naming, None);
        pane.apply(host(run, Some(node("10.0.0.11", &[80]))));
        assert_eq!(pane.nodes.len(), 2, "nothing after the stop");
    }

    fn snapshot(hosts: &[DiscoveredNode]) -> ScanSnapshot {
        ScanSnapshot::new("10.0.0.0/28", network_history::now_secs(), hosts)
    }

    #[test]
    fn a_completed_scan_is_saved_learned_and_compared_with_the_history() {
        let folder = tempfile::tempdir().expect("folder");
        let store = ScanStore::new(folder.path().to_owned());
        let mut older = snapshot(&[node("10.0.0.1", &[22]), node("10.0.0.2", &[80])]);
        older.timestamp -= 3600;
        let older_name = store.save_snapshot(&older).expect("saved");

        let mut pane = NetworkCartographyPane::new(Some(folder.path().to_owned()));
        pane.stored(StoredView::read(&store));
        assert_eq!(pane.history.len(), 1);
        assert_eq!(pane.knowledge.map(|k| k.hosts), Some(0));

        let discovery = started(&mut pane, "10.0.0.0/28");
        assert!(discovery.store.is_some());
        let newer = snapshot(&[node("10.0.0.1", &[22, 443]), node("10.0.0.3", &[80])]);
        let saved = store.save_snapshot(&newer).expect("saved");
        let mut knowledge = store.load_knowledge();
        knowledge.merge(&newer);
        store.save_knowledge(&knowledge).expect("saved");
        pane.apply(NetworkCartographyMessage::Ended(
            discovery.run,
            Some(ScanEnd::Completed {
                snapshot: newer,
                saved: Some(saved.clone()),
                stored: Some(StoredView::read(&store)),
            }),
        ));
        assert!(pane.notice.is_some(), "its save is said");
        assert_eq!(pane.knowledge.map(|k| k.hosts), Some(2));
        let choices: Vec<&str> = pane
            .history
            .iter()
            .map(|c| c.0.file_name.as_str())
            .collect();
        assert_eq!(
            choices,
            [older_name.as_str()],
            "its own snapshot is left out"
        );

        assert!(matches!(
            pane.apply(NetworkCartographyMessage::Compare),
            Step::None
        ));
        pane.apply(NetworkCartographyMessage::CompareSelected(
            pane.history[0].clone(),
        ));
        let Step::Load(_, name) = pane.apply(NetworkCartographyMessage::Compare) else {
            panic!("the snapshot is read");
        };
        pane.apply(NetworkCartographyMessage::Compared(
            name,
            store.load_snapshot(&older_name),
        ));
        assert_eq!(
            pane.diff,
            Some(ScanDiff {
                new_hosts: 1,
                removed_hosts: 1,
                changed_hosts: 1
            })
        );
        pane.apply(NetworkCartographyMessage::Compared(
            older_name.clone(),
            None,
        ));
        assert!(
            pane.error.is_some(),
            "a snapshot that cannot be read is said"
        );
    }

    #[test]
    fn the_knowledge_base_is_cleared_once_confirmed_and_never_during_a_scan() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = NetworkCartographyPane::new(Some(folder.path().to_owned()));
        pane.apply(NetworkCartographyMessage::ClearKnowledge);
        assert!(pane.confirm_clear);
        assert!(matches!(
            pane.apply(NetworkCartographyMessage::ConfirmClear(false)),
            Step::None
        ));
        assert!(!pane.confirm_clear);
        pane.apply(NetworkCartographyMessage::ClearKnowledge);
        assert!(matches!(
            pane.apply(NetworkCartographyMessage::ConfirmClear(true)),
            Step::Clear(_)
        ));
        pane.apply(NetworkCartographyMessage::Cleared(Ok(StoredView {
            history: Vec::new(),
            knowledge: KnowledgeStats {
                hosts: 0,
                last_updated: 0,
            },
        })));
        assert!(pane.notice.is_some());
        started(&mut pane, "10.0.0.0/30");
        pane.apply(NetworkCartographyMessage::ClearKnowledge);
        assert!(!pane.confirm_clear, "not while a scan runs");
        pane.apply(NetworkCartographyMessage::Cleared(Err("denied".to_owned())));
        assert!(pane.error.is_some());
        let mut alone = NetworkCartographyPane::new(None);
        alone.apply(NetworkCartographyMessage::ClearKnowledge);
        assert!(!alone.confirm_clear, "no store, nothing to clear");
    }

    #[test]
    fn the_knowledge_base_and_a_diff_are_said_in_words() {
        let stats = |hosts, last_updated| KnowledgeStats {
            hosts,
            last_updated,
        };
        let now = 1_000_000;
        assert_eq!(
            knowledge_text(stats(0, 0), now),
            fl!("ui-tool-netmap-kb-empty")
        );
        let just_now = knowledge_text(stats(3, now - 10), now);
        assert!(
            just_now.contains(&fl!("ui-tool-netmap-kb-just-now")),
            "{just_now}"
        );
        let hours = knowledge_text(stats(3, now - 3 * SECS_PER_HOUR), now);
        assert!(
            hours.contains(&fl!("ui-tool-netmap-kb-hours-ago", count = 3)),
            "{hours}"
        );
        assert_eq!(
            diff_text(ScanDiff::default()),
            fl!("ui-tool-netmap-diff-none")
        );
        let both = diff_text(ScanDiff {
            new_hosts: 2,
            removed_hosts: 0,
            changed_hosts: 1,
        });
        assert_eq!(both.matches(" | ").count(), 1, "{both}");
    }
}
