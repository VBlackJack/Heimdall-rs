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

//! The import report of a shared source, as the C# `SharedSessionImportReportBuilder`: what a
//! read of the file did not take as it stood, so the person who shares it can fix the file.
//!
//! A report holds the sessions the file listed and Heimdall refused (another protocol, a host
//! that is not one, a session listed twice), the logins that name a `[NAME]` variable with no
//! value, the sessions whose name is not their host's, and the hosts filed twice in one
//! folder (same protocol, as the C#), and the sessions whose gateway differs from the one most
//! of their folder takes.
//!
//! It is plain data, so the offline copy of a source can keep it.

use std::collections::HashMap;
use std::net::IpAddr;

use serde_json::{Value, json};

/// The share of a folder, in percent, one route must reach to be the folder's usual one: the
/// C# default of `SharedReportGatewayMajorityPercent` (`AppSettings.cs:451`, range 51-100).
pub const GATEWAY_MAJORITY_PERCENT: usize = 60;

/// The fewest sessions a folder needs before it is checked for a gateway outlier: the C#
/// default of `SharedReportGatewayMinFolderSessions` (`AppSettings.cs:455`, range 2-1000).
pub const GATEWAY_MIN_FOLDER_SESSIONS: usize = 3;

const PERCENT: usize = 100;

/// What a report entry is about, in the C# `SharedSessionReportCheck` order, which is the
/// order a report lists them in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReportCheck {
    /// A session of the file that was not taken.
    SkippedSession,
    /// A login that names a variable the user has not defined.
    UnresolvedVariable,
    /// A session whose name is neither its host nor the host's first label.
    NameHostMismatch,
    /// A session that takes another route than the one most of its folder takes.
    GatewayOutlier,
    /// A host filed more than once in the same folder.
    DuplicateHost,
}

/// Why a session of the file was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Not an SSH, SFTP, RDP, Telnet or VNC session, nor one of the two types said apart below;
    /// the detail is the session type of the file.
    OtherProtocol,
    /// An FTP session: it sends its password in clear and is not taken yet.
    FtpNotSupported,
    /// A WSL distribution: a Linux system on the computer that runs `MobaXterm`, with nothing to
    /// connect to from another one.
    WslNotSupported,
    /// The host is not a host name nor an address.
    InvalidHost,
    /// Another session of the file has the same folder and name.
    DuplicateSession,
    /// The source already holds as many sessions as one may.
    OverLimit,
    /// The session names an SSH gateway whose host is not one: taking it would reach its
    /// host around the gateway the file asked for.
    InvalidGateway,
    /// The session goes through an SSH jump host its protocol cannot be routed through
    /// (Telnet, VNC): a direct connection would go around it.
    JumpHostNotSupported,
    /// The session goes through a proxy Heimdall does not use: a direct connection would
    /// go around it.
    ProxyNotSupported,
    /// The RDP session signs in to its RD Gateway with a smart card, or in a way Heimdall does
    /// not reproduce: a direct connection would go around the gateway.
    RdGatewayNotSupported,
    /// The RDP session names an RD Gateway whose host is not one: taking it without the gateway
    /// would go around it.
    InvalidRdGateway,
}

impl SkipReason {
    fn key(self) -> &'static str {
        match self {
            Self::OtherProtocol => "other-protocol",
            Self::FtpNotSupported => "ftp-not-supported",
            Self::WslNotSupported => "wsl-not-supported",
            Self::InvalidHost => "invalid-host",
            Self::DuplicateSession => "duplicate-session",
            Self::OverLimit => "over-limit",
            Self::InvalidGateway => "invalid-gateway",
            Self::JumpHostNotSupported => "jump-host-not-supported",
            Self::ProxyNotSupported => "proxy-not-supported",
            Self::RdGatewayNotSupported => "rd-gateway-not-supported",
            Self::InvalidRdGateway => "invalid-rd-gateway",
        }
    }

    fn of_key(key: &str) -> Option<Self> {
        [
            Self::OtherProtocol,
            Self::FtpNotSupported,
            Self::WslNotSupported,
            Self::InvalidHost,
            Self::DuplicateSession,
            Self::OverLimit,
            Self::InvalidGateway,
            Self::JumpHostNotSupported,
            Self::ProxyNotSupported,
            Self::RdGatewayNotSupported,
            Self::InvalidRdGateway,
        ]
        .into_iter()
        .find(|reason| reason.key() == key)
    }
}

