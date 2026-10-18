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

//! The built-in tools as the window shows them: each tool's name, description and icon, as
//! the C# `ToolRegistry` and its locale keys give them; the state of each tool's tab, as the
//! C# tool views and their view models keep it; and how a tool's tab is drawn.
//!
//! A tool's tab is the core's ([`heimdall_app::tools`]): opened, shown and closed as any tab.
//! What is typed in it is the window's, as the integrated editor's text is: kept here by
//! tab, made when the tab opens and dropped when it closes.
//!
//! Adding a tool: its arms in [`label`], [`description`] and [`icon`], its pane module, and
//! its arms in [`Pane`], [`ToolPane::new`], [`ToolPanes::update`] (or `update_copying`, for
//! a tool whose update only copies) and [`view`].

#![allow(clippy::all, clippy::pedantic)]

mod arp_monitor_tool;
mod banner_tool;
mod base64_tool;
pub mod catalog;
mod cert_inspector_tool;
mod certgen_tool;
mod chmod_tool;
mod command_library_tool;
mod context;
mod cron_job_tool;
mod crontab_tool;
mod crypto_parts;
mod cve_tool;
mod datetime_tool;
mod defaultcreds_tool;
mod diagram_tool;
mod diff_tool;
mod dns_batch_tool;
mod dns_tool;
mod dnssec_tool;
pub mod file_export;
mod firewall_tester_tool;
mod hash_tool;
mod hmac_tool;
mod hosts_editor_tool;
mod http_headers_tool;
mod http_status_tool;
mod ip_converter_tool;
mod json_tool;
mod jwt_tool;
mod key_parts;
mod log_viewer_tool;
mod network_calculator_tool;
mod network_cartography_tool;
mod network_interfaces_tool;
mod network_run;
mod notes_tool;
mod open_ports_tool;
mod password_tool;
mod ping_tool;
mod placement_bar;
mod port_scan_tool;
mod pwdaudit_tool;
mod regex_tool;
mod route_bar;
mod route_table_tool;
mod secnumcloud_tool;
mod service_status_tool;
mod smb_tool;
mod snmp_walker_tool;
mod ssh_config_tool;
mod sshaudit_tool;
mod sshkey_tool;
mod subnet_tool;
mod system_tables;
mod tcp_ping_tool;
mod tcp_traceroute_tool;
mod text_case_tool;
mod tlsaudit_tool;
mod totp_tool;
mod ulid_tool;
mod url_tool;
mod uuid_tool;
mod whois_tool;
mod wifi_networks_tool;
mod wol_tool;

use std::collections::HashMap;
use std::time::Duration;

use heimdall_app::tool_route::RouteSupport;
use heimdall_app::tools::{ToolCategory, ToolGroup, ToolId};
use heimdall_app::{App, TabId};
use heimdall_core::tools::command_library::SnippetId;
use heimdall_core::tools::hash_computer::HashAlgorithm;
use iced::advanced::text::highlighter::PlainText;
use iced::widget::text_editor::{self, Action, Content};
use iced::widget::{Column, button, column, container, row, scrollable, text, tooltip};
use iced::{Color, Element, Length, Subscription, Task, Theme, window};

pub use arp_monitor_tool::ArpMonitorMessage;
pub use banner_tool::BannerMessage;
pub use base64_tool::Base64Message;
pub use cert_inspector_tool::CertInspectorMessage;
pub use certgen_tool::{
    CertCopy, CertGenMessage, CertSave, Generated as GeneratedCertificates, KeySizeChoice,
};
pub use chmod_tool::ChmodMessage;
pub use command_library_tool::CommandLibraryMessage;
pub use context::{inherited_context, inherited_host, tab_title, titled};
pub use cron_job_tool::CronJobMessage;
pub use crontab_tool::{CronOption, CrontabMessage};
pub use cve_tool::{CveMessage, Outcome as CveOutcome};
pub use datetime_tool::{DateField, DateTimeMessage, ZoneChoice};
pub use defaultcreds_tool::{DefaultCredsMessage, Outcome as DefaultCredsOutcome};
pub use diagram_tool::DiagramMessage;
pub use diff_tool::{Computed as DiffComputed, DiffMessage, compute as compute_diff};
pub use dns_batch_tool::DnsBatchMessage;
pub use dns_tool::DnsMessage;
pub use dnssec_tool::{DnsSecMessage, Outcome as DnsSecOutcome};
pub use firewall_tester_tool::FirewallTesterMessage;
pub use hash_tool::HashMessage;
pub use hmac_tool::HmacMessage;
pub use hosts_editor_tool::HostsMessage;
pub use http_headers_tool::HttpHeadersMessage;
pub use http_status_tool::HttpStatusMessage;
pub use ip_converter_tool::{IpConverterMessage, IpField};
pub use json_tool::JsonMessage;
pub use jwt_tool::JwtMessage;
pub use log_viewer_tool::LogViewerMessage;
pub use network_calculator_tool::{NetCalcField, NetCalcMessage, NetCalcMode};
pub use network_cartography_tool::NetworkCartographyMessage;
pub use network_interfaces_tool::NetIfMessage;
pub use notes_tool::NotesMessage;
pub use open_ports_tool::OpenPortsMessage;
pub use password_tool::{BuiltIn, PasswordCopy, PasswordMessage, PresetEntry};
pub use ping_tool::PingMessage;
pub use port_scan_tool::PortScanMessage;
pub use pwdaudit_tool::{PolicyChoice, PwdAuditMessage};
pub use regex_tool::RegexMessage;
pub use route_bar::RouteMessage;
pub use route_table_tool::RouteTableMessage;
pub use secnumcloud_tool::{Outcome as SecNumCloudOutcome, SecNumCloudMessage};
pub use service_status_tool::ServiceStatusMessage;
pub use smb_tool::{Outcome as SmbOutcome, SmbMessage};
pub use snmp_walker_tool::SnmpWalkerMessage;
pub use ssh_config_tool::{SshConfigMessage, SshField};
pub use sshaudit_tool::{
    LoadedFile as SshAuditLoadedFile, Outcome as SshAuditOutcome, SshAuditMessage,
};
pub use sshkey_tool::{AlgorithmChoice, SshCopy, SshKeyMessage, SshSave};
pub use subnet_tool::{SubnetField, SubnetMessage};
pub use tcp_ping_tool::TcpPingMessage;
pub use tcp_traceroute_tool::TcpTracerouteMessage;
pub use text_case_tool::TextCaseMessage;
pub use tlsaudit_tool::{Outcome as TlsAuditOutcome, TlsAuditMessage};
pub use totp_tool::TotpMessage;
pub use ulid_tool::UlidMessage;
pub use url_tool::UrlMessage;
pub use uuid_tool::UuidMessage;
pub use whois_tool::WhoisMessage;
pub use wifi_networks_tool::WifiMessage;
pub use wol_tool::WolMessage;

use crate::i18n::fl;
use crate::icons::Icon;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// How long a copy button shows its check mark, as the C# `CopyFeedbackHelper`'s
/// `FeedbackDuration`.
const COPY_FEEDBACK: Duration = Duration::from_secs(1);

/// What a copy button shows meanwhile, as the C# check mark.
const CHECK_MARK: &str = "\u{2713}";

/// What the help button shows, as the C# one: a sign, the same in every language.
const HELP_GLYPH: &str = "?";

/// Padding of a tool's header, as the C# `ToolHeaderPadding` (12 across, 8 down).
const HEADER_PADDING: [f32; 2] = [8.0, 12.0];

/// Side of the help button, as the C# `ToolHelpButtonStyle`.
const HELP_BUTTON_SIDE: f32 = 32.0;

/// Height of the help text before it scrolls, as the C# help panel's `ScrollViewer`.
const HELP_MAX_HEIGHT: f32 = 200.0;

/// Padding of the help panel, as the C# `PaddingSectionCard`.
const HELP_PADDING: f32 = 12.0;

/// Widest a tool's content is drawn, as the C# `ToolContentMaxWidth`.
const CONTENT_MAX_WIDTH: f32 = 700.0;

/// Widest a tool's content is drawn when it is a table of many columns: at
/// [`CONTENT_MAX_WIDTH`] the ARP monitor's seven columns left its vendor column about 60
/// pixels, its names drawn over the status.
const TABLE_MAX_WIDTH: f32 = 1100.0;

/// Padding inside a text box, as the C# `PaddingInput` (8 across, 6 down).
const INPUT_PADDING: [f32; 2] = [6.0, 8.0];

/// Padding of a tool's main buttons, as the C# `PaddingButtonPrimary` (16 across, 6 down).
const PRIMARY_PADDING: [f32; 2] = [6.0, 16.0];

/// Padding of a tool's copy buttons, as the C# `PaddingButtonCopy` (10 across, 4 down).
const COPY_PADDING: [f32; 2] = [4.0, 10.0];

/// The text of a tool's boxes: monospaced, as the C# Consolas.
const BOX_FONT: iced::Font = iced::Font::MONOSPACE;

/// Padding of a tool's content, as the C# `ContentAreaMargin`: none above, under the header.
const BODY_PADDING: iced::Padding = iced::Padding {
    top: 0.0,
    right: spacing::LG - spacing::XS,
    bottom: spacing::LG - spacing::XS,
    left: spacing::LG - spacing::XS,
};

/// Room around an empty state, as the C# `ToolEmptyStateStyle`'s padding.
const EMPTY_STATE_PADDING: f32 = 24.0;

/// How strongly the accent marks a match of the regular expression tester, as the C#'s
/// 80 of 255.
const REGEX_MATCH_ALPHA: f32 = 80.0 / 255.0;

/// How strongly the warning colour marks a match with a named group, as the C#'s 100 of
/// 255.
const REGEX_GROUP_ALPHA: f32 = 100.0 / 255.0;

/// How strongly a line removed or added is coloured, as the C# `Diff*LineBrush`'s 48 of 255.
const DIFF_LINE_ALPHA: f32 = 48.0 / 255.0;

/// How strongly a word removed or added is coloured, as the C# `Diff*WordBrush`'s 96 of 255.
const DIFF_WORD_ALPHA: f32 = 96.0 / 255.0;

/// A line break written into a tool's output, as .NET's `Environment.NewLine`.
#[cfg(windows)]
const NEW_LINE: &str = "\r\n";
#[cfg(not(windows))]
const NEW_LINE: &str = "\n";

