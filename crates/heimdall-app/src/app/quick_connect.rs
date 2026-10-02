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

//! Quick Connect, the C# Heimdall's Ctrl+K palette without its tools and snippets: the
//! sessions found as it scores them, the first ten when nothing is typed; when none is
//! found, `ssh user@host:port` or `user@host` opens an SSH session saved nowhere, and a bare
//! host or address offers SSH and RDP to it.

use std::net::IpAddr;

use heimdall_core::profile::{DEFAULT_RDP_PORT, DEFAULT_SSH_PORT, RdpProfile, SshProfile};

use super::reconnect::Reopen;
use super::tree::ProfileSummary;
use super::{App, Effect, Message, TabProfile};
use crate::driver::Purpose;

/// Sessions shown when nothing is typed, as the C# palette.
const FIRST_SESSIONS: usize = 10;

/// Most sessions a search shows, as the C# palette.
const MOST_FOUND: usize = 20;

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

/// How well `profile` matches `query`: its name and host fully, its folder, account and
/// protocol by half.
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
    /// What Quick Connect offers for `query`.
    #[must_use]
    pub fn quick_results(&self, query: &str) -> Vec<QuickResult> {
        let query = query.trim();
        let mut profiles = self.profile_summaries();
        if query.is_empty() {
            profiles.sort_by_key(|profile| profile.name.to_lowercase());
            return profiles
                .into_iter()
                .take(FIRST_SESSIONS)
                .map(QuickResult::Profile)
                .collect();
        }
        let mut scored: Vec<(usize, ProfileSummary)> = profiles
            .into_iter()
            .map(|profile| (score(&profile, query), profile))
            .filter(|(score, _)| *score > 0)
            .collect();
        // The best first; equal ones by name.
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()))
        });
        let mut results: Vec<QuickResult> = scored
            .into_iter()
            .take(MOST_FOUND)
            .map(|(_, profile)| QuickResult::Profile(profile))
            .collect();
        if results.is_empty() {
            if let Some((username, host, port)) = parse_ssh(query) {
                results.push(QuickResult::Ssh {
                    username: Some(username),
                    host,
                    port,
                });
            } else if is_host(query) {
                results.push(QuickResult::Ssh {
                    username: None,
                    host: query.to_owned(),
                    port: DEFAULT_SSH_PORT,
                });
                results.push(QuickResult::Rdp {
                    host: query.to_owned(),
                });
            }
        }
        results
    }

    /// Opens what Quick Connect offered.
    pub(super) fn quick_connect(&mut self, result: QuickResult) -> Vec<Effect> {
        let (profile, purpose) = match result {
            QuickResult::Profile(profile) => {
                return self.update(Message::ConnectProfile(profile.id));
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
                    vault_entry: None,
                    forwards: heimdall_core::profile::Forwards::default(),
                    post_connect: heimdall_core::post_connect::PostConnect::default(),
                    forward_agent: false,
                    compression: false,
                    sftp: false,
                }),
                Purpose::Shell,
            ),
            QuickResult::Rdp { host } => (
                TabProfile::Rdp(RdpProfile {
                    id: super::connect_as::transient_id(),
                    name: host.clone(),
                    group: None,
                    host,
                    port: DEFAULT_RDP_PORT,
                    username: None,
                    domain: None,
                    allow_tls_only: false,
                    gateway: None,
                    redirect_clipboard: true,
                    redirect_drives: false,
                    options: heimdall_core::profile::RdpOptions::default(),
                    vault_entry: None,
                    forwards: heimdall_core::profile::Forwards::default(),
                    // Nothing of its own: the application's RDP options.
                    follow_defaults: true,
                    several_servers: false,
                    anti_idle: false,
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
