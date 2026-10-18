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

//! The Tools area of the core: the registry, a tool opened in a tab as the C#
//! `OpenToolTabAsync`, the tools pinned and used lately as the C# `FavoriteToolSet` and
//! `RecentToolList`, and the sidebar's tab and folded groups kept across runs.

use std::path::Path;

use heimdall_app::tools::{
    MAX_RECENT_TOOLS, RecentTools, SidebarTab, ToolCategory, ToolContext, ToolGroup, ToolId,
    toggle_favorite, tool_invocation,
};
use heimdall_app::{
    App, AppConfig, Dialog, Effect, Message, Phase, TabGroup, TabMenuMessage, TabProfile,
    ToolsMessage,
};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn app(dir: &Path) -> App {
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

fn open(app: &mut App, tool: ToolId) {
    let _ = app.update(Message::Tools(ToolsMessage::Open {
        tool,
        title: tool.code().to_owned(),
        context: heimdall_app::tools::ToolContext::default(),
    }));
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_registry_holds_the_tools_ported_with_their_csharp_entries() {
    assert_eq!(
        ToolId::ALL,
        [
            ToolId::Ping,
            ToolId::Dns,
            ToolId::Cert,
            ToolId::PortScan,
            ToolId::SubnetCalculator,
            ToolId::IpConverter,
            ToolId::HttpStatus,
            ToolId::Whois,
            ToolId::HttpHeaders,
            ToolId::BannerGrabber,
            ToolId::TcpTrace,
            ToolId::SnmpWalk,
            ToolId::ArpMon,
            ToolId::FwTest,
            ToolId::NetMap,
            ToolId::NetworkCalculator,
            ToolId::TcpPing,
            ToolId::Hash,
            ToolId::Hmac,
            ToolId::Password,
            ToolId::SshKey,
            ToolId::CertGen,
            ToolId::Jwt,
            ToolId::Totp,
            ToolId::PwdAudit,
            ToolId::SshAudit,
            ToolId::TlsAudit,
            ToolId::DnsSec,
            ToolId::Smb,
            ToolId::DefaultCreds,
            ToolId::CveLookup,
            ToolId::SecNumCloud,
            ToolId::Base64,
            ToolId::UrlEncoder,
            ToolId::JsonFormatter,
            ToolId::RegexTester,
            ToolId::TextDiff,
            ToolId::TextCase,
            ToolId::Chmod,
            ToolId::DateTime,
            ToolId::Uuid,
            ToolId::Ulid,
            ToolId::Crontab,
            ToolId::LogView,
            ToolId::Hosts,
            ToolId::SshConfig,
            ToolId::CronJob,
            ToolId::Services,
            ToolId::Notes,
            ToolId::Diagram,
            ToolId::CmdLib,
            ToolId::WakeOnLan,
            ToolId::OpenPorts,
            ToolId::NetIf,
            ToolId::Routes,
            ToolId::DnsBatch,
            ToolId::Wifi,
        ]
    );
    assert_eq!(ToolId::Ping.code(), "PING");
    assert!(ToolId::Ping.is_network());
    assert_eq!(ToolId::Dns.code(), "DNS");
    assert_eq!(ToolId::Dns.prefixes(), ["dns", "nslookup", "dig"]);
    assert_eq!(ToolId::DnsBatch.code(), "DNSBATCH");
    assert_eq!(ToolId::DnsBatch.category(), ToolCategory::External);
    assert_eq!(ToolId::PortScan.code(), "PORTSCAN");
    assert!(ToolId::PortScan.is_network());
    assert_eq!(ToolId::HttpHeaders.code(), "HTTPHEADERS");
    assert_eq!(ToolId::HttpStatus.code(), "HTTP");
    assert_eq!(ToolId::Whois.code(), "WHOIS");
    assert_eq!(ToolId::BannerGrabber.code(), "BANNER");
    assert_eq!(ToolId::TcpPing.code(), "TCPPING");
    assert_eq!(ToolId::WakeOnLan.code(), "WOL");
    assert_eq!(ToolId::WakeOnLan.category(), ToolCategory::External);
    assert!(ToolId::TcpPing.is_network());
    assert!(ToolId::BannerGrabber.is_network());
    assert!(ToolId::Whois.is_network());
    assert!(!ToolId::HttpStatus.is_network());
    assert_eq!(ToolId::SubnetCalculator.code(), "SUBNET");
    assert_eq!(
        ToolId::NetworkCalculator.prefixes(),
        ["netcalc", "vlan", "supernet"]
    );
    assert_eq!(ToolId::IpConverter.category(), ToolCategory::Network);
    assert_eq!(ToolId::SshConfig.prefixes(), ["sshconfig", "ssh-config"]);
    assert_eq!(ToolId::Crontab.category(), ToolCategory::System);
    assert_eq!(ToolId::from_code("tool:datetime"), Some(ToolId::DateTime));
    assert_eq!(ToolId::Totp.prefixes(), ["totp", "otp", "2fa"]);
    assert_eq!(ToolId::Jwt.code(), "JWT");
    assert_eq!(ToolId::Smb.code(), "SMBENUM");
    assert_eq!(ToolId::Smb.prefixes(), ["smb", "ntlm", "netbios"]);
    assert!(ToolId::Smb.is_network());
    assert!(
        [ToolId::Hash, ToolId::Hmac, ToolId::Jwt, ToolId::Totp]
            .iter()
            .all(|tool| tool.category() == ToolCategory::Security)
    );
    assert_eq!(ToolId::Base64.code(), "BASE64");
    assert_eq!(ToolId::UrlEncoder.prefixes(), ["url", "urlencode"]);
    assert_eq!(ToolId::Uuid.prefixes(), ["uuid", "guid"]);
    assert_eq!(ToolId::TextCase.prefixes(), ["case", "textcase"]);
    assert_eq!(ToolId::from_code("json"), Some(ToolId::JsonFormatter));
    assert_eq!(ToolId::from_code("TOOL:REGEX"), Some(ToolId::RegexTester));
    assert_eq!(ToolId::from_code("diff"), Some(ToolId::TextDiff));
    assert_eq!(ToolId::TextDiff.category(), ToolCategory::Encoding);
    assert_eq!(ToolId::Base64.category(), ToolCategory::Encoding);
    assert_eq!(ToolId::Uuid.category(), ToolCategory::System);
    assert!(ToolId::TlsAudit.is_network());
    assert!(ToolId::DnsSec.is_network());
    assert_eq!(ToolId::DnsSec.code(), "DNSSEC");
    assert_eq!(ToolId::DnsSec.category(), ToolCategory::Security);
    assert_eq!(
        ToolId::DnsSec.prefixes(),
        ["dnssec", "spf", "dmarc", "dkim"]
    );
    // List-only by owner decision (2026-10-17, item 10): the tool's model exposes
    // no network action, unlike the C# tool it mirrors the credential catalog of.
    assert!(!ToolId::DefaultCreds.is_network());
    assert_eq!(ToolId::DefaultCreds.code(), "DEFAULTCREDS");
    assert_eq!(ToolId::DefaultCreds.category(), ToolCategory::Security);
    assert_eq!(ToolId::DefaultCreds.prefixes(), ["defaultcreds", "creds"]);
    assert_eq!(
        ToolId::from_code("defaultcreds"),
        Some(ToolId::DefaultCreds)
    );
    assert_eq!(
        ToolId::from_code("TOOL:DEFAULTCREDS"),
        Some(ToolId::DefaultCreds)
    );
    assert!(!ToolId::CveLookup.is_network());
    assert_eq!(ToolId::CveLookup.code(), "CVELOOKUP");
    assert_eq!(ToolId::CveLookup.category(), ToolCategory::Security);
    assert_eq!(
        ToolId::CveLookup.prefixes(),
        ["cve", "vuln", "vulnerability"]
    );
    assert_eq!(ToolId::from_code("cvelookup"), Some(ToolId::CveLookup));
    assert!(!ToolId::SecNumCloud.is_network());
    assert_eq!(ToolId::SecNumCloud.code(), "SECNUMCLOUD");
    assert_eq!(ToolId::SecNumCloud.category(), ToolCategory::Security);
    assert_eq!(
        ToolId::SecNumCloud.prefixes(),
        ["secnumcloud", "audit", "compliance", "anssi"]
    );
    assert_eq!(ToolId::from_code("secnumcloud"), Some(ToolId::SecNumCloud));
    assert_eq!(
        ToolId::from_code("TOOL:SECNUMCLOUD"),
        Some(ToolId::SecNumCloud)
    );
    assert!(!ToolId::SshAudit.is_network());
    // Looked up as the C# registry looks up, case aside and its prefix taken off.
    assert_eq!(ToolId::from_code("urlenc"), Some(ToolId::UrlEncoder));
    assert_eq!(ToolId::from_code("TOOL:uuid"), Some(ToolId::Uuid));
    assert_eq!(ToolId::from_code("hmac"), Some(ToolId::Hmac));
    assert_eq!(ToolId::from_code("certgen"), Some(ToolId::CertGen));
    assert_eq!(ToolId::from_code("TOOL:SSHKEY"), Some(ToolId::SshKey));
    assert_eq!(ToolId::Password.prefixes(), ["password", "pwgen"]);
    assert_eq!(ToolId::PwdAudit.code(), "PWDAUDIT");
    assert!(
        [
            ToolId::Password,
            ToolId::SshKey,
            ToolId::CertGen,
            ToolId::PwdAudit
        ]
        .iter()
        .all(|tool| tool.category() == ToolCategory::Security)
    );
    assert_eq!(ToolId::from_code("PING"), Some(ToolId::Ping));
    assert_eq!(ToolId::from_code("TCPTRACE"), Some(ToolId::TcpTrace));
    assert_eq!(ToolId::from_code("OPENPORTS"), Some(ToolId::OpenPorts));
    assert_eq!(ToolId::OpenPorts.category(), ToolCategory::External);
    assert_eq!(ToolId::from_code("NETIF"), Some(ToolId::NetIf));
    assert_eq!(ToolId::NetIf.category(), ToolCategory::External);
    assert_eq!(ToolId::from_code("ROUTES"), Some(ToolId::Routes));
    assert_eq!(ToolId::Routes.category(), ToolCategory::External);
    assert_eq!(ToolId::from_code("WIFI"), Some(ToolId::Wifi));
    assert_eq!(ToolId::Wifi.category(), ToolCategory::External);
    assert_eq!(ToolId::from_code("HOSTS"), Some(ToolId::Hosts));
    assert_eq!(ToolId::Hosts.category(), ToolCategory::System);
    assert_eq!(ToolId::from_code("SERVICES"), Some(ToolId::Services));
    assert_eq!(ToolId::Services.category(), ToolCategory::System);
    assert_eq!(ToolId::from_code("LOGVIEW"), Some(ToolId::LogView));
    assert_eq!(ToolId::LogView.category(), ToolCategory::System);
    assert_eq!(ToolId::from_code("CRONJOB"), Some(ToolId::CronJob));
    assert_eq!(ToolId::CronJob.category(), ToolCategory::System);
    assert_eq!(ToolId::from_code("NOTES"), Some(ToolId::Notes));
    assert_eq!(ToolId::Notes.category(), ToolCategory::System);
    assert_eq!(ToolId::from_code("DIAGRAM"), Some(ToolId::Diagram));
    assert_eq!(ToolId::Diagram.category(), ToolCategory::System);
    assert_eq!(ToolId::from_code("CMDLIB"), Some(ToolId::CmdLib));
    assert_eq!(ToolId::CmdLib.category(), ToolCategory::System);
    assert_eq!(ToolId::from_code("PRIVLAUNCH"), None, "not ported");
}

#[test]
fn pinning_follows_the_csharp_favorite_tool_set() {
    let none: Vec<String> = Vec::new();
    let pinned = toggle_favorite(&none, ToolId::Uuid);
    assert!(pinned.added);
    assert_eq!(pinned.favorites, ["UUID"]);
    // Membership case aside; every spelling taken off; the others' order kept.
    let held = vec![
        "JSON".to_owned(),
        "uuid".to_owned(),
        "PING".to_owned(),
        "Uuid".to_owned(),
    ];
    let unpinned = toggle_favorite(&held, ToolId::Uuid);
    assert!(!unpinned.added);
    assert_eq!(unpinned.favorites, ["JSON", "PING"]);
    assert_eq!(held.len(), 4, "the caller's list is not changed");
}

#[test]
fn the_tools_used_lately_are_five_at_most_the_newest_first() {
    let mut recent = RecentTools::default();
    recent.track(ToolId::Base64);
    recent.track(ToolId::Uuid);
    recent.track(ToolId::Base64);
    assert_eq!(recent.ids(), [ToolId::Base64, ToolId::Uuid]);
    for _ in 0..MAX_RECENT_TOOLS * 2 {
        recent.track(ToolId::UrlEncoder);
    }
    assert!(recent.ids().len() <= MAX_RECENT_TOOLS);
    assert_eq!(recent.ids()[0], ToolId::UrlEncoder);
}

#[test]
fn a_tool_opens_in_a_tab_of_its_own_once_ready_and_is_shown_again_when_open() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    open(&mut app, ToolId::Uuid);
    assert_eq!(app.tabs.len(), 1);
    let tab = &app.tabs[0];
    assert!(matches!(tab.profile, TabProfile::Tool(ToolId::Uuid)));
    assert_eq!(tab.tool(), Some(ToolId::Uuid));
    assert_eq!(tab.phase, Phase::Connected);
    assert_eq!(tab.display_title(), "UUID");
    assert!(!tab.is_live(), "no session is lost when it closes");
    let uuid = tab.id;
    assert_eq!(app.recent_tools(), [ToolId::Uuid]);
    open(&mut app, ToolId::Base64);
    assert_eq!(app.tabs.len(), 2);
    // As the C# singleton tools: shown again, no second tab.
    open(&mut app, ToolId::Uuid);
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active, Some(uuid));
    assert_eq!(app.recent_tools(), [ToolId::Uuid, ToolId::Base64]);
}

