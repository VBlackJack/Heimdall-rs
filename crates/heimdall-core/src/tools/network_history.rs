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

//! The scan history and the knowledge base of the Network Cartography tool, as the C#
//! `Heimdall.Core.Discovery.ScanHistoryManager` and `KnowledgeBaseManager`: local files in
//! Heimdall's data folder, nothing sent anywhere. Every completed scan leaves a snapshot,
//! the newest [`MAX_RETAINED_SNAPSHOTS`] kept, which a later scan is compared with; and what
//! is known of each host across scans, which can spare a scan the probes whose answers are
//! still fresh and fill in a name or a MAC address the scan did not find.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::date_time::{civil_from_days, days_from_civil};
use super::mac_vendor;
use super::network_cartography::{DiscoveredNode, Prior, classify_role};
use super::private_file::write_private;

/// The folder of the snapshots, in the data folder, as the C# `GetNetworkScansDirectory`.
pub const HISTORY_DIR_NAME: &str = "network-scans";
/// The knowledge base's file, in the data folder, as the C# `network-kb.json`.
pub const KNOWLEDGE_FILE_NAME: &str = "network-kb.json";
/// Snapshots kept, the oldest removed past them, as the C# `MaxRetainedSnapshots`.
pub const MAX_RETAINED_SNAPSHOTS: usize = 20;
/// How long a host known up is taken for up without an echo request, as the C#
/// `HostAliveHours` (4).
pub const HOST_ALIVE_TTL_SECS: u64 = 4 * SECONDS_PER_HOUR;
/// How long the open ports known of a host are taken as they are, as the C#
/// `PortScanHours` (24).
pub const PORT_SCAN_TTL_SECS: u64 = 24 * SECONDS_PER_HOUR;
/// The version of the files written, read back only by its own.
const FORMAT_VERSION: u32 = 1;
/// The largest file read back: a `/16` fully up is far below.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// What starts and ends a snapshot's file name, as the C# `scan_*.json`.
const SNAPSHOT_PREFIX: &str = "scan_";
const SNAPSHOT_EXTENSION: &str = ".json";
/// Characters of the moment in a snapshot's name, `yyyyMMdd_HHmmss` as the C#.
const STAMP_LEN: usize = 15;
/// The character standing for the `/` of the subnet in a snapshot's name, as the C#.
const PREFIX_MARK: char = '-';
const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR;

/// The seconds since 1970 now.
#[must_use]
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// A host as a snapshot keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotHost {
    pub ip: String,
    pub hostname: Option<String>,
    pub mac: Option<String>,
    pub open_ports: Vec<u16>,
    pub response_time_ms: u64,
}

/// What one completed scan found, as the C# `NetworkScanSnapshot`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanSnapshot {
    pub version: u32,
    /// When the scan ended, in seconds since 1970.
    pub timestamp: u64,
    /// The subnet scanned, as [`super::network_cartography::canonical_subnet`] writes it.
    pub subnet: String,
    pub hosts: Vec<SnapshotHost>,
}

impl ScanSnapshot {
    /// The snapshot of the scan of `subnet` that found `nodes`, ended at `timestamp`.
    #[must_use]
    pub fn new(subnet: &str, timestamp: u64, nodes: &[DiscoveredNode]) -> Self {
        Self {
            version: FORMAT_VERSION,
            timestamp,
            subnet: subnet.to_owned(),
            hosts: nodes
                .iter()
                .map(|node| SnapshotHost {
                    ip: node.ip.clone(),
                    hostname: node.hostname.clone(),
                    mac: node.mac.clone(),
                    open_ports: node.open_ports.clone(),
                    response_time_ms: node.response_time_ms,
                })
                .collect(),
        }
    }
}

/// A snapshot in the history, as its file name says it, as the C# `ListSnapshots`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotInfo {
    pub file_name: String,
    /// When its scan ended, in seconds since 1970.
    pub timestamp: u64,
    pub subnet: String,
}

/// What changed from one scan to a later one, as the C# `ScanDiff`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanDiff {
    /// Hosts up now that were not then.
    pub new_hosts: usize,
    /// Hosts up then that are not now.
    pub removed_hosts: usize,
    /// Hosts up both times whose ports, name or hardware vendor changed.
    pub changed_hosts: usize,
}