/// The name of `tool`, as the C# `PaletteTool*` keys name it: its tab's title, its card's
/// and its row's in the sidebar.
#[must_use]
pub fn label(tool: ToolId) -> String {
    match tool {
        ToolId::SubnetCalculator => fl!("ui-tool-subnet-name"),
        ToolId::IpConverter => fl!("ui-tool-ipconv-name"),
        ToolId::NetworkCalculator => fl!("ui-tool-netcalc-name"),
        ToolId::Hash => fl!("ui-tool-hash-name"),
        ToolId::Hmac => fl!("ui-tool-hmac-name"),
        ToolId::Password => fl!("ui-tool-password-name"),
        ToolId::SshKey => fl!("ui-tool-sshkey-name"),
        ToolId::CertGen => fl!("ui-tool-certgen-name"),
        ToolId::PwdAudit => fl!("ui-tool-pwdaudit-name"),
        ToolId::Jwt => fl!("ui-tool-jwt-name"),
        ToolId::Totp => fl!("ui-tool-totp-name"),
        ToolId::Base64 => fl!("ui-tool-base64-name"),
        ToolId::UrlEncoder => fl!("ui-tool-urlenc-name"),
        ToolId::JsonFormatter => fl!("ui-tool-json-name"),
        ToolId::RegexTester => fl!("ui-tool-regex-name"),
        ToolId::TextDiff => fl!("ui-tool-diff-name"),
        ToolId::TextCase => fl!("ui-tool-textcase-name"),
        ToolId::Chmod => fl!("ui-tool-chmod-name"),
        ToolId::DateTime => fl!("ui-tool-datetime-name"),
        ToolId::Uuid => fl!("ui-tool-uuid-name"),
        ToolId::Ulid => fl!("ui-tool-ulid-name"),
        ToolId::Crontab => fl!("ui-tool-crontab-name"),
        ToolId::SshConfig => fl!("ui-tool-sshconfig-name"),
        ToolId::SshAudit => fl!("ui-tool-sshaudit-name"),
        ToolId::TlsAudit => fl!("ui-tool-tlsaudit-name"),
        ToolId::DnsSec => fl!("ui-tool-dnssec-name"),
        ToolId::DefaultCreds => fl!("ui-tool-defaultcreds-name"),
        ToolId::CveLookup => fl!("ui-tool-cve-name"),
        ToolId::SecNumCloud => fl!("ui-tool-secnumcloud-name"),
        ToolId::Smb => fl!("ui-tool-smb-name"),
        ToolId::HttpStatus => fl!("ui-tool-http-name"),
        ToolId::TcpPing => fl!("ui-tool-tcpping-name"),
        ToolId::WakeOnLan => fl!("ui-tool-wol-name"),
        ToolId::Whois => fl!("ui-tool-whois-name"),
        ToolId::BannerGrabber => fl!("ui-tool-banner-name"),
        ToolId::Ping => fl!("ui-tool-ping-name"),
        ToolId::Dns => fl!("ui-tool-dns-name"),
        ToolId::DnsBatch => fl!("ui-tool-dnsbatch-name"),
        ToolId::PortScan => fl!("ui-tool-portscan-name"),
        ToolId::HttpHeaders => fl!("ui-tool-headers-name"),
        ToolId::Cert => fl!("ui-tool-cert-name"),
        ToolId::TcpTrace => fl!("ui-tool-tcptrace-name"),
        ToolId::SnmpWalk => fl!("ui-tool-snmpwalk-name"),
        ToolId::ArpMon => fl!("ui-tool-arpmon-name"),
        ToolId::FwTest => fl!("ui-tool-fwtest-name"),
        ToolId::NetMap => fl!("ui-tool-netmap-name"),
        ToolId::OpenPorts => fl!("ui-tool-openports-name"),
        ToolId::NetIf => fl!("ui-tool-netif-name"),
        ToolId::Routes => fl!("ui-tool-routes-name"),
        ToolId::Wifi => fl!("ui-tool-wifi-name"),
        ToolId::Hosts => fl!("ui-tool-hosts-name"),
        ToolId::Services => fl!("ui-tool-services-name"),
        ToolId::LogView => fl!("ui-tool-logview-name"),
        ToolId::CronJob => fl!("ui-tool-cronjob-name"),
        ToolId::Notes => fl!("ui-tool-notes-name"),
        ToolId::Diagram => fl!("ui-tool-diagram-name"),
        ToolId::CmdLib => fl!("ui-tool-cmdlib-name"),
    }
}

/// What `tool` does, as the C# `ToolDesc*` keys say it: under its name on its card.
#[must_use]
pub fn description(tool: ToolId) -> String {
    match tool {
        ToolId::SubnetCalculator => fl!("ui-tool-subnet-description"),
        ToolId::IpConverter => fl!("ui-tool-ipconv-description"),
        ToolId::NetworkCalculator => fl!("ui-tool-netcalc-description"),
        ToolId::Hash => fl!("ui-tool-hash-description"),
        ToolId::Hmac => fl!("ui-tool-hmac-description"),
        ToolId::Password => fl!("ui-tool-password-description"),
        ToolId::SshKey => fl!("ui-tool-sshkey-description"),
        ToolId::CertGen => fl!("ui-tool-certgen-description"),
        ToolId::PwdAudit => fl!("ui-tool-pwdaudit-description"),
        ToolId::Jwt => fl!("ui-tool-jwt-description"),
        ToolId::Totp => fl!("ui-tool-totp-description"),
        ToolId::Base64 => fl!("ui-tool-base64-description"),
        ToolId::UrlEncoder => fl!("ui-tool-urlenc-description"),
        ToolId::JsonFormatter => fl!("ui-tool-json-description"),
        ToolId::RegexTester => fl!("ui-tool-regex-description"),
        ToolId::TextDiff => fl!("ui-tool-diff-description"),
        ToolId::TextCase => fl!("ui-tool-textcase-description"),
        ToolId::Chmod => fl!("ui-tool-chmod-description"),
        ToolId::DateTime => fl!("ui-tool-datetime-description"),
        ToolId::Uuid => fl!("ui-tool-uuid-description"),
        ToolId::Ulid => fl!("ui-tool-ulid-description"),
        ToolId::Crontab => fl!("ui-tool-crontab-description"),
        ToolId::SshConfig => fl!("ui-tool-sshconfig-description"),
        ToolId::SshAudit => fl!("ui-tool-sshaudit-description"),
        ToolId::TlsAudit => fl!("ui-tool-tlsaudit-description"),
        ToolId::DnsSec => fl!("ui-tool-dnssec-description"),
        ToolId::DefaultCreds => fl!("ui-tool-defaultcreds-description"),
        ToolId::CveLookup => fl!("ui-tool-cve-description"),
        ToolId::SecNumCloud => fl!("ui-tool-secnumcloud-description"),
        ToolId::Smb => fl!("ui-tool-smb-description"),
        ToolId::HttpStatus => fl!("ui-tool-http-description"),
        ToolId::TcpPing => fl!("ui-tool-tcpping-description"),
        ToolId::WakeOnLan => fl!("ui-tool-wol-description"),
        ToolId::Whois => fl!("ui-tool-whois-description"),
        ToolId::BannerGrabber => fl!("ui-tool-banner-description"),
        ToolId::Ping => fl!("ui-tool-ping-description"),
        ToolId::Dns => fl!("ui-tool-dns-description"),
        ToolId::DnsBatch => fl!("ui-tool-dnsbatch-description"),
        ToolId::PortScan => fl!("ui-tool-portscan-description"),
        ToolId::HttpHeaders => fl!("ui-tool-headers-description"),
        ToolId::Cert => fl!("ui-tool-cert-description"),
        ToolId::TcpTrace => fl!("ui-tool-tcptrace-description"),
        ToolId::SnmpWalk => fl!("ui-tool-snmpwalk-description"),
        ToolId::ArpMon => fl!("ui-tool-arpmon-description"),
        ToolId::FwTest => fl!("ui-tool-fwtest-description"),
        ToolId::NetMap => fl!("ui-tool-netmap-description"),
        ToolId::OpenPorts => fl!("ui-tool-openports-description"),
        ToolId::NetIf => fl!("ui-tool-netif-description"),
        ToolId::Routes => fl!("ui-tool-routes-description"),
        ToolId::Wifi => fl!("ui-tool-wifi-description"),
        ToolId::Hosts => fl!("ui-tool-hosts-description"),
        ToolId::Services => fl!("ui-tool-services-description"),
        ToolId::LogView => fl!("ui-tool-logview-description"),
        ToolId::CronJob => fl!("ui-tool-cronjob-description"),
        ToolId::Notes => fl!("ui-tool-notes-description"),
        ToolId::Diagram => fl!("ui-tool-diagram-description"),
        ToolId::CmdLib => fl!("ui-tool-cmdlib-description"),
    }
}

/// The icon of `tool`, as the C# registry's `Geo.Tool.*` geometry.
#[must_use]
pub const fn icon(tool: ToolId) -> Icon {
    match tool {
        ToolId::SubnetCalculator => Icon::ToolSubnet,
        ToolId::IpConverter => Icon::ToolIpConverter,
        ToolId::NetworkCalculator => Icon::ToolNetworkCalculator,
        // The C# registry draws both with the same geometry.
        ToolId::Hash | ToolId::Hmac => Icon::ToolHash,
        ToolId::Password => Icon::ToolPasswordGenerator,
        ToolId::SshKey => Icon::ToolSshKeyGenerator,
        ToolId::CertGen => Icon::ToolCertificateGenerator,
        ToolId::PwdAudit => Icon::ToolPasswordAudit,
        ToolId::Jwt => Icon::ToolJwt,
        ToolId::Totp => Icon::ToolTotp,
        ToolId::Base64 => Icon::ToolBase64,
        ToolId::UrlEncoder => Icon::ToolUrlEncoder,
        ToolId::JsonFormatter => Icon::ToolJson,
        ToolId::RegexTester => Icon::ToolRegex,
        ToolId::TextDiff => Icon::ToolDiff,
        ToolId::TextCase => Icon::ToolTextCase,
        ToolId::Chmod => Icon::ToolChmod,
        ToolId::DateTime => Icon::ToolDateTime,
        ToolId::Uuid => Icon::ToolUuid,
        ToolId::Ulid => Icon::ToolUlid,
        ToolId::Crontab => Icon::ToolCrontab,
        ToolId::SshConfig => Icon::ToolSshConfig,
        ToolId::SshAudit => Icon::ToolSshAudit,
        ToolId::TlsAudit => Icon::ToolTlsAudit,
        ToolId::DnsSec => Icon::ToolDnsSec,
        ToolId::DefaultCreds => Icon::ToolDefaultCreds,
        ToolId::CveLookup => Icon::ToolCve,
        ToolId::SecNumCloud => Icon::ToolSecNumCloud,
        ToolId::Smb => Icon::ToolSmb,
        ToolId::HttpStatus => Icon::ToolHttpStatus,
        ToolId::TcpPing => Icon::ToolSubnet,
        ToolId::WakeOnLan => Icon::ToolWakeOnLan,
        ToolId::Whois => Icon::ToolWhois,
        ToolId::BannerGrabber => Icon::ToolBannerGrabber,
        ToolId::Ping => Icon::ToolSubnet,
        ToolId::Dns | ToolId::DnsBatch => Icon::ToolDnsLookup,
        ToolId::PortScan => Icon::ToolPortScanner,
        ToolId::HttpHeaders => Icon::ToolHttpHeaders,
        ToolId::Cert => Icon::ToolCertInspector,
        ToolId::TcpTrace => Icon::ToolTraceroute,
        ToolId::SnmpWalk => Icon::ToolSnmpWalker,
        ToolId::ArpMon => Icon::ToolArpMonitor,
        ToolId::FwTest => Icon::ToolFirewallTester,
        ToolId::NetMap => Icon::ToolNetMap,
        ToolId::OpenPorts => Icon::ToolPortScanner,
        ToolId::NetIf => Icon::ToolSubnet,
        ToolId::Routes => Icon::ToolTraceroute,
        ToolId::Wifi => Icon::ToolSubnet,
        ToolId::Hosts => Icon::ToolHostsFileEditor,
        ToolId::Services => Icon::ToolServiceStatus,
        ToolId::LogView => Icon::ToolLogViewer,
        ToolId::CronJob => Icon::ToolCronJobManager,
        ToolId::Notes => Icon::ToolNotes,
        ToolId::Diagram => Icon::ToolDiagram,
        ToolId::CmdLib => Icon::ToolCommandLibrary,
    }
}

/// The name of `category`, as the C# `ToolCategory*` keys.
#[must_use]
pub fn category_label(category: ToolCategory) -> String {
    match category {
        ToolCategory::Network => fl!("ui-tools-category-network"),
        ToolCategory::Security => fl!("ui-tools-category-security"),
        ToolCategory::Encoding => fl!("ui-tools-category-encoding"),
        ToolCategory::System => fl!("ui-tools-category-system"),
        ToolCategory::External => fl!("ui-tools-category-external"),
    }
}