#[test]
fn a_tool_tab_closes_without_asking_is_neither_detached_nor_split_nor_reopened() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    open(&mut app, ToolId::Base64);
    open(&mut app, ToolId::Uuid);
    let (base64, uuid) = (app.tabs[0].id, app.tabs[1].id);
    assert!(!app.can_detach(&app.tabs[0]));
    assert!(app.merge_candidates(base64).is_empty());
    assert!(!app.can_reopen(&app.tabs[0]));
    assert!(!app.can_restart(&app.tabs[0]));
    let _ = app.update(Message::RequestCloseTab(uuid));
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(app.tabs.len(), 1);
}

#[test]
fn pins_the_sidebar_tab_and_folded_groups_are_kept_across_runs() {
    let dir = tempfile::tempdir().expect("dir");
    let mut first = app(dir.path());
    assert_eq!(first.sidebar_tab(), SidebarTab::Sessions);
    assert!(first.tool_group_open(ToolGroup::Favorites));
    let _ = first.update(Message::Tools(ToolsMessage::ToggleFavorite(ToolId::Uuid)));
    let _ = first.update(Message::Tools(ToolsMessage::ToggleFavorite(ToolId::Base64)));
    let _ = first.update(Message::Tools(ToolsMessage::ToggleSidebarTab));
    let _ = first.update(Message::Tools(ToolsMessage::ToggleGroup(
        ToolGroup::Category(ToolCategory::Encoding),
    )));
    assert_eq!(first.favorite_tools(), [ToolId::Uuid, ToolId::Base64]);
    assert!(first.is_favorite_tool(ToolId::Uuid));
    open(&mut first, ToolId::Uuid);
    drop(first);

    let mut second = app(dir.path());
    assert_eq!(second.favorite_tools(), [ToolId::Uuid, ToolId::Base64]);
    assert_eq!(second.sidebar_tab(), SidebarTab::Tools);
    assert!(!second.tool_group_open(ToolGroup::Category(ToolCategory::Encoding)));
    assert!(second.tool_group_open(ToolGroup::Category(ToolCategory::System)));
    assert!(
        second.recent_tools().is_empty(),
        "the C# keeps them for the run only"
    );
    let _ = second.update(Message::Tools(ToolsMessage::ShowSidebarTab(
        SidebarTab::Sessions,
    )));
    let _ = second.update(Message::Tools(ToolsMessage::ToggleFavorite(ToolId::Uuid)));
    assert_eq!(second.sidebar_tab(), SidebarTab::Sessions);
    assert_eq!(second.favorite_tools(), [ToolId::Base64]);
    assert!(second.tabs.is_empty(), "a tool is not restored");
}