impl ReportCheck {
    fn key(self) -> &'static str {
        match self {
            Self::SkippedSession => "skipped",
            Self::UnresolvedVariable => "unresolved",
            Self::NameHostMismatch => "mismatch",
            Self::GatewayOutlier => "gateway-outlier",
            Self::DuplicateHost => "duplicate-host",
        }
    }

    fn of_key(key: &str) -> Option<Self> {
        [
            Self::SkippedSession,
            Self::UnresolvedVariable,
            Self::NameHostMismatch,
            Self::GatewayOutlier,
            Self::DuplicateHost,
        ]
        .into_iter()
        .find(|check| check.key() == key)
    }
}

/// One finding of a report, as the C# `SharedSessionReportEntry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportEntry {
    /// What it is about.
    pub check: ReportCheck,
    /// The folder the session is filed in, empty at the root.
    pub folder: String,
    /// The session's name.
    pub session: String,
    /// What is wrong: the variable, the protocol or the host the file gives, by check. For a
    /// gateway outlier, the route the session takes: the gateway's name, empty for a direct
    /// connection.
    pub detail: String,
    /// More about it: for a host filed twice, the other sessions that have it; for a gateway
    /// outlier, the route most of the folder takes, named as `detail` is.
    pub context: String,
    /// Why a session was not taken, for [`ReportCheck::SkippedSession`].
    pub reason: Option<SkipReason>,
}

/// A session of the file that was not taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// The folder the file files it in.
    pub folder: String,
    /// Its name, else its host.
    pub name: String,
    /// Why.
    pub reason: SkipReason,
    /// The protocol or the host the file gives.
    pub detail: String,
}

/// A session that was taken, as the checks look at it.
#[derive(Debug, Clone, Copy)]
pub struct Taken<'a> {
    /// The folder the file files it in, empty at the root.
    pub folder: &'a str,
    /// Its name.
    pub name: &'a str,
    /// Its host.
    pub host: &'a str,
    /// Its protocol: a host reached over two protocols is not filed twice.
    pub protocol: &'a str,
    /// The identifier of the gateway it goes through; empty for a direct connection.
    pub route: &'a str,
    /// The name of that gateway, shown in the report; empty for a direct connection.
    pub route_name: &'a str,
}

/// A source's report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// The findings, ordered by check, folder and session.
    pub entries: Vec<ReportEntry>,
}