/// The name of a group of the sidebar's tools.
#[must_use]
pub fn group_label(group: ToolGroup) -> String {
    match group {
        ToolGroup::Favorites => fl!("ui-tools-favorites"),
        ToolGroup::Category(category) => category_label(category),
    }
}

/// Whether the sidebar's filter `filter` finds `tool`, as the C# `FilterSidebarTools`: its
/// name or one of its aliases holds the filter, whatever the case; an empty filter finds
/// every tool.
#[must_use]
pub fn sidebar_matches(tool: ToolId, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    if filter.is_empty() {
        return true;
    }
    let searchable = format!("{} {}", label(tool), tool.prefixes().join(" ")).to_lowercase();
    searchable.contains(&filter)
}

/// Whether the Tools page's search `search` finds `tool`, as the C#
/// `RefreshToolsTabSections`: its name, its aliases or its description holds it, whatever
/// the case.
#[must_use]
pub fn page_matches(tool: ToolId, search: &str) -> bool {
    let search = search.trim().to_lowercase();
    if search.is_empty() {
        return true;
    }
    let searchable = format!(
        "{} {} {}",
        label(tool),
        tool.prefixes().join(" "),
        description(tool)
    )
    .to_lowercase();
    searchable.contains(&search)
}

/// A copy button of a tool, by what it copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopySlot {
    /// A digest of the hash generator, by its algorithm.
    HashDigest(HashAlgorithm),
    /// The HMAC generator's output.
    HmacOutput,
    /// The JWT parser's header.
    JwtHeader,
    /// The JWT parser's payload.
    JwtPayload,
    /// The JWT parser's signature.
    JwtSignature,
    /// The TOTP generator's code.
    TotpCode,
    /// The Base64 output.
    Base64Output,
    /// The URL encoder's decoded text.
    UrlDecoded,
    /// The URL encoder's encoded text.
    UrlEncoded,
    /// The UUID generated.
    UuidSingle,
    /// The batch of UUIDs.
    UuidBatch,
    /// The JSON formatter's output.
    JsonOutput,
    /// The regular expression tester's matches.
    RegexMatches,
    /// The text comparison's unified diff.
    DiffUnified,
    /// The text case converter's output.
    TextCaseOutput,
    /// The certificate generator's fingerprint.
    CertFingerprint,
    /// The certificate generator's first certificate.
    CertCert,
    /// Its key.
    CertKey,
    /// The CA's leaf.
    CertLeafCert,
    /// Its key.
    CertLeafKey,
    /// The SSH key generator's fingerprint.
    SshFingerprint,
    /// Its public key.
    SshPublic,
    /// Its private key.
    SshPrivate,
    /// The password shown.
    PasswordMain,
    /// Its phonetic reading.
    PasswordPhonetic,
    /// The whole batch.
    PasswordBatchAll,
    /// A row of the batch.
    PasswordBatchRow(usize),
    /// A password of the history.
    PasswordHistory(usize),
    /// A value of the subnet calculator.
    Subnet(SubnetField),
    /// A form of the IP converter.
    IpConverter(IpField),
    /// The network calculator's result.
    NetCalcResult,
    /// The chmod command.
    ChmodCommand,
    /// The octal mode.
    ChmodOctal,
    /// The `rwx` form.
    ChmodSymbolic,
    /// A form of the date and time converter.
    DateTime(DateField),
    /// The ULID generated.
    UlidSingle,
    /// The batch of ULIDs.
    UlidBatch,
    /// The cron expression.
    CrontabExpression,
    /// The SSH config block.
    SshConfigOutput,
    /// The SSH key auditor's fingerprint.
    SshAuditFingerprint,
    /// The TLS auditor's report text.
    TlsAuditReport,
    /// The DNS security checker's report text.
    DnsSecReport,
    /// The CVE lookup report text.
    CveReport,
    /// The default credentials scanner report text.
    DefaultCredsReport,
    /// The default credentials scanner CSV export.
    DefaultCredsCsv,
    /// The SecNumCloud compliance audit CSV export.
    SecNumCloudCsv,
    /// The SecNumCloud compliance audit HTML report.
    SecNumCloudHtml,
    /// The SMB enumerator report text.
    SmbReport,
    /// A status code entry of the HTTP status tool.
    HttpStatus(u16),
    /// The TCP ping results log.
    TcpPingResults,
    /// The WHOIS output text.
    WhoisOutput,
    /// The banner grabber results.
    BannerGrabResults,
    /// The banner grabber CSV export.
    BannerGrabCsv,
    /// The ICMP ping results text.
    PingResults,
    /// The ICMP ping CSV export.
    PingCsv,
    /// The DNS lookup output text.
    DnsOutput,
    /// The DNS batch resolver CSV export.
    DnsBatchCsv,
    /// The TCP port scanner CSV export.
    PortScanCsv,
    /// The HTTP header analyzer report text.
    HttpHeadersReport,
    /// The SSL/TLS certificate details text.
    CertDetails,
    /// The SSL/TLS certificate scan CSV export.
    CertCsv,
    /// The TCP traceroute CSV export.
    TcpTraceCsv,
    /// The SNMP walk CSV export.
    SnmpWalkCsv,
    /// The ARP table monitor CSV export.
    ArpMonCsv,
    /// The firewall egress tester CSV export.
    FwTestCsv,
    /// The network cartography CSV export.
    NetMapCsv,
    /// The open TCP/UDP ports CSV export.
    OpenPortsCsv,
    /// The network interfaces CSV export.
    NetIfCsv,
    /// The routing table CSV export.
    RoutesCsv,
    /// The Wi-Fi scanner CSV export.
    WifiCsv,
    /// The serialized hosts file text.
    HostsFileText,
    /// The address of the hosts file entry selected.
    HostsIp,
    /// The host name of the hosts file entry selected.
    HostsHostname,
    /// The service status dashboard CSV export.
    ServicesCsv,
    /// The log viewer's lines shown.
    LogViewText,
    /// The cron job manager's list.
    CronJobExport,
    /// The note shown, as Markdown.
    NotesMarkdown,
    /// The note shown, as an HTML document.
    NotesHtml,
    /// The diagram's source.
    DiagramSource,
    /// The command of a snippet of the command library.
    CmdLibCommand(SnippetId),
    /// The command of the user's command at this place of the library.
    CmdLibUserCommand(usize),
}

/// What a tool's tab is asked.
#[derive(Debug, Clone)]
pub enum ToolMessage {
    /// The "Route via" bar's.
    Route(RouteMessage),
    /// Show or hide the tool's help, as the C# "?" button.
    ToggleHelp,
    /// Hide the help, as its close button.
    CloseHelp,
    /// The check mark of copy number `.0` has been shown long enough.
    CopyShown(u64),
    /// How the export of the button of `.0` to a file went.
    Exported(CopySlot, file_export::Saved),
    /// The hash generator's.
    Hash(HashMessage),
    /// The HMAC generator's.
    Hmac(HmacMessage),
    /// The JWT parser's.
    Jwt(JwtMessage),
    /// The TOTP generator's.
    Totp(TotpMessage),
    /// The Base64 tool's.
    Base64(Base64Message),
    /// The URL encoder's.
    Url(UrlMessage),
    /// The UUID generator's.
    Uuid(UuidMessage),
    /// The JSON formatter's.
    Json(JsonMessage),
    /// The regular expression tester's.
    Regex(RegexMessage),
    /// The text comparison's.
    Diff(DiffMessage),
    /// The text case converter's.
    TextCase(TextCaseMessage),
    /// The certificate generator's.
    CertGen(CertGenMessage),
    /// The SSH key generator's.
    SshKey(SshKeyMessage),
    /// The password generator's.
    Password(PasswordMessage),
    /// The password audit's.
    PwdAudit(PwdAuditMessage),
    /// The subnet calculator's.
    Subnet(SubnetMessage),
    /// The IP converter's.
    IpConverter(IpConverterMessage),
    /// The network calculator's.
    NetCalc(NetCalcMessage),
    /// The chmod calculator's.
    Chmod(ChmodMessage),
    /// The date and time converter's.
    DateTime(DateTimeMessage),
    /// The ULID generator's.
    Ulid(UlidMessage),
    /// The crontab builder's.
    Crontab(CrontabMessage),
    /// The SSH config generator's.
    SshConfig(SshConfigMessage),
    /// The SSH key auditor's.
    SshAudit(SshAuditMessage),
    /// The TLS auditor's.
    TlsAudit(TlsAuditMessage),
    /// The DNS security checker's.
    DnsSec(DnsSecMessage),
    /// The CVE lookup tool's.
    Cve(CveMessage),
    /// The default credentials scanner tool's.
    DefaultCreds(DefaultCredsMessage),
    /// The SecNumCloud compliance audit tool's.
    SecNumCloud(SecNumCloudMessage),
    /// The SMB enumerator tool's.
    Smb(SmbMessage),
    /// The HTTP status codes tool's.
    HttpStatus(HttpStatusMessage),
    /// The TCP ping tool's.
    TcpPing(TcpPingMessage),
    /// The Wake-on-LAN tool's.
    Wol(WolMessage),
    /// The WHOIS lookup tool's.
    Whois(WhoisMessage),
    /// The banner grabber tool's.
    Banner(BannerMessage),
    /// The ICMP ping tool's.
    Ping(PingMessage),
    /// The DNS lookup tool's.
    Dns(DnsMessage),
    /// The DNS batch resolver tool's.
    DnsBatch(DnsBatchMessage),
    /// The TCP port scanner tool's.
    PortScan(PortScanMessage),
    /// The HTTP header analyzer tool's.
    HttpHeaders(HttpHeadersMessage),
    /// The SSL/TLS certificate inspector tool's.
    Cert(CertInspectorMessage),
    /// The TCP traceroute tool's.
    TcpTrace(TcpTracerouteMessage),
    /// The SNMP MIB walker tool's.
    SnmpWalk(SnmpWalkerMessage),
    /// The ARP table monitor tool's.
    ArpMon(ArpMonitorMessage),
    /// The firewall egress tester tool's.
    FwTest(FirewallTesterMessage),
    /// The network cartography tool's.
    NetMap(NetworkCartographyMessage),
    /// The open ports tool's.
    OpenPorts(OpenPortsMessage),
    /// The network interfaces tool's.
    NetIf(NetIfMessage),
    /// The routing table tool's.
    Routes(RouteTableMessage),
    /// The Wi-Fi scanner tool's.
    Wifi(WifiMessage),
    /// The hosts file editor tool's.
    Hosts(HostsMessage),
    /// The service status dashboard tool's.
    Services(ServiceStatusMessage),
    /// The log viewer tool's.
    LogView(LogViewerMessage),
    /// The cron job manager tool's.
    CronJob(CronJobMessage),
    /// The notes scratchpad tool's.
    Notes(NotesMessage),
    /// The diagram editor tool's.
    Diagram(DiagramMessage),
    /// The command library tool's.
    CmdLib(CommandLibraryMessage),
}