#[test]
fn a_network_tool_would_inherit_the_selected_session_host() {
    use heimdall_core::profile::{ProfileId, SshProfile};
    let dir = tempfile::tempdir().expect("dir");
    let mut store =
        heimdall_core::store::ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "web".to_owned(),
        group: None,
        host: "  web.lab ".to_owned(),
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
    }]);
    store.save().expect("save");
    let mut app = app(dir.path());
    assert_eq!(app.tool_target_host(), None, "none selected");
    app.selected_profile = Some(ProfileId::new("web"));
    assert_eq!(app.tool_target_host().as_deref(), Some("web.lab"));
}

#[test]
fn a_tool_tab_holding_unsaved_changes_asks_save_discard_or_cancel_first() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    open(&mut app, ToolId::Diagram);
    let tab = app.tabs[0].id;
    app.set_tool_unsaved(tab, Some("net.mmd".to_owned()));
    assert!(app.tabs[0].holds_unsaved_text());
    let _ = app.update(Message::RequestCloseTab(tab));
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmCloseTool {
            tab,
            name: "net.mmd".to_owned()
        })
    );
    assert!(
        !app.dialog.as_ref().is_some_and(Dialog::confirms_on_enter),
        "lost by a click alone"
    );
    // Cancel: the tab stays, its changes with it.
    let _ = app.update(Message::DismissDialog);
    assert_eq!(app.tabs.len(), 1);
    // Save: the window saves, then asks to close again; the tab stays meanwhile.
    let _ = app.update(Message::RequestCloseTab(tab));
    let effects = app.update(Message::Tools(ToolsMessage::SaveBeforeClose));
    assert!(
        matches!(effects.as_slice(), [Effect::SaveTool(saved)] if *saved == tab),
        "{effects:?}"
    );
    assert_eq!(app.dialog, None);
    assert_eq!(app.tabs.len(), 1);
    // Saved: it closes without asking.
    app.set_tool_unsaved(tab, None);
    let _ = app.update(Message::RequestCloseTab(tab));
    assert_eq!(app.dialog, None);
    assert!(app.tabs.is_empty());
}

