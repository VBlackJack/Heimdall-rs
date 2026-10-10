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

//! Quick Connect, the C# Heimdall's Ctrl+K palette without its snippets and its tools'
//! arguments: the sessions and the tools found as it scores them, the first ten sessions and
//! the tools used lately when nothing is typed, those of the hosts last connected to first;
//! when none is found, `ssh user@host:port` or `user@host` opens an SSH session saved
//! nowhere, and a bare host or address offers SSH and RDP to it, the one last used with it
//! first. Opened from a tab's "Split...", the sessions open in other tabs come first, to be
//! merged into it without reconnecting; Ctrl+Enter opens a session as a split of the tab
//! shown, as the C# `ConnectSplitFromPaletteAsync`.

use std::net::IpAddr;

use heimdall_core::profile::{DEFAULT_RDP_PORT, DEFAULT_SSH_PORT, RdpProfile, SshProfile};

use super::reconnect::Reopen;
use super::split::{Axis, Placement, SplitMessage};
use super::tree::{ProfileKind, ProfileSummary};
use super::{App, Effect, Message, Phase, TabProfile, ToolsMessage};
use crate::driver::Purpose;
use crate::ids::TabId;
use crate::tools::{ToolCategory, ToolId};

/// Sessions shown when nothing is typed, as the C# palette.
const FIRST_SESSIONS: usize = 10;

/// Most sessions a search shows, as the C# palette.
const MOST_FOUND: usize = 20;

/// Connections remembered, as the C# tracker's `MaxEntries`.
const RECENT_KEPT: usize = 50;

/// The score of a tool whose command word is typed whole, as the C# `ToolExactAliasScore`:
/// above any other.
const TOOL_EXACT_ALIAS_SCORE: usize = 999;

/// What is typed to list every tool, as the C# palette's "tool" and "tools".
const ALL_TOOLS_WORDS: [&str; 2] = ["tool", "tools"];

/// What Quick Connect offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickResult {
    /// A saved session.
    Profile(ProfileSummary),
    /// SSH to a host, saved nowhere.
    Ssh {
        /// The account, when typed.
        username: Option<String>,
        /// The host.
        host: String,
        /// The port.
        port: u16,
    },
    /// RDP to a host, saved nowhere.
    Rdp {
        /// The host.
        host: String,
    },
    /// A session open in another tab, merged into the tab split without reconnecting, as
    /// the C# split mode's "Active Sessions".
    Session {
        /// Its tab.
        tab: TabId,
        /// The tab's title, as it is called on the strip.
        title: String,
        /// Its protocol.
        kind: ProfileKind,
    },
    /// A built-in tool, opened in a tab of its own.
    Tool {
        /// The tool.
        tool: ToolId,
        /// Its name in the language shown, its tab's title.
        name: String,
    },
}

/// The section of the palette a result is listed under, as the C# palette groups its
/// results by their `Group`, in the order each first appears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickGroup {
    /// A saved session's folder.
    Folder(String),
    /// A saved session in no folder, as the C# `PaletteServersHeader` names it.
    Servers,
    /// A host typed, as the C# `PaletteQuickConnectHeader`.
    QuickConnect,
    /// The sessions open to merge, as the C# `SplitActiveSessionsHeader`.
    ActiveSessions,
    /// The tools used lately, when nothing is typed, as the C# `PaletteRecentToolsHeader`.
    RecentTools,
    /// A tool found, under its category.
    Category(ToolCategory),
}

/// A line of the palette: a result and the section it is listed under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickRow {
    /// What is offered.
    pub result: QuickResult,
    /// Its section.
    pub group: QuickGroup,
}

/// The words that find a tool in the language shown, as the C# scores its label and its
/// category's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolWords {
    /// Its name.
    pub name: String,
    /// Its category's name.
    pub category: String,
}