impl ToolMessage {
    /// Whether the message asks a network tool to start reaching a host, which is when the
    /// "Route via" choice is read: the tools a gateway is offered on, routed or refused.
    /// A tick of a monitor already going is not one: it was started on the choice then.
    fn starts_run(&self) -> bool {
        matches!(
            self,
            Self::PortScan(PortScanMessage::Start | PortScanMessage::Confirm)
                | Self::Banner(BannerMessage::Start)
                | Self::TcpPing(TcpPingMessage::Start)
                | Self::HttpHeaders(HttpHeadersMessage::Analyze)
                | Self::Cert(CertInspectorMessage::Inspect)
                | Self::Ping(PingMessage::Start)
                | Self::TcpTrace(TcpTracerouteMessage::Start)
                | Self::SnmpWalk(SnmpWalkerMessage::Start | SnmpWalkerMessage::TestCommunities)
                | Self::ArpMon(ArpMonitorMessage::Scan | ArpMonitorMessage::ToggleMonitoring)
                | Self::Wol(WolMessage::SendPacket)
                | Self::NetMap(
                    NetworkCartographyMessage::Start | NetworkCartographyMessage::ConfirmLarge
                )
                | Self::Dns(DnsMessage::Lookup)
                | Self::DnsBatch(DnsBatchMessage::Resolve)
                | Self::DnsSec(DnsSecMessage::Check)
        )
    }
}

/// A tool's own state.
#[derive(Debug)]
enum Pane {
    Hash(hash_tool::HashPane),
    Hmac(hmac_tool::HmacPane),
    Jwt(jwt_tool::JwtPane),
    Totp(totp_tool::TotpPane),
    Base64(base64_tool::Base64Pane),
    Url(url_tool::UrlPane),
    Uuid(uuid_tool::UuidPane),
    Json(json_tool::JsonPane),
    Regex(regex_tool::RegexPane),
    Diff(diff_tool::DiffPane),
    TextCase(text_case_tool::TextCasePane),
    CertGen(Box<certgen_tool::CertGenPane>),
    SshKey(Box<sshkey_tool::SshKeyPane>),
    Password(Box<password_tool::PasswordPane>),
    PwdAudit(pwdaudit_tool::PwdAuditPane),
    Subnet(subnet_tool::SubnetPane),
    IpConverter(ip_converter_tool::IpConverterPane),
    NetCalc(network_calculator_tool::NetCalcPane),
    Chmod(chmod_tool::ChmodPane),
    DateTime(Box<datetime_tool::DateTimePane>),
    Ulid(ulid_tool::UlidPane),
    Crontab(crontab_tool::CrontabPane),
    SshConfig(ssh_config_tool::SshConfigPane),
    SshAudit(Box<sshaudit_tool::SshAuditPane>),
    TlsAudit(Box<tlsaudit_tool::TlsAuditPane>),
    DnsSec(Box<dnssec_tool::DnsSecPane>),
    Cve(Box<cve_tool::CvePane>),
    DefaultCreds(Box<defaultcreds_tool::DefaultCredsPane>),
    SecNumCloud(Box<secnumcloud_tool::SecNumCloudPane>),
    Smb(Box<smb_tool::SmbPane>),
    HttpStatus(http_status_tool::HttpStatusPane),
    TcpPing(tcp_ping_tool::TcpPingPane),
    Wol(wol_tool::WolPane),
    Whois(whois_tool::WhoisPane),
    Banner(banner_tool::BannerPane),
    Ping(ping_tool::PingPane),
    Dns(dns_tool::DnsPane),
    DnsBatch(dns_batch_tool::DnsBatchPane),
    PortScan(port_scan_tool::PortScanPane),
    HttpHeaders(http_headers_tool::HttpHeadersPane),
    Cert(cert_inspector_tool::CertInspectorPane),
    TcpTrace(tcp_traceroute_tool::TcpTraceroutePane),
    SnmpWalk(snmp_walker_tool::SnmpWalkerPane),
    ArpMon(arp_monitor_tool::ArpMonitorPane),
    FwTest(firewall_tester_tool::FirewallTesterPane),
    NetMap(network_cartography_tool::NetworkCartographyPane),
    OpenPorts(open_ports_tool::OpenPortsPane),
    NetIf(network_interfaces_tool::NetIfPane),
    Routes(route_table_tool::RouteTablePane),
    Wifi(wifi_networks_tool::WifiNetworksPane),
    Hosts(hosts_editor_tool::HostsEditorPane),
    Services(service_status_tool::ServiceStatusPane),
    LogView(log_viewer_tool::LogViewerPane),
    CronJob(cron_job_tool::CronJobPane),
    Notes(notes_tool::NotesPane),
    Diagram(diagram_tool::DiagramPane),
    CmdLib(command_library_tool::CommandLibraryPane),
}

/// What a tool's tab holds: the tool's state, its help shown or not, the copy or export
/// button that shows its check mark, why the last export failed.
#[derive(Debug)]
pub struct ToolPane {
    tool: ToolId,
    pane: Pane,
    /// The "Route via" bar, Direct until the user picks a gateway.
    route: route_bar::RouteBar,
    help: bool,
    copied: Option<(CopySlot, u64)>,
    copies: u64,
    export_failed: Option<String>,
}

impl ToolPane {
    /// A new tab's state for `tool`, as the C# view's `Initialize`; what a tool keeps lives
    /// beside the profiles file `profiles_file`.
    fn new(tool: ToolId, profiles_file: Option<&std::path::Path>) -> Self {
        let pane = match tool {
            ToolId::Password => {
                Pane::Password(Box::new(password_tool::PasswordPane::new(profiles_file)))
            }
            ToolId::SshKey => Pane::SshKey(Box::new(sshkey_tool::SshKeyPane::new())),
            ToolId::CertGen => Pane::CertGen(Box::default()),
            ToolId::PwdAudit => Pane::PwdAudit(pwdaudit_tool::PwdAuditPane::default()),
            ToolId::Hash => Pane::Hash(hash_tool::HashPane::default()),
            ToolId::Hmac => Pane::Hmac(hmac_tool::HmacPane::default()),
            ToolId::Jwt => Pane::Jwt(jwt_tool::JwtPane::new()),
            ToolId::Totp => Pane::Totp(totp_tool::TotpPane::default()),
            ToolId::Base64 => Pane::Base64(base64_tool::Base64Pane::default()),
            ToolId::UrlEncoder => Pane::Url(url_tool::UrlPane::default()),
            ToolId::Uuid => Pane::Uuid(uuid_tool::UuidPane::new()),
            ToolId::JsonFormatter => Pane::Json(json_tool::JsonPane::default()),
            ToolId::RegexTester => Pane::Regex(regex_tool::RegexPane::default()),
            ToolId::TextDiff => Pane::Diff(diff_tool::DiffPane::default()),
            ToolId::TextCase => Pane::TextCase(text_case_tool::TextCasePane::default()),
            ToolId::SubnetCalculator => Pane::Subnet(subnet_tool::SubnetPane::default()),
            ToolId::IpConverter => Pane::IpConverter(ip_converter_tool::IpConverterPane::default()),
            ToolId::NetworkCalculator => {
                Pane::NetCalc(network_calculator_tool::NetCalcPane::default())
            }
            ToolId::Chmod => Pane::Chmod(chmod_tool::ChmodPane::default()),
            ToolId::DateTime => Pane::DateTime(Box::new(datetime_tool::DateTimePane::new())),
            ToolId::Ulid => Pane::Ulid(ulid_tool::UlidPane::new()),
            ToolId::Crontab => Pane::Crontab(crontab_tool::CrontabPane::new()),
            ToolId::SshConfig => Pane::SshConfig(ssh_config_tool::SshConfigPane::default()),
            ToolId::SshAudit => Pane::SshAudit(Box::default()),
            ToolId::TlsAudit => Pane::TlsAudit(Box::default()),
            ToolId::DnsSec => Pane::DnsSec(Box::default()),
            ToolId::DefaultCreds => Pane::DefaultCreds(Box::default()),
            ToolId::CveLookup => Pane::Cve(Box::default()),
            ToolId::SecNumCloud => Pane::SecNumCloud(Box::default()),
            ToolId::Smb => Pane::Smb(Box::default()),
            ToolId::HttpStatus => Pane::HttpStatus(http_status_tool::HttpStatusPane::default()),
            ToolId::TcpPing => Pane::TcpPing(tcp_ping_tool::TcpPingPane::default()),
            ToolId::WakeOnLan => Pane::Wol(wol_tool::WolPane::default()),
            ToolId::Whois => Pane::Whois(whois_tool::WhoisPane::default()),
            ToolId::BannerGrabber => Pane::Banner(banner_tool::BannerPane::default()),
            ToolId::Ping => Pane::Ping(ping_tool::PingPane::default()),
            ToolId::Dns => Pane::Dns(dns_tool::DnsPane::default()),
            ToolId::DnsBatch => Pane::DnsBatch(dns_batch_tool::DnsBatchPane::default()),
            ToolId::PortScan => Pane::PortScan(port_scan_tool::PortScanPane::default()),
            ToolId::HttpHeaders => Pane::HttpHeaders(http_headers_tool::HttpHeadersPane::default()),
            ToolId::Cert => Pane::Cert(cert_inspector_tool::CertInspectorPane::default()),
            ToolId::TcpTrace => Pane::TcpTrace(tcp_traceroute_tool::TcpTraceroutePane::default()),
            ToolId::SnmpWalk => Pane::SnmpWalk(snmp_walker_tool::SnmpWalkerPane::default()),
            ToolId::ArpMon => Pane::ArpMon(arp_monitor_tool::ArpMonitorPane::default()),
            ToolId::FwTest => Pane::FwTest(firewall_tester_tool::FirewallTesterPane::default()),
            ToolId::NetMap => Pane::NetMap(network_cartography_tool::NetworkCartographyPane::new(
                heimdall_core::paths::data_dir(),
            )),
            ToolId::OpenPorts => Pane::OpenPorts(open_ports_tool::OpenPortsPane::default()),
            ToolId::NetIf => Pane::NetIf(network_interfaces_tool::NetIfPane::default()),
            ToolId::Routes => Pane::Routes(route_table_tool::RouteTablePane::default()),
            ToolId::Wifi => Pane::Wifi(wifi_networks_tool::WifiNetworksPane::default()),
            ToolId::Hosts => Pane::Hosts(hosts_editor_tool::HostsEditorPane::default()),
            ToolId::Services => Pane::Services(service_status_tool::ServiceStatusPane::default()),
            ToolId::LogView => Pane::LogView(log_viewer_tool::LogViewerPane::default()),
            ToolId::CronJob => Pane::CronJob(cron_job_tool::CronJobPane::default()),
            ToolId::Notes => Pane::Notes(notes_tool::NotesPane::default()),
            ToolId::Diagram => Pane::Diagram(diagram_tool::DiagramPane::default()),
            ToolId::CmdLib => {
                Pane::CmdLib(command_library_tool::CommandLibraryPane::new(profiles_file))
            }
        };
        Self {
            tool,
            pane,
            route: route_bar::RouteBar::default(),
            help: false,
            copied: None,
            copies: 0,
            export_failed: None,
        }
    }

    /// Whether its help is shown.
    #[must_use]
    pub fn help_shown(&self) -> bool {
        self.help
    }

    /// The name of the document closing it would lose, or `None`.
    fn unsaved(&self) -> Option<String> {
        match &self.pane {
            Pane::Notes(pane) => pane.unsaved(),
            Pane::Diagram(pane) => pane.unsaved(),
            _ => None,
        }
    }

    /// The copy button of `slot` shows its check mark.
    fn copied(&self, slot: CopySlot) -> bool {
        self.copied.is_some_and(|(shown, _)| shown == slot)
    }

    /// `content` copied by the button of `slot`: written to the clipboard, the button's
    /// check mark shown for [`COPY_FEEDBACK`], as the C# copy buttons; nothing to copy,
    /// nothing happens.
    fn copy(&mut self, tab: TabId, slot: CopySlot, content: String) -> Task<Message> {
        if content.is_empty() {
            return Task::none();
        }
        Task::batch([iced::clipboard::write(content), self.check_mark(tab, slot)])
    }