impl ImportReport {
    /// Whether it has nothing to say.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many findings are of `check`.
    #[must_use]
    pub fn count_of(&self, check: ReportCheck) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.check == check)
            .count()
    }

    /// The report of a read: `refused` sessions, the `unresolved` variables as (folder,
    /// session, variable), and the checks over the sessions `taken`.
    #[must_use]
    pub fn build(
        refused: Vec<Refused>,
        unresolved: Vec<(String, String, String)>,
        taken: &[Taken<'_>],
    ) -> Self {
        let mut entries = Vec::new();
        for (folder, session, variable) in unresolved {
            entries.push(ReportEntry {
                check: ReportCheck::UnresolvedVariable,
                folder,
                session,
                detail: variable,
                context: String::new(),
                reason: None,
            });
        }
        for session in refused {
            entries.push(ReportEntry {
                check: ReportCheck::SkippedSession,
                folder: session.folder,
                session: session.name,
                detail: session.detail,
                context: String::new(),
                reason: Some(session.reason),
            });
        }
        for session in taken {
            if name_matches_host(session.name, session.host) == Some(false) {
                entries.push(ReportEntry {
                    check: ReportCheck::NameHostMismatch,
                    folder: session.folder.to_owned(),
                    session: session.name.to_owned(),
                    detail: session.host.to_owned(),
                    context: String::new(),
                    reason: None,
                });
            }
        }
        entries.extend(gateway_outliers(taken));
        entries.extend(duplicate_hosts(taken));
        entries.sort_by(|a, b| {
            a.check
                .cmp(&b.check)
                .then_with(|| a.folder.to_lowercase().cmp(&b.folder.to_lowercase()))
                .then_with(|| a.session.to_lowercase().cmp(&b.session.to_lowercase()))
        });
        Self { entries }
    }

    /// The report as JSON, for the offline copy.
    #[must_use]
    pub fn to_json(&self) -> Value {
        Value::Array(
            self.entries
                .iter()
                .map(|entry| {
                    json!({
                        "check": entry.check.key(),
                        "folder": entry.folder,
                        "session": entry.session,
                        "detail": entry.detail,
                        "context": entry.context,
                        "reason": entry.reason.map(SkipReason::key),
                    })
                })
                .collect(),
        )
    }

    /// The report from [`Self::to_json`]; entries it cannot read are left out.
    #[must_use]
    pub fn from_json(value: &Value) -> Self {
        let text = |entry: &Value, key: &str| {
            entry
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let entries = value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|entry| {
                        Some(ReportEntry {
                            check: ReportCheck::of_key(entry.get("check")?.as_str()?)?,
                            folder: text(entry, "folder"),
                            session: text(entry, "session"),
                            detail: text(entry, "detail"),
                            context: text(entry, "context"),
                            reason: entry
                                .get("reason")
                                .and_then(Value::as_str)
                                .and_then(SkipReason::of_key),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { entries }
    }
}

/// Whether `name` is `host` or its first label, in any case; `None` when `host` is an
/// address or empty, which a name need not match, as the C# `NameMatchesHost`.
#[must_use]
pub fn name_matches_host(name: &str, host: &str) -> Option<bool> {
    let name = name.trim();
    let host = host.trim();
    if host.is_empty() || host.parse::<IpAddr>().is_ok() {
        return None;
    }
    let first = host.split('.').next().unwrap_or(host);
    Some(name.eq_ignore_ascii_case(host) || name.eq_ignore_ascii_case(first))
}

/// The sessions of a folder that take another route than the one most of the folder takes, as
/// the C# `GatewayOutliers`: only in a folder of at least [`GATEWAY_MIN_FOLDER_SESSIONS`]
/// sessions where one route (a gateway, or a direct connection) is taken by at least
/// [`GATEWAY_MAJORITY_PERCENT`] percent of them. A folder without such a majority has no
/// outlier: a tie or a plurality is not a rule the folder follows.
fn gateway_outliers(taken: &[Taken<'_>]) -> Vec<ReportEntry> {
    // Folders compare without case, in the order they first appear.
    let mut folders: Vec<(String, Vec<&Taken<'_>>)> = Vec::new();
    // Only the sessions a MobaXterm file can give a jump host vote or stand out
    // (`MobaXtermImporter.CanCarryGateway`): a Telnet, VNC or SFTP session is direct whatever
    // its folder does.
    for session in taken
        .iter()
        .filter(|session| can_carry_gateway(session.protocol))
    {
        let key = session.folder.trim().to_uppercase();
        match folders.iter_mut().find(|(known, _)| *known == key) {
            Some((_, sessions)) => sessions.push(session),
            None => folders.push((key, vec![session])),
        }
    }
    let mut entries = Vec::new();
    for (_, sessions) in folders {
        if sessions.len() < GATEWAY_MIN_FOLDER_SESSIONS {
            continue;
        }
        let mut votes: HashMap<&str, usize> = HashMap::new();
        for session in &sessions {
            *votes.entry(session.route).or_default() += 1;
        }
        let Some((usual, count)) = votes.into_iter().max_by_key(|(_, count)| *count) else {
            continue;
        };
        if count * PERCENT < GATEWAY_MAJORITY_PERCENT * sessions.len() {
            continue;
        }
        // The majority is above half, so it is the only route that can be the usual one.
        let usual_name = sessions
            .iter()
            .find(|session| session.route == usual)
            .map(|session| session.route_name)
            .unwrap_or_default();
        for session in sessions.iter().filter(|session| session.route != usual) {
            entries.push(ReportEntry {
                check: ReportCheck::GatewayOutlier,
                folder: session.folder.trim().to_owned(),
                session: session.name.to_owned(),
                detail: session.route_name.to_owned(),
                context: usual_name.to_owned(),
                reason: None,
            });
        }
    }
    entries
}

/// Whether sessions of `protocol` can name an SSH jump host in a `MobaXterm` file: SSH and RDP.
fn can_carry_gateway(protocol: &str) -> bool {
    ["SSH", "RDP"]
        .iter()
        .any(|known| known.eq_ignore_ascii_case(protocol.trim()))
}

/// The sessions that share a host and a protocol with another in the same folder, in any
/// case, whatever their port.
fn duplicate_hosts(taken: &[Taken<'_>]) -> Vec<ReportEntry> {
    let key = |session: &Taken<'_>| {
        (
            session.folder.trim().to_uppercase(),
            session.host.trim().to_uppercase(),
            session.protocol.trim().to_uppercase(),
        )
    };
    let mut entries = Vec::new();
    for session in taken
        .iter()
        .filter(|session| !session.host.trim().is_empty())
    {
        let own = key(session);
        let others: Vec<&str> = taken
            .iter()
            .filter(|other| !std::ptr::eq(*other, session) && key(other) == own)
            .map(|other| other.name)
            .collect();
        if others.is_empty() {
            continue;
        }
        entries.push(ReportEntry {
            check: ReportCheck::DuplicateHost,
            folder: session.folder.trim().to_owned(),
            session: session.name.to_owned(),
            detail: session.host.trim().to_owned(),
            context: others.join(", "),
            reason: None,
        });
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taken<'a>(folder: &'a str, name: &'a str, host: &'a str) -> Taken<'a> {
        Taken {
            folder,
            name,
            host,
            protocol: "SSH",
            route: "",
            route_name: "",
        }
    }

    fn routed<'a>(name: &'a str, route: &'a str) -> Taken<'a> {
        Taken {
            route,
            route_name: route,
            ..taken("Prod", name, name)
        }
    }

    fn outliers(sessions: &[Taken<'_>]) -> Vec<ReportEntry> {
        ImportReport::build(Vec::new(), Vec::new(), sessions)
            .entries
            .into_iter()
            .filter(|entry| entry.check == ReportCheck::GatewayOutlier)
            .collect()
    }

    #[test]
    fn a_name_matches_its_host_or_the_hosts_first_label() {
        assert_eq!(name_matches_host("web01", "web01.example.com"), Some(true));
        assert_eq!(
            name_matches_host("WEB01.example.com", "web01.example.com"),
            Some(true)
        );
        assert_eq!(name_matches_host("db", "web01.example.com"), Some(false));
        assert_eq!(name_matches_host("anything", "10.0.0.1"), None);
        assert_eq!(name_matches_host("anything", "::1"), None);
        assert_eq!(name_matches_host("anything", ""), None);
    }

    #[test]
    fn the_report_lists_every_finding_by_check_then_folder_then_session() {
        let report = ImportReport::build(
            vec![Refused {
                folder: "Prod".to_owned(),
                name: "ftp01".to_owned(),
                reason: SkipReason::OtherProtocol,
                detail: "FTP".to_owned(),
            }],
            vec![("Prod".to_owned(), "web01".to_owned(), "DOMAIN".to_owned())],
            &[
                taken("Prod", "web01", "web01.example.com"),
                taken("Prod", "primary", "db01.example.com"),
                taken("Prod", "replica", "db01.example.com"),
                taken("Lab", "ip", "10.0.0.5"),
            ],
        );
        let checks: Vec<ReportCheck> = report.entries.iter().map(|entry| entry.check).collect();
        assert_eq!(
            checks,
            [
                ReportCheck::SkippedSession,
                ReportCheck::UnresolvedVariable,
                ReportCheck::NameHostMismatch,
                ReportCheck::NameHostMismatch,
                ReportCheck::DuplicateHost,
                ReportCheck::DuplicateHost,
            ]
        );
        assert_eq!(report.entries[0].reason, Some(SkipReason::OtherProtocol));
        assert_eq!(report.entries[0].detail, "FTP");
        assert_eq!(report.entries[1].detail, "DOMAIN");
        assert_eq!(report.entries[2].session, "primary");
        assert_eq!(report.entries[4].session, "primary");
        assert_eq!(report.entries[4].context, "replica");
        assert_eq!(report.count_of(ReportCheck::DuplicateHost), 2);
        assert!(!report.is_empty());
    }

    #[test]
    fn a_clean_file_has_an_empty_report() {
        let report = ImportReport::build(
            Vec::new(),
            Vec::new(),
            &[taken("", "web01", "web01.example.com")],
        );
        assert!(report.is_empty());
    }

    #[test]
    fn a_report_survives_the_offline_copy() {
        let report = ImportReport::build(
            vec![Refused {
                folder: String::new(),
                name: "x".to_owned(),
                reason: SkipReason::InvalidHost,
                detail: "a b".to_owned(),
            }],
            Vec::new(),
            &[taken("F", "a", "b.example.com")],
        );
        assert_eq!(ImportReport::from_json(&report.to_json()), report);
        assert!(ImportReport::from_json(&json!("nope")).is_empty());
    }

    #[test]
    fn a_session_off_the_route_of_its_folder_is_an_outlier() {
        let found = outliers(&[
            routed("a", "bastion"),
            routed("b", "bastion"),
            routed("c", "bastion"),
            routed("d", ""),
        ]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].session, "d");
        assert_eq!(found[0].detail, "");
        assert_eq!(found[0].context, "bastion");
        assert_eq!(found[0].folder, "Prod");
    }

    #[test]
    fn a_direct_majority_makes_the_gatewayed_session_the_outlier() {
        let found = outliers(&[routed("a", ""), routed("b", ""), routed("c", "bastion")]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].session, "c");
        assert_eq!(found[0].detail, "bastion");
        assert_eq!(found[0].context, "");
    }

    #[test]
    fn a_folder_without_a_majority_or_too_small_has_no_outlier() {
        // 2 of 4 is 50 percent, under the 60 of the rule: a tie is not a rule.
        assert!(
            outliers(&[
                routed("a", "x"),
                routed("b", "x"),
                routed("c", "y"),
                routed("d", "y"),
            ])
            .is_empty()
        );
        // Two sessions are fewer than the three a folder needs.
        assert!(outliers(&[routed("a", "x"), routed("b", "")]).is_empty());
        // 3 of 5 is exactly 60 percent: enough.
        assert_eq!(
            outliers(&[
                routed("a", "x"),
                routed("b", "x"),
                routed("c", "x"),
                routed("d", "y"),
                routed("e", "z"),
            ])
            .len(),
            2
        );
    }

    #[test]
    fn folders_are_checked_apart_and_without_case() {
        let mut other = routed("z", "");
        other.folder = "Lab";
        let mut lower = routed("c", "x");
        lower.folder = "prod";
        let found = outliers(&[
            routed("a", "x"),
            routed("b", "x"),
            lower,
            routed("d", ""),
            other,
        ]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].session, "d");
    }

    #[test]
    fn one_host_over_two_protocols_is_not_filed_twice() {
        let ssh = taken("Prod", "box", "box.example.com");
        let rdp = Taken {
            protocol: "RDP",
            name: "box-rdp",
            ..ssh
        };
        let report = ImportReport::build(Vec::new(), Vec::new(), &[ssh, rdp]);
        assert_eq!(report.count_of(ReportCheck::DuplicateHost), 0);
        let again = Taken {
            name: "box2",
            ..ssh
        };
        let report = ImportReport::build(Vec::new(), Vec::new(), &[ssh, again]);
        assert_eq!(report.count_of(ReportCheck::DuplicateHost), 2);
    }

    #[test]
    fn a_session_that_cannot_name_a_gateway_neither_votes_nor_stands_out() {
        let direct = |name: &'static str, protocol: &'static str| Taken {
            protocol,
            ..routed(name, "")
        };
        let sessions = [
            routed("a", "g1"),
            routed("b", "g1"),
            routed("c", "g1"),
            direct("telnet", "Telnet"),
            direct("vnc", "VNC"),
            direct("files", "SFTP"),
        ];
        assert!(outliers(&sessions).is_empty());
        // An SSH session that is direct still stands out.
        let sessions = [
            routed("a", "g1"),
            routed("b", "g1"),
            routed("c", "g1"),
            direct("telnet", "Telnet"),
            direct("ssh", "SSH"),
        ];
        let found = outliers(&sessions);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].session, "ssh");
    }

    #[test]
    fn the_new_skip_reasons_survive_the_offline_copy() {
        let report = ImportReport::build(
            vec![
                Refused {
                    folder: String::new(),
                    name: "tel".to_owned(),
                    reason: SkipReason::JumpHostNotSupported,
                    detail: "jump".to_owned(),
                },
                Refused {
                    folder: String::new(),
                    name: "vnc".to_owned(),
                    reason: SkipReason::ProxyNotSupported,
                    detail: String::new(),
                },
            ],
            Vec::new(),
            &[],
        );
        assert_eq!(ImportReport::from_json(&report.to_json()), report);
    }
}