impl QuickRow {
    /// A saved session's line, under its folder.
    fn profile(profile: ProfileSummary) -> Self {
        let group = profile
            .group
            .as_deref()
            .map(str::trim)
            .filter(|folder| !folder.is_empty())
            .map_or(QuickGroup::Servers, |folder| {
                QuickGroup::Folder(folder.to_owned())
            });
        Self {
            result: QuickResult::Profile(profile),
            group,
        }
    }

    /// A tool's line, under `group`.
    fn tool(tool: ToolId, words: &dyn Fn(ToolId) -> ToolWords, group: QuickGroup) -> Self {
        Self {
            result: QuickResult::Tool {
                tool,
                name: words(tool).name,
            },
            group,
        }
    }
}

/// `rows` with those of a section together, the sections in the order each first appears,
/// as the C# palette's grouped list shows them.
fn grouped(rows: Vec<QuickRow>) -> Vec<QuickRow> {
    let mut sections: Vec<(QuickGroup, Vec<QuickRow>)> = Vec::new();
    for row in rows {
        match sections.iter_mut().find(|(group, _)| *group == row.group) {
            Some((_, section)) => section.push(row),
            None => sections.push((row.group.clone(), vec![row])),
        }
    }
    sections.into_iter().flat_map(|(_, rows)| rows).collect()
}

/// How well `tool` matches `query`, as the C# `ScoreToolDescriptor`: its name, its command
/// words, a word typed whole above all, and its category's name by half.
fn score_tool(tool: ToolId, words: &ToolWords, query: &str) -> usize {
    let mut best = score_text(&words.name, query);
    for prefix in tool.prefixes() {
        if prefix.eq_ignore_ascii_case(query) {
            return TOOL_EXACT_ALIAS_SCORE;
        }
        best = best.max(score_text(prefix, query));
    }
    best.max(score_text(&words.category, query) / 2)
}

/// How well `text` matches `query`, as the C# palette scores it: a start best, then
/// anywhere, then every character in order, more for those in a row; 0 for no match.
fn score_text(text: &str, query: &str) -> usize {
    if query.is_empty() {
        return 0;
    }
    let (text, query) = (text.to_lowercase(), query.to_lowercase());
    let length = query.chars().count();
    if text.starts_with(&query) {
        return 100 + length * 10;
    }
    if text.contains(&query) {
        return 50 + length * 5;
    }
    let mut wanted = query.chars().peekable();
    let (mut score, mut in_a_row) = (0, 0);
    for found in text.chars() {
        match wanted.peek() {
            Some(&next) if next == found => {
                wanted.next();
                in_a_row += 1;
                score += in_a_row * 2;
            }
            Some(_) => in_a_row = 0,
            None => break,
        }
    }
    if wanted.peek().is_none() { score } else { 0 }
}

/// How well `profile` matches `query`: its name and host fully, its folder, account,
/// protocol, environment and tags by half, as the C# palette scores them.
fn score(profile: &ProfileSummary, query: &str) -> usize {
    let host = profile
        .endpoint
        .as_ref()
        .map_or("", |(host, _)| host.as_str());
    [
        score_text(&profile.name, query),
        score_text(host, query),
        score_text(profile.group.as_deref().unwrap_or_default(), query) / 2,
        score_text(profile.username.as_deref().unwrap_or_default(), query) / 2,
        score_text(profile.kind.label(), query) / 2,
        score_text(profile.metadata.environment.map_or("", |e| e.name()), query) / 2,
        score_text(&profile.metadata.tags, query) / 2,
    ]
    .into_iter()
    .max()
    .unwrap_or(0)
}

