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

//! What a tool opened on a host or an argument starts with, as each C# view's `Initialize`
//! reads its `ToolContext`: the boxes filled as if typed, through the same messages, so a
//! value given goes through every check a value typed goes through.
//!
//! Two rules on top of the C#. A host given to a tool that reaches it is first checked as
//! [`checked_host`] checks a typed one, and left out when refused: nothing then fills the
//! box. And no tool starts reaching a host on its own: as the C#, the network tools are only
//! filled (`WhoisLookupViewModel.SearchWith` sets the domain and nothing else); what starts
//! at once is local, the CVE search in the bundled catalogue, the Base64 encoding, the log
//! file read, as their C# views do. An argument is plain text, never run.

use std::sync::Arc;

use heimdall_app::App;
use heimdall_app::tools::{ToolContext, ToolId};
use heimdall_core::metadata::MacAddress;
use heimdall_core::tools::net_host::checked_host;
use heimdall_core::tools::network_cartography::{canonical_subnet, normalize_subnet};
use heimdall_core::tools::posix_mode::PosixMode;
use iced::widget::text_editor::{Action, Edit};

use super::{
    BannerMessage, Base64Message, CertInspectorMessage, ChmodMessage, CrontabMessage, CveMessage,
    DateTimeMessage, DefaultCredsMessage, DiffMessage, DnsBatchMessage, DnsMessage, DnsSecMessage,
    FirewallTesterMessage, HashMessage, HmacMessage, HttpHeadersMessage, IpConverterMessage,
    JsonMessage, JwtMessage, LogViewerMessage, NetworkCartographyMessage, PingMessage,
    PortScanMessage, RegexMessage, SmbMessage, SnmpWalkerMessage, SshConfigMessage, SshField,
    SubnetMessage, TcpPingMessage, TcpTracerouteMessage, TlsAuditMessage, ToolMessage, UrlMessage,
    WhoisMessage, WolMessage,
};
use crate::i18n::fl;

/// The ports an HTTP URL leaves unsaid, as the C# `HttpHeaderAnalyzerView` leaves them out.
const IMPLICIT_HTTP_PORTS: [u16; 2] = [80, 443];

/// The context a tool opened from its card or the sidebar inherits, as the C#
/// `CreateInheritedToolContext` (`ToolsTabPopulationService.cs:719-728`): a tool reaching a
/// host, the selected session's, when [`inherited_host`] keeps it; nothing otherwise. The
/// port stays unsaid, as the C#: an SSH session's 22 is not a certificate's port.
#[must_use]
pub fn inherited_context(app: &App, tool: ToolId) -> ToolContext {
    if !tool.is_network() {
        return ToolContext::default();
    }
    inherited_host(app).map_or_else(ToolContext::default, |host| ToolContext::on_host(&host))
}

/// The selected session's host, as the C# `GetInheritedToolTargetHost`, when it is a host a
/// tool could be typed: an address, or a name [`checked_host`] takes.
#[must_use]
pub fn inherited_host(app: &App) -> Option<String> {
    app.tool_target_host().as_deref().and_then(checked_host)
}

/// The title of `tool`'s tab opened on `context`, as the C# `ResolveToolTabTitle`
/// (`ToolsTabPopulationService.cs:736-744`) and its palette: its name, then the host or the
/// argument given.
#[must_use]
pub fn tab_title(tool: ToolId, context: &ToolContext) -> String {
    context
        .shown()
        .map_or_else(|| super::label(tool), |shown| titled(tool, shown))
}

/// `tool`'s name with `shown` after it, as the C# `PaletteTool*With` keys say it.
#[must_use]
pub fn titled(tool: ToolId, shown: &str) -> String {
    fl!(
        "ui-tools-tab-with",
        tool = super::label(tool),
        target = shown
    )
}

