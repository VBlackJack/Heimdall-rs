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

//! Import of an OpenSSH client configuration (`~/.ssh/config`), as the C# Heimdall reads
//! it (`OpenSshConfigParser`, `OpenSshConfigImporter`).
//!
//! The subset read: `Host` blocks and, in them, `HostName`, `Port`, `User`, `IdentityFile`,
//! `ProxyJump` and `ProxyCommand`, the first value of each kept, as OpenSSH keeps it. A
//! `Host` line gives one profile per alias; a wildcard or negated alias is skipped, as is
//! an alias seen before. `Match` blocks and `Include` are not followed. `HostName` expands
//! `%h` and `%%`, the only tokens that need no running state. A `ProxyJump` of
//! `[user@]host[:port]` hops becomes a chain of SSH gateways, a hop named by another
//! `Host` block taking that block's settings. Everything left out is said, with its line.
//!
//! Beyond the C# reader: `Keyword=value` is read as OpenSSH reads it, and a gateway is
//! reused only when its parent is the same, so one host reached two ways gets two gateways.

use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};

use crate::post_connect::PostConnect;
use crate::profile::{DEFAULT_SSH_PORT, Forwards, ProfileId, SshGateway, SshProfile};

/// Starts a token in `HostName` and `ProxyJump`.
const TOKEN: char = '%';

/// The `HostName` token standing for the alias.
const ALIAS_TOKEN: char = 'h';

/// How much a diagnostic matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Read differently from the file, as intended.
    Info,
    /// Left out, or read in a way the user should check.
    Warning,
}

/// What a diagnostic says, as the C# `OpenSshDiagnosticCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// A `Match` block, not read.
    MatchBlockIgnored,
    /// An `Include`, not followed.
    IncludeIgnored,
    /// A wildcard or negated alias, not a server.
    WildcardAliasIgnored,
    /// A directive not read.
    UnknownDirectiveIgnored,
    /// A port that is not 1 to 65535: 22 is used.
    InvalidPort,
    /// An alias already given by an earlier `Host` line.
    DuplicateAlias,
    /// `ProxyCommand`, which runs a program: not imported.
    ProxyCommandUnsupported,
    /// `ProxyJump` beside `ProxyCommand`: neither is imported.
    ProxyJumpWithProxyCommand,
    /// A token in `ProxyJump`, which needs running state.
    ProxyJumpToken,
    /// A `ProxyJump` chain that comes back to itself.
    ProxyJumpCycle,
    /// A `ProxyJump` that is not `[user@]host[:port]` hops.
    ProxyJumpSyntax,
    /// `~` in `IdentityFile` replaced by the home folder.
    IdentityFileTildeExpanded,
    /// No `HostName`: the alias is the host.
    HostNameFallbackToAlias,
    /// A token in `HostName` other than `%h` and `%%`: the alias is not imported.
    HostNameToken,
}

/// Something the import says about a line of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// How much it matters.
    pub level: Level,
    /// Its line, from 1.
    pub line: usize,
    /// What it says.
    pub code: Code,
    /// The value or name concerned, when there is one.
    pub context: Option<String>,
}

/// A gateway on the way to a server, from `ProxyJump`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop {
    /// The host as `ProxyJump` names it: an alias, or a host.
    pub host: String,
    /// The host reached: the alias's `HostName` when it names a block.
    pub host_name: String,
    /// Its port.
    pub port: u16,
    /// Its account.
    pub user: Option<String>,
    /// Its key, from the block the alias names.
    pub identity_file: Option<String>,
}

/// A server of the file, one per alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The alias: the profile's name.
    pub alias: String,
    /// The host reached.
    pub host_name: String,
    /// Its port.
    pub port: u16,
    /// Its account.
    pub user: Option<String>,
    /// Its key.
    pub identity_file: Option<String>,
    /// The line of its `Host`.
    pub line: usize,
    /// The gateways on the way, first one first.
    pub proxy_jump: Vec<Hop>,
}

/// What a file gives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    /// The servers, in the file's order.
    pub candidates: Vec<Candidate>,
    /// What was read differently or left out.
    pub diagnostics: Vec<Diagnostic>,
}

