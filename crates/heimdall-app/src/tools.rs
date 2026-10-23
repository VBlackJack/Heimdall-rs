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

//! The built-in tools, as the C# `ToolRegistry` (`Services/ToolRegistry.cs:55-124`) lists
//! them: one entry per tool, its C# identifier, its category, the words that find it and
//! whether it reaches a host. Only the tools ported are listed: a tool appears in the
//! sidebar, on the Tools page and in a tab once it has its engine and its view.
//!
//! Adding a tool: a [`ToolId`] variant, its place in [`ToolId::ALL`] (the C# order) and its
//! [`ToolId::descriptor`] arm here; its label, description, icon and view in the window
//! crate, whose matches over [`ToolId`] say where.

use heimdall_core::profile::RemoteOs;

/// The categories of the tools, in the C# `ToolCategory` order (`Enums.cs:108-115`): the
/// order the sidebar and the Tools page group them in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolCategory {
    /// Network tools.
    Network,
    /// Security tools.
    Security,
    /// Encoding and format tools.
    Encoding,
    /// System tools.
    System,
    /// Tools of this computer and other programs.
    External,
}

impl ToolCategory {
    /// Every category, in order.
    pub const ALL: [Self; 5] = [
        Self::Network,
        Self::Security,
        Self::Encoding,
        Self::System,
        Self::External,
    ];

    /// The name a category is kept under in the settings, folded or not.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Security => "security",
            Self::Encoding => "encoding",
            Self::System => "system",
            Self::External => "external",
        }
    }
}

/// A group of the sidebar's Tools tab: the tools pinned first, then each category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolGroup {
    /// The tools pinned, as the C# "Favorites" category.
    Favorites,
    /// A category's tools.
    Category(ToolCategory),
}

/// The name the pinned tools' group is kept under in the settings, folded or not.
pub const FAVORITES_GROUP_KEY: &str = "favorites";

impl ToolGroup {
    /// The name it is kept under in the settings, folded or not.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Favorites => FAVORITES_GROUP_KEY,
            Self::Category(category) => category.key(),
        }
    }
}