/// Whether `text` is an address, or a name of letters, digits, dots and hyphens starting
/// with a letter or digit.
fn is_host(text: &str) -> bool {
    text.parse::<IpAddr>().is_ok()
        || text
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// `ssh user@host:port`, `user@host:port` or without the port: the account, the host and
/// the port, 22 when not typed.
fn parse_ssh(query: &str) -> Option<(String, String, u16)> {
    let text = query.trim();
    let text = text
        .get(..4)
        .filter(|start| start.eq_ignore_ascii_case("ssh "))
        .map_or(text, |_| text[4..].trim());
    let (user, rest) = text.split_once('@')?;
    let (host, port) = match rest.split_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None => (rest, DEFAULT_SSH_PORT),
    };
    (!user.is_empty() && !host.is_empty() && !user.contains(char::is_whitespace))
        .then(|| (user.to_owned(), host.to_owned(), port))
        .filter(|(_, host, _)| is_host(host))
}

impl App {
    /// The saved sessions Quick Connect offers when nothing is typed: those of the hosts
    /// last connected to first, newest first, then by name, as the C#; `most` of them when
    /// it says.
    fn first_profiles(&self, most: Option<usize>) -> Vec<ProfileSummary> {
        let mut profiles = self.profile_summaries();
        profiles.sort_by_cached_key(|profile| {
            let recency = profile
                .endpoint
                .as_ref()
                .and_then(|(host, _)| self.recency(host))
                .unwrap_or(usize::MAX);
            (recency, profile.name.to_lowercase())
        });
        profiles.truncate(most.unwrap_or(usize::MAX));
        profiles
    }