    /// The check mark of the button of `slot` shown for [`COPY_FEEDBACK`].
    fn check_mark(&mut self, tab: TabId, slot: CopySlot) -> Task<Message> {
        self.copies += 1;
        let copy = self.copies;
        self.copied = Some((slot, copy));
        Task::perform(wait(COPY_FEEDBACK), move |()| {
            Message::Tool(tab, ToolMessage::CopyShown(copy))
        })
    }

    /// `export` saved to the file the user picks, over the main window `main`; the failure
    /// of an earlier one no longer said.
    fn export(
        &mut self,
        tab: TabId,
        export: file_export::FileExport,
        main: Option<window::Id>,
    ) -> Task<Message> {
        self.export_failed = None;
        export.task(tab, main)
    }

    /// How the export of the button of `slot` went, as the C# export buttons say it: the
    /// button's check mark once written, as `CopyFeedbackHelper`, or why not under the
    /// header, as the C# `StatusExportFailed`.
    fn exported(&mut self, tab: TabId, slot: CopySlot, saved: file_export::Saved) -> Task<Message> {
        match saved {
            file_export::Saved::Written => {
                self.export_failed = None;
                self.check_mark(tab, slot)
            }
            file_export::Saved::Cancelled => Task::none(),
            file_export::Saved::Failed(detail) => {
                self.export_failed = Some(detail);
                Task::none()
            }
        }
    }

    /// Why the last export failed, when it did.
    #[must_use]
    pub fn export_failure(&self) -> Option<&str> {
        self.export_failed.as_deref()
    }
}

/// The tool tabs' states, by tab.
#[derive(Debug, Default)]
pub struct ToolPanes {
    panes: HashMap<TabId, ToolPane>,
    /// Tool tabs made whose opening work, as the C# view's `Initialize`, is still to start.
    opening: Vec<TabId>,
    /// What the tool tabs just made were told of their context left to run.
    prefilled: Vec<Task<Message>>,
}

impl ToolPanes {
    /// Follows the core's tabs: a state made for each tool tab just opened, as the C# view
    /// is made with its tab; the states of tabs closed let go.
    pub fn sync(&mut self, app: &App) {
        self.panes.retain(|id, state| {
            let open = app.tab(*id).and_then(heimdall_app::Tab::tool).is_some();
            // A tab closing writes down what it keeps, as the C# views' `Unloaded`.
            if !open {
                match &mut state.pane {
                    Pane::Password(pane) => pane.closing(),
                    Pane::Notes(pane) => pane.closing(),
                    _ => {}
                }
            }
            open
        });
        for tab in &app.tabs {
            let Some(tool) = tab.tool() else {
                continue;
            };
            if self.panes.contains_key(&tab.id) {
                continue;
            }
            self.panes
                .insert(tab.id, ToolPane::new(tool, Some(app.profiles_file())));
            self.opening.push(tab.id);
            // What it opened on, as the C# view's `Initialize` reads its `ToolContext`: told
            // as typed, so it is checked as typed.
            for message in context::messages(tool, tab.tool_context()) {
                let task = self.update(tab.id, message, None, app);
                self.prefilled.push(task);
            }
        }
        // The gateways a network tool can be routed through, as the core lists them now.
        let gateways = app.tool_gateways();
        for state in self.panes.values_mut() {
            if state.tool.route_support() != RouteSupport::NotOffered {
                state.route.set_gateways(gateways.clone());
            }
        }
        // The terminals a command can be sent to, as the core offers them now.
        let targets = app.send_targets();
        for state in self.panes.values_mut() {
            if let Pane::CmdLib(pane) = &mut state.pane {
                pane.set_targets(targets.clone());
            }
        }
    }

    /// The documents the tool tabs would lose if closed now, told to the core, so closing a
    /// tab or the window asks first, as the C# tools' `CanClose`.
    pub fn report_unsaved(&self, app: &mut App) {
        for (tab, state) in &self.panes {
            app.set_tool_unsaved(*tab, state.unsaved());
        }
    }

    /// A close asked: the notes written now, as the C# Notes `CanClose`
    /// (`NotesToolView.xaml.cs:107`); what cannot be written stays unsaved, and asked about.
    pub fn save_before_closing(&mut self) {
        for state in self.panes.values_mut() {
            if let Pane::Notes(pane) = &mut state.pane {
                pane.save_now();
            }
        }
    }

    /// Tool tab `tab`'s document saved, as its close question's Save asks, then its close
    /// asked again once written; a failure is said in the tab, which stays open.
    pub fn save_before_close(&mut self, tab: TabId, main: Option<window::Id>) -> Task<Message> {
        let close = || Task::done(Message::App(heimdall_app::Message::RequestCloseTab(tab)));
        match self.panes.get_mut(&tab).map(|state| &mut state.pane) {
            Some(Pane::Notes(pane)) => {
                if pane.save_now() {
                    close()
                } else {
                    Task::none()
                }
            }
            Some(Pane::Diagram(pane)) => pane.save_before_close().task(tab, main),
            Some(_) => close(),
            None => Task::none(),
        }
    }

    /// The work of the tool tabs made since last asked, as the C# view's `Initialize`: the
    /// Hosts File Editor reads the hosts file, the Network Cartography its scan history and
    /// its knowledge base; what a tab opened on a host or an argument was told runs on, a
    /// log file read or a conversion due.
    pub fn opening(&mut self) -> Task<Message> {
        let mut tasks = std::mem::take(&mut self.prefilled);
        for tab in std::mem::take(&mut self.opening) {
            match self.panes.get_mut(&tab).map(|state| &mut state.pane) {
                Some(Pane::Hosts(pane)) => tasks.push(pane.open(tab)),
                Some(Pane::NetMap(pane)) => tasks.push(pane.open(tab)),
                _ => {}
            }
        }
        Task::batch(tasks)
    }

    /// The state of tool tab `tab`.
    #[must_use]
    pub fn get(&self, tab: TabId) -> Option<&ToolPane> {
        self.panes.get(&tab)
    }