/// A built-in tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolId {
    /// The subnet calculator, the C# `SUBNET`.
    SubnetCalculator,
    /// The IP address converter, the C# `IPCONV`.
    IpConverter,
    /// The network calculator, the C# `NETCALC`.
    NetworkCalculator,
    /// The hash generator, the C# `HASH`.
    Hash,
    /// The HMAC generator, the C# `HMAC`.
    Hmac,
    /// The password generator, the C# `PASSWORD`.
    Password,
    /// The SSH key generator, the C# `SSHKEY`.
    SshKey,
    /// The certificate generator, the C# `CERTGEN`.
    CertGen,
    /// The JWT parser, the C# `JWT`.
    Jwt,
    /// The TOTP generator, the C# `TOTP`.
    Totp,
    /// The password audit, the C# `PWDAUDIT`.
    PwdAudit,
    /// The Base64 encoder and decoder, the C# `BASE64`.
    Base64,
    /// The URL encoder and decoder, the C# `URLENC`.
    UrlEncoder,
    /// The JSON formatter, the C# `JSON`.
    JsonFormatter,
    /// The regular expression tester, the C# `REGEX`.
    RegexTester,
    /// The text comparison, the C# `DIFF`.
    TextDiff,
    /// The text case converter, the C# `TEXTCASE`.
    TextCase,
    /// The chmod calculator, the C# `CHMOD`.
    Chmod,
    /// The date and time converter, the C# `DATETIME`.
    DateTime,
    /// The UUID generator, the C# `UUID`.
    Uuid,
    /// The ULID generator, the C# `ULID`.
    Ulid,
    /// The crontab builder, the C# `CRONTAB`.
    Crontab,
    /// The SSH config generator, the C# `SSHCONFIG`.
    SshConfig,
    /// The SSH key auditor, the C# `SSHAUDIT`.
    SshAudit,
    /// The TLS auditor, the C# `TLSAUDIT`.
    TlsAudit,
    /// The DNS security checker, the C# `DNSSEC`.
    DnsSec,
    /// The default credentials list, the C# `DEFAULTCREDS`. List-only, unlike the
    /// C# tool: see `heimdall_core::tools::default_creds`.
    DefaultCreds,
    /// The offline CVE lookup engine, the C# `CVELOOKUP`.
    CveLookup,
    /// The `SecNumCloud` v3.2 compliance audit, the C# `SECNUMCLOUD`.
    SecNumCloud,
    /// The SMB protocol enumerator, the C# `SMBENUM`.
    Smb,
    /// The HTTP status codes reference, the C# `HTTP`.
    HttpStatus,
    /// The TCP ping latency probe, the C# `TCPPING`.
    TcpPing,
    /// The Wake-on-LAN magic packet tool, the C# `WOL`.
    WakeOnLan,
    /// The WHOIS domain lookup tool, the C# `WHOIS`.
    Whois,
    /// The TCP banner grabber, the C# `BANNER`.
    BannerGrabber,
    /// The ICMP ping latency monitor, the C# `PING`.
    Ping,
    /// The DNS record lookup tool, the C# `DNS`.
    Dns,
    /// The batch DNS resolver, the C# `DNSBATCH`.
    DnsBatch,
    /// The TCP port scanner, the C# `PORTSCAN`.
    PortScan,
    /// The HTTP header analyzer and security scorer, the C# `HTTPHEADERS`.
    HttpHeaders,
    /// The SSL/TLS certificate inspector, the C# `CERT`.
    Cert,
    /// The TCP traceroute path discovery tool, the C# `TCPTRACE`.
    TcpTrace,
    /// The SNMP MIB tree walker, the C# `SNMPWALK`.
    SnmpWalk,
    /// The ARP table anomaly monitor, the C# `ARPMON`.
    ArpMon,
    /// The firewall egress connectivity tester, the C# `FWTEST`.
    FwTest,
    /// The network topology and cartography scanner, the C# `NETMAP`.
    NetMap,
    /// The open TCP/UDP ports and sockets monitor, the C# `OPENPORTS`.
    OpenPorts,
    /// The network interfaces snapshot and speed inspector, the C# `NETIF`.
    NetIf,
    /// The IPv4 routing table inspector, the C# `ROUTES`.
    Routes,
    /// The nearby Wi-Fi network scanner, the C# `WIFI`.
    Wifi,
    /// The system hosts file editor, the C# `HOSTS`.
    Hosts,
    /// The Windows service status dashboard and controller, the C# `SERVICES`.
    Services,
    /// The log file viewer with tailing and regex filter, the C# `LOGVIEW`.
    LogView,
    /// The cron job manager and task scheduler, the C# `CRONJOB`.
    CronJob,
    /// The notes and scratchpad editor, the C# `NOTES`.
    Notes,
    /// The architecture and topology diagram editor, the C# `DIAGRAM`.
    Diagram,
    /// The command snippet and script library, the C# `CMDLIB`.
    CmdLib,
}

/// What the registry says of a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolDescriptor {
    /// The C# identifier: what the favourites keep, as the C# `FavoriteToolIds`.
    pub code: &'static str,
    /// Its category.
    pub category: ToolCategory,
    /// The words that find it besides its name, as the C# `CommandPrefixes`.
    pub prefixes: &'static [&'static str],
    /// Whether it reaches a host, the selected session's inherited when it opens, as the
    /// C# `IsNetworkTool`: such a tool opens in a new tab each time.
    pub network: bool,
}

impl ToolDescriptor {
    /// The entry of a tool that reaches no host.
    const fn local(
        code: &'static str,
        category: ToolCategory,
        prefixes: &'static [&'static str],
    ) -> Self {
        Self {
            code,
            category,
            prefixes,
            network: false,
        }
    }

    /// The entry of a tool that reaches a host.
    const fn network(
        code: &'static str,
        category: ToolCategory,
        prefixes: &'static [&'static str],
    ) -> Self {
        Self {
            code,
            category,
            prefixes,
            network: true,
        }
    }
}