    /// The saved sessions `query` finds, the best first, equal ones by name, each with its
    /// score.
    fn found_profiles(&self, query: &str) -> Vec<(usize, ProfileSummary)> {
        let mut scored: Vec<(usize, ProfileSummary)> = self
            .profile_summaries()
            .into_iter()
            .map(|profile| (score(&profile, query), profile))
            .filter(|(score, _)| *score > 0)
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()))
        });
        scored
    }

    /// What `query` offers when nothing is found: `ssh user@host:port` an SSH session, a
    /// bare host SSH and RDP to it, the protocol last used with it first, as the C# leans.
    fn typed_destinations(&self, query: &str) -> Vec<QuickResult> {
        if let Some((username, host, port)) = parse_ssh(query) {
            return vec![QuickResult::Ssh {
                username: Some(username),
                host,
                port,
            }];
        }
        if !is_host(query) {
            return Vec::new();
        }
        let mut offered = vec![
            QuickResult::Ssh {
                username: None,
                host: query.to_owned(),
                port: DEFAULT_SSH_PORT,
            },
            QuickResult::Rdp {
                host: query.to_owned(),
            },
        ];
        if self.last_kind(query) == Some(ProfileKind::Rdp) {
            offered.reverse();
        }
        offered
    }

    /// The saved sessions Quick Connect finds for `query`, the best first, and the hosts
    /// typed it offers when none is: sessions only, neither tools nor sections.
    #[must_use]
    pub fn quick_results(&self, query: &str) -> Vec<QuickResult> {
        let query = query.trim();
        if query.is_empty() {
            return self
                .first_profiles(Some(FIRST_SESSIONS))
                .into_iter()
                .map(QuickResult::Profile)
                .collect();
        }
        let results: Vec<QuickResult> = self
            .found_profiles(query)
            .into_iter()
            .take(MOST_FOUND)
            .map(|(_, profile)| QuickResult::Profile(profile))
            .collect();
        if results.is_empty() {
            self.typed_destinations(query)
        } else {
            results
        }
    }

    /// The palette's lines for `query`, as the C# `OnSearchTextChanged` builds them and its
    /// grouped list shows them, each section together; `words` names the tools in the
    /// language shown.
    ///
    /// Nothing typed: opened from tab `split`'s "Split...", the sessions open in the other
    /// tabs it can be merged with, then the profiles last split with the one its pane shows,
    /// the most recent first, then every other profile; otherwise the first ten profiles.
    /// The tools used lately follow. "tool" or "tools" lists every tool. Anything else is
    /// scored across the tools and the profiles, the best twenty kept, and a host typed is
    /// offered when nothing is found.
    #[must_use]
    pub fn quick_results_in(
        &self,
        query: &str,
        split: Option<TabId>,
        words: &dyn Fn(ToolId) -> ToolWords,
    ) -> Vec<QuickRow> {
        let query = query.trim();
        if query.is_empty() {
            return grouped(self.first_rows(split, words));
        }
        if ALL_TOOLS_WORDS
            .iter()
            .any(|all| all.eq_ignore_ascii_case(query))
        {
            return ToolId::ALL
                .into_iter()
                .map(|tool| QuickRow::tool(tool, words, QuickGroup::Category(tool.category())))
                .collect();
        }
        // Tools first, then the profiles, as the C# adds them; the sort keeps that order
        // between equal scores.
        let mut scored: Vec<(usize, QuickRow)> = ToolId::ALL
            .into_iter()
            .filter_map(|tool| {
                let score = score_tool(tool, &words(tool), query);
                (score > 0).then(|| {
                    let row = QuickRow::tool(tool, words, QuickGroup::Category(tool.category()));
                    (score, row)
                })
            })
            .chain(
                self.found_profiles(query)
                    .into_iter()
                    .map(|(score, profile)| (score, QuickRow::profile(profile))),
            )
            .collect();
        scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        let rows: Vec<QuickRow> = scored
            .into_iter()
            .take(MOST_FOUND)
            .map(|(_, row)| row)
            .collect();
        if !rows.is_empty() {
            return grouped(rows);
        }
        self.typed_destinations(query)
            .into_iter()
            .map(|result| QuickRow {
                result,
                group: QuickGroup::QuickConnect,
            })
            .collect()
    }

    /// The palette's lines when nothing is typed, before they are put in sections.
    fn first_rows(
        &self,
        split: Option<TabId>,
        words: &dyn Fn(ToolId) -> ToolWords,
    ) -> Vec<QuickRow> {
        let mut rows: Vec<QuickRow> = Vec::new();
        if let Some(host) = split {
            // The sessions open, to merge without reconnecting, as the C# lists them first.
            rows.extend(self.merge_candidates(host).into_iter().map(|tab| QuickRow {
                result: QuickResult::Session {
                    tab: tab.id,
                    title: tab.display_title().to_owned(),
                    kind: self.tab_kind(tab),
                },
                group: QuickGroup::ActiveSessions,
            }));
            // Then the profiles last split with the one its pane shows, the most recent
            // first, then every other one: all of them, as the C# split mode lists them.
            let profiles = self.profile_summaries();
            let partners: Vec<ProfileSummary> = self
                .saved_profile(self.focus_of(host))
                .map(|own| self.split_layouts.partners(&own))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|partner| profiles.iter().find(|profile| profile.id == *partner))
                .cloned()
                .collect();
            let others: Vec<ProfileSummary> = self
                .first_profiles(None)
                .into_iter()
                .filter(|profile| !partners.iter().any(|partner| partner.id == profile.id))
                .collect();
            rows.extend(partners.into_iter().chain(others).map(QuickRow::profile));
        } else {
            rows.extend(
                self.first_profiles(Some(FIRST_SESSIONS))
                    .into_iter()
                    .map(QuickRow::profile),
            );
        }
        rows.extend(
            self.recent_tools()
                .iter()
                .map(|tool| QuickRow::tool(*tool, words, QuickGroup::RecentTools)),
        );
        rows
    }

    /// What opens `result`, chosen in the palette opened from a tab's "Split..." when
    /// `split` says which and how, and with Ctrl+Enter when `beside`, as the C#
    /// `ConnectFromPaletteAsync` and `ConnectSplitFromPaletteAsync` route it.
    ///
    /// A session open is merged into the tab split. A tool opens in a tab of its own: a
    /// tool's tab is neither split nor merged here. With Ctrl+Enter, a saved session opens
    /// as a split of the tab shown, side by side, and as usual when no tab is shown or the
    /// one shown is a tool's; a host typed opens as usual, as the C#. Without it, in split
    /// mode, what is chosen is merged into the tab split; otherwise it opens as usual. Each
    /// message goes through the gates an open goes through.
    #[must_use]
    pub fn quick_open(
        &self,
        result: QuickResult,
        split: Option<(TabId, Axis)>,
        beside: bool,
    ) -> Message {
        match result {
            QuickResult::Session { tab, .. } => match split {
                Some((host, axis)) => Message::Split(SplitMessage::Merge {
                    host,
                    tab,
                    axis,
                    placement: Placement::Second,
                }),
                None => Message::SelectTab(tab),
            },
            QuickResult::Tool { tool, name } => {
                Message::Tools(ToolsMessage::Open { tool, title: name })
            }
            result @ QuickResult::Profile(_) if beside => match self.splittable_shown() {
                Some(host) => Message::Split(SplitMessage::QuickConnect {
                    host,
                    axis: Axis::SideBySide,
                    result,
                }),
                None => Message::QuickConnect(result),
            },
            result if beside => Message::QuickConnect(result),
            result => match split {
                Some((host, axis)) => {
                    Message::Split(SplitMessage::QuickConnect { host, axis, result })
                }
                None => Message::QuickConnect(result),
            },
        }
    }

    /// The tab shown when a session can be opened as a split of it: not a tool's.
    fn splittable_shown(&self) -> Option<TabId> {
        self.shown_tab()
            .filter(|tab| tab.tool().is_none())
            .map(|tab| tab.id)
    }

    /// Records that `tab_id` just connected: its host, with its protocol, first among the
    /// recent ones, once per host and protocol. A local program has no host to keep.
    pub(super) fn note_recent(&mut self, tab_id: TabId) {
        let Some(tab) = self.tab(tab_id).filter(|tab| tab.phase == Phase::Connected) else {
            return;
        };
        let Some(host) = tab
            .profile
            .endpoint()
            .map(|(host, _)| host.trim().to_lowercase())
            .filter(|host| !host.is_empty())
        else {
            return;
        };
        let kind = self.tab_kind(tab);
        self.recent_hosts
            .retain(|(known, known_kind)| !(*known == host && *known_kind == kind));
        self.recent_hosts.insert(0, (host, kind));
        self.recent_hosts.truncate(RECENT_KEPT);
    }

    /// How recently `host` was connected to: 0 for the last one; `None` when it was not.
    fn recency(&self, host: &str) -> Option<usize> {
        let host = host.trim().to_lowercase();
        self.recent_hosts
            .iter()
            .position(|(known, _)| *known == host)
    }

    /// The protocol `host` was last connected with.
    fn last_kind(&self, host: &str) -> Option<ProfileKind> {
        let host = host.trim().to_lowercase();
        self.recent_hosts
            .iter()
            .find(|(known, _)| *known == host)
            .map(|(_, kind)| *kind)
    }

    /// Opens what Quick Connect offered.
    pub(super) fn quick_connect(&mut self, result: QuickResult) -> Vec<Effect> {
        let (profile, purpose) = match result {
            QuickResult::Profile(profile) => {
                return self.update(Message::ConnectProfile(profile.id));
            }
            // Routed by `quick_open`; reaching here, they open as it would outside split mode.
            result @ (QuickResult::Session { .. } | QuickResult::Tool { .. }) => {
                return self.update(self.quick_open(result, None, false));
            }
            QuickResult::Ssh {
                username,
                host,
                port,
            } => (
                TabProfile::Ssh(SshProfile {
                    id: super::connect_as::transient_id(),
                    name: host.clone(),
                    group: None,
                    host,
                    port,
                    username,
                    key_path: None,
                    gateway: None,
                    local_tunnel_port: None,
                    vault_entry: None,
                    forwards: heimdall_core::profile::Forwards::default(),
                    post_connect: heimdall_core::post_connect::PostConnect::default(),
                    forward_agent: false,
                    compression: false,
                    sftp: false,
                    legacy_algorithms: false,
                    session_logging: None,
                    ssh_mode: heimdall_core::profile::SshMode::Embedded,
                    x11_forwarding: false,
                }),
                Purpose::Shell,
            ),
            QuickResult::Rdp { host } => (
                TabProfile::Rdp(RdpProfile {
                    extras: heimdall_core::profile::RdpExtras::default(),
                    id: super::connect_as::transient_id(),
                    name: host.clone(),
                    group: None,
                    host,
                    port: DEFAULT_RDP_PORT,
                    username: None,
                    domain: None,
                    allow_tls_only: false,
                    gateway: None,
                    local_tunnel_port: None,
                    redirect_clipboard: true,
                    redirect_drives: false,
                    options: heimdall_core::profile::RdpOptions::default(),
                    vault_entry: None,
                    forwards: heimdall_core::profile::Forwards::default(),
                    // Nothing of its own: the application's RDP options.
                    follow_defaults: true,
                    several_servers: false,
                    anti_idle: false,
                    auto_reconnect: true,
                }),
                Purpose::Rdp,
            ),
        };
        let effects = self.open_transient(profile.clone(), purpose);
        self.reopened_by(Reopen::Transient(Box::new(profile), purpose));
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_beats_a_middle_beats_letters_in_order() {
        assert_eq!(score_text("webserver", "web"), 130);
        assert_eq!(score_text("my-web", "web"), 65);
        assert_eq!(score_text("w-e-b", "web"), 6, "three apart: 2 each");
        assert_eq!(score_text("wxeb", "web"), 2 + 2 + 4, "e and b in a row");
        assert_eq!(score_text("bew", "web"), 0, "out of order");
        assert_eq!(score_text("WebServer", "WEB"), 130, "whatever the case");
        assert_eq!(score_text("", "web"), 0);
        assert_eq!(score_text("web", ""), 0);
    }

    #[test]
    fn the_name_and_host_count_fully_the_folder_account_and_protocol_by_half() {
        use super::super::tree::ProfileKind;
        let profile = ProfileSummary {
            id: heimdall_core::profile::ProfileId::new("p"),
            name: "alpha".to_owned(),
            group: Some("webfarm".to_owned()),
            kind: ProfileKind::Ssh,
            endpoint: Some(("web.lab".to_owned(), 22)),
            username: Some("webmaster".to_owned()),
            gateway: None,
            favorite: false,
            metadata: heimdall_core::metadata::ProfileMetadata::default(),
        };
        assert_eq!(score(&profile, "web"), 130, "its host");
        assert_eq!(score(&profile, "alp"), 130, "its name");
        assert_eq!(score(&profile, "webf"), 70, "its folder, by half");
        assert_eq!(score(&profile, "webm"), 70, "its account, by half");
        assert_eq!(score(&profile, "ssh"), 65, "its protocol, by half");
        let bare = ProfileSummary {
            endpoint: None,
            group: None,
            username: None,
            ..profile
        };
        assert_eq!(score(&bare, "web"), 0);
    }

    #[test]
    fn an_ssh_destination_is_read_as_typed() {
        assert_eq!(
            parse_ssh("ssh admin@web.lab:2222"),
            Some(("admin".to_owned(), "web.lab".to_owned(), 2222))
        );
        assert_eq!(
            parse_ssh("SSH  root@10.0.0.5"),
            Some(("root".to_owned(), "10.0.0.5".to_owned(), 22))
        );
        assert_eq!(parse_ssh("admin@web.lab:port"), None);
        assert_eq!(parse_ssh("admin@"), None);
        assert_eq!(parse_ssh("@web.lab"), None);
        assert_eq!(parse_ssh("web.lab"), None);
        assert_eq!(parse_ssh("admin@we b"), None, "not a host");
        assert_eq!(parse_ssh("ad min@web.lab"), None, "not an account");
        assert!(is_host("10.0.0.5") && is_host("::1") && is_host("web-01.lab"));
        assert!(!is_host("-web") && !is_host("web lab") && !is_host("web_lab") && !is_host(""));
    }
}