/// A `Host` block being read.
#[derive(Debug)]
struct Block {
    line: usize,
    aliases: Vec<String>,
    host_name: Option<(String, usize)>,
    port: Option<u16>,
    user: Option<String>,
    identity_file: Option<String>,
    proxy_jump: Option<(String, usize, bool)>,
    proxy_command: Option<(String, usize)>,
}

impl Block {
    fn port(&self) -> u16 {
        self.port.unwrap_or(DEFAULT_SSH_PORT)
    }
}

/// A `ProxyJump` hop as written.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawHop {
    host: String,
    user: Option<String>,
    port: Option<u16>,
}

/// Reads `contents`; `home` replaces a leading `~` of `IdentityFile`.
#[must_use]
pub fn parse(contents: &str, home: Option<&Path>) -> Parsed {
    let mut diagnostics = Vec::new();
    let mut blocks = Vec::new();
    let mut seen = HashSet::new();
    let mut current: Option<Block> = None;
    let mut in_match = false;
    let say = |diagnostics: &mut Vec<Diagnostic>, level, line, code, context: Option<&str>| {
        diagnostics.push(Diagnostic {
            level,
            line,
            code,
            context: context.map(str::to_owned),
        });
    };
    let text = contents.replace("\r\n", "\n").replace('\r', "\n");
    for (index, raw) in text.split('\n').enumerate() {
        let line = index + 1;
        let Some((directive, value, quoted)) = split_directive(strip_comment(raw).trim()) else {
            continue;
        };
        // Matched whatever its case; named as written.
        let keyword = directive.to_ascii_lowercase();
        if in_match {
            match keyword.as_str() {
                "host" => in_match = false,
                "match" => {
                    say(
                        &mut diagnostics,
                        Level::Warning,
                        line,
                        Code::MatchBlockIgnored,
                        None,
                    );
                    continue;
                }
                _ => continue,
            }
        }
        match keyword.as_str() {
            "host" => {
                blocks.extend(current.take());
                current = host_block(&value, line, &mut diagnostics, &mut seen);
            }
            "match" => {
                blocks.extend(current.take());
                say(
                    &mut diagnostics,
                    Level::Warning,
                    line,
                    Code::MatchBlockIgnored,
                    None,
                );
                in_match = true;
            }
            "include" => say(
                &mut diagnostics,
                Level::Warning,
                line,
                Code::IncludeIgnored,
                Some(&value),
            ),
            _ => match current.as_mut() {
                Some(block) => {
                    apply(
                        block,
                        &directive,
                        value,
                        quoted,
                        home,
                        line,
                        &mut diagnostics,
                    );
                }
                None => say(
                    &mut diagnostics,
                    Level::Info,
                    line,
                    Code::UnknownDirectiveIgnored,
                    Some(&directive),
                ),
            },
        }
    }
    blocks.extend(current);
    let candidates = candidates(&blocks, &mut diagnostics);
    // Said in the file's order, whenever each was found.
    diagnostics.sort_by_key(|diagnostic| diagnostic.line);
    Parsed {
        candidates,
        diagnostics,
    }
}

/// A `Host` line's block, `None` when none of its aliases is a server not seen before.
fn host_block(
    value: &str,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
    seen: &mut HashSet<String>,
) -> Option<Block> {
    let mut aliases = Vec::new();
    for alias in split_tokens(value) {
        let code = if is_wildcard(&alias) {
            Code::WildcardAliasIgnored
        } else if !seen.insert(alias.to_lowercase()) {
            Code::DuplicateAlias
        } else {
            aliases.push(alias);
            continue;
        };
        diagnostics.push(Diagnostic {
            level: Level::Warning,
            line,
            code,
            context: Some(alias),
        });
    }
    (!aliases.is_empty()).then_some(Block {
        line,
        aliases,
        host_name: None,
        port: None,
        user: None,
        identity_file: None,
        proxy_jump: None,
        proxy_command: None,
    })
}