impl ToolId {
    /// Every tool ported, in the C# registry's order.
    pub const ALL: [Self; 57] = [
        Self::Ping,
        Self::Dns,
        Self::Cert,
        Self::PortScan,
        Self::SubnetCalculator,
        Self::IpConverter,
        Self::HttpStatus,
        Self::Whois,
        Self::HttpHeaders,
        Self::BannerGrabber,
        Self::TcpTrace,
        Self::SnmpWalk,
        Self::ArpMon,
        Self::FwTest,
        Self::NetMap,
        Self::NetworkCalculator,
        Self::TcpPing,
        Self::Hash,
        Self::Hmac,
        Self::Password,
        Self::SshKey,
        Self::CertGen,
        Self::Jwt,
        Self::Totp,
        Self::PwdAudit,
        Self::SshAudit,
        Self::TlsAudit,
        Self::DnsSec,
        Self::Smb,
        Self::DefaultCreds,
        Self::CveLookup,
        Self::SecNumCloud,
        Self::Base64,
        Self::UrlEncoder,
        Self::JsonFormatter,
        Self::RegexTester,
        Self::TextDiff,
        Self::TextCase,
        Self::Chmod,
        Self::DateTime,
        Self::Uuid,
        Self::Ulid,
        Self::Crontab,
        Self::LogView,
        Self::Hosts,
        Self::SshConfig,
        Self::CronJob,
        Self::Services,
        Self::Notes,
        Self::Diagram,
        Self::CmdLib,
        Self::WakeOnLan,
        Self::OpenPorts,
        Self::NetIf,
        Self::Routes,
        Self::DnsBatch,
        Self::Wifi,
    ];