#[test]
fn discarding_a_tool_tabs_unsaved_changes_closes_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    open(&mut app, ToolId::Notes);
    let tab = app.tabs[0].id;
    app.set_tool_unsaved(tab, Some("Ops".to_owned()));
    let _ = app.update(Message::RequestCloseTab(tab));
    assert!(matches!(app.dialog, Some(Dialog::ConfirmCloseTool { .. })));
    let _ = app.update(Message::ConfirmDialog);
    assert!(app.tabs.is_empty());
    // Saved without a question asked: nothing happens.
    let effects = app.update(Message::Tools(ToolsMessage::SaveBeforeClose));
    assert!(effects.is_empty());
}

#[test]
fn quitting_or_closing_tabs_together_counts_the_tool_tabs_unsaved_changes() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    open(&mut app, ToolId::Uuid);
    open(&mut app, ToolId::Diagram);
    let (uuid, diagram) = (app.tabs[0].id, app.tabs[1].id);
    app.set_tool_unsaved(diagram, Some("net.mmd".to_owned()));
    let _ = app.update(Message::WindowCloseRequested);
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmExit {
            live: 0,
            unsaved: 1
        })
    );
    let _ = app.update(Message::DismissDialog);
    let _ = app.update(Message::TabMenu(TabMenuMessage::Close {
        tab: uuid,
        group: TabGroup::Others,
    }));
    assert!(
        matches!(
            app.dialog,
            Some(Dialog::ConfirmCloseTabs { unsaved: 1, .. })
        ),
        "{:?}",
        app.dialog
    );
    assert_eq!(app.tabs.len(), 2, "nothing closed before it is answered");
}