impl ScanDiff {
    /// Whether nothing changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.new_hosts == 0 && self.removed_hosts == 0 && self.changed_hosts == 0
    }
}

/// What changed from `older` to `newer`, as the C# `ComputeDiff`: hosts new and removed, and
/// those whose open ports, name or hardware vendor changed (the role follows the ports).
#[must_use]
pub fn compare(older: &ScanSnapshot, newer: &ScanSnapshot) -> ScanDiff {
    let then: HashMap<&str, &SnapshotHost> = older
        .hosts
        .iter()
        .map(|host| (host.ip.as_str(), host))
        .collect();
    let now: HashSet<&str> = newer.hosts.iter().map(|host| host.ip.as_str()).collect();
    let vendor = |host: &SnapshotHost| host.mac.as_deref().and_then(mac_vendor::lookup);
    let mut diff = ScanDiff {
        removed_hosts: older
            .hosts
            .iter()
            .filter(|host| !now.contains(host.ip.as_str()))
            .count(),
        ..ScanDiff::default()
    };
    for host in &newer.hosts {
        match then.get(host.ip.as_str()) {
            None => diff.new_hosts += 1,
            Some(old) => {
                let ports =
                    |host: &SnapshotHost| host.open_ports.iter().copied().collect::<HashSet<u16>>();
                if ports(old) != ports(host)
                    || old.hostname != host.hostname
                    || vendor(old) != vendor(host)
                {
                    diff.changed_hosts += 1;
                }
            }
        }
    }
    diff
}

/// One thing known of a host, and when it was seen, as the C# `Observation`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observed<T> {
    pub value: T,
    /// When it was seen, in seconds since 1970.
    pub at: u64,
}

/// What is known of a host across scans, as the C# `KnownHost`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownHost {
    pub first_seen: u64,
    pub last_seen: u64,
    pub scan_count: u32,
    pub hostname: Option<Observed<String>>,
    pub mac: Option<Observed<String>>,
    /// Seen up, with this response time.
    pub alive_ms: Option<Observed<u64>>,
    pub open_ports: Option<Observed<Vec<u16>>>,
}

/// What is known of the hosts of every scan, as the C# `NetworkKnowledgeBase`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeBase {
    pub version: u32,
    /// When it last learned something, in seconds since 1970.
    pub last_updated: u64,
    /// By address.
    pub hosts: BTreeMap<String, KnownHost>,
}

impl Default for KnowledgeBase {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            last_updated: 0,
            hosts: BTreeMap::new(),
        }
    }
}

/// `seen`, newer, when there is one; else what was known, as the C# `MergeObservation`.
fn newest<T>(known: Option<Observed<T>>, seen: Option<T>, at: u64) -> Option<Observed<T>> {
    match seen {
        Some(value) => Some(Observed { value, at }),
        None => known,
    }
}

/// Whether `observed` was seen within `ttl_secs` of `now`.
fn fresh<T>(observed: Option<&Observed<T>>, now: u64, ttl_secs: u64) -> bool {
    observed.is_some_and(|observed| now.saturating_sub(observed.at) < ttl_secs)
}

impl KnowledgeBase {
    /// Hosts known.
    #[must_use]
    pub fn host_count(&self) -> usize {
        self.hosts.len()
    }

    /// What `snapshot` found, learned host by host, the newest wins, as the C#
    /// `MergeSnapshot`: a host seen is up, its open ports those found; a name or a MAC
    /// address it did not find is kept from before.
    pub fn merge(&mut self, snapshot: &ScanSnapshot) {
        let at = snapshot.timestamp;
        for host in &snapshot.hosts {
            let known = self.hosts.remove(&host.ip);
            let merged = match known {
                None => KnownHost {
                    first_seen: at,
                    last_seen: at,
                    scan_count: 1,
                    hostname: newest(None, host.hostname.clone(), at),
                    mac: newest(None, host.mac.clone(), at),
                    alive_ms: Some(Observed {
                        value: host.response_time_ms,
                        at,
                    }),
                    open_ports: Some(Observed {
                        value: host.open_ports.clone(),
                        at,
                    }),
                },
                Some(known) => KnownHost {
                    first_seen: known.first_seen.min(at),
                    last_seen: known.last_seen.max(at),
                    scan_count: known.scan_count.saturating_add(1),
                    hostname: newest(known.hostname, host.hostname.clone(), at),
                    mac: newest(known.mac, host.mac.clone(), at),
                    alive_ms: Some(Observed {
                        value: host.response_time_ms,
                        at,
                    }),
                    open_ports: Some(Observed {
                        value: host.open_ports.clone(),
                        at,
                    }),
                },
            };
            self.hosts.insert(host.ip.clone(), merged);
        }
        self.last_updated = self.last_updated.max(at);
    }