    /// Its entry, as the C# registry's (`ToolRegistry.cs:62-109`).
    #[must_use]
    #[expect(clippy::too_many_lines, reason = "one arm per tool")]
    pub const fn descriptor(self) -> ToolDescriptor {
        match self {
            Self::SubnetCalculator => {
                ToolDescriptor::local("SUBNET", ToolCategory::Network, &["subnet"])
            }
            Self::IpConverter => {
                ToolDescriptor::local("IPCONV", ToolCategory::Network, &["ip", "ipconv"])
            }
            Self::NetworkCalculator => ToolDescriptor::local(
                "NETCALC",
                ToolCategory::Network,
                &["netcalc", "vlan", "supernet"],
            ),
            Self::Hash => ToolDescriptor::local("HASH", ToolCategory::Security, &["hash"]),
            Self::Hmac => ToolDescriptor::local("HMAC", ToolCategory::Security, &["hmac"]),
            Self::Password => {
                ToolDescriptor::local("PASSWORD", ToolCategory::Security, &["password", "pwgen"])
            }
            Self::SshKey => {
                ToolDescriptor::local("SSHKEY", ToolCategory::Security, &["sshkey", "keygen"])
            }
            Self::CertGen => ToolDescriptor::local(
                "CERTGEN",
                ToolCategory::Security,
                &["certgen", "certificate", "openssl"],
            ),
            Self::Jwt => ToolDescriptor::local("JWT", ToolCategory::Security, &["jwt"]),
            Self::Totp => {
                ToolDescriptor::local("TOTP", ToolCategory::Security, &["totp", "otp", "2fa"])
            }
            Self::PwdAudit => ToolDescriptor::local(
                "PWDAUDIT",
                ToolCategory::Security,
                &["pwdaudit", "password-audit", "passcheck"],
            ),
            Self::SshAudit => ToolDescriptor::local(
                "SSHAUDIT",
                ToolCategory::Security,
                &["sshaudit", "keyaudit"],
            ),
            Self::TlsAudit => ToolDescriptor::network(
                "TLSAUDIT",
                ToolCategory::Security,
                &["tlsaudit", "sslaudit"],
            ),
            Self::DnsSec => ToolDescriptor::network(
                "DNSSEC",
                ToolCategory::Security,
                &["dnssec", "spf", "dmarc", "dkim"],
            ),
            Self::Smb => ToolDescriptor::network(
                "SMBENUM",
                ToolCategory::Security,
                &["smb", "ntlm", "netbios"],
            ),
            // List-only by owner decision (2026-10-17, item 10): it never reaches a
            // host, unlike the C# tool it mirrors the catalog of.
            Self::DefaultCreds => ToolDescriptor::local(
                "DEFAULTCREDS",
                ToolCategory::Security,
                &["defaultcreds", "creds"],
            ),
            Self::CveLookup => ToolDescriptor::local(
                "CVELOOKUP",
                ToolCategory::Security,
                &["cve", "vuln", "vulnerability"],
            ),
            Self::SecNumCloud => ToolDescriptor::local(
                "SECNUMCLOUD",
                ToolCategory::Security,
                &["secnumcloud", "audit", "compliance", "anssi"],
            ),
            Self::Base64 => ToolDescriptor::local("BASE64", ToolCategory::Encoding, &["base64"]),
            Self::UrlEncoder => {
                ToolDescriptor::local("URLENC", ToolCategory::Encoding, &["url", "urlencode"])
            }
            Self::JsonFormatter => ToolDescriptor::local("JSON", ToolCategory::Encoding, &["json"]),
            Self::RegexTester => ToolDescriptor::local("REGEX", ToolCategory::Encoding, &["regex"]),
            Self::TextDiff => ToolDescriptor::local("DIFF", ToolCategory::Encoding, &["diff"]),
            Self::TextCase => {
                ToolDescriptor::local("TEXTCASE", ToolCategory::Encoding, &["case", "textcase"])
            }
            Self::Chmod => ToolDescriptor::local("CHMOD", ToolCategory::System, &["chmod"]),
            Self::DateTime => {
                ToolDescriptor::local("DATETIME", ToolCategory::System, &["datetime", "epoch"])
            }
            Self::Uuid => ToolDescriptor::local("UUID", ToolCategory::System, &["uuid", "guid"]),
            Self::Ulid => ToolDescriptor::local("ULID", ToolCategory::System, &["ulid"]),
            Self::Crontab => {
                ToolDescriptor::local("CRONTAB", ToolCategory::System, &["cron", "crontab"])
            }
            Self::SshConfig => ToolDescriptor::local(
                "SSHCONFIG",
                ToolCategory::System,
                &["sshconfig", "ssh-config"],
            ),
            Self::HttpStatus => {
                ToolDescriptor::local("HTTP", ToolCategory::Network, &["http", "status"])
            }
            Self::Whois => ToolDescriptor::network("WHOIS", ToolCategory::Network, &["whois"]),
            Self::BannerGrabber => {
                ToolDescriptor::network("BANNER", ToolCategory::Network, &["banner", "grab"])
            }
            Self::TcpPing => ToolDescriptor::network(
                "TCPPING",
                ToolCategory::Network,
                &["tcpping", "tcping", "portping"],
            ),
            Self::WakeOnLan => {
                ToolDescriptor::local("WOL", ToolCategory::External, &["wol", "wake", "wakeonlan"])
            }
            Self::Ping => ToolDescriptor::network("PING", ToolCategory::Network, &["ping"]),
            Self::Dns => {
                ToolDescriptor::network("DNS", ToolCategory::Network, &["dns", "nslookup", "dig"])
            }
            Self::DnsBatch => ToolDescriptor::local(
                "DNSBATCH",
                ToolCategory::External,
                &["dnsbatch", "resolve", "resolver"],
            ),
            Self::PortScan => {
                ToolDescriptor::network("PORTSCAN", ToolCategory::Network, &["portscan", "scan"])
            }
            Self::HttpHeaders => ToolDescriptor::network(
                "HTTPHEADERS",
                ToolCategory::Network,
                &["headers", "httpheaders", "secheaders"],
            ),
            Self::Cert => ToolDescriptor::network("CERT", ToolCategory::Network, &["cert", "ssl"]),
            Self::TcpTrace => ToolDescriptor::network(
                "TCPTRACE",
                ToolCategory::Network,
                &["trace", "traceroute", "tracert"],
            ),
            Self::SnmpWalk => {
                ToolDescriptor::network("SNMPWALK", ToolCategory::Network, &["snmp", "snmpwalk"])
            }
            Self::ArpMon => {
                ToolDescriptor::local("ARPMON", ToolCategory::Network, &["arp", "arpmon"])
            }
            Self::FwTest => ToolDescriptor::local(
                "FWTEST",
                ToolCategory::Network,
                &["fw", "firewall", "fwtest"],
            ),
            Self::NetMap => ToolDescriptor::local(
                "NETMAP",
                ToolCategory::Network,
                &["netmap", "cartography", "discover"],
            ),
            Self::OpenPorts => ToolDescriptor::local(
                "OPENPORTS",
                ToolCategory::External,
                &["openports", "ports", "netstat", "connections"],
            ),
            Self::NetIf => ToolDescriptor::local(
                "NETIF",
                ToolCategory::External,
                &["netif", "interfaces", "nic", "adapter"],
            ),
            Self::Routes => ToolDescriptor::local(
                "ROUTES",
                ToolCategory::External,
                &["routes", "route", "routing"],
            ),
            Self::Wifi => ToolDescriptor::local(
                "WIFI",
                ToolCategory::External,
                &["wifi", "wlan", "ssid", "wireless"],
            ),
            Self::Hosts => ToolDescriptor::local("HOSTS", ToolCategory::System, &["hosts"]),
            Self::Services => ToolDescriptor::local(
                "SERVICES",
                ToolCategory::System,
                &["services", "svc", "systemctl"],
            ),
            Self::LogView => {
                ToolDescriptor::local("LOGVIEW", ToolCategory::System, &["log", "tail", "logview"])
            }
            Self::CronJob => ToolDescriptor::local(
                "CRONJOB",
                ToolCategory::System,
                &["cronjob", "crontab-manager", "tasks"],
            ),
            Self::Notes => ToolDescriptor::local(
                "NOTES",
                ToolCategory::System,
                &["notes", "note", "markdown", "md", "confluence"],
            ),
            Self::Diagram => ToolDescriptor::local(
                "DIAGRAM",
                ToolCategory::System,
                &["diagram", "drawio", "schema"],
            ),
            Self::CmdLib => ToolDescriptor::local(
                "CMDLIB",
                ToolCategory::System,
                &[
                    "cmd",
                    "command",
                    "library",
                    "snippet",
                    "twinshell",
                    "cheat",
                    "cheatsheet",
                    "powershell",
                    "bash",
                ],
            ),
        }
    }