/// One directive of a block, as written: the first value of each is kept, as OpenSSH keeps
/// it.
fn apply(
    block: &mut Block,
    directive: &str,
    value: String,
    quoted: bool,
    home: Option<&Path>,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut say = |level, code, context: String| {
        diagnostics.push(Diagnostic {
            level,
            line,
            code,
            context: Some(context),
        });
    };
    match directive.to_ascii_lowercase().as_str() {
        "hostname" => {
            block.host_name.get_or_insert((value, line));
        }
        "port" if block.port.is_none() => {
            block.port = Some(match value.parse::<u16>() {
                Ok(port) if port != 0 => port,
                _ => {
                    say(Level::Warning, Code::InvalidPort, value);
                    DEFAULT_SSH_PORT
                }
            });
        }
        "user" => {
            block.user.get_or_insert(value);
        }
        "identityfile" if block.identity_file.is_none() => {
            let expanded = expand_tilde(&value, home);
            if expanded != value {
                say(Level::Info, Code::IdentityFileTildeExpanded, value);
            }
            block.identity_file = Some(expanded);
        }
        "proxyjump" => {
            block.proxy_jump.get_or_insert((value, line, quoted));
        }
        "proxycommand" => {
            block.proxy_command.get_or_insert((value, line));
        }
        "port" | "identityfile" => {}
        _ => say(
            Level::Info,
            Code::UnknownDirectiveIgnored,
            directive.to_owned(),
        ),
    }
}

/// `~` or `~/...` under `home`; anything else, `~user` included, as written.
fn expand_tilde(value: &str, home: Option<&Path>) -> String {
    let Some(home) = home else {
        return value.to_owned();
    };
    let Some(rest) = value.strip_prefix('~') else {
        return value.to_owned();
    };
    if !rest.is_empty() && !rest.starts_with(['/', '\\']) {
        return value.to_owned();
    }
    let rest = rest.trim_start_matches(['/', '\\']);
    if rest.is_empty() {
        return home.to_string_lossy().into_owned();
    }
    home.join(rest.replace(['/', '\\'], std::path::MAIN_SEPARATOR_STR))
        .to_string_lossy()
        .into_owned()
}

fn candidates(blocks: &[Block], diagnostics: &mut Vec<Diagnostic>) -> Vec<Candidate> {
    let by_alias: HashMap<String, &Block> = blocks
        .iter()
        .flat_map(|block| {
            block
                .aliases
                .iter()
                .map(move |alias| (alias.to_lowercase(), block))
        })
        .collect();
    let mut found = Vec::new();
    for block in blocks {
        for alias in &block.aliases {
            let Some(host_name) = host_name(alias, block, diagnostics) else {
                continue;
            };
            let proxy_jump = chain(alias, &host_name, block, &by_alias, diagnostics);
            found.push(Candidate {
                alias: alias.clone(),
                host_name,
                port: block.port(),
                user: block.user.clone(),
                identity_file: block.identity_file.clone(),
                line: block.line,
                proxy_jump,
            });
        }
    }
    found
}

/// The host of `alias`: its block's `HostName`, `%h` expanded, or the alias itself; `None`
/// when a token needs running state.
fn host_name(alias: &str, block: &Block, diagnostics: &mut Vec<Diagnostic>) -> Option<String> {
    let Some((value, line)) = block
        .host_name
        .as_ref()
        .filter(|(value, _)| !value.trim().is_empty())
    else {
        diagnostics.push(Diagnostic {
            level: Level::Info,
            line: block.line,
            code: Code::HostNameFallbackToAlias,
            context: Some(alias.to_owned()),
        });
        return Some(alias.to_owned());
    };
    let expanded = expand_tokens(value, alias);
    if expanded.is_none() {
        diagnostics.push(Diagnostic {
            level: Level::Warning,
            line: *line,
            code: Code::HostNameToken,
            context: Some(value.clone()),
        });
    }
    expanded
}

/// `value` with `%h` as `alias` and `%%` as `%`; `None` for any other token.
fn expand_tokens(value: &str, alias: &str) -> Option<String> {
    let mut expanded = String::with_capacity(value.len() + alias.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != TOKEN {
            expanded.push(c);
            continue;
        }
        match chars.next() {
            Some(TOKEN) => expanded.push(TOKEN),
            Some(ALIAS_TOKEN) => expanded.push_str(alias),
            _ => return None,
        }
    }
    Some(expanded)
}