    /// What a scan of `ips` at `now` is told before it starts, as the C# `ScanAsync` reads
    /// the knowledge base: a host seen up within [`HOST_ALIVE_TTL_SECS`] is not pinged;
    /// one whose open ports were also seen within [`PORT_SCAN_TTL_SECS`], some of them open,
    /// is shown as known, never probed; and every host known is given its name and its MAC
    /// address, should the scan find none.
    #[must_use]
    pub fn priors(&self, ips: &[Ipv4Addr], now: u64) -> HashMap<Ipv4Addr, Prior> {
        let mut priors = HashMap::new();
        for ip in ips {
            let Some(host) = self.hosts.get(&ip.to_string()) else {
                continue;
            };
            let alive = fresh(host.alive_ms.as_ref(), now, HOST_ALIVE_TTL_SECS);
            let alive_ms = alive
                .then(|| host.alive_ms.as_ref().map(|seen| seen.value))
                .flatten();
            let ports = host
                .open_ports
                .as_ref()
                .filter(|ports| !ports.value.is_empty())
                .filter(|_| fresh(host.open_ports.as_ref(), now, PORT_SCAN_TTL_SECS));
            let hostname = host.hostname.as_ref().map(|seen| seen.value.clone());
            let mac = host.mac.as_ref().map(|seen| seen.value.clone());
            let reuse = match (alive_ms, ports) {
                (Some(ms), Some(ports)) => Some(DiscoveredNode {
                    ip: ip.to_string(),
                    hostname: hostname.clone(),
                    vendor: mac.as_deref().and_then(mac_vendor::lookup),
                    mac: mac.clone(),
                    role: classify_role(&ports.value),
                    open_ports: ports.value.clone(),
                    response_time_ms: ms,
                    fingerprint: Box::default(),
                }),
                _ => None,
            };
            priors.insert(
                *ip,
                Prior {
                    reuse,
                    alive_ms,
                    hostname,
                    mac,
                },
            );
        }
        priors
    }
}

/// The UTC moment `secs` as a snapshot's name writes it, `yyyyMMdd_HHmmss`.
fn stamp(secs: u64) -> String {
    let days = i64::try_from(secs / SECONDS_PER_DAY).unwrap_or(i64::MAX);
    let (year, month, day) = civil_from_days(days);
    let in_day = secs % SECONDS_PER_DAY;
    format!(
        "{year:04}{month:02}{day:02}_{:02}{:02}{:02}",
        in_day / SECONDS_PER_HOUR,
        in_day % SECONDS_PER_HOUR / SECONDS_PER_MINUTE,
        in_day % SECONDS_PER_MINUTE
    )
}

/// The moment a snapshot's name writes, back in seconds since 1970.
fn unstamp(text: &str) -> Option<u64> {
    let (date, time) = text.split_once('_')?;
    if date.len() != 8 || time.len() != 6 || !text.chars().all(|c| c.is_ascii_digit() || c == '_') {
        return None;
    }
    let number = |part: &str| part.parse::<u32>().ok();
    let year = i64::from(number(&date[..4])?);
    let (month, day) = (number(&date[4..6])?, number(&date[6..])?);
    let (hour, minute, second) = (
        number(&time[..2])?,
        number(&time[2..4])?,
        number(&time[4..])?,
    );
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let days = u64::try_from(days_from_civil(year, month, day)).ok()?;
    Some(
        days * SECONDS_PER_DAY
            + u64::from(hour) * SECONDS_PER_HOUR
            + u64::from(minute) * SECONDS_PER_MINUTE
            + u64::from(second),
    )
}