    /// The C# identifier.
    #[must_use]
    pub const fn code(self) -> &'static str {
        self.descriptor().code
    }

    /// Its category.
    #[must_use]
    pub const fn category(self) -> ToolCategory {
        self.descriptor().category
    }

    /// The words that find it besides its name.
    #[must_use]
    pub const fn prefixes(self) -> &'static [&'static str] {
        self.descriptor().prefixes
    }

    /// Whether it reaches a host.
    #[must_use]
    pub const fn is_network(self) -> bool {
        self.descriptor().network
    }

    /// The tool of C# identifier `code`, whatever its case, as the C# registry looks it up;
    /// the C# `TOOL:` prefix is taken off first.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        let code = code.trim();
        let code = code
            .get(..TOOL_PREFIX.len())
            .filter(|prefix| prefix.eq_ignore_ascii_case(TOOL_PREFIX))
            .map_or(code, |_| &code[TOOL_PREFIX.len()..]);
        Self::ALL
            .into_iter()
            .find(|tool| tool.code().eq_ignore_ascii_case(code))
    }
}

/// The prefix the C# writes before a tool's identifier in a connection type
/// (`ConnectionTypeCatalog.ToolPrefix`).
const TOOL_PREFIX: &str = "TOOL:";

/// What a tool opens on, as the C# `ToolContext` (`ToolContext.cs:26-46`): the host it
/// reaches, its port, and the text typed after its command word in the palette. Only the
/// window reads it, each tool as its C# view's `Initialize` does: a host or an argument
/// fills a box, as typed there, and goes through the checks a typed one goes through.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolContext {
    /// The host, as the C# `TargetHost`.
    pub host: Option<String>,
    /// The port, as the C# `TargetPort`.
    pub port: Option<u16>,
    /// The text typed after the tool's command word, as the C# `Argument`: plain text,
    /// never run.
    pub argument: Option<String>,
    /// The account of the session the tool was opened from, as the C# `Username`.
    pub user: Option<String>,
    /// The platform of that session's far end, as the C# `RemoteOs`: what the profile
    /// declares, else the protocol's guess; `Auto` when it cannot be told.
    pub remote_os: RemoteOs,
}

impl ToolContext {
    /// A context on `host`, trimmed; empty when nothing is left.
    #[must_use]
    pub fn on_host(host: &str) -> Self {
        let host = host.trim();
        Self {
            host: (!host.is_empty()).then(|| host.to_owned()),
            ..Self::default()
        }
    }

    /// A context holding `argument`, trimmed, as the palette's "ping 10.0.0.1" gives it;
    /// empty when nothing is left.
    #[must_use]
    pub fn with_argument(argument: &str) -> Self {
        let argument = argument.trim();
        Self {
            argument: (!argument.is_empty()).then(|| argument.to_owned()),
            ..Self::default()
        }
    }