/// What `tool` opened on `context` is told first, as if typed, in order.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one arm per tool, as the C# views")]
pub(super) fn messages(tool: ToolId, context: &ToolContext) -> Vec<ToolMessage> {
    let argument = context.argument.as_deref();
    // The host the tool reaches: the one given, else the palette's argument, checked.
    let host = || context.shown().and_then(checked_host);
    match tool {
        ToolId::Ping => host()
            .map(|h| ToolMessage::Ping(PingMessage::HostChanged(h)))
            .into_iter()
            .collect(),
        ToolId::Dns => host()
            .map(|h| ToolMessage::Dns(DnsMessage::HostChanged(h)))
            .into_iter()
            .collect(),
        ToolId::PortScan => host()
            .map(|h| ToolMessage::PortScan(PortScanMessage::HostChanged(h)))
            .into_iter()
            .collect(),
        ToolId::Whois => host()
            .map(|h| ToolMessage::Whois(WhoisMessage::DomainChanged(h)))
            .into_iter()
            .collect(),
        ToolId::BannerGrabber => host()
            .map(|h| ToolMessage::Banner(BannerMessage::HostChanged(h)))
            .into_iter()
            .collect(),
        ToolId::TcpTrace => host()
            .map(|h| ToolMessage::TcpTrace(TcpTracerouteMessage::HostChanged(h)))
            .into_iter()
            .collect(),
        ToolId::SnmpWalk => host()
            .map(|h| ToolMessage::SnmpWalk(SnmpWalkerMessage::HostChanged(h)))
            .into_iter()
            .collect(),
        ToolId::Smb => host()
            .map(|h| ToolMessage::Smb(SmbMessage::HostChanged(h)))
            .into_iter()
            .collect(),
        ToolId::DefaultCreds => host()
            .map(|h| ToolMessage::DefaultCreds(DefaultCredsMessage::HostChanged(h)))
            .into_iter()
            .collect(),
        ToolId::DnsSec => host()
            .map(|h| ToolMessage::DnsSec(DnsSecMessage::DomainChanged(h)))
            .into_iter()
            .collect(),
        ToolId::FwTest => host()
            .map(|h| ToolMessage::FwTest(FirewallTesterMessage::HostsAction(paste(h))))
            .into_iter()
            .collect(),
        ToolId::DnsBatch => host()
            .map(|h| ToolMessage::DnsBatch(DnsBatchMessage::EditorAction(paste(h))))
            .into_iter()
            .collect(),
        ToolId::Cert => {
            endpoint(context)
                .map(|(h, port)| {
                    let mut told = vec![ToolMessage::Cert(CertInspectorMessage::HostChanged(h))];
                    told.extend(port.map(|p| {
                        ToolMessage::Cert(CertInspectorMessage::PortsChanged(p.to_string()))
                    }));
                    told
                })
                .unwrap_or_default()
        }
        ToolId::TlsAudit => {
            endpoint(context)
                .map(|(h, port)| {
                    let mut told = vec![ToolMessage::TlsAudit(TlsAuditMessage::HostChanged(h))];
                    told.extend(port.map(|p| {
                        ToolMessage::TlsAudit(TlsAuditMessage::PortChanged(p.to_string()))
                    }));
                    told
                })
                .unwrap_or_default()
        }
        ToolId::TcpPing => endpoint(context)
            .map(|(h, port)| {
                let mut told = vec![ToolMessage::TcpPing(TcpPingMessage::HostChanged(h))];
                told.extend(
                    port.map(|p| ToolMessage::TcpPing(TcpPingMessage::PortChanged(p.to_string()))),
                );
                told
            })
            .unwrap_or_default(),
        ToolId::HttpHeaders => http_url(context)
            .map(|url| ToolMessage::HttpHeaders(HttpHeadersMessage::UrlChanged(url)))
            .into_iter()
            .collect(),
        ToolId::SshConfig => match host() {
            Some(h) => {
                let mut told = vec![
                    ToolMessage::SshConfig(SshConfigMessage::Edited(SshField::HostName, h.clone())),
                    ToolMessage::SshConfig(SshConfigMessage::Edited(SshField::Alias, h)),
                ];
                told.extend(context.port.map(|p| {
                    ToolMessage::SshConfig(SshConfigMessage::Edited(SshField::Port, p.to_string()))
                }));
                told
            }
            None => Vec::new(),
        },
        // A subnet, not a host: kept when it reads as one, as the scan reads it.
        ToolId::NetMap => context
            .shown()
            .filter(|typed| canonical_subnet(&normalize_subnet(typed)).is_some())
            .map(|typed| {
                ToolMessage::NetMap(NetworkCartographyMessage::CidrChanged(typed.to_owned()))
            })
            .into_iter()
            .collect(),
        // As the C# `MacAddressParser.TryNormalize`: the argument's address, else the host's.
        ToolId::WakeOnLan => argument
            .and_then(|typed| typed.parse::<MacAddress>().ok())
            .or_else(|| context.host.as_deref().and_then(|h| h.parse().ok()))
            .map(|mac| ToolMessage::Wol(WolMessage::MacChanged(mac.to_string())))
            .into_iter()
            .collect(),
        // Computed here, nothing reached: the text as given.
        ToolId::SubnetCalculator => context
            .shown()
            .map(|typed| ToolMessage::Subnet(SubnetMessage::InputEdited(typed.to_owned())))
            .into_iter()
            .collect(),
        ToolId::IpConverter => context
            .shown()
            .map(|typed| {
                ToolMessage::IpConverter(IpConverterMessage::InputEdited(typed.to_owned()))
            })
            .into_iter()
            .collect(),
        // The C# `PrefillInput`: the text encoded at once.
        ToolId::Base64 => argument
            .map(|typed| {
                vec![
                    ToolMessage::Base64(Base64Message::Input(paste(typed.to_owned()))),
                    ToolMessage::Base64(Base64Message::Encode),
                ]
            })
            .unwrap_or_default(),
        ToolId::UrlEncoder => argument
            .map(|typed| ToolMessage::Url(UrlMessage::Decoded(paste(typed.to_owned()))))
            .into_iter()
            .collect(),
        ToolId::JsonFormatter => argument
            .map(|typed| ToolMessage::Json(JsonMessage::Input(paste(typed.to_owned()))))
            .into_iter()
            .collect(),
        ToolId::RegexTester => argument
            .map(|typed| ToolMessage::Regex(RegexMessage::Pattern(typed.to_owned())))
            .into_iter()
            .collect(),
        ToolId::TextDiff => argument
            .map(|typed| ToolMessage::Diff(DiffMessage::Original(paste(typed.to_owned()))))
            .into_iter()
            .collect(),
        // The C# `ApplyPrefill`: a mode read, else the default left.
        ToolId::Chmod => argument
            .filter(|typed| PosixMode::parse_octal(typed).is_some())
            .map(|typed| ToolMessage::Chmod(ChmodMessage::OctalEdited(typed.to_owned())))
            .into_iter()
            .collect(),
        ToolId::DateTime => argument
            .map(|typed| ToolMessage::DateTime(DateTimeMessage::InputEdited(typed.to_owned())))
            .into_iter()
            .collect(),
        // As Enter in the box: an expression not read is said, where the C# says nothing.
        ToolId::Crontab => argument
            .map(|typed| {
                vec![
                    ToolMessage::Crontab(CrontabMessage::ManualEdited(typed.to_owned())),
                    ToolMessage::Crontab(CrontabMessage::ManualSubmitted),
                ]
            })
            .unwrap_or_default(),
        // The C# `LoadFile`: the file read at once, as Open does.
        ToolId::LogView => argument
            .map(|typed| {
                vec![
                    ToolMessage::LogView(LogViewerMessage::Path(typed.to_owned())),
                    ToolMessage::LogView(LogViewerMessage::Open),
                ]
            })
            .unwrap_or_default(),
        ToolId::Hash => argument
            .map(|typed| ToolMessage::Hash(HashMessage::Input(paste(typed.to_owned()))))
            .into_iter()
            .collect(),
        ToolId::Hmac => argument
            .map(|typed| ToolMessage::Hmac(HmacMessage::Input(paste(typed.to_owned()))))
            .into_iter()
            .collect(),
        ToolId::Jwt => argument
            .map(|typed| ToolMessage::Jwt(JwtMessage::Input(paste(typed.to_owned()))))
            .into_iter()
            .collect(),
        // The C# `SearchWith`: searched at once, in the catalogue shipped, nothing reached.
        ToolId::CveLookup => argument
            .map(|typed| {
                vec![
                    ToolMessage::Cve(CveMessage::InputChanged(typed.to_owned())),
                    ToolMessage::Cve(CveMessage::Search),
                ]
            })
            .unwrap_or_default(),
        // The others read nothing of it, as their C# views.
        _ => Vec::new(),
    }
}