    /// The timers of the tool tabs that follow something: each log viewer tailing its
    /// file looked at every [`TAIL_INTERVAL`](heimdall_core::tools::log_viewer::TAIL_INTERVAL),
    /// as the C# `_tailTimer`; each ARP monitor monitoring reads its table every interval
    /// chosen, as the C# polling loop.
    #[must_use]
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(self.panes.iter().filter_map(|(tab, state)| {
            match &state.pane {
                Pane::LogView(pane) if pane.tailing() => Some(
                    iced::time::every(heimdall_core::tools::log_viewer::TAIL_INTERVAL)
                        .with(*tab)
                        .map(|(tab, _)| {
                            Message::Tool(tab, ToolMessage::LogView(LogViewerMessage::TailTick))
                        }),
                ),
                Pane::ArpMon(pane) if pane.monitoring() => Some(
                    iced::time::every(pane.interval())
                        .with(*tab)
                        .map(|(tab, _)| {
                            Message::Tool(tab, ToolMessage::ArpMon(ArpMonitorMessage::Tick))
                        }),
                ),
                _ => None,
            }
        }))
    }

    /// Applies `message` to tool tab `tab`; the window `main` holds the file dialogs.
    #[expect(clippy::too_many_lines, reason = "one arm per tool")]
    pub fn update(
        &mut self,
        tab: TabId,
        message: ToolMessage,
        main: Option<window::Id>,
        app: &App,
    ) -> Task<Message> {
        let Some(state) = self.panes.get_mut(&tab) else {
            return Task::none();
        };
        if let ToolMessage::Route(message) = message {
            state.route.update(message);
            return Task::none();
        }
        // The gateway a run goes through, resolved when the run is asked. A run that cannot
        // go through the gateway chosen is not made at all, and never made directly.
        let via = if message.starts_run() {
            match state.route.route_for(state.tool, app) {
                Ok(via) => via,
                Err(route_bar::Refused) => return Task::none(),
            }
        } else {
            None
        };
        match (message, &mut state.pane) {
            (ToolMessage::ToggleHelp, _) => state.help = !state.help,
            (ToolMessage::CloseHelp, _) => state.help = false,
            (ToolMessage::Exported(slot, saved), _) => {
                return state.exported(tab, slot, saved);
            }
            (ToolMessage::CopyShown(copy), _) => {
                if state.copied.is_some_and(|(_, shown)| shown == copy) {
                    state.copied = None;
                }
            }
            (ToolMessage::Hash(message), Pane::Hash(pane)) => {
                return match pane.update(message) {
                    hash_tool::Outcome::Copy(kind, content) => {
                        state.copy(tab, CopySlot::HashDigest(kind), content)
                    }
                    outcome => outcome.task(tab, main),
                };
            }
            (ToolMessage::Hmac(message), Pane::Hmac(pane)) => {
                if let Some(content) = pane.update(message) {
                    return state.copy(tab, CopySlot::HmacOutput, content);
                }
            }
            (ToolMessage::Jwt(message), Pane::Jwt(pane)) => {
                if let Some((slot, content)) = pane.update(message) {
                    return state.copy(tab, slot, content);
                }
            }
            (ToolMessage::Totp(message), Pane::Totp(pane)) => {
                return match pane.update(message) {
                    totp_tool::Outcome::Copy(content) => {
                        state.copy(tab, CopySlot::TotpCode, content)
                    }
                    outcome => outcome.task(tab),
                };
            }
            (ToolMessage::Base64(message), Pane::Base64(pane)) => {
                return match pane.update(message) {
                    base64_tool::Outcome::Copy(content) => {
                        state.copy(tab, CopySlot::Base64Output, content)
                    }
                    outcome => outcome.task(tab, main),
                };
            }
            (ToolMessage::Url(message), Pane::Url(pane)) => {
                if let Some((slot, content)) = pane.update(message) {
                    return state.copy(tab, slot, content);
                }
            }
            (ToolMessage::Uuid(message), Pane::Uuid(pane)) => {
                if let Some((slot, content)) = pane.update(message) {
                    return state.copy(tab, slot, content);
                }
            }
            (ToolMessage::Json(message), Pane::Json(pane)) => {
                return match pane.update(message) {
                    json_tool::Outcome::Copy(content) => {
                        state.copy(tab, CopySlot::JsonOutput, content)
                    }
                    outcome => outcome.task(tab),
                };
            }
            (ToolMessage::Regex(message), Pane::Regex(pane)) => {
                return match pane.update(message) {
                    regex_tool::Outcome::Copy(content) => {
                        state.copy(tab, CopySlot::RegexMatches, content)
                    }
                    outcome => outcome.task(tab),
                };
            }
            (ToolMessage::Diff(message), Pane::Diff(pane)) => {
                return match pane.update(message) {
                    diff_tool::Outcome::Copy(content) => {
                        state.copy(tab, CopySlot::DiffUnified, content)
                    }
                    outcome => outcome.task(tab),
                };
            }
            (ToolMessage::TextCase(message), Pane::TextCase(pane)) => {
                if let Some((slot, content)) = pane.update(message) {
                    return state.copy(tab, slot, content);
                }
            }
            (ToolMessage::CertGen(message), Pane::CertGen(pane)) => {
                return match pane.update(message) {
                    certgen_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    outcome => outcome.task(tab, main),
                };
            }
            (ToolMessage::SshKey(message), Pane::SshKey(pane)) => {
                return match pane.update(message) {
                    sshkey_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    outcome => outcome.task(tab, main),
                };
            }
            (ToolMessage::Password(message), Pane::Password(pane)) => {
                return match pane.update(message) {
                    password_tool::Outcome::Copy(slot, content, timer) => {
                        let copied = state.copy(tab, slot, content);
                        match timer {
                            Some(timer) => Task::batch([
                                copied,
                                password_tool::Outcome::Tick(timer).task(tab, main),
                            ]),
                            None => copied,
                        }
                    }
                    outcome => outcome.task(tab, main),
                };
            }
            (ToolMessage::PwdAudit(message), Pane::PwdAudit(pane)) => pane.update(message),
            (ToolMessage::DateTime(message), Pane::DateTime(pane)) => {
                return match pane.update(message) {
                    datetime_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    outcome => outcome.task(tab),
                };
            }
            (ToolMessage::SshAudit(message), Pane::SshAudit(pane)) => {
                return match pane.update(message, tab, main) {
                    sshaudit_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    sshaudit_tool::Outcome::Task(task) => task,
                    sshaudit_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::TlsAudit(message), Pane::TlsAudit(pane)) => {
                return match pane.update(message, tab) {
                    tlsaudit_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    tlsaudit_tool::Outcome::Task(task) => task,
                    tlsaudit_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::DnsSec(message), Pane::DnsSec(pane)) => {
                return match pane.update(message, tab) {
                    dnssec_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    dnssec_tool::Outcome::Task(task) => task,
                    dnssec_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::Cve(message), Pane::Cve(pane)) => {
                return match pane.update(message) {
                    cve_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    cve_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::DefaultCreds(message), Pane::DefaultCreds(pane)) => {
                return match pane.update(message) {
                    defaultcreds_tool::Outcome::Copy(slot, content) => {
                        state.copy(tab, slot, content)
                    }
                    defaultcreds_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::SecNumCloud(message), Pane::SecNumCloud(pane)) => {
                return match pane.update(message, tab) {
                    secnumcloud_tool::Outcome::Task(task) => task,
                    secnumcloud_tool::Outcome::Save(export) => state.export(tab, export, main),
                    secnumcloud_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::Smb(message), Pane::Smb(pane)) => {
                return match pane.update(message, tab) {
                    smb_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    smb_tool::Outcome::Task(task) => task,
                    smb_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::HttpStatus(message), Pane::HttpStatus(pane)) => {
                if let http_status_tool::Outcome::Copy(slot, content) = pane.update(message) {
                    return state.copy(tab, slot, content);
                }
            }
            (ToolMessage::TcpPing(message), Pane::TcpPing(pane)) => {
                return match pane.update(tab, message, via) {
                    tcp_ping_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    tcp_ping_tool::Outcome::Task(task) => task,
                    tcp_ping_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::Wol(message), Pane::Wol(pane)) => {
                return match pane.update(tab, message) {
                    wol_tool::Outcome::Task(task) => task,
                    wol_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::Whois(message), Pane::Whois(pane)) => {
                return match pane.update(tab, message) {
                    whois_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    whois_tool::Outcome::Task(task) => task,
                    whois_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::Banner(message), Pane::Banner(pane)) => {
                return match pane.update(tab, message, via) {
                    banner_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    banner_tool::Outcome::Task(task) => task,
                    banner_tool::Outcome::Save(export) => state.export(tab, export, main),
                    banner_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::Ping(message), Pane::Ping(pane)) => {
                return match pane.update(tab, message) {
                    ping_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    ping_tool::Outcome::Task(task) => task,
                    ping_tool::Outcome::Save(export) => state.export(tab, export, main),
                    ping_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::Dns(message), Pane::Dns(pane)) => {
                return match pane.update(tab, message) {
                    dns_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    dns_tool::Outcome::Task(task) => task,
                    dns_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::DnsBatch(message), Pane::DnsBatch(pane)) => {
                return match pane.update(tab, message) {
                    dns_batch_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    dns_batch_tool::Outcome::Task(task) => task,
                    dns_batch_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::PortScan(message), Pane::PortScan(pane)) => {
                return match pane.update(tab, message, via) {
                    port_scan_tool::Outcome::Task(task) => task,
                    port_scan_tool::Outcome::Save(export) => state.export(tab, export, main),
                    port_scan_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::HttpHeaders(message), Pane::HttpHeaders(pane)) => {
                return match pane.update(tab, message, via) {
                    http_headers_tool::Outcome::Copy(slot, content) => {
                        state.copy(tab, slot, content)
                    }
                    http_headers_tool::Outcome::Task(task) => task,
                    http_headers_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::Cert(message), Pane::Cert(pane)) => {
                return match pane.update(tab, message, via) {
                    cert_inspector_tool::Outcome::Copy(slot, content) => {
                        state.copy(tab, slot, content)
                    }
                    cert_inspector_tool::Outcome::Task(task) => task,
                    cert_inspector_tool::Outcome::Save(export) => state.export(tab, export, main),
                    cert_inspector_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::TcpTrace(message), Pane::TcpTrace(pane)) => {
                return match pane.update(tab, message) {
                    tcp_traceroute_tool::Outcome::Task(task) => task,
                    tcp_traceroute_tool::Outcome::Save(export) => state.export(tab, export, main),
                    tcp_traceroute_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::SnmpWalk(message), Pane::SnmpWalk(pane)) => {
                return match pane.update(tab, message) {
                    snmp_walker_tool::Outcome::Task(task) => task,
                    snmp_walker_tool::Outcome::Save(export) => state.export(tab, export, main),
                    snmp_walker_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::ArpMon(message), Pane::ArpMon(pane)) => {
                return match pane.update(tab, message) {
                    arp_monitor_tool::Outcome::Copy(slot, content) => {
                        state.copy(tab, slot, content)
                    }
                    arp_monitor_tool::Outcome::Task(task) => task,
                    arp_monitor_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::FwTest(message), Pane::FwTest(pane)) => {
                return match pane.update(tab, message) {
                    firewall_tester_tool::Outcome::Task(task) => task,
                    firewall_tester_tool::Outcome::Save(export) => state.export(tab, export, main),
                    firewall_tester_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::NetMap(message), Pane::NetMap(pane)) => {
                return match pane.update(tab, message) {
                    network_cartography_tool::Outcome::Task(task) => task,
                    network_cartography_tool::Outcome::Save(export) => {
                        state.export(tab, export, main)
                    }
                    network_cartography_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::OpenPorts(message), Pane::OpenPorts(pane)) => {
                return match pane.update(tab, message) {
                    open_ports_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    open_ports_tool::Outcome::Task(task) => task,
                    open_ports_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::NetIf(message), Pane::NetIf(pane)) => {
                return match pane.update(tab, message) {
                    network_interfaces_tool::Outcome::Copy(slot, content) => {
                        state.copy(tab, slot, content)
                    }
                    network_interfaces_tool::Outcome::Task(task) => task,
                    network_interfaces_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::Routes(message), Pane::Routes(pane)) => {
                return match pane.update(tab, message) {
                    route_table_tool::Outcome::Copy(slot, content) => {
                        state.copy(tab, slot, content)
                    }
                    route_table_tool::Outcome::Task(task) => task,
                    route_table_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::Wifi(message), Pane::Wifi(pane)) => {
                return match pane.update(tab, message) {
                    wifi_networks_tool::Outcome::Copy(slot, content) => {
                        state.copy(tab, slot, content)
                    }
                    wifi_networks_tool::Outcome::Task(task) => task,
                    wifi_networks_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::Hosts(message), Pane::Hosts(pane)) => {
                return match pane.update(tab, message) {
                    hosts_editor_tool::Outcome::Copy(slot, content) => {
                        state.copy(tab, slot, content)
                    }
                    hosts_editor_tool::Outcome::Task(task) => task,
                    hosts_editor_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::Services(message), Pane::Services(pane)) => {
                return match pane.update(tab, message) {
                    service_status_tool::Outcome::Copy(slot, content) => {
                        state.copy(tab, slot, content)
                    }
                    service_status_tool::Outcome::Task(task) => task,
                    service_status_tool::Outcome::None => Task::none(),
                };
            }
            (ToolMessage::LogView(message), Pane::LogView(pane)) => {
                return match pane.update(message) {
                    log_viewer_tool::Outcome::Copy(content) => {
                        state.copy(tab, CopySlot::LogViewText, content)
                    }
                    outcome => outcome.task(tab, main),
                };
            }
            (ToolMessage::CronJob(message), Pane::CronJob(pane)) => {
                return match pane.update(message) {
                    cron_job_tool::Outcome::Copy(content) => {
                        state.copy(tab, CopySlot::CronJobExport, content)
                    }
                    outcome => outcome.task(tab),
                };
            }
            (ToolMessage::Notes(message), Pane::Notes(pane)) => {
                return match pane.update(message) {
                    notes_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    outcome => outcome.task(tab),
                };
            }
            (ToolMessage::Diagram(message), Pane::Diagram(pane)) => {
                return match pane.update(message) {
                    diagram_tool::Outcome::Copy(content) => {
                        state.copy(tab, CopySlot::DiagramSource, content)
                    }
                    outcome => outcome.task(tab, main),
                };
            }
            (ToolMessage::CmdLib(message), Pane::CmdLib(pane)) => {
                return match pane.update(message) {
                    command_library_tool::Outcome::None => Task::none(),
                    command_library_tool::Outcome::Copy(slot, content) => {
                        state.copy(tab, slot, content)
                    }
                    // Only asks: the application writes nothing before the user agrees.
                    command_library_tool::Outcome::Send {
                        target,
                        command,
                        dangerous,
                    } => Task::done(Message::App(heimdall_app::Message::Tools(
                        heimdall_app::ToolsMessage::SendCommand {
                            target,
                            command,
                            dangerous,
                        },
                    ))),
                };
            }
            (message, pane) => {
                if let Some((slot, content)) = update_copying(message, pane) {
                    return state.copy(tab, slot, content);
                }
            }
        }
        Task::none()
    }
}

/// Applies `message` to `pane`, a tool whose update only ever copies; what a copy button
/// copies, when one is pressed. A message of another tool's, its tab closed and another in
/// its place, does nothing.
fn update_copying(message: ToolMessage, pane: &mut Pane) -> Option<(CopySlot, String)> {
    match (message, pane) {
        (ToolMessage::Subnet(message), Pane::Subnet(pane)) => pane.update(message),
        (ToolMessage::IpConverter(message), Pane::IpConverter(pane)) => pane.update(message),
        (ToolMessage::NetCalc(message), Pane::NetCalc(pane)) => pane.update(message),
        (ToolMessage::Chmod(message), Pane::Chmod(pane)) => pane.update(message),
        (ToolMessage::Ulid(message), Pane::Ulid(pane)) => pane.update(message),
        (ToolMessage::Crontab(message), Pane::Crontab(pane)) => pane.update(message),
        (ToolMessage::SshConfig(message), Pane::SshConfig(pane)) => pane.update(message),
        _ => None,
    }
}

/// The colours a tool marks its text with, read from the theme: the regular expression
/// tester's matches, as the C# `RenderHighlight`, and the text comparison's lines and words,
/// as the C# `BuildDerivedBrushes`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Marks {
    /// The text, as the C# `TextPrimaryBrush`.
    pub text: Color,
    /// The secondary text, as the C# `TextSecondaryBrush`.
    pub secondary: Color,
    /// Behind a match: the accent, faint.
    pub regex_match: Color,
    /// Behind a match with a named group: the warning colour, faint.
    pub regex_group: Color,
    /// A removed line's prefix: the error colour.
    pub removed: Color,
    /// An added line's prefix: the success colour.
    pub added: Color,
    /// Behind a removed line.
    pub removed_line: Color,
    /// Behind an added line.
    pub added_line: Color,
    /// Behind a removed word.
    pub removed_word: Color,
    /// Behind an added word.
    pub added_word: Color,
}

impl Marks {
    /// The marks of `theme`.
    #[must_use]
    pub fn of(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let removed = palette.danger.base.color;
        let added = palette.success.base.color;
        Self {
            text: palette.background.base.text,
            secondary: palette.secondary.base.color,
            regex_match: palette.primary.base.color.scale_alpha(REGEX_MATCH_ALPHA),
            regex_group: palette.warning.base.color.scale_alpha(REGEX_GROUP_ALPHA),
            removed,
            added,
            removed_line: removed.scale_alpha(DIFF_LINE_ALPHA),
            added_line: added.scale_alpha(DIFF_LINE_ALPHA),
            removed_word: removed.scale_alpha(DIFF_WORD_ALPHA),
            added_word: added.scale_alpha(DIFF_WORD_ALPHA),
        }
    }
}

/// What tool tab `tab`, showing `tool`, draws in `theme`.
#[must_use]
pub fn view<'a>(
    tab: TabId,
    tool: ToolId,
    state: Option<&'a ToolPane>,
    theme: &Theme,
) -> Element<'a, Message> {
    let Some(state) = state else {
        return column![].into();
    };
    let marks = Marks::of(theme);
    let (title, help) = match tool {
        ToolId::Hash => (fl!("ui-tool-hash-title"), fl!("ui-tool-hash-help")),
        ToolId::Hmac => (fl!("ui-tool-hmac-title"), fl!("ui-tool-hmac-help")),
        ToolId::Jwt => (fl!("ui-tool-jwt-title"), fl!("ui-tool-jwt-help")),
        ToolId::Totp => (fl!("ui-tool-totp-title"), fl!("ui-tool-totp-help")),
        ToolId::Base64 => (fl!("ui-tool-base64-title"), fl!("ui-tool-base64-help")),
        ToolId::UrlEncoder => (fl!("ui-tool-urlenc-title"), fl!("ui-tool-urlenc-help")),
        ToolId::JsonFormatter => (fl!("ui-tool-json-title"), fl!("ui-tool-json-help")),
        ToolId::RegexTester => (fl!("ui-tool-regex-title"), fl!("ui-tool-regex-help")),
        ToolId::TextDiff => (fl!("ui-tool-diff-title"), fl!("ui-tool-diff-help")),
        ToolId::TextCase => (fl!("ui-tool-textcase-title"), fl!("ui-tool-textcase-help")),
        ToolId::Uuid => (fl!("ui-tool-uuid-title"), fl!("ui-tool-uuid-help")),
        ToolId::Password => (fl!("ui-tool-password-title"), fl!("ui-tool-password-help")),
        ToolId::SshKey => (fl!("ui-tool-sshkey-title"), fl!("ui-tool-sshkey-help")),
        ToolId::CertGen => (fl!("ui-tool-certgen-title"), fl!("ui-tool-certgen-help")),
        ToolId::PwdAudit => (fl!("ui-tool-pwdaudit-title"), fl!("ui-tool-pwdaudit-help")),
        ToolId::SubnetCalculator => (fl!("ui-tool-subnet-title"), fl!("ui-tool-subnet-help")),
        ToolId::IpConverter => (fl!("ui-tool-ipconv-title"), fl!("ui-tool-ipconv-help")),
        ToolId::NetworkCalculator => (fl!("ui-tool-netcalc-title"), fl!("ui-tool-netcalc-help")),
        ToolId::Chmod => (fl!("ui-tool-chmod-title"), fl!("ui-tool-chmod-help")),
        ToolId::DateTime => (fl!("ui-tool-datetime-title"), fl!("ui-tool-datetime-help")),
        ToolId::Ulid => (fl!("ui-tool-ulid-title"), fl!("ui-tool-ulid-help")),
        ToolId::Crontab => (fl!("ui-tool-crontab-title"), fl!("ui-tool-crontab-help")),
        ToolId::SshConfig => (
            fl!("ui-tool-sshconfig-title"),
            fl!("ui-tool-sshconfig-help"),
        ),
        ToolId::SshAudit => (fl!("ui-tool-sshaudit-title"), fl!("ui-tool-sshaudit-help")),
        ToolId::TlsAudit => (fl!("ui-tool-tlsaudit-title"), fl!("ui-tool-tlsaudit-help")),
        ToolId::DnsSec => (fl!("ui-tool-dnssec-title"), fl!("ui-tool-dnssec-help")),
        ToolId::DefaultCreds => (
            fl!("ui-tool-defaultcreds-title"),
            fl!("ui-tool-defaultcreds-help"),
        ),
        ToolId::CveLookup => (fl!("ui-tool-cve-title"), fl!("ui-tool-cve-help")),
        ToolId::SecNumCloud => (
            fl!("ui-tool-secnumcloud-title"),
            fl!("ui-tool-secnumcloud-help"),
        ),
        ToolId::Smb => (fl!("ui-tool-smb-title"), fl!("ui-tool-smb-help")),
        ToolId::HttpStatus => (fl!("ui-tool-http-name"), fl!("ui-tool-http-help")),
        ToolId::TcpPing => (fl!("ui-tool-tcpping-name"), fl!("ui-tool-tcpping-help")),
        ToolId::WakeOnLan => (fl!("ui-tool-wol-name"), fl!("ui-tool-wol-help")),
        ToolId::Whois => (fl!("ui-tool-whois-name"), fl!("ui-tool-whois-help")),
        ToolId::BannerGrabber => (fl!("ui-tool-banner-name"), fl!("ui-tool-banner-help")),
        ToolId::Ping => (fl!("ui-tool-ping-name"), fl!("ui-tool-ping-help")),
        ToolId::Dns => (fl!("ui-tool-dns-name"), fl!("ui-tool-dns-help")),
        ToolId::DnsBatch => (fl!("ui-tool-dnsbatch-name"), fl!("ui-tool-dnsbatch-help")),
        ToolId::PortScan => (fl!("ui-tool-portscan-name"), fl!("ui-tool-portscan-help")),
        ToolId::HttpHeaders => (fl!("ui-tool-headers-name"), fl!("ui-tool-headers-help")),
        ToolId::Cert => (fl!("ui-tool-cert-name"), fl!("ui-tool-cert-description")),
        ToolId::TcpTrace => (
            fl!("ui-tool-tcptrace-name"),
            fl!("ui-tool-tcptrace-description"),
        ),
        ToolId::SnmpWalk => (
            fl!("ui-tool-snmpwalk-name"),
            fl!("ui-tool-snmpwalk-description"),
        ),
        ToolId::ArpMon => (
            fl!("ui-tool-arpmon-name"),
            fl!("ui-tool-arpmon-description"),
        ),
        ToolId::FwTest => (
            fl!("ui-tool-fwtest-name"),
            fl!("ui-tool-fwtest-description"),
        ),
        ToolId::NetMap => (
            fl!("ui-tool-netmap-name"),
            fl!("ui-tool-netmap-description"),
        ),
        ToolId::OpenPorts => (
            fl!("ui-tool-openports-title"),
            fl!("ui-tool-openports-help"),
        ),
        ToolId::NetIf => (fl!("ui-tool-netif-title"), fl!("ui-tool-netif-help")),
        ToolId::Routes => (fl!("ui-tool-routes-title"), fl!("ui-tool-routes-help")),
        ToolId::Wifi => (fl!("ui-tool-wifi-title"), fl!("ui-tool-wifi-help")),
        ToolId::Hosts => (fl!("ui-tool-hosts-title"), fl!("ui-tool-hosts-help")),
        ToolId::Services => (fl!("ui-tool-services-title"), fl!("ui-tool-services-help")),
        ToolId::LogView => (fl!("ui-tool-logview-title"), fl!("ui-tool-logview-help")),
        ToolId::CronJob => (fl!("ui-tool-cronjob-title"), fl!("ui-tool-cronjob-help")),
        ToolId::Notes => (fl!("ui-tool-notes-title"), fl!("ui-tool-notes-help")),
        ToolId::Diagram => (fl!("ui-tool-diagram-title"), fl!("ui-tool-diagram-help")),
        ToolId::CmdLib => (fl!("ui-tool-cmdlib-title"), fl!("ui-tool-cmdlib-help")),
    };
    let (body, actions) = match &state.pane {
        Pane::Hash(pane) => (pane.view(tab, state), None),
        Pane::Hmac(pane) => (pane.view(tab, state), None),
        Pane::Jwt(pane) => (pane.view(tab, state), None),
        Pane::Totp(pane) => (pane.view(tab, state), None),
        Pane::Base64(pane) => (pane.view(tab, state), None),
        Pane::Url(pane) => (pane.view(tab, state), None),
        Pane::Uuid(pane) => (pane.view(tab, state), None),
        Pane::Json(pane) => (pane.view(tab, state), None),
        Pane::Regex(pane) => (pane.view(tab, state, marks), None),
        Pane::Diff(pane) => (pane.view(tab, state, marks), Some(pane.header_actions(tab))),
        Pane::TextCase(pane) => (pane.view(tab, state), None),
        Pane::CertGen(pane) => (pane.view(tab, state), None),
        Pane::SshKey(pane) => (pane.view(tab, state), None),
        Pane::Password(pane) => (pane.view(tab, state), None),
        Pane::PwdAudit(pane) => (pane.view(tab, state), None),
        Pane::Subnet(pane) => (pane.view(tab, state), None),
        Pane::IpConverter(pane) => (pane.view(tab, state), None),
        Pane::NetCalc(pane) => (pane.view(tab, state), None),
        Pane::Chmod(pane) => (pane.view(tab, state), None),
        Pane::DateTime(pane) => (pane.view(tab, state), None),
        Pane::Ulid(pane) => (pane.view(tab, state), None),
        Pane::Crontab(pane) => (pane.view(tab, state), None),
        Pane::SshConfig(pane) => (pane.view(tab, state), None),
        Pane::SshAudit(pane) => (pane.view(tab, state, theme), None),
        Pane::TlsAudit(pane) => (pane.view(tab, state, theme), None),
        Pane::DnsSec(pane) => (pane.view(tab, state, theme), None),
        Pane::DefaultCreds(pane) => (pane.view(tab, state), None),
        Pane::Cve(pane) => (pane.view(tab, state, theme), None),
        Pane::SecNumCloud(pane) => (pane.view(tab, state, theme), None),
        Pane::Smb(pane) => (pane.view(tab, state, theme), None),
        Pane::HttpStatus(pane) => (pane.view(tab, state, theme), None),
        Pane::TcpPing(pane) => (pane.view(tab, state, theme), None),
        Pane::Wol(pane) => (pane.view(tab, state, theme), None),
        Pane::Whois(pane) => (pane.view(tab, state, theme), None),
        Pane::Banner(pane) => (pane.view(tab, state, theme), None),
        Pane::Ping(pane) => (pane.view(tab, state, theme), None),
        Pane::Dns(pane) => (pane.view(tab, state, theme), None),
        Pane::DnsBatch(pane) => (pane.view(tab, state, theme), None),
        Pane::PortScan(pane) => (pane.view(tab, state, theme), None),
        Pane::HttpHeaders(pane) => (pane.view(tab, state, theme), None),
        Pane::Cert(pane) => (pane.view(tab, state, theme), None),
        Pane::TcpTrace(pane) => (pane.view(tab, state, theme), None),
        Pane::SnmpWalk(pane) => (pane.view(tab, state, theme), None),
        Pane::ArpMon(pane) => (pane.view(tab, state, theme), None),
        Pane::FwTest(pane) => (pane.view(tab, state, theme), None),
        Pane::NetMap(pane) => (pane.view(tab, state, theme), None),
        Pane::OpenPorts(pane) => (pane.view(tab, state, theme), None),
        Pane::NetIf(pane) => (pane.view(tab, state, theme), None),
        Pane::Routes(pane) => (pane.view(tab, state, theme), None),
        Pane::Wifi(pane) => (pane.view(tab, state, theme), None),
        Pane::Hosts(pane) => (pane.view(tab, state, theme), None),
        Pane::Services(pane) => (pane.view(tab, state, theme), None),
        Pane::LogView(pane) => (pane.view(tab, state), None),
        Pane::CronJob(pane) => (pane.view(tab, state), None),
        Pane::Notes(pane) => (pane.view(tab, state), None),
        Pane::Diagram(pane) => (pane.view(tab, state), None),
        Pane::CmdLib(pane) => (pane.view(tab, state), None),
    };
    column![
        header(tab, title, actions),
        state.help.then(|| help_panel(tab, help)),
        state.export_failure().map(export_failed_strip),
        state.route.view(tab, tool.route_support()),
    ]
    .push(body)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// A tool's header, as the C# tool views': its title, what the tool puts there, and the "?"
/// button at its right.
fn header(
    tab: TabId,
    title: String,
    actions: Option<Element<'_, Message>>,
) -> Element<'_, Message> {
    container(
        row![
            text(title)
                .size(font_size::SUBTITLE)
                .font(styles::SEMIBOLD)
                .width(Length::Fill),
        ]
        .push(actions)
        .push(
            tooltip(
                button(
                    container(
                        text(HELP_GLYPH)
                            .size(font_size::BODY)
                            .font(styles::SEMIBOLD),
                    )
                    .center(Length::Fill),
                )
                .width(HELP_BUTTON_SIDE)
                .height(HELP_BUTTON_SIDE)
                .padding(0.0)
                .style(styles::secondary)
                .on_press(Message::Tool(tab, ToolMessage::ToggleHelp)),
                text(fl!("ui-tool-help-tooltip")).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .padding(styles::TOOLTIP_PADDING)
            .style(styles::tooltip),
        )
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center),
    )
    .padding(HEADER_PADDING)
    .width(Length::Fill)
    .style(styles::strip)
    .into()
}

/// Why the last export failed, under the header, as the C# `StatusExportFailed` line.
fn export_failed_strip<'a>(detail: &str) -> Element<'a, Message> {
    container(error_line(fl!("ui-tool-export-failed", detail = detail)))
        .padding(HELP_PADDING)
        .width(Length::Fill)
        .style(styles::strip)
        .into()
}

/// The help under the header, as the C# inline help panel: the text, and a close button.
fn help_panel<'a>(tab: TabId, help: String) -> Element<'a, Message> {
    container(
        row![
            scrollable(text(help).size(font_size::BODY).style(text::secondary))
                .height(Length::Shrink)
                .width(Length::Fill)
                .style(styles::scrollbars),
            tooltip(
                crate::icons::button(Icon::Close)
                    .style(styles::subtle)
                    .on_press(Message::Tool(tab, ToolMessage::CloseHelp)),
                text(fl!("ui-tool-help-close")).size(font_size::CAPTION),
                tooltip::Position::Left,
            )
            .padding(styles::TOOLTIP_PADDING)
            .style(styles::tooltip),
        ]
        .spacing(spacing::SM),
    )
    .max_height(HELP_MAX_HEIGHT)
    .padding(HELP_PADDING)
    .width(Length::Fill)
    .style(styles::strip)
    .into()
}

/// A copy button of a tool: `label`, or the check mark while its copy is shown, disabled
/// then, as the C# `CopyFeedbackHelper`; its tooltip says it copies to the clipboard.
fn copy_button<'a>(
    label: String,
    copied: bool,
    on_press: Message,
    padding: [f32; 2],
) -> Element<'a, Message> {
    feedback_button(
        label,
        copied,
        on_press,
        padding,
        fl!("ui-tool-copy-tooltip"),
    )
}

/// An export button of a tool: `label`, or the check mark once its file is written, as the
/// C# export buttons show `CopyFeedbackHelper` after `File.WriteAllText`; its tooltip says
/// it saves to a file.
fn export_button<'a>(label: String, saved: bool, on_press: Message) -> Element<'a, Message> {
    feedback_button(
        label,
        saved,
        on_press,
        COPY_PADDING,
        fl!("ui-tool-export-tooltip"),
    )
}

/// A button showing `label`, or the check mark while `done`, disabled then; `tip` its
/// tooltip.
fn feedback_button<'a>(
    label: String,
    done: bool,
    on_press: Message,
    padding: [f32; 2],
    tip: String,
) -> Element<'a, Message> {
    let shown = if done { CHECK_MARK.to_owned() } else { label };
    tooltip(
        button(text(shown).size(font_size::BODY))
            .padding(padding)
            .style(styles::secondary)
            .on_press_maybe((!done).then_some(on_press)),
        text(tip).size(font_size::CAPTION),
        tooltip::Position::Bottom,
    )
    .padding(styles::TOOLTIP_PADDING)
    .style(styles::tooltip)
    .into()
}

/// The hardware vendor a MAC address names, as the ARP monitor and the cartography say it:
/// empty when unknown.
fn vendor_label(vendor: Option<heimdall_core::tools::mac_vendor::MacVendor>) -> String {
    use heimdall_core::tools::mac_vendor::MacVendor;
    match vendor {
        Some(MacVendor::Named(name)) => name.to_owned(),
        Some(MacVendor::Randomized) => fl!("ui-tool-mac-vendor-randomized"),
        None => String::new(),
    }
}

/// Why a network tool's probe got no answer, in the user's language: the reasons of the
/// tree's reachability check, the system's words led by what they are.
fn down_reason_text(reason: &heimdall_app::reachability::DownReason) -> String {
    use heimdall_app::reachability::DownReason;
    match reason {
        DownReason::Timeout => fl!("ui-reachability-reason-timeout"),
        DownReason::Refused => fl!("ui-reachability-reason-refused"),
        DownReason::Unreachable => fl!("ui-reachability-reason-unreachable"),
        DownReason::Dns => fl!("ui-reachability-reason-dns"),
        DownReason::Other(detail) => fl!("ui-tool-net-reason-other", detail = detail.as_str()),
    }
}

/// A tool's main button: `label`, `primary` filled with the accent as the C#
/// `PrimaryButtonStyle`, else secondary.
fn action_button<'a>(
    label: String,
    primary: bool,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    button(text(label).size(font_size::BODY))
        .padding(PRIMARY_PADDING)
        .style(if primary {
            styles::primary
        } else {
            styles::secondary
        })
        .on_press_maybe(on_press)
        .into()
}

/// `content` with `action` applied unless it is an edit: a box that shows a result, which
/// the user selects and scrolls but does not change, as the C# `IsReadOnly` boxes.
fn read_only(content: &mut Content, action: Action) {
    if !action.is_edit() {
        content.perform(action);
    }
}

/// The text of a tool's box, without the line break iced's box may end it with, which the
/// C# box does not have.
fn box_text(content: &Content) -> String {
    let text = content.text();
    text.strip_suffix('\n')
        .map_or_else(|| text.clone(), str::to_owned)
}

/// A box of a tool's text, `content`, in the tool's font, as the C# `TextBox`: its caller
/// says what an action in it does.
fn text_box(
    content: &Content,
    placeholder: Option<String>,
) -> text_editor::TextEditor<'_, PlainText, Message> {
    let editor = iced::widget::text_editor(content)
        .font(BOX_FONT)
        .size(font_size::BODY_LARGE)
        .padding(INPUT_PADDING)
        .style(styles::text_box);
    match placeholder {
        Some(placeholder) => editor.placeholder(placeholder),
        None => editor,
    }
}

/// A tool's content, as the C# tool views': at most [`CONTENT_MAX_WIDTH`] wide, scrolled.
fn content_column(content: Column<'_, Message>) -> Element<'_, Message> {
    styles::scroll(
        container(content.max_width(CONTENT_MAX_WIDTH))
            .padding(spacing::LG)
            .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

/// A label over a tool's box, as the C# labels in the secondary text.
fn field_label<'a>(label: String) -> Element<'a, Message> {
    text(label)
        .size(font_size::BODY)
        .style(text::secondary)
        .into()
}

/// What a tool shows before it has a result, as the C# `ToolEmptyStateStyle`: its hint,
/// centred in the room left.
fn empty_state<'a>(hint: String) -> Element<'a, Message> {
    container(
        text(hint)
            .size(font_size::BODY_LARGE)
            .style(text::secondary),
    )
    .padding(EMPTY_STATE_PADDING)
    .center(Length::Fill)
    .into()
}

/// A tool's status line: what it says, in the error colour when it is an error, as the C#
/// `StatusForegroundBrushKey`; nothing when it says nothing.
fn status_line<'a>(said: Option<(String, bool)>) -> Element<'a, Message> {
    match said {
        Some((said, error)) => text(said)
            .size(font_size::CAPTION)
            .style(if error { text::danger } else { text::secondary })
            .into(),
        None => column![].into(),
    }
}

/// A tool's content holding a table of many columns: as [`content_column`], at most
/// [`TABLE_MAX_WIDTH`] wide.
fn table_column(content: Column<'_, Message>) -> Element<'_, Message> {
    styles::scroll(
        container(content.max_width(TABLE_MAX_WIDTH))
            .padding(spacing::LG)
            .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

/// A tool's content that fills its tab, as the C# tool grids under the header.
fn tool_body(content: Column<'_, Message>) -> Element<'_, Message> {
    container(content)
        .padding(BODY_PADDING)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// A wait of `duration`, its timer made when it is first awaited: in the runtime that runs
/// it, not in the update that asks for it.
async fn wait(duration: Duration) {
    tokio::time::sleep(duration).await;
}

/// Width of a result's label, as the C# results grids' label column.
const RESULT_LABEL_WIDTH: f32 = 160.0;

/// Room between a result's label and its value, as the C# grids' column of 16.
const RESULT_LABEL_GAP: f32 = 16.0;

/// What a tool that scrolls shows before it has a result, as the C# `ToolEmptyStateStyle`:
/// its hint, centred across; a page that scrolls has no height to fill.
fn hint<'a>(hint: String) -> Element<'a, Message> {
    container(
        text(hint)
            .size(font_size::BODY_LARGE)
            .style(text::secondary),
    )
    .padding(EMPTY_STATE_PADDING)
    .center_x(Length::Fill)
    .into()
}

/// A tool's error under its input, in the error colour, as the C# `TxtError` lines.
fn error_line<'a>(said: String) -> Element<'a, Message> {
    status_line(Some((said, true)))
}

/// A value a tool shows, as the C# read-only boxes: monospaced, in the accent, framed.
fn value_box<'a>(value: String) -> Element<'a, Message> {
    container(
        text(value)
            .font(BOX_FONT)
            .size(font_size::BODY_LARGE)
            .style(|theme: &iced::Theme| text::Style {
                color: Some(theme.palette().primary),
            }),
    )
    .padding(INPUT_PADDING)
    .width(Length::Fill)
    .style(styles::field_box)
    .into()
}

/// A row of a tool's results, as the C# results grids: its label, its value monospaced,
/// and its copy button.
fn result_row(label: String, value: String, copy: Element<'_, Message>) -> Element<'_, Message> {
    row![
        text(label)
            .size(font_size::BODY)
            .style(text::secondary)
            .width(RESULT_LABEL_WIDTH),
        iced::widget::space().width(RESULT_LABEL_GAP),
        text(value)
            .font(BOX_FONT)
            .size(font_size::BODY)
            .width(Length::Fill),
        copy,
    ]
    .align_y(iced::Alignment::Center)
    .into()
}

/// A tool's results, one a row, in a card, as the C# results panels.
fn results_card<'a>(rows: impl IntoIterator<Item = Element<'a, Message>>) -> Element<'a, Message> {
    container(Column::with_children(rows).spacing(spacing::SM))
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(styles::card)
        .into()
}