    /// A context carrying the facts of a session: its host and account, trimmed and left out
    /// when blank, and the platform of its far end.
    #[must_use]
    pub fn of_session(host: Option<&str>, user: Option<&str>, remote_os: RemoteOs) -> Self {
        let given = |value: Option<&str>| {
            value
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        Self {
            host: given(host),
            user: given(user),
            remote_os,
            ..Self::default()
        }
    }

    /// Whether it holds neither a host nor an argument nor a session's facts, as the C#
    /// `hasContext` reads it (`MainViewModel.cs:1305`): such a tool reaching no host shows its
    /// open tab again.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let blank = |value: &Option<String>| value.as_deref().is_none_or(|v| v.trim().is_empty());
        blank(&self.host) && blank(&self.argument) && blank(&self.user) && self.remote_os.is_auto()
    }

    /// What its title names after the tool's: the host, else the argument.
    #[must_use]
    pub fn shown(&self) -> Option<&str> {
        self.host.as_deref().or(self.argument.as_deref())
    }
}

/// `query` read as a tool and its argument, as the C# `TryParseExplicitToolInvocation`
/// (`CommandPaletteViewModel.Search.cs:328-353`): one of a tool's command words, whatever
/// its case, a space, then something; the first tool in the registry's order whose word
/// matches wins. A word alone is not an invocation, so "ping" is still scored as before.
#[must_use]
pub fn tool_invocation(query: &str) -> Option<(ToolId, String)> {
    let query = query.trim();
    ToolId::ALL.into_iter().find_map(|tool| {
        tool.prefixes().iter().find_map(|prefix| {
            let typed = query.get(..prefix.len())?;
            let rest = query[prefix.len()..].strip_prefix(' ')?;
            let rest = rest.trim();
            (typed.eq_ignore_ascii_case(prefix) && !rest.is_empty())
                .then(|| (tool, rest.to_owned()))
        })
    })
}

/// Tools remembered as used lately, as the C# `RecentToolList.MaxEntries`.
pub const MAX_RECENT_TOOLS: usize = 5;

/// The tools used lately, the newest first, as the C# `RecentToolList`: for this run only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecentTools(Vec<ToolId>);

impl RecentTools {
    /// Records `tool` as the one used last: moved to the front if it was there, the oldest
    /// let go past [`MAX_RECENT_TOOLS`].
    pub fn track(&mut self, tool: ToolId) {
        self.0.retain(|known| *known != tool);
        self.0.insert(0, tool);
        self.0.truncate(MAX_RECENT_TOOLS);
    }

    /// The tools, the newest first.
    #[must_use]
    pub fn ids(&self) -> &[ToolId] {
        &self.0
    }
}

/// What pinning or unpinning a tool makes of the favourites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteToggle {
    /// The favourites after it, the caller's list untouched.
    pub favorites: Vec<String>,
    /// Whether the tool became a favourite.
    pub added: bool,
}

/// `favorites` with `tool` added at the end if absent, or removed if present, as the C#
/// `FavoriteToolSet.Toggle` (`FavoriteToolSet.cs:50-75`): membership whatever the case, the
/// identifier stored upper-cased, every spelling of it removed, the others' order kept.
#[must_use]
pub fn toggle_favorite(favorites: &[String], tool: ToolId) -> FavoriteToggle {
    let code = tool.code().to_ascii_uppercase();
    let mut kept = Vec::with_capacity(favorites.len() + 1);
    let mut removed = false;
    for favorite in favorites {
        if favorite.eq_ignore_ascii_case(&code) {
            removed = true;
        } else {
            kept.push(favorite.clone());
        }
    }
    if !removed {
        kept.push(code);
    }
    FavoriteToggle {
        favorites: kept,
        added: !removed,
    }
}

/// The tab the sidebar shows, as the C# segmented "Sessions | Tools".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarTab {
    /// The sessions' tree.
    #[default]
    Sessions,
    /// The tools, by category.
    Tools,
}

impl SidebarTab {
    /// The other one, as Ctrl+Shift+T flips them.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Sessions => Self::Tools,
            Self::Tools => Self::Sessions,
        }
    }
}