#[test]
fn a_tool_word_and_an_argument_name_the_tool_and_what_follows() {
    // As the C# `TryParseExplicitToolInvocation`: a command word, a space, something.
    assert_eq!(
        tool_invocation("ping 10.0.0.1"),
        Some((ToolId::Ping, "10.0.0.1".to_owned()))
    );
    assert_eq!(
        tool_invocation("  PING   10.0.0.1  "),
        Some((ToolId::Ping, "10.0.0.1".to_owned())),
        "whatever its case, trimmed"
    );
    assert_eq!(
        tool_invocation("nslookup example.com"),
        Some((ToolId::Dns, "example.com".to_owned())),
        "any of its words"
    );
    assert_eq!(
        tool_invocation("subnet 10.0.0.0/8"),
        Some((ToolId::SubnetCalculator, "10.0.0.0/8".to_owned()))
    );
    assert_eq!(
        tool_invocation("base64 hello world"),
        Some((ToolId::Base64, "hello world".to_owned())),
        "the rest whole, spaces kept"
    );
    assert_eq!(tool_invocation("ping"), None, "a word alone is scored");
    assert_eq!(tool_invocation("ping   "), None);
    assert_eq!(tool_invocation("pingx 10.0.0.1"), None, "not its word");
    assert_eq!(tool_invocation("web server"), None);
    assert_eq!(tool_invocation(""), None);
    assert_eq!(
        tool_invocation("\u{e9}t\u{e9} x"),
        None,
        "not cut inside a letter"
    );
}