/// Whether `subnet` is one a snapshot's name can hold: an address and a prefix, nothing
/// that could step out of the history folder.
fn plain_subnet(subnet: &str) -> bool {
    !subnet.is_empty()
        && subnet
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '/')
}

/// The name of the snapshot of `snapshot`, as the C# `scan_{yyyyMMdd_HHmmss}_{subnet}.json`.
fn snapshot_file_name(snapshot: &ScanSnapshot) -> Option<String> {
    plain_subnet(&snapshot.subnet).then(|| {
        format!(
            "{SNAPSHOT_PREFIX}{}_{}{SNAPSHOT_EXTENSION}",
            stamp(snapshot.timestamp),
            snapshot.subnet.replace('/', &PREFIX_MARK.to_string())
        )
    })
}

/// What a snapshot's file name says; `None` for a file that is not one, as the C#
/// `ListSnapshots` and `LoadSnapshot` refuse it.
fn snapshot_info(file_name: &str) -> Option<SnapshotInfo> {
    let rest = file_name
        .strip_prefix(SNAPSHOT_PREFIX)?
        .strip_suffix(SNAPSHOT_EXTENSION)?;
    if rest.len() <= STAMP_LEN + 1 || !rest.is_char_boundary(STAMP_LEN) {
        return None;
    }
    let (moment, subnet) = rest.split_at(STAMP_LEN);
    let subnet = subnet.strip_prefix('_')?.replace(PREFIX_MARK, "/");
    if !plain_subnet(&subnet) {
        return None;
    }
    Some(SnapshotInfo {
        file_name: file_name.to_owned(),
        timestamp: unstamp(moment)?,
        subnet,
    })
}

/// `path` read as JSON into a `T`, when it is no larger than [`MAX_FILE_BYTES`].
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let size = fs::metadata(path).ok()?.len();
    if size > MAX_FILE_BYTES {
        return None;
    }
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// `value` written as JSON at `path`, readable by its owner alone, as the C# writes these
/// files through `SecureFileWriter` after a rename.
fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let json = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    write_private(path, &json)
}

/// Where the scan history and the knowledge base are kept: a data folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanStore {
    dir: PathBuf,
}

impl ScanStore {
    /// The store in `data_dir`, Heimdall's data folder, or a test's own.
    #[must_use]
    pub fn new(data_dir: PathBuf) -> Self {
        Self { dir: data_dir }
    }

    fn history_dir(&self) -> PathBuf {
        self.dir.join(HISTORY_DIR_NAME)
    }

    fn knowledge_file(&self) -> PathBuf {
        self.dir.join(KNOWLEDGE_FILE_NAME)
    }

    /// The knowledge base; an empty one when there is none yet or it cannot be read, as the
    /// C# falls back to `CreateEmpty`.
    #[must_use]
    pub fn load_knowledge(&self) -> KnowledgeBase {
        read_json::<KnowledgeBase>(&self.knowledge_file())
            .filter(|knowledge| knowledge.version == FORMAT_VERSION)
            .unwrap_or_default()
    }

    /// Writes `knowledge` down.
    ///
    /// # Errors
    ///
    /// The error of the write.
    pub fn save_knowledge(&self, knowledge: &KnowledgeBase) -> io::Result<()> {
        write_json(&self.knowledge_file(), knowledge)
    }

    /// Writes `snapshot` down in the history, then removes the oldest past
    /// [`MAX_RETAINED_SNAPSHOTS`], as the C# `SaveSnapshotAsync`; its file name.
    ///
    /// # Errors
    ///
    /// The error of the write, or `InvalidInput` for a subnet no file name can hold.
    pub fn save_snapshot(&self, snapshot: &ScanSnapshot) -> io::Result<String> {
        let name = snapshot_file_name(snapshot).ok_or(io::ErrorKind::InvalidInput)?;
        write_json(&self.history_dir().join(&name), snapshot)?;
        for old in self
            .list_snapshots()
            .into_iter()
            .skip(MAX_RETAINED_SNAPSHOTS)
        {
            let _ = fs::remove_file(self.history_dir().join(old.file_name));
        }
        Ok(name)
    }

