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
//! folder. The C# also flags a session whose gateway differs from its folder's usual one:
//! gateways of a shared file are not read here, so that check has nothing to look at.
//!
//! It is plain data, so the offline copy of a source can keep it.

use std::net::IpAddr;

use serde_json::{Value, json};

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
    /// A host filed more than once in the same folder.
    DuplicateHost,
}

/// Why a session of the file was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Not an SSH or an RDP session.
    OtherProtocol,
    /// The host is not a host name nor an address.
    InvalidHost,
    /// Another session of the file has the same folder and name.
    DuplicateSession,
    /// The source already holds as many sessions as one may.
    OverLimit,
}

impl SkipReason {
    fn key(self) -> &'static str {
        match self {
            Self::OtherProtocol => "other-protocol",
            Self::InvalidHost => "invalid-host",
            Self::DuplicateSession => "duplicate-session",
            Self::OverLimit => "over-limit",
        }
    }

    fn of_key(key: &str) -> Option<Self> {
        [
            Self::OtherProtocol,
            Self::InvalidHost,
            Self::DuplicateSession,
            Self::OverLimit,
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
            Self::DuplicateHost => "duplicate-host",
        }
    }

    fn of_key(key: &str) -> Option<Self> {
        [
            Self::SkippedSession,
            Self::UnresolvedVariable,
            Self::NameHostMismatch,
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
    /// What is wrong: the variable, the protocol or the host the file gives, by check.
    pub detail: String,
    /// More about it: for a host filed twice, the other sessions that have it.
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

/// The sessions that share a host with another in the same folder, in any case.
fn duplicate_hosts(taken: &[Taken<'_>]) -> Vec<ReportEntry> {
    let key = |session: &Taken<'_>| {
        (
            session.folder.trim().to_uppercase(),
            session.host.trim().to_uppercase(),
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
        Taken { folder, name, host }
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
}