/// The gateways on the way to `alias`, from its block's `ProxyJump`; none, said, when it
/// cannot be imported.
fn chain(
    alias: &str,
    host_name: &str,
    block: &Block,
    by_alias: &HashMap<String, &Block>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Hop> {
    let mut say = |line: usize, code, context: &str| {
        diagnostics.push(Diagnostic {
            level: Level::Warning,
            line,
            code,
            context: Some(context.to_owned()),
        });
    };
    let jump = block
        .proxy_jump
        .as_ref()
        .filter(|(value, ..)| !value.trim().is_empty());
    let Some((value, line, quoted)) = jump else {
        if let Some((command, line)) = block
            .proxy_command
            .as_ref()
            .filter(|(value, _)| !value.trim().is_empty())
        {
            say(*line, Code::ProxyCommandUnsupported, command);
        }
        return Vec::new();
    };
    if block
        .proxy_command
        .as_ref()
        .is_some_and(|(value, _)| !value.trim().is_empty())
    {
        say(*line, Code::ProxyJumpWithProxyCommand, value);
        return Vec::new();
    }
    if value.eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    if value.contains(TOKEN) {
        say(*line, Code::ProxyJumpToken, value);
        return Vec::new();
    }
    let Some(raw) = parse_jump(value, *quoted) else {
        say(*line, Code::ProxyJumpSyntax, value);
        return Vec::new();
    };
    if has_cycle(alias, host_name, &raw, by_alias) {
        say(*line, Code::ProxyJumpCycle, alias);
        return Vec::new();
    }
    raw.into_iter().map(|hop| resolve(hop, by_alias)).collect()
}

/// A hop with the settings of the block its host names, when it names one.
fn resolve(raw: RawHop, by_alias: &HashMap<String, &Block>) -> Hop {
    let Some(block) = by_alias.get(&raw.host.to_lowercase()) else {
        return Hop {
            host_name: raw.host.clone(),
            port: raw.port.unwrap_or(DEFAULT_SSH_PORT),
            user: raw.user,
            identity_file: None,
            host: raw.host,
        };
    };
    let host_name = match &block.host_name {
        Some((value, _)) if !value.trim().is_empty() => {
            expand_tokens(value, &raw.host).unwrap_or_else(|| value.clone())
        }
        _ => raw.host.clone(),
    };
    Hop {
        host_name,
        port: raw.port.unwrap_or_else(|| block.port()),
        user: raw
            .user
            .filter(|user| !user.trim().is_empty())
            .or_else(|| block.user.clone()),
        identity_file: block.identity_file.clone(),
        host: raw.host,
    }
}

/// Whether the chain of `alias` passes twice by the same host, or comes back to `alias`
/// through the chains of the blocks it names.
fn has_cycle(
    alias: &str,
    host_name: &str,
    hops: &[RawHop],
    by_alias: &HashMap<String, &Block>,
) -> bool {
    let mut seen: HashSet<String> = [alias.to_lowercase(), host_name.to_lowercase()].into();
    for hop in hops {
        if !seen.insert(hop.host.to_lowercase()) {
            return true;
        }
        if let Some(block) = by_alias.get(&hop.host.to_lowercase()) {
            let named = block
                .host_name
                .as_ref()
                .filter(|(value, _)| !value.trim().is_empty());
            if let Some((name, _)) = named
                && !seen.insert(name.to_lowercase())
            {
                return true;
            }
            if comes_back(alias, block, by_alias, &mut Vec::new()) {
                return true;
            }
        }
    }
    false
}

/// Whether following `block`'s own chain reaches `root`.
fn comes_back<'a>(
    root: &str,
    block: &'a Block,
    by_alias: &HashMap<String, &'a Block>,
    visited: &mut Vec<&'a Block>,
) -> bool {
    if visited.iter().any(|seen| std::ptr::eq(*seen, block)) {
        return false;
    }
    visited.push(block);
    let Some((value, _, quoted)) = &block.proxy_jump else {
        return false;
    };
    if value.eq_ignore_ascii_case("none") {
        return false;
    }
    let Some(hops) = parse_jump(value, *quoted) else {
        return false;
    };
    hops.iter().any(|hop| {
        hop.host.eq_ignore_ascii_case(root)
            || by_alias
                .get(&hop.host.to_lowercase())
                .is_some_and(|next| comes_back(root, next, by_alias, visited))
    })
}

/// `[user@]host[:port]` hops, comma separated; `None` for anything else.
fn parse_jump(value: &str, quoted: bool) -> Option<Vec<RawHop>> {
    if quoted || value.trim().is_empty() || has_unsafe_character(value) {
        return None;
    }
    value.split(',').map(parse_hop).collect()
}