/// `text` pasted into an empty box, as typed there.
fn paste(text: String) -> Action {
    Action::Edit(Edit::Paste(Arc::new(text)))
}

/// The host and port of a tool with a port box, as the C# `ParseArgument` reads them: the
/// host given and its port, else the argument, `host:port` or a host alone; the host
/// checked, the port from 1 to 65535.
fn endpoint(context: &ToolContext) -> Option<(String, Option<u16>)> {
    if let Some(host) = context.host.as_deref() {
        return checked_host(host).map(|h| (h, context.port.filter(|p| *p > 0)));
    }
    let typed = context.argument.as_deref()?.trim();
    // A bare IPv6 address holds colons of its own: read whole first.
    if let Some(host) = checked_host(typed) {
        return Some((host, None));
    }
    let (host, port) = typed.rsplit_once(':')?;
    let port = port.parse::<u16>().ok().filter(|p| *p > 0)?;
    checked_host(host).map(|h| (h, Some(port)))
}

/// The URL the HTTP header analyzer opens on, as the C# `HttpHeaderAnalyzerView`: the
/// argument as typed, else `https://` and the host, with its port unless 80 or 443.
fn http_url(context: &ToolContext) -> Option<String> {
    if let Some(typed) = context.argument.as_deref() {
        return Some(typed.trim().to_owned());
    }
    let host = context.host.as_deref().and_then(checked_host)?;
    // An IPv6 address goes in brackets in a URL, RFC 3986 section 3.2.2.
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host
    };
    Some(match context.port {
        Some(port) if port > 0 && !IMPLICIT_HTTP_PORTS.contains(&port) => {
            format!("https://{host}:{port}")
        }
        _ => format!("https://{host}"),
    })
}