#[test]
fn a_context_is_empty_without_a_host_or_an_argument() {
    assert!(ToolContext::default().is_empty());
    assert!(ToolContext::on_host("   ").is_empty());
    assert!(ToolContext::with_argument(" ").is_empty());
    let host = ToolContext::on_host(" web.lab ");
    assert_eq!(host.host.as_deref(), Some("web.lab"), "trimmed");
    assert_eq!(host.shown(), Some("web.lab"));
    let argument = ToolContext::with_argument(" 10.0.0.1 ");
    assert!(!argument.is_empty());
    assert_eq!(argument.shown(), Some("10.0.0.1"));
    let blank = ToolContext {
        host: Some("  ".to_owned()),
        ..ToolContext::default()
    };
    assert!(
        blank.is_empty(),
        "a blank host is no host, as the C# hasContext"
    );
}

#[test]
fn a_tool_opened_on_a_context_keeps_it_in_a_tab_of_its_own() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    open(&mut app, ToolId::Hash);
    open(&mut app, ToolId::Hash);
    assert_eq!(app.tabs.len(), 1, "no context: the open tab shown again");
    assert!(app.tabs[0].tool_context().is_empty());
    // Given an argument, a tab of its own, as the C# `hasContext`.
    let context = ToolContext::with_argument("hello");
    let _ = app.update(Message::Tools(ToolsMessage::Open {
        tool: ToolId::Hash,
        title: "HASH: hello".to_owned(),
        context: context.clone(),
    }));
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.tabs[1].title, "HASH: hello");
    assert_eq!(app.tabs[1].tool_context(), &context);
    // A network tool, a tab each time, its host kept.
    for _ in 0..2 {
        let _ = app.update(Message::Tools(ToolsMessage::Open {
            tool: ToolId::Ping,
            title: "PING: web.lab".to_owned(),
            context: ToolContext::on_host("web.lab"),
        }));
    }
    assert_eq!(app.tabs.len(), 4);
    assert_eq!(app.tabs[3].tool_context().host.as_deref(), Some("web.lab"));
}