fn has_unsafe_character(value: &str) -> bool {
    value.chars().any(char::is_whitespace) || value.contains(['"', '\'', '\\'])
}

fn parse_hop(value: &str) -> Option<RawHop> {
    if value.is_empty() || has_unsafe_character(value) || value.contains(TOKEN) {
        return None;
    }
    let (user, host_port) = match value.split_once('@') {
        Some((user, rest)) if !user.is_empty() && !rest.is_empty() && !rest.contains('@') => {
            (Some(user.to_owned()), rest)
        }
        Some(_) => return None,
        None => (None, value),
    };
    let (host, port) = match host_port.split_once(':') {
        Some((host, port)) => {
            let port = port.parse::<u16>().ok().filter(|port| *port != 0)?;
            (host, Some(port))
        }
        None => (host_port, None),
    };
    (!host.is_empty()).then(|| RawHop {
        host: host.to_owned(),
        user,
        port,
    })
}

/// A line without its comment: `#` outside double quotes ends it.
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (index, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..index],
            _ => {}
        }
    }
    line
}

/// The directive and its value; `Keyword value`, `Keyword=value` or `Keyword = value`, as
/// OpenSSH reads them. A value in double quotes loses them, and says it had them.
fn split_directive(line: &str) -> Option<(String, String, bool)> {
    if line.is_empty() {
        return None;
    }
    let end = line.find([' ', '\t', '=']).unwrap_or(line.len());
    let directive = &line[..end];
    if directive.is_empty() {
        return None;
    }
    let rest = line[end..].trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest).trim();
    let quoted = rest.len() >= 2 && rest.starts_with('"') && rest.ends_with('"');
    let value = if quoted {
        &rest[1..rest.len() - 1]
    } else {
        rest
    };
    Some((directive.to_owned(), value.to_owned(), quoted))
}

/// Space-separated words; double quotes group words and are dropped.
fn split_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    for c in value.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if c.is_whitespace() && !quoted {
            if !word.is_empty() {
                tokens.push(std::mem::take(&mut word));
            }
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    tokens
}

fn is_wildcard(alias: &str) -> bool {
    alias.starts_with('!') || alias.contains(['*', '?'])
}

/// Whether a candidate is new, or its name is already a profile's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// No profile has its name.
    New,
    /// A profile has its name: not imported, as the C# import leaves it.
    Duplicate,
    /// No host to reach, as a `PuTTY` session can be: never imported.
    Invalid,
}

/// A gateway on the way to a candidate, as the preview shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayStep {
    /// Host.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Account.
    pub user: Option<String>,
    /// The saved gateway it will be, when one already is.
    pub reused: Option<String>,
}

/// A candidate with what the import would do with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assessment {
    /// The candidate.
    pub candidate: Candidate,
    /// New or already there.
    pub status: Status,
    /// The gateways on its way.
    pub gateways: Vec<GatewayStep>,
}

/// What importing the chosen candidates adds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// The new profiles.
    pub profiles: Vec<SshProfile>,
    /// The new gateways, each after its parent.
    pub gateways: Vec<SshGateway>,
    /// The names left out: a profile already has them.
    pub duplicates: Vec<String>,
    /// The names left out: no host to reach.
    pub invalid: Vec<String>,
}

/// Finds and makes the gateways of chains: a saved one when host, port, account and parent
/// are the same, one made earlier in the same import likewise, a new one otherwise, named
/// `user@host` or `host`, unique among gateway names.
struct Gateways<'a> {
    saved: &'a [SshGateway],
    made: Vec<SshGateway>,
    names: HashSet<String>,
}

impl<'a> Gateways<'a> {
    fn new(saved: &'a [SshGateway]) -> Self {
        Self {
            saved,
            made: Vec::new(),
            names: saved
                .iter()
                .map(|gateway| gateway.name.to_lowercase())
                .collect(),
        }
    }