#[cfg(test)]
mod tests {
    use heimdall_app::{AppConfig, Message as AppMessage, TabId, ToolsMessage};
    use heimdall_ssh::AgentSource;
    use heimdall_term::GridSize;

    use super::super::{Pane, ToolPanes};
    use super::*;

    /// A host every check takes.
    const HOST: &str = "probe-7.lab";

    /// A host no check takes: a space, a shell's separator, an option's dash.
    const BAD_HOST: &str = "probe-7.lab; rm -rf /tmp/x -oProxyCommand=x";

    /// The tools that reach a host, list hosts to reach, or compute on an address.
    const HOST_TOOLS: [ToolId; 21] = [
        ToolId::Ping,
        ToolId::Dns,
        ToolId::Cert,
        ToolId::PortScan,
        ToolId::Whois,
        ToolId::HttpHeaders,
        ToolId::BannerGrabber,
        ToolId::TcpTrace,
        ToolId::TcpPing,
        ToolId::SnmpWalk,
        ToolId::FwTest,
        ToolId::NetMap,
        ToolId::WakeOnLan,
        ToolId::DnsBatch,
        ToolId::TlsAudit,
        ToolId::DnsSec,
        ToolId::Smb,
        ToolId::DefaultCreds,
        ToolId::SshConfig,
        ToolId::SubnetCalculator,
        ToolId::IpConverter,
    ];

    /// The tools that reach a host and so check it as typed.
    const NETWORK_TOOLS: [ToolId; 19] = [
        ToolId::Ping,
        ToolId::Dns,
        ToolId::Cert,
        ToolId::PortScan,
        ToolId::Whois,
        ToolId::HttpHeaders,
        ToolId::BannerGrabber,
        ToolId::TcpTrace,
        ToolId::TcpPing,
        ToolId::SnmpWalk,
        ToolId::FwTest,
        ToolId::NetMap,
        ToolId::WakeOnLan,
        ToolId::DnsBatch,
        ToolId::TlsAudit,
        ToolId::DnsSec,
        ToolId::Smb,
        ToolId::DefaultCreds,
        ToolId::SshConfig,
    ];

    fn app(dir: &std::path::Path) -> App {
        App::new(AppConfig {
            profiles_file: dir.join("profiles.toml"),
            known_hosts: dir.join("known_hosts"),
            legacy_dir: None,
            agent: AgentSource::Disabled,
            initial_grid: GridSize { cols: 80, rows: 24 },
            files_start: dir.to_owned(),
            system_credentials: heimdall_app::SystemCredentials::memory(),
        })
    }

    /// `tool` opened on `context` as the window opens it: its tab in the core, its state
    /// made and told.
    fn opened(tool: ToolId, context: ToolContext) -> (ToolPanes, TabId) {
        let dir = tempfile::tempdir().expect("dir");
        let mut app = app(dir.path());
        let _ = app.update(AppMessage::Tools(ToolsMessage::Open {
            tool,
            title: tab_title(tool, &context),
            context,
        }));
        let mut panes = ToolPanes::default();
        panes.sync(&app);
        (panes, app.tabs.last().expect("its tab").id)
    }