    /// The snapshots of the history, the most recent first, as the C# `ListSnapshots`.
    #[must_use]
    pub fn list_snapshots(&self) -> Vec<SnapshotInfo> {
        let Ok(read) = fs::read_dir(self.history_dir()) else {
            return Vec::new();
        };
        let mut found: Vec<SnapshotInfo> = read
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .filter_map(|entry| snapshot_info(entry.file_name().to_str()?))
            .collect();
        found.sort_by(|a, b| {
            b.timestamp
                .cmp(&a.timestamp)
                .then_with(|| b.file_name.cmp(&a.file_name))
        });
        found
    }

    /// The snapshot named `file_name`; `None` for a name that is not a snapshot's, as the
    /// C# `LoadSnapshot` refuses one that could leave the history folder.
    #[must_use]
    pub fn load_snapshot(&self, file_name: &str) -> Option<ScanSnapshot> {
        snapshot_info(file_name)?;
        read_json::<ScanSnapshot>(&self.history_dir().join(file_name))
            .filter(|snapshot| snapshot.version == FORMAT_VERSION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(ip: &str, ports: &[u16], hostname: Option<&str>, mac: Option<&str>) -> SnapshotHost {
        SnapshotHost {
            ip: ip.to_owned(),
            hostname: hostname.map(str::to_owned),
            mac: mac.map(str::to_owned),
            open_ports: ports.to_vec(),
            response_time_ms: 3,
        }
    }

    fn snapshot(timestamp: u64, hosts: Vec<SnapshotHost>) -> ScanSnapshot {
        ScanSnapshot {
            version: FORMAT_VERSION,
            timestamp,
            subnet: "10.0.0.0/24".to_owned(),
            hosts,
        }
    }

    /// 2026-10-16 12:34:56 UTC.
    const MOMENT: u64 = 1_792_154_096;

    #[test]
    fn a_snapshot_is_named_as_the_csharp_names_it_and_read_back() {
        let named = snapshot_file_name(&snapshot(MOMENT, Vec::new())).expect("named");
        assert_eq!(named, "scan_20261016_123456_10.0.0.0-24.json");
        let info = snapshot_info(&named).expect("read back");
        assert_eq!(info.timestamp, MOMENT);
        assert_eq!(info.subnet, "10.0.0.0/24");
        for refused in [
            "scan_20261016_123456_..-..-x.json",
            "scan_20261016_123456_.json",
            "scan_20261316_123456_10.0.0.0-24.json",
            "scan_2026101_6123456_10.0.0.0-24.json",
            "other_20261016_123456_10.0.0.0-24.json",
            "scan_20261016_123456_10.0.0.0-24.txt",
            "scan_\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}_10.0.0.0-24.json",
        ] {
            assert_eq!(snapshot_info(refused), None, "{refused}");
        }
        let mut odd = snapshot(MOMENT, Vec::new());
        odd.subnet = "../x".to_owned();
        assert_eq!(snapshot_file_name(&odd), None);
    }

    #[test]
    fn the_history_keeps_the_newest_and_lists_them_first() {
        let folder = tempfile::tempdir().expect("folder");
        let store = ScanStore::new(folder.path().to_owned());
        assert!(store.list_snapshots().is_empty());
        for n in 0..(MAX_RETAINED_SNAPSHOTS as u64 + 3) {
            store
                .save_snapshot(&snapshot(
                    MOMENT + n,
                    vec![host("10.0.0.1", &[22], None, None)],
                ))
                .expect("saved");
        }
        fs::write(store.history_dir().join("notes.txt"), "x").expect("other file");
        let listed = store.list_snapshots();
        assert_eq!(listed.len(), MAX_RETAINED_SNAPSHOTS);
        assert_eq!(
            listed[0].timestamp,
            MOMENT + MAX_RETAINED_SNAPSHOTS as u64 + 2
        );
        assert_eq!(listed.last().expect("oldest").timestamp, MOMENT + 3);
        let loaded = store.load_snapshot(&listed[0].file_name).expect("loaded");
        assert_eq!(loaded.hosts.len(), 1);
        assert_eq!(store.load_snapshot("../network-kb.json"), None);
        assert_eq!(store.load_snapshot("notes.txt"), None);
    }

    #[test]
    fn a_scan_is_compared_by_hosts_ports_names_and_vendors() {
        let older = snapshot(
            MOMENT,
            vec![
                host(
                    "10.0.0.1",
                    &[22, 80],
                    Some("a.lab"),
                    Some("00-50-56-00-00-01"),
                ),
                host("10.0.0.2", &[22], None, None),
                host("10.0.0.3", &[443], None, Some("00-50-56-00-00-03")),
                host("10.0.0.4", &[80], None, None),
            ],
        );
        let newer = snapshot(
            MOMENT + 60,
            vec![
                // Same ports in another order, same vendor by another MAC address.
                host(
                    "10.0.0.1",
                    &[80, 22],
                    Some("a.lab"),
                    Some("00-0C-29-00-00-01"),
                ),
                host("10.0.0.2", &[22, 3389], None, None),
                host("10.0.0.3", &[443], None, Some("B8-27-EB-00-00-03")),
                host("10.0.0.5", &[], None, None),
            ],
        );
        let diff = compare(&older, &newer);
        assert_eq!(
            diff,
            ScanDiff {
                new_hosts: 1,
                removed_hosts: 1,
                changed_hosts: 2
            }
        );
        assert!(compare(&newer, &newer).is_empty());
    }

    #[test]
    fn the_knowledge_base_learns_newest_first_and_says_what_is_fresh() {
        let folder = tempfile::tempdir().expect("folder");
        let store = ScanStore::new(folder.path().to_owned());
        assert_eq!(store.load_knowledge(), KnowledgeBase::default(), "none yet");
        let mut knowledge = store.load_knowledge();
        knowledge.merge(&snapshot(
            MOMENT,
            vec![
                host("10.0.0.1", &[22], Some("a.lab"), Some("00-50-56-00-00-01")),
                host("10.0.0.2", &[], None, None),
            ],
        ));
        // A later scan without the name keeps the one known.
        knowledge.merge(&snapshot(
            MOMENT + 10,
            vec![host("10.0.0.1", &[22, 80], None, None)],
        ));
        assert_eq!(knowledge.host_count(), 2);
        let one = &knowledge.hosts["10.0.0.1"];
        assert_eq!(one.scan_count, 2);
        assert_eq!(one.first_seen, MOMENT);
        assert_eq!(
            one.hostname.as_ref().map(|seen| seen.value.as_str()),
            Some("a.lab")
        );
        assert_eq!(
            one.open_ports.as_ref().map(|seen| seen.value.clone()),
            Some(vec![22, 80])
        );
        store.save_knowledge(&knowledge).expect("saved");
        assert_eq!(store.load_knowledge(), knowledge, "read back");

        let ips = [
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(10, 0, 0, 2),
            Ipv4Addr::new(10, 0, 0, 9),
        ];
        let soon = knowledge.priors(&ips, MOMENT + 60);
        let one = &soon[&ips[0]];
        let reused = one.reuse.as_ref().expect("all fresh: reused");
        assert_eq!(reused.open_ports, [22, 80]);
        assert_eq!(reused.hostname.as_deref(), Some("a.lab"));
        assert_eq!(reused.vendor, Some(mac_vendor::MacVendor::Named("VMware")));
        let two = &soon[&ips[1]];
        assert_eq!(two.reuse, None, "no open port known: probed");
        assert_eq!(two.alive_ms, Some(3), "but not pinged");
        assert!(!soon.contains_key(&ips[2]));

        let later = knowledge.priors(&ips, MOMENT + HOST_ALIVE_TTL_SECS + 60);
        let one = &later[&ips[0]];
        assert_eq!(one.reuse, None);
        assert_eq!(one.alive_ms, None, "up too long ago: pinged");
        assert_eq!(
            one.hostname.as_deref(),
            Some("a.lab"),
            "its name still given"
        );
        assert_eq!(one.mac.as_deref(), Some("00-50-56-00-00-01"));
    }

    #[test]
    fn an_unreadable_knowledge_base_is_an_empty_one() {
        let folder = tempfile::tempdir().expect("folder");
        let store = ScanStore::new(folder.path().to_owned());
        fs::write(store.knowledge_file(), "{ not json").expect("written");
        assert_eq!(store.load_knowledge(), KnowledgeBase::default());
    }
}