    /// The last gateway of `hops`, and the steps on the way; `new_id` names the made ones.
    fn walk(
        &mut self,
        hops: &[Hop],
        new_id: &mut dyn FnMut() -> ProfileId,
    ) -> (Option<ProfileId>, Vec<GatewayStep>) {
        let mut parent: Option<ProfileId> = None;
        let mut steps = Vec::new();
        for hop in hops {
            let user = hop
                .user
                .as_deref()
                .map(str::trim)
                .filter(|user| !user.is_empty());
            let same = |gateway: &&SshGateway| {
                gateway.host.eq_ignore_ascii_case(&hop.host_name)
                    && gateway.port == hop.port
                    && gateway
                        .username
                        .as_deref()
                        .map(str::trim)
                        .filter(|u| !u.is_empty())
                        == user
                    && gateway.parent == parent
            };
            let reused = self
                .saved
                .iter()
                .find(same)
                .map(|gateway| gateway.name.clone());
            let id = if let Some(gateway) = self.saved.iter().chain(&self.made).find(same) {
                gateway.id.clone()
            } else {
                let id = new_id();
                let base = user.map_or_else(
                    || hop.host_name.clone(),
                    |user| format!("{user}@{}", hop.host_name),
                );
                let name = self.unique(&base);
                self.made.push(SshGateway {
                    id: id.clone(),
                    name,
                    host: hop.host_name.clone(),
                    port: hop.port,
                    username: user.map(str::to_owned),
                    key_path: hop.identity_file.as_ref().map(PathBuf::from),
                    parent: parent.clone(),
                });
                id
            };
            steps.push(GatewayStep {
                host: hop.host_name.clone(),
                port: hop.port,
                user: user.map(str::to_owned),
                reused,
            });
            parent = Some(id);
        }
        (parent, steps)
    }

    /// `base`, or `base (2)`, `base (3)`... the first no gateway has.
    fn unique(&mut self, base: &str) -> String {
        let mut name = base.to_owned();
        let mut suffix = 2_u32;
        while !self.names.insert(name.to_lowercase()) {
            name = format!("{base} ({suffix})");
            suffix += 1;
        }
        name
    }
}

/// What the import would do with each candidate, against the saved profile names and
/// gateways, as the C# preview shows it.
#[must_use]
pub fn assess<S: BuildHasher>(
    candidates: &[Candidate],
    names: &HashSet<String, S>,
    gateways: &[SshGateway],
) -> Vec<Assessment> {
    let mut planner = Gateways::new(gateways);
    let mut count = 0_u64;
    let mut placeholder = || {
        count += 1;
        ProfileId::new(format!("planned-{count}"))
    };
    candidates
        .iter()
        .map(|candidate| Assessment {
            status: if candidate.host_name.trim().is_empty() {
                Status::Invalid
            } else if names.contains(&candidate.alias.to_lowercase()) {
                Status::Duplicate
            } else {
                Status::New
            },
            gateways: planner.walk(&candidate.proxy_jump, &mut placeholder).1,
            candidate: candidate.clone(),
        })
        .collect()
}

/// The profiles and gateways importing `chosen` adds, a name already taken left out;
/// `names` are the saved profiles' names, lowercase.
pub fn plan<S: BuildHasher>(
    chosen: &[Candidate],
    names: &HashSet<String, S>,
    gateways: &[SshGateway],
    new_id: &mut dyn FnMut() -> ProfileId,
) -> Plan {
    let mut taken = HashSet::new();
    let mut planner = Gateways::new(gateways);
    let mut plan = Plan::default();
    for candidate in chosen {
        if candidate.host_name.trim().is_empty() {
            plan.invalid.push(candidate.alias.clone());
            continue;
        }
        let name = candidate.alias.to_lowercase();
        if names.contains(&name) || !taken.insert(name) {
            plan.duplicates.push(candidate.alias.clone());
            continue;
        }
        let (gateway, _) = planner.walk(&candidate.proxy_jump, new_id);
        plan.profiles.push(SshProfile {
            id: new_id(),
            name: candidate.alias.clone(),
            group: None,
            host: candidate.host_name.clone(),
            port: candidate.port,
            username: candidate
                .user
                .clone()
                .filter(|user| !user.trim().is_empty()),
            key_path: candidate
                .identity_file
                .as_ref()
                .filter(|path| !path.trim().is_empty())
                .map(PathBuf::from),
            gateway,
            vault_entry: None,
            forwards: Forwards::default(),
            post_connect: PostConnect::default(),
            forward_agent: false,
            compression: false,
            sftp: false,
            legacy_algorithms: false,
            session_logging: None,
        });
    }
    plan.gateways = planner.made;
    plan
}