    /// What the state of `tool` opened on `context` holds, written out: its fields, and
    /// the text of its first box when it has one.
    fn state(tool: ToolId, context: ToolContext) -> String {
        let (panes, tab) = opened(tool, context);
        let pane = &panes.get(tab).expect("its state").pane;
        let first_box = match pane {
            Pane::Base64(pane) => pane.first_box(),
            Pane::Url(pane) => pane.first_box(),
            Pane::Json(pane) => pane.first_box(),
            Pane::Diff(pane) => pane.first_box(),
            Pane::Hash(pane) => pane.first_box(),
            Pane::Hmac(pane) => pane.first_box(),
            Pane::Jwt(pane) => pane.first_box(),
            Pane::FwTest(pane) => pane.first_box(),
            Pane::DnsBatch(pane) => pane.first_box(),
            _ => String::new(),
        };
        format!("{pane:?}\n{first_box}")
    }

    fn on(host: &str, port: Option<u16>) -> ToolContext {
        ToolContext {
            port,
            ..ToolContext::on_host(host)
        }
    }

    fn argument(typed: &str) -> ToolContext {
        ToolContext::with_argument(typed)
    }

    #[test]
    fn the_context_reaches_each_tool_s_state_as_its_csharp_view_reads_it() {
        let table: Vec<(ToolId, ToolContext, &[&str])> = vec![
            (ToolId::Ping, on(HOST, None), &[HOST]),
            (ToolId::Dns, on(HOST, None), &[HOST]),
            (ToolId::Cert, on(HOST, Some(8443)), &[HOST, "8443"]),
            (ToolId::Cert, argument("probe-7.lab:9443"), &[HOST, "9443"]),
            (ToolId::PortScan, on(HOST, None), &[HOST]),
            (ToolId::Whois, on(HOST, None), &[HOST]),
            (
                ToolId::HttpHeaders,
                on(HOST, Some(8080)),
                &["https://probe-7.lab:8080"],
            ),
            (ToolId::HttpHeaders, on("::1", None), &["https://[::1]"]),
            (
                ToolId::HttpHeaders,
                argument("http://probe-7.lab/x"),
                &["http://probe-7.lab/x"],
            ),
            (ToolId::BannerGrabber, on(HOST, None), &[HOST]),
            (ToolId::TcpTrace, on(HOST, None), &[HOST]),
            (ToolId::TcpPing, on(HOST, Some(3389)), &[HOST, "3389"]),
            (
                ToolId::SubnetCalculator,
                argument("10.1.2.0/24"),
                &["10.1.2.0/24"],
            ),
            (ToolId::IpConverter, on("10.9.8.7", None), &["10.9.8.7"]),
            (ToolId::SnmpWalk, on(HOST, None), &[HOST]),
            (ToolId::FwTest, on(HOST, None), &[HOST]),
            (ToolId::NetMap, argument("10.1.2.0/24"), &["10.1.2.0/24"]),
            (
                ToolId::WakeOnLan,
                argument("aa-bb-cc-dd-ee-ff"),
                &["AA:BB:CC:DD:EE:FF"],
            ),
            (ToolId::DnsBatch, on(HOST, None), &[HOST]),
            (ToolId::TlsAudit, on(HOST, Some(8443)), &[HOST, "8443"]),
            (ToolId::TlsAudit, argument("[::1]:993"), &["\"::1\"", "993"]),
            (ToolId::DnsSec, argument("probe.lab"), &["probe.lab"]),
            (ToolId::Smb, on(HOST, None), &[HOST]),
            (ToolId::DefaultCreds, on(HOST, None), &[HOST]),
            (ToolId::SshConfig, on(HOST, Some(2222)), &[HOST, "2222"]),
            // The C# `PrefillInput`: encoded at once.
            (ToolId::Base64, argument("hello"), &["hello", "aGVsbG8="]),
            (ToolId::UrlEncoder, argument("a b&c"), &["a b&c"]),
            (
                ToolId::JsonFormatter,
                argument("{\"marker\":1}"),
                &["{\"marker\":1}"],
            ),
            (ToolId::RegexTester, argument("mark(er)+"), &["mark(er)+"]),
            (ToolId::TextDiff, argument("marker text"), &["marker text"]),
            (ToolId::Chmod, argument("640"), &["640"]),
            (ToolId::DateTime, argument("1700000000"), &["1700000000"]),
            (ToolId::Crontab, argument("*/5 * * * *"), &["*/5 * * * *"]),
            (ToolId::LogView, argument("marker.log"), &["marker.log"]),
            (ToolId::Hash, argument("marker text"), &["marker text"]),
            (ToolId::Hmac, argument("marker text"), &["marker text"]),
            (ToolId::Jwt, argument("marker.token.x"), &["marker.token.x"]),
            // The C# `SearchWith`: searched at once, in the catalogue shipped.
            (
                ToolId::CveLookup,
                argument("openssh 7.4"),
                &["openssh 7.4", "searched: true"],
            ),
        ];
        for (tool, context, expected) in table {
            let held = state(tool, context.clone());
            for wanted in expected {
                assert!(
                    held.contains(wanted),
                    "{tool:?} on {context:?} holds {wanted}: {held}"
                );
            }
        }
    }

    #[test]
    fn a_host_refused_as_typed_is_not_filled_and_nothing_starts() {
        for tool in NETWORK_TOOLS {
            for context in [on(BAD_HOST, Some(443)), argument(BAD_HOST)] {
                let told = messages(tool, &context);
                // The analyzer takes its argument as a URL typed, checked when run.
                if tool == ToolId::HttpHeaders && context.argument.is_some() {
                    assert_eq!(told.len(), 1);
                    continue;
                }
                assert!(told.is_empty(), "{tool:?} told {told:?}");
                assert!(
                    !state(tool, context).contains("rm -rf"),
                    "{tool:?} holds the refused host"
                );
            }
        }
    }

    #[test]
    fn a_tool_reaching_a_host_is_only_filled_never_started() {
        // Each message one a key in a box sends: a host given never starts a probe, a scan
        // or a lookup on its own, as the C# views.
        let fills = |message: &ToolMessage| {
            let written = format!("{message:?}");
            ["Changed(", "Edited(", "Action("]
                .iter()
                .any(|fill| written.contains(fill))
        };
        for tool in HOST_TOOLS {
            for context in [
                on(HOST, Some(8443)),
                argument("10.1.2.0/24"),
                argument(HOST),
            ] {
                for message in messages(tool, &context) {
                    assert!(fills(&message), "{tool:?} told {message:?}");
                }
            }
        }
    }

    #[test]
    fn a_value_not_read_leaves_the_tool_as_it_opens() {
        assert!(
            messages(ToolId::Chmod, &argument("rwx")).is_empty(),
            "755 left"
        );
        assert!(messages(ToolId::WakeOnLan, &argument("not a mac")).is_empty());
        assert!(messages(ToolId::NetMap, &argument("probe lab")).is_empty());
        assert!(messages(ToolId::Cert, &argument("probe-7.lab:99999")).is_empty());
        assert!(messages(ToolId::Ping, &ToolContext::default()).is_empty());
        // A tool reading nothing of it, as its C# view.
        assert!(messages(ToolId::Uuid, &argument("x")).is_empty());
    }

    #[test]
    fn the_tab_is_named_after_the_tool_and_what_it_opened_on() {
        let ping = super::super::label(ToolId::Ping);
        assert_eq!(tab_title(ToolId::Ping, &ToolContext::default()), ping);
        assert_eq!(
            tab_title(ToolId::Ping, &on("10.0.0.1", None)),
            format!("{ping}: 10.0.0.1")
        );
        let base64 = super::super::label(ToolId::Base64);
        assert_eq!(
            tab_title(ToolId::Base64, &argument("hello")),
            format!("{base64}: hello")
        );
    }

    #[test]
    fn only_a_network_tool_inherits_the_selected_host_and_only_a_valid_one() {
        use heimdall_core::profile::{ProfileId, SshProfile};
        let dir = tempfile::tempdir().expect("dir");
        let mut store = heimdall_core::store::ProfileStore::open(dir.path().join("profiles.toml"))
            .expect("store");
        let ssh = |id: &str, host: &str| SshProfile {
            id: ProfileId::new(id),
            name: id.to_owned(),
            group: None,
            host: host.to_owned(),
            port: 22,
            username: None,
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
        };
        store.merge([ssh("good", " web.lab "), ssh("bad", "-oProxyCommand=x")]);
        store.save().expect("save");
        let mut app = app(dir.path());
        app.selected_profile = Some(ProfileId::new("good"));
        assert_eq!(
            inherited_context(&app, ToolId::Ping),
            ToolContext::on_host("web.lab"),
            "trimmed, its port unsaid"
        );
        assert_eq!(
            inherited_context(&app, ToolId::Base64),
            ToolContext::default(),
            "a tool reaching no host"
        );
        app.selected_profile = Some(ProfileId::new("bad"));
        assert_eq!(inherited_host(&app), None, "refused as typed");
        assert_eq!(
            inherited_context(&app, ToolId::Ping),
            ToolContext::default()
        );
    }
}
